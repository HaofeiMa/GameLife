use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::params;
use rusqlite::Connection;

use gamelife_core::{SAMPLE_INTERVAL_SECS, slot_start, strip_url_query_fragment};

use crate::db::{migrate, open, write_heartbeat};
use crate::db_error::DbOpError;
use crate::scheduler::{
    day_str_for_ts, ensure_slot, slot_rng, startup_from_heartbeat, tick_capture,
};

pub trait SampleSource: Send + Sync {
    fn frontmost_app(&self) -> Result<(String, String), ()>;
    fn idle_seconds(&self) -> i64;
    fn screen_locked(&self) -> bool;
    fn secure_input_on(&self) -> bool;
    fn optional_browser_url(&self) -> Option<String>;
    fn paused(&self) -> bool;
}

pub struct MacSampleSource {
    paused: Arc<AtomicBool>,
}

impl MacSampleSource {
    pub fn new(paused: Arc<AtomicBool>) -> Self {
        Self { paused }
    }
}

impl SampleSource for MacSampleSource {
    fn frontmost_app(&self) -> Result<(String, String), ()> {
        crate::macos::frontmost_app()
    }

    fn idle_seconds(&self) -> i64 {
        crate::macos::idle_seconds()
    }

    fn screen_locked(&self) -> bool {
        crate::macos::screen_locked()
    }

    fn secure_input_on(&self) -> bool {
        crate::macos::secure_input_on()
    }

    fn optional_browser_url(&self) -> Option<String> {
        crate::macos::optional_browser_url()
    }

    fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
}

pub struct FakeSampleSource {
    pub app: String,
    pub title: String,
    pub url: Option<String>,
    pub idle: i64,
    pub locked: bool,
    pub secure: bool,
    pub paused: bool,
}

impl SampleSource for FakeSampleSource {
    fn frontmost_app(&self) -> Result<(String, String), ()> {
        Ok((self.app.clone(), self.title.clone()))
    }

    fn idle_seconds(&self) -> i64 {
        self.idle
    }

    fn screen_locked(&self) -> bool {
        self.locked
    }

    fn secure_input_on(&self) -> bool {
        self.secure
    }

    fn optional_browser_url(&self) -> Option<String> {
        self.url.clone()
    }

    fn paused(&self) -> bool {
        self.paused
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn insert_sample(
    conn: &Connection,
    ts: i64,
    day: &str,
    app: &str,
    title: &str,
    url: Option<&str>,
    idle: i64,
    locked: bool,
    paused: bool,
) -> Result<(), DbOpError> {
    migrate(conn)?;
    conn.execute(
        "INSERT OR IGNORE INTO samples (ts, day, app, title, url, path, idle_seconds, locked, paused)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8)",
        params![
            ts,
            day,
            app,
            title,
            url,
            idle,
            locked as i64,
            paused as i64,
        ],
    )
    .map_err(crate::db_error::map_rusqlite)?;
    Ok(())
}

pub fn sample_once(conn: &Connection, source: &dyn SampleSource, ts: i64) -> Result<(), DbOpError> {
    let day = day_str_for_ts(ts);
    let ss = slot_start(ts);
    let rng = slot_rng(&day, ss);
    ensure_slot(conn, &day, ss, rng)?;

    let locked = source.screen_locked();
    let paused = source.paused();
    let secure = source.secure_input_on();
    let idle = source.idle_seconds();

    let (app, title) = match source.frontmost_app() {
        Ok(v) => v,
        Err(()) => (String::new(), String::new()),
    };
    let url = source
        .optional_browser_url()
        .map(|u| strip_url_query_fragment(&u));
    let url_ref = url.as_deref().filter(|s| !s.is_empty());

    insert_sample(
        conn,
        ts,
        &day,
        &app,
        &title,
        url_ref,
        idle,
        locked,
        paused,
    )?;
    write_heartbeat(conn)?;
    tick_capture(conn, &day, ss, ts, &app, secure, locked, paused)?;
    Ok(())
}

pub fn run_sampler_loop(db_path: PathBuf, source: &dyn SampleSource) {
    let conn = match open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sampler: open db failed: {e:?}");
            return;
        }
    };
    if let Err(e) = migrate(&conn) {
        eprintln!("sampler: migrate failed: {e:?}");
        return;
    }
    let now = now_secs();
    if let Err(e) = startup_from_heartbeat(&conn, now) {
        eprintln!("sampler: startup heartbeat fill failed: {e:?}");
    }
    let interval = Duration::from_secs(SAMPLE_INTERVAL_SECS);
    loop {
        let ts = now_secs();
        if let Err(e) = sample_once(&conn, source, ts) {
            eprintln!("sampler: sample failed: {e:?}");
        }
        thread::sleep(interval);
    }
}

pub fn start_sampler_thread(db_path: PathBuf, paused: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let source = MacSampleSource::new(paused);
        run_sampler_loop(db_path, &source);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use rusqlite::Connection;

    #[test]
    fn sample_once_inserts_row_and_heartbeat() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let source = FakeSampleSource {
            app: "Cursor".into(),
            title: "lib.rs".into(),
            url: Some("https://example.com/x?y=1#z".into()),
            idle: 3,
            locked: false,
            secure: false,
            paused: false,
        };
        let ts = 1_700_000_000i64;
        sample_once(&conn, &source, ts).unwrap();
        let app: String = conn
            .query_row("SELECT app FROM samples WHERE ts = ?1", [ts], |r| r.get(0))
            .unwrap();
        assert_eq!(app, "Cursor");
        let url: String = conn
            .query_row("SELECT url FROM samples WHERE ts = ?1", [ts], |r| r.get(0))
            .unwrap();
        assert_eq!(url, "https://example.com/x");
        let hb: i64 = conn
            .query_row("SELECT ts FROM heartbeat WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(hb > 0);
    }
}
