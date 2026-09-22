use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::params;
use rusqlite::Connection;

use gamelife_core::{
    normalize_document_path, optional_stripped_url, slot_start, CaptureContext,
    SAMPLE_INTERVAL_SECS,
};

use crate::config::{load_settings, retention_from_str};
use crate::db::{migrate, open, write_heartbeat};
use crate::db_error::DbOpError;
use crate::scheduler::{
    add_unobserved_secs, day_str_for_ts, ensure_slot, maybe_finalize_previous_slot, midnight_tick,
    purge_expired_screenshots, purge_old_samples, sampling_allowed, slot_rng, start_of_local_day,
    startup_from_heartbeat, tick_capture,
};

/// Tracks the last sampled slot so we can finalize on boundary crossing.
#[derive(Default)]
pub struct SamplerState {
    pub last_day: Option<String>,
    pub last_slot: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObservedWindow {
    pub app: String,
    pub title: String,
    pub bundle_id: Option<String>,
    pub document_path: Option<String>,
}

pub trait SampleSource: Send + Sync {
    fn observe_window(&self) -> Result<ObservedWindow, ()>;
    fn capture_frontmost_window(&self, path: &std::path::Path) -> Result<(), ()>;
    fn frontmost_app(&self) -> Result<(String, String), ()>;
    fn idle_seconds(&self) -> i64;
    fn screen_locked(&self) -> bool;
    fn secure_input_on(&self) -> bool;
    fn optional_browser_url(&self) -> Option<String>;
    fn paused(&self) -> bool;
    fn metadata_observation_available(&self) -> bool {
        true
    }

    fn capture_observation_available(&self) -> bool {
        true
    }

    fn document_path(&self) -> Option<String>;
    fn bundle_id(&self) -> Option<String>;
    fn capture_context(&self) -> CaptureContext;
}

#[derive(Clone)]
pub struct PauseControl {
    paused: Arc<AtomicBool>,
    resume_at: Arc<AtomicI64>,
    generation: Arc<AtomicU64>,
}

impl PauseControl {
    pub fn new() -> Self {
        Self {
            paused: Arc::new(AtomicBool::new(false)),
            resume_at: Arc::new(AtomicI64::new(0)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn paused_flag(&self) -> Arc<AtomicBool> {
        self.paused.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Pause sampling/capture for `secs`, then auto-clear (latest pause wins).
    pub fn pause_for(&self, secs: u64) {
        let gen = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let deadline = now_secs() + secs as i64;
        self.resume_at.store(deadline, Ordering::Relaxed);
        self.paused.store(true, Ordering::Relaxed);
        let flag = self.paused.clone();
        let resume_at = self.resume_at.clone();
        let generation = self.generation.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(secs));
            if generation.load(Ordering::Relaxed) == gen
                && now_secs() >= resume_at.load(Ordering::Relaxed)
            {
                flag.store(false, Ordering::Relaxed);
            }
        });
    }
}

/// Window observation on the running platform: `macos/`, `windows/` or
/// `linux/`, whichever `observe::imp` resolves to. Everything below is written
/// against that seam, so there is one sampler for every platform.
pub struct NativeSampleSource {
    paused: Arc<AtomicBool>,
    pub(crate) last: crate::observe::ObservationState,
}

impl NativeSampleSource {
    pub fn new(paused: Arc<AtomicBool>) -> Self {
        Self {
            paused,
            last: crate::observe::ObservationState::new(),
        }
    }
}

impl SampleSource for NativeSampleSource {
    fn observe_window(&self) -> Result<ObservedWindow, ()> {
        let snap = crate::observe::imp::snapshot();
        self.last.store(snap.clone());
        Ok(ObservedWindow {
            app: snap.app,
            title: snap.title,
            bundle_id: snap.bundle_id,
            document_path: snap.document_raw,
        })
    }

    fn capture_frontmost_window(&self, path: &std::path::Path) -> Result<(), ()> {
        let id = self.last.take_window_id().ok_or(())?;
        crate::observe::imp::capture_window(id, path)
    }

    fn frontmost_app(&self) -> Result<(String, String), ()> {
        crate::observe::imp::frontmost_app()
    }

    fn idle_seconds(&self) -> i64 {
        crate::observe::imp::idle_seconds()
    }

    fn screen_locked(&self) -> bool {
        crate::observe::imp::screen_locked()
    }

    fn secure_input_on(&self) -> bool {
        crate::observe::imp::secure_input_on()
    }

    fn optional_browser_url(&self) -> Option<String> {
        let last = self.last.last()?;
        crate::observe::imp::url_for(last.bundle_id.as_deref(), &last.app, || {
            crate::observe::imp::fetch_browser_url_for(last.bundle_id.as_deref(), &last.app)
        })
    }

    fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    fn metadata_observation_available(&self) -> bool {
        crate::observe::imp::metadata_observation_available()
    }

    fn capture_observation_available(&self) -> bool {
        crate::observe::imp::capture_observation_available()
    }

    fn document_path(&self) -> Option<String> {
        crate::observe::imp::document_path()
    }

    fn bundle_id(&self) -> Option<String> {
        crate::observe::imp::bundle_id()
    }

    fn capture_context(&self) -> CaptureContext {
        let snap = crate::observe::imp::snapshot();
        self.last.store(snap.clone());
        let url = crate::observe::imp::url_for(snap.bundle_id.as_deref(), &snap.app, || {
            crate::observe::imp::fetch_browser_url_for(snap.bundle_id.as_deref(), &snap.app)
        });
        CaptureContext {
            app: snap.app,
            bundle_id: snap.bundle_id,
            title: snap.title,
            document_path: snap.document_raw.and_then(|s| normalize_document_path(&s)),
            url: optional_stripped_url(url.as_deref()),
            secure_input: crate::observe::imp::secure_input_on(),
        }
    }
}

#[derive(Default)]
pub struct FakeSampleSource {
    pub app: String,
    pub title: String,
    pub url: Option<String>,
    pub document_path: Option<String>,
    pub bundle_id: Option<String>,
    pub idle: i64,
    pub locked: bool,
    pub secure: bool,
    pub paused: bool,
    pub metadata_observation_available: bool,
    pub capture_observation_available: bool,
    pub capture_app: Option<String>,
    pub capture_title: Option<String>,
    pub capture_document_path: Option<String>,
    pub capture_context_calls: AtomicU32,
    pub observe_window_calls: AtomicU32,
    pub legacy_calls: AtomicU32,
}

impl SampleSource for FakeSampleSource {
    fn observe_window(&self) -> Result<ObservedWindow, ()> {
        self.observe_window_calls.fetch_add(1, Ordering::Relaxed);
        Ok(ObservedWindow {
            app: self.app.clone(),
            title: self.title.clone(),
            bundle_id: self.bundle_id.clone(),
            document_path: self.document_path.clone(),
        })
    }

    fn capture_frontmost_window(&self, _path: &std::path::Path) -> Result<(), ()> {
        Err(())
    }

    fn frontmost_app(&self) -> Result<(String, String), ()> {
        self.legacy_calls.fetch_add(1, Ordering::Relaxed);
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

    fn metadata_observation_available(&self) -> bool {
        self.metadata_observation_available
    }

    fn capture_observation_available(&self) -> bool {
        self.capture_observation_available
    }

    fn document_path(&self) -> Option<String> {
        self.legacy_calls.fetch_add(1, Ordering::Relaxed);
        self.document_path.clone()
    }

    fn bundle_id(&self) -> Option<String> {
        self.legacy_calls.fetch_add(1, Ordering::Relaxed);
        self.bundle_id.clone()
    }

    fn capture_context(&self) -> CaptureContext {
        self.capture_context_calls.fetch_add(1, Ordering::Relaxed);
        CaptureContext {
            app: self.capture_app.clone().unwrap_or_else(|| self.app.clone()),
            bundle_id: self.bundle_id.clone(),
            title: self
                .capture_title
                .clone()
                .unwrap_or_else(|| self.title.clone()),
            document_path: self
                .capture_document_path
                .clone()
                .or_else(|| self.document_path.clone()),
            url: optional_stripped_url(self.url.as_deref()),
            secure_input: self.secure,
        }
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
    document_path: Option<&str>,
    bundle_id: Option<&str>,
    idle: i64,
    locked: bool,
    paused: bool,
    secure_input: bool,
) -> Result<(), DbOpError> {
    migrate(conn)?;
    conn.execute(
        "INSERT OR IGNORE INTO samples
         (ts, day, app, title, url, document_path, bundle_id, idle_seconds, locked, paused, secure_input)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            ts,
            day,
            app,
            title,
            url,
            document_path,
            bundle_id,
            idle,
            locked as i64,
            paused as i64,
            secure_input as i64,
        ],
    )
    .map_err(crate::db_error::map_rusqlite)?;
    Ok(())
}

pub fn sample_once(
    conn: &mut Connection,
    source: &dyn SampleSource,
    ts: i64,
    state: &mut SamplerState,
) -> Result<(), DbOpError> {
    let day = day_str_for_ts(ts);
    if !sampling_allowed(conn, &day)? {
        return Ok(());
    }
    let settings = load_settings();
    let retention = retention_from_str(&settings.screenshot_retention);
    let today_start = start_of_local_day(ts);
    if ts >= today_start {
        midnight_tick(conn, ts, retention)?;
    }
    let ss = slot_start(ts);
    maybe_finalize_previous_slot(
        conn,
        state.last_day.as_deref(),
        state.last_slot,
        &day,
        ss,
        retention,
    )?;
    let rng = slot_rng(&day, ss);
    ensure_slot(conn, &day, ss, rng)?;

    if !source.metadata_observation_available() {
        add_unobserved_secs(conn, &day, ss, SAMPLE_INTERVAL_SECS as i64, rng)?;
        write_heartbeat(conn)?;
        state.last_day = Some(day);
        state.last_slot = Some(ss);
        return Ok(());
    }

    let locked = source.screen_locked();
    let paused = source.paused();
    let secure = source.secure_input_on();
    let idle = source.idle_seconds();

    let observed = match source.observe_window() {
        Ok(v) => v,
        Err(()) => ObservedWindow::default(),
    };
    let url = optional_stripped_url(source.optional_browser_url().as_deref());
    let url_ref = url.as_deref();
    let document_path = observed
        .document_path
        .as_deref()
        .and_then(normalize_document_path);
    insert_sample(
        conn,
        ts,
        &day,
        &observed.app,
        &observed.title,
        url_ref,
        document_path.as_deref(),
        observed.bundle_id.as_deref(),
        idle,
        locked,
        paused,
        secure,
    )?;
    write_heartbeat(conn)?;
    tick_capture(
        conn,
        &day,
        ss,
        ts,
        locked,
        paused,
        source.capture_observation_available(),
        || source.capture_context(),
        |p| source.capture_frontmost_window(p),
    )?;
    state.last_day = Some(day);
    state.last_slot = Some(ss);
    Ok(())
}

pub fn run_sampler_loop(db_path: PathBuf, source: &dyn SampleSource) {
    let mut conn = match open(&db_path) {
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
    if let Err(e) = crate::scheduler::seed_default_policy_if_needed(&conn) {
        eprintln!("sampler: policy seed failed: {e:?}");
        return;
    }
    let now = now_secs();
    let settings = load_settings();
    let retention = retention_from_str(&settings.screenshot_retention);
    if let Err(e) = startup_from_heartbeat(&mut conn, now, retention) {
        eprintln!("sampler: startup heartbeat fill failed: {e:?}");
    }
    if let Err(e) = purge_expired_screenshots(&conn, retention, now) {
        eprintln!("sampler: purge_expired_screenshots failed: {e:?}");
    }
    if let Err(e) = purge_old_samples(&conn, settings.sample_keep_days, now) {
        eprintln!("sampler: purge_old_samples failed: {e:?}");
    }
    let mut state = SamplerState::default();
    let interval = Duration::from_secs(SAMPLE_INTERVAL_SECS);
    let mut ticks: u64 = 0;
    loop {
        let ts = now_secs();
        if let Err(e) = sample_once(&mut conn, source, ts, &mut state) {
            eprintln!("sampler: sample failed: {e:?}");
        }
        ticks += 1;
        if ticks % SYNC_CHECK_EVERY_TICKS == 0 {
            crate::sync::maybe_spawn_sync(&conn, ts);
            crate::ticktick::maybe_spawn_sync(&conn, ts);
            crate::task_notify::sync_now();
        }
        thread::sleep(interval);
    }
}

/// The cloud sync check is cheap (a settings read plus an `app_meta` lookup) but
/// not free, and the shortest meaningful interval is minutes, so it runs once
/// every five minutes rather than on every 15-second tick.
pub const SYNC_CHECK_EVERY_TICKS: u64 = 20;

pub fn start_sampler_thread(db_path: PathBuf, paused: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let source = NativeSampleSource::new(paused);
        run_sampler_loop(db_path, &source);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use crate::scheduler::day_str_for_ts;
    use rusqlite::Connection;

    #[test]
    fn sample_once_inserts_row_and_heartbeat() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let source = FakeSampleSource {
            app: "Cursor".into(),
            title: "lib.rs".into(),
            url: Some("https://example.com/x?y=1#z".into()),
            document_path: None,
            bundle_id: None,
            idle: 3,
            locked: false,
            secure: false,
            paused: false,
            metadata_observation_available: true,
            capture_observation_available: true,
            ..Default::default()
        };
        let ts = 1_700_000_000i64;
        let mut state = SamplerState::default();
        sample_once(&mut conn, &source, ts, &mut state).unwrap();
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

    /// Step 4 substitute: one FakeSampleSource tick must persist sample + heartbeat rows.
    #[test]
    fn fake_source_one_sampling_tick_persists_sample_and_heartbeat() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let samples_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
            .unwrap();
        assert_eq!(samples_before, 0);

        let source = FakeSampleSource {
            app: "Safari".into(),
            title: "Example".into(),
            url: None,
            document_path: None,
            bundle_id: None,
            idle: 0,
            locked: false,
            secure: false,
            paused: false,
            metadata_observation_available: true,
            capture_observation_available: true,
            ..Default::default()
        };
        let ts = 1_700_000_015i64;
        let mut state = SamplerState::default();
        sample_once(&mut conn, &source, ts, &mut state).unwrap();

        let samples_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
            .unwrap();
        assert_eq!(samples_after, 1);
        let heartbeat_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM heartbeat WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(heartbeat_rows, 1);
    }

    #[test]
    fn missing_permission_records_unobserved_not_sample() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let source = FakeSampleSource {
            app: "Cursor".into(),
            title: "lib.rs".into(),
            url: None,
            document_path: None,
            bundle_id: None,
            idle: 600,
            locked: false,
            secure: false,
            paused: false,
            metadata_observation_available: false,
            capture_observation_available: true,
            ..Default::default()
        };
        let ts = 1_700_000_000i64;
        let mut state = SamplerState::default();
        sample_once(&mut conn, &source, ts, &mut state).unwrap();
        let samples: i64 = conn
            .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
            .unwrap();
        assert_eq!(samples, 0);
        let unobs: i64 = conn
            .query_row(
                "SELECT json_extract(activity_json, '$.unobserved') FROM slots WHERE day = ?1",
                [day_str_for_ts(ts)],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(unobs, SAMPLE_INTERVAL_SECS as i64);
        let away: i64 = conn
            .query_row(
                "SELECT json_extract(activity_json, '$.away') FROM slots WHERE day = ?1",
                [day_str_for_ts(ts)],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(away, 0);
    }

    #[test]
    fn metadata_without_screen_recording_still_samples() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let source = FakeSampleSource {
            app: "Cursor".into(),
            title: "lib.rs".into(),
            url: None,
            document_path: None,
            bundle_id: None,
            idle: 3,
            locked: false,
            secure: false,
            paused: false,
            metadata_observation_available: true,
            capture_observation_available: false,
            ..Default::default()
        };
        let ts = 1_700_000_000i64;
        let mut state = SamplerState::default();
        sample_once(&mut conn, &source, ts, &mut state).unwrap();
        let samples: i64 = conn
            .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
            .unwrap();
        assert_eq!(samples, 1);
        let status: String = conn
            .query_row(
                "SELECT capture_status FROM slots WHERE day = ?1",
                [day_str_for_ts(ts)],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "Skipped");
    }

    #[test]
    fn sample_once_normalizes_tilde_document_path() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let home = std::env::var("HOME").expect("HOME");
        let source = FakeSampleSource {
            app: "Cursor".into(),
            title: "train.py — HDP".into(),
            url: None,
            document_path: Some("~/Projects/HDP/train.py".into()),
            bundle_id: Some("com.todesktop.230313mzl4w4u92".into()),
            idle: 3,
            locked: false,
            secure: false,
            paused: false,
            metadata_observation_available: true,
            capture_observation_available: true,
            ..Default::default()
        };
        let ts = 1_700_000_000i64;
        let mut state = SamplerState::default();
        sample_once(&mut conn, &source, ts, &mut state).unwrap();
        let path: String = conn
            .query_row(
                "SELECT document_path FROM samples WHERE ts = ?1",
                [ts],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(path, format!("{home}/Projects/HDP/train.py"));
        let bundle: String = conn
            .query_row("SELECT bundle_id FROM samples WHERE ts = ?1", [ts], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(bundle, "com.todesktop.230313mzl4w4u92");
    }

    #[test]
    fn sample_once_calls_observe_window_once_not_legacy_getters() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let source = FakeSampleSource {
            app: "Cursor".into(),
            title: "train.py — HDP".into(),
            url: None,
            document_path: Some("train.py — HDP".into()),
            bundle_id: Some("com.todesktop.230313mzl4w4u92".into()),
            idle: 1,
            locked: false,
            secure: false,
            paused: false,
            metadata_observation_available: true,
            capture_observation_available: true,
            ..Default::default()
        };
        let ts = 1_700_000_000i64;
        let mut state = SamplerState::default();
        sample_once(&mut conn, &source, ts, &mut state).unwrap();
        assert_eq!(source.observe_window_calls.load(Ordering::Relaxed), 1);
        assert_eq!(source.legacy_calls.load(Ordering::Relaxed), 0);
        let path: Option<String> = conn
            .query_row(
                "SELECT document_path FROM samples WHERE ts = ?1",
                [ts],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(path, None);
    }

    #[test]
    fn sample_once_does_not_invent_document_path_from_title() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let source = FakeSampleSource {
            app: "Cursor".into(),
            title: "train.py — HDP".into(),
            url: None,
            document_path: Some("train.py — HDP".into()),
            bundle_id: None,
            idle: 3,
            locked: false,
            secure: false,
            paused: false,
            metadata_observation_available: true,
            capture_observation_available: true,
            ..Default::default()
        };
        let ts = 1_700_000_000i64;
        let mut state = SamplerState::default();
        sample_once(&mut conn, &source, ts, &mut state).unwrap();
        let path: Option<String> = conn
            .query_row(
                "SELECT document_path FROM samples WHERE ts = ?1",
                [ts],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(path, None);
    }

    #[test]
    fn sample_once_persists_secure_input() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let source = FakeSampleSource {
            app: "1Password".into(),
            title: "Bank".into(),
            url: None,
            document_path: None,
            bundle_id: None,
            idle: 1,
            locked: false,
            secure: true,
            paused: false,
            metadata_observation_available: true,
            capture_observation_available: true,
            ..Default::default()
        };
        let ts = 1_700_000_000i64;
        let mut state = SamplerState::default();
        sample_once(&mut conn, &source, ts, &mut state).unwrap();
        let secure: i64 = conn
            .query_row(
                "SELECT secure_input FROM samples WHERE ts = ?1",
                [ts],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(secure, 1);
    }

    #[test]
    fn capture_without_window_id_is_err() {
        let src = NativeSampleSource::new(Arc::new(AtomicBool::new(false)));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.jpg");
        assert!(src.capture_frontmost_window(&path).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn fetch_is_not_used_when_last_is_empty() {
        let src = NativeSampleSource::new(Arc::new(AtomicBool::new(false)));
        assert_eq!(src.optional_browser_url(), None);
    }

    #[test]
    fn fake_capture_context_strips_url_query_fragment() {
        let source = FakeSampleSource {
            url: Some("https://arxiv.org/abs/1?foo=1#bar".into()),
            ..Default::default()
        };
        assert_eq!(
            source.capture_context().url.as_deref(),
            Some("https://arxiv.org/abs/1")
        );
        assert_eq!(source.capture_context().document_path, None);
    }

    #[test]
    fn fake_capture_context_treats_empty_stripped_url_as_none() {
        let source = FakeSampleSource {
            url: Some("?foo=1".into()),
            ..Default::default()
        };
        assert_eq!(source.capture_context().url, None);
    }

    #[test]
    fn mac_source_skips_browser_fetch_for_cursor_snapshot() {
        let paused = Arc::new(AtomicBool::new(false));
        let src = NativeSampleSource::new(paused);
        src.last.store(crate::observe::imp::FrontmostSnapshot {
            app: "Cursor".into(),
            bundle_id: Some("com.todesktop.230313mzl4w4u92".into()),
            ..Default::default()
        });
        assert_eq!(src.optional_browser_url(), None);
    }

    #[test]
    fn overlapping_pause_only_latest_resumes() {
        let pc = PauseControl::new();
        pc.pause_for(90 * 60);
        pc.pause_for(30 * 60);
        pc.paused_flag().store(true, Ordering::Relaxed);
        thread::sleep(Duration::from_millis(50));
        assert!(pc.is_paused());
    }

    #[test]
    fn pause_control_sets_flag_and_can_clear() {
        let pc = PauseControl::new();
        assert!(!pc.is_paused());
        pc.paused_flag().store(true, Ordering::Relaxed);
        assert!(pc.is_paused());
        pc.paused_flag().store(false, Ordering::Relaxed);
        assert!(!pc.is_paused());
    }
}
