use std::path::{Path, PathBuf};

use chrono::{Local, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};

use gamelife_core::{
    builtin_never_capture, builtin_side_project_rules, capture_on_resume, heartbeat_unobserved,
    hint_sample, judge_slot, schedule_capture, slot_end_exclusive, slot_start, spans_for_slot,
    CaptureStatus, JudgeInput, Policy, Quest, Sample,
};
use gamelife_core::types::Hint;
use gamelife_core::judge::VisionResult;
use gamelife_core::observe::SpanKind;
use gamelife_core::types::ActivitySeconds;

use crate::db::migrate;
use crate::db_error::{map_rusqlite, DbOpError};
use crate::resolve::resolve_slot;
use crate::vision;

const LOW_INPUT_IDLE_SECS: i64 = 180;
const AWAY_DOMINANT_SECS: i64 = 600;
const STRONG_CORE_AUTO_SECS: i64 = 780;
const SIDE_DISTRACTION_DOMINANT_SECS: i64 = 300;
const SIDE_DISTRACTION_MAX_FOR_AUTO_CORE: i64 = 60;
const READING_BRIDGE_CAP: i64 = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotRetention {
    None,
    Hours24,
    Days3,
    Days14,
}

pub fn default_screenshot_retention() -> ScreenshotRetention {
    ScreenshotRetention::None
}

/// `secure||locked||paused||never.iter().any(|n| app.contains(n))` → Skipped; else Scheduled.
pub fn decide_capture(
    app: &str,
    secure: bool,
    locked: bool,
    paused: bool,
    never: &[String],
) -> CaptureStatus {
    if secure || locked || paused || never.iter().any(|n| app.contains(n)) {
        CaptureStatus::Skipped
    } else {
        CaptureStatus::Scheduled
    }
}

pub fn app_support_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home).join("Library/Application Support/GameLife")
    })
}

pub fn screenshots_dir() -> Option<PathBuf> {
    app_support_dir().map(|d| d.join("screenshots"))
}

fn screenshot_path_for(day: &str, slot_start_ts: i64, ts: i64) -> Option<PathBuf> {
    screenshots_dir().map(|d| d.join(format!("{day}_{slot_start_ts}_{ts}.jpg")))
}

pub fn metadata_decidable(
    activity: &ActivitySeconds,
    strong_core: i64,
    reading_bridge: i64,
    actual: i64,
    quests_empty: bool,
) -> bool {
    if activity.unobserved == actual {
        return true;
    }
    if activity.away >= AWAY_DOMINANT_SECS && strong_core < 300 {
        return true;
    }
    if !quests_empty
        && strong_core >= STRONG_CORE_AUTO_SECS
        && activity.side + activity.distraction <= SIDE_DISTRACTION_MAX_FOR_AUTO_CORE
    {
        return true;
    }
    if activity.side + activity.distraction >= SIDE_DISTRACTION_DOMINANT_SECS
        && activity.side + activity.distraction > strong_core + reading_bridge
    {
        return true;
    }
    false
}

pub fn compute_slot_activity(
    samples: &[Sample],
    policy: &Policy,
    quests: &[Quest],
    slot_start: i64,
    slot_end: i64,
) -> (ActivitySeconds, i64, i64) {
    let mut last_core_interaction_ts: Option<i64> = None;
    let mut hints = Vec::with_capacity(samples.len());
    for sample in samples {
        let hint = hint_sample(sample, policy, quests, last_core_interaction_ts);
        if sample.idle_seconds < LOW_INPUT_IDLE_SECS && hint == Hint::CoreCandidate {
            last_core_interaction_ts = Some(sample.ts);
        }
        hints.push(hint);
    }
    let spans = spans_for_slot(samples, &hints, slot_start, slot_end);
    let mut activity = ActivitySeconds::default();
    let mut strong_core = 0_i64;
    let mut reading_bridge = 0_i64;
    for span in &spans {
        let secs = span.end - span.start;
        match span.kind {
            SpanKind::Unobserved => activity.unobserved += secs,
            SpanKind::Observed(hint) => {
                accumulate_hint_activity(&mut activity, hint, secs);
                let idle = sample_idle_at(samples, span.start);
                match hint {
                    Hint::CoreCandidate if idle < LOW_INPUT_IDLE_SECS => strong_core += secs,
                    Hint::CoreReading => reading_bridge += secs,
                    _ => {}
                }
            }
        }
    }
    reading_bridge = reading_bridge.min(READING_BRIDGE_CAP);
    (activity, strong_core, reading_bridge)
}

fn accumulate_hint_activity(activity: &mut ActivitySeconds, hint: Hint, secs: i64) {
    match hint {
        Hint::Away => activity.away += secs,
        Hint::Distraction => activity.distraction += secs,
        Hint::Side => activity.side += secs,
        Hint::CoreCandidate | Hint::CoreReading => {}
        Hint::UnsureReading | Hint::Unsure => {}
    }
}

fn sample_idle_at(samples: &[Sample], ts: i64) -> i64 {
    samples
        .iter()
        .filter(|s| s.ts <= ts)
        .map(|s| s.idle_seconds)
        .last()
        .unwrap_or(0)
}

pub fn maybe_vision_for_gray_zone(
    metadata_decidable: bool,
    capture: CaptureStatus,
    screenshot_path: &Path,
    capture_app: &str,
    api_key: Option<&str>,
) -> Option<VisionResult> {
    if metadata_decidable || capture != CaptureStatus::Captured {
        return None;
    }
    let key = api_key?;
    vision::analyze_screenshot(screenshot_path, key, Some(capture_app)).ok()
}

pub fn apply_screenshot_retention(path: &Path, retention: ScreenshotRetention) {
    if retention == ScreenshotRetention::None {
        let _ = std::fs::remove_file(path);
    }
}

type CaptureFn = fn(&Path) -> Result<(), ()>;

fn default_capture_fn() -> CaptureFn {
    crate::macos::capture_frontmost_window
}

pub fn slot_rng(day: &str, slot_start_ts: i64) -> u32 {
    let mut h: u32 = slot_start_ts as u32;
    for b in day.bytes() {
        h = h.rotate_left(5) ^ b as u32;
    }
    h
}

/// Insert a slot row with `capture_scheduled_at` on first sight only; never rewrite that column.
pub fn ensure_slot(
    conn: &Connection,
    day: &str,
    slot_start_ts: i64,
    rng: u32,
) -> Result<(), DbOpError> {
    migrate(conn)?;
    let scheduled = schedule_capture(slot_start_ts, rng);
    conn.execute(
        "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status)
         VALUES (?1, ?2, ?3, 'Scheduled')
         ON CONFLICT(day, slot_start) DO NOTHING",
        params![day, slot_start_ts, scheduled],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

pub fn day_str_for_ts(ts: i64) -> String {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".into())
}

pub fn end_of_local_day(ts: i64) -> i64 {
    let dt = Local.timestamp_opt(ts, 0).single().unwrap();
    let date = dt.date_naive();
    let next = date.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap();
    Local.from_local_datetime(&next).unwrap().timestamp()
}

pub fn same_local_day(a: i64, b: i64) -> bool {
    day_str_for_ts(a) == day_str_for_ts(b)
}

pub fn read_heartbeat_ts(conn: &Connection) -> Result<Option<i64>, DbOpError> {
    conn.query_row("SELECT ts FROM heartbeat WHERE id = 1", [], |r| r.get(0))
        .optional()
        .map_err(map_rusqlite)
}

fn slot_is_final(conn: &Connection, day: &str, slot_start_ts: i64) -> Result<bool, DbOpError> {
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM slots WHERE day = ?1 AND slot_start = ?2",
            params![day, slot_start_ts],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    Ok(matches!(status.as_deref(), Some("final" | "unknown")))
}

fn add_unobserved_secs(
    conn: &Connection,
    day: &str,
    slot_start_ts: i64,
    secs: i64,
    rng: u32,
) -> Result<(), DbOpError> {
    if secs <= 0 {
        return Ok(());
    }
    if slot_is_final(conn, day, slot_start_ts)? {
        return Ok(());
    }
    ensure_slot(conn, day, slot_start_ts, rng)?;
    conn.execute(
        "UPDATE slots SET activity_json = json_set(
           COALESCE(activity_json, '{\"core\":0,\"support\":0,\"admin\":0,\"side\":0,\"distraction\":0,\"away\":0,\"unobserved\":0}'),
           '$.unobserved',
           COALESCE(json_extract(activity_json, '$.unobserved'), 0) + ?1
         )
         WHERE day = ?2 AND slot_start = ?3
           AND (status IS NULL OR status NOT IN ('final', 'unknown'))",
        params![secs, day, slot_start_ts],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

/// Distribute heartbeat gap seconds into slot `activity_json.unobserved` (non-final slots only).
pub fn fill_heartbeat_unobserved(conn: &Connection, last_heartbeat: i64, now: i64) -> Result<(), DbOpError> {
    let same_day = same_local_day(last_heartbeat, now);
    let end_day = end_of_local_day(last_heartbeat);
    let ranges = heartbeat_unobserved(last_heartbeat, now, same_day, end_day);
    for (start, end) in ranges {
        let mut t = start;
        while t < end {
            let ss = slot_start(t);
            let se = slot_end_exclusive(ss);
            let overlap_end = end.min(se);
            let secs = overlap_end - t;
            if secs > 0 {
                let day = day_str_for_ts(ss);
                let rng = slot_rng(&day, ss);
                add_unobserved_secs(conn, &day, ss, secs, rng)?;
            }
            t = overlap_end;
        }
    }
    Ok(())
}

pub fn startup_from_heartbeat(conn: &Connection, now: i64) -> Result<(), DbOpError> {
    migrate(conn)?;
    if let Some(last) = read_heartbeat_ts(conn)? {
        fill_heartbeat_unobserved(conn, last, now)?;
    }
    apply_capture_on_resume(conn, now)?;
    Ok(())
}

fn capture_status_from_str(s: &str) -> CaptureStatus {
    match s {
        "Captured" => CaptureStatus::Captured,
        "Missed" => CaptureStatus::Missed,
        "Skipped" => CaptureStatus::Skipped,
        _ => CaptureStatus::Scheduled,
    }
}

fn capture_status_to_str(s: CaptureStatus) -> &'static str {
    match s {
        CaptureStatus::Scheduled => "Scheduled",
        CaptureStatus::Captured => "Captured",
        CaptureStatus::Missed => "Missed",
        CaptureStatus::Skipped => "Skipped",
    }
}

pub fn apply_capture_on_resume(conn: &Connection, now: i64) -> Result<(), DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT day, slot_start, capture_scheduled_at, capture_status
             FROM slots
             WHERE capture_status = 'Scheduled' AND capture_scheduled_at IS NOT NULL",
        )
        .map_err(map_rusqlite)?;
    let rows: Vec<(String, i64, i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(map_rusqlite)?
        .filter_map(|r| r.ok())
        .collect();
    for (day, ss, scheduled_at, status_str) in rows {
        let current = capture_status_from_str(&status_str);
        let next = capture_on_resume(scheduled_at, now, current);
        if next != current {
            conn.execute(
                "UPDATE slots SET capture_status = ?1 WHERE day = ?2 AND slot_start = ?3",
                params![capture_status_to_str(next), day, ss],
            )
            .map_err(map_rusqlite)?;
        }
    }
    Ok(())
}

pub fn should_skip_capture(app: &str, secure: bool, locked: bool, paused: bool) -> bool {
    decide_capture(app, secure, locked, paused, &builtin_never_capture()) == CaptureStatus::Skipped
}

/// At/after scheduled time: skip → Skipped; else attempt frontmost capture → Captured or Missed.
pub fn tick_capture(
    conn: &Connection,
    day: &str,
    slot_start_ts: i64,
    now: i64,
    app: &str,
    secure: bool,
    locked: bool,
    paused: bool,
) -> Result<(), DbOpError> {
    tick_capture_impl(
        conn,
        day,
        slot_start_ts,
        now,
        app,
        secure,
        locked,
        paused,
        default_capture_fn(),
    )
}

fn tick_capture_impl(
    conn: &Connection,
    day: &str,
    slot_start_ts: i64,
    now: i64,
    app: &str,
    secure: bool,
    locked: bool,
    paused: bool,
    capture_fn: CaptureFn,
) -> Result<(), DbOpError> {
    let row: Option<(i64, String)> = conn
        .query_row(
            "SELECT capture_scheduled_at, capture_status FROM slots
             WHERE day = ?1 AND slot_start = ?2",
            params![day, slot_start_ts],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(map_rusqlite)?;
    let Some((scheduled_at, status_str)) = row else {
        return Ok(());
    };
    if capture_status_from_str(&status_str) != CaptureStatus::Scheduled {
        return Ok(());
    }
    if now < scheduled_at {
        return Ok(());
    }
    let never = builtin_never_capture();
    let next = if decide_capture(app, secure, locked, paused, &never) == CaptureStatus::Skipped {
        CaptureStatus::Skipped
    } else if let Some(path) = screenshot_path_for(day, slot_start_ts, now) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if capture_fn(&path).is_ok() {
            conn.execute(
                "UPDATE samples SET path = ?1 WHERE ts = ?2",
                params![path.to_string_lossy().to_string(), now],
            )
            .map_err(map_rusqlite)?;
            CaptureStatus::Captured
        } else {
            CaptureStatus::Missed
        }
    } else {
        CaptureStatus::Missed
    };
    if next != CaptureStatus::Scheduled {
        conn.execute(
            "UPDATE slots SET capture_status = ?1 WHERE day = ?2 AND slot_start = ?3",
            params![capture_status_to_str(next), day, slot_start_ts],
        )
        .map_err(map_rusqlite)?;
    }
    Ok(())
}

fn default_policy() -> Policy {
    Policy {
        trusted_apps: vec![],
        distraction_rules: vec![],
        side_project_rules: builtin_side_project_rules(),
        reading_apps: vec![],
        never_capture_apps: builtin_never_capture(),
    }
}

fn load_samples_for_slot(
    conn: &Connection,
    day: &str,
    slot_start: i64,
    slot_end: i64,
) -> Result<Vec<Sample>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT ts, app, title, url, path, idle_seconds, locked, paused
             FROM samples WHERE day = ?1 AND ts >= ?2 AND ts < ?3 ORDER BY ts",
        )
        .map_err(map_rusqlite)?;
    let rows = stmt
        .query_map(params![day, slot_start, slot_end], |r| {
            Ok(Sample {
                ts: r.get(0)?,
                app: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                window_title: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                url: r.get(3)?,
                path: r.get(4)?,
                idle_seconds: r.get(5)?,
                screen_locked: r.get::<_, i64>(6)? != 0,
                paused: r.get::<_, i64>(7)? != 0,
            })
        })
        .map_err(map_rusqlite)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(map_rusqlite)
}

/// Slot end: strong metadata → no upload; gray zone + Captured → vision HTTP; then judge + resolve.
pub fn finalize_slot_end(
    conn: &mut Connection,
    day: &str,
    slot_start: i64,
    slot_end: i64,
    capture_app: Option<&str>,
    retention: ScreenshotRetention,
    credited_before: i64,
    early_coins: i64,
) -> Result<(), DbOpError> {
    let capture_status_str: Option<String> = conn
        .query_row(
            "SELECT capture_status FROM slots WHERE day = ?1 AND slot_start = ?2",
            params![day, slot_start],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    let capture = capture_status_str
        .as_deref()
        .map(capture_status_from_str)
        .unwrap_or(CaptureStatus::Scheduled);

    let samples = load_samples_for_slot(conn, day, slot_start, slot_end)?;
    let policy = default_policy();
    let quests: Vec<Quest> = vec![];
    let actual = slot_end - slot_start;
    let (activity, strong_core, reading_bridge) =
        compute_slot_activity(&samples, &policy, &quests, slot_start, slot_end);
    let decidable = metadata_decidable(&activity, strong_core, reading_bridge, actual, quests.is_empty());

    let screenshot_path = samples
        .iter()
        .rev()
        .find_map(|s| s.path.as_deref().map(PathBuf::from));

    let api_key = crate::keychain::get_openai_api_key().ok();
    let vision = screenshot_path.as_ref().and_then(|path| {
        capture_app.and_then(|app| {
            maybe_vision_for_gray_zone(decidable, capture, path, app, api_key.as_deref())
        })
    });

    let output = judge_slot(JudgeInput {
        slot_start,
        slot_end,
        samples: &samples,
        quests: &quests,
        policy: &policy,
        capture,
        vision,
        manual_core: None,
    });

    resolve_slot(conn, day, slot_start, &output, credited_before, early_coins)?;

    if let Some(path) = screenshot_path.as_ref() {
        apply_screenshot_retention(path, retention);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use gamelife_core::builtin_never_capture;
    use rusqlite::Connection;
    use std::io::Write;

    fn test_capture_ok(path: &Path) -> Result<(), ()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| ())?;
        }
        let mut f = std::fs::File::create(path).map_err(|_| ())?;
        // Minimal valid JPEG header for vision resize tests if needed.
        f.write_all(&[0xFF, 0xD8, 0xFF, 0xD9]).map_err(|_| ())?;
        Ok(())
    }

    #[test]
    fn decide_capture_skips_1password() {
        let never = builtin_never_capture();
        assert_eq!(
            decide_capture("1Password", false, false, false, &never),
            CaptureStatus::Skipped
        );
    }

    #[test]
    fn decide_capture_scheduled_for_normal_app() {
        let never = builtin_never_capture();
        assert_eq!(
            decide_capture("Cursor", false, false, false, &never),
            CaptureStatus::Scheduled
        );
    }

    #[test]
    fn decide_capture_skips_when_paused() {
        let never = builtin_never_capture();
        assert_eq!(
            decide_capture("Cursor", false, false, true, &never),
            CaptureStatus::Skipped
        );
    }

    #[test]
    fn metadata_decidable_strong_core_path() {
        let activity = ActivitySeconds {
            core: 800,
            side: 0,
            distraction: 30,
            ..Default::default()
        };
        assert!(metadata_decidable(&activity, 800, 0, 900, false));
    }

    #[test]
    fn metadata_not_decidable_gray_zone() {
        let activity = ActivitySeconds {
            side: 100,
            distraction: 50,
            ..Default::default()
        };
        assert!(!metadata_decidable(&activity, 200, 0, 900, false));
    }

    #[test]
    fn ensure_slot_does_not_rewrite_capture_scheduled_at() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let slot_start_ts = 900i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status)
             VALUES (?1, ?2, 42, 'Scheduled')",
            params![day, slot_start_ts],
        )
        .unwrap();
        ensure_slot(&conn, day, slot_start_ts, 99_999).unwrap();
        let scheduled: i64 = conn
            .query_row(
                "SELECT capture_scheduled_at FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, slot_start_ts],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(scheduled, 42);
    }

    #[test]
    fn ensure_slot_inserts_capture_schedule_on_new_slot() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 1_000i64;
        let rng = 7u32;
        ensure_slot(&conn, day, ss, rng).unwrap();
        let scheduled: i64 = conn
            .query_row(
                "SELECT capture_scheduled_at FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(scheduled, schedule_capture(ss, rng));
        let status: String = conn
            .query_row(
                "SELECT capture_status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "Scheduled");
    }

    #[test]
    fn fill_unobserved_skips_final_slot() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, activity_json, credited_core_seconds, observed_seconds, used_vision)
             VALUES (?1, ?2, 'final', '{\"core\":100,\"support\":0,\"admin\":0,\"side\":0,\"distraction\":0,\"away\":0,\"unobserved\":0}', 100, 100, 0)",
            params![day, ss],
        )
        .unwrap();
        fill_heartbeat_unobserved(&conn, 0, 600).unwrap();
        let unobs: i64 = conn
            .query_row(
                "SELECT json_extract(activity_json, '$.unobserved') FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(unobs, 0);
    }

    #[test]
    fn tick_capture_marks_missed_when_past_scheduled() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status)
             VALUES (?1, ?2, 100, 'Scheduled')",
            params![day, ss],
        )
        .unwrap();
        tick_capture(&conn, day, ss, 200, "Cursor", false, false, false).unwrap();
        let status: String = conn
            .query_row(
                "SELECT capture_status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "Missed");
    }

    #[test]
    fn tick_capture_skips_when_paused() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status)
             VALUES (?1, ?2, 100, 'Scheduled')",
            params![day, ss],
        )
        .unwrap();
        tick_capture(&conn, day, ss, 100, "Cursor", false, false, true).unwrap();
        let status: String = conn
            .query_row(
                "SELECT capture_status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "Skipped");
    }

    #[test]
    fn tick_capture_skips_never_capture_app() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status)
             VALUES (?1, ?2, 100, 'Scheduled')",
            params![day, ss],
        )
        .unwrap();
        tick_capture(&conn, day, ss, 100, "1Password", false, false, false).unwrap();
        let status: String = conn
            .query_row(
                "SELECT capture_status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "Skipped");
    }

    #[test]
    fn tick_capture_marks_captured_when_capture_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        unsafe { std::env::set_var("HOME", &home); }

        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        let now = 100i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status)
             VALUES (?1, ?2, 100, 'Scheduled')",
            params![day, ss],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO samples (ts, day, app, title, idle_seconds, locked, paused)
             VALUES (?1, ?2, 'Cursor', 'lib.rs', 0, 0, 0)",
            params![now, day],
        )
        .unwrap();
        tick_capture_impl(
            &conn,
            day,
            ss,
            now,
            "Cursor",
            false,
            false,
            false,
            test_capture_ok,
        )
        .unwrap();
        let status: String = conn
            .query_row(
                "SELECT capture_status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "Captured");
        let path: Option<String> = conn
            .query_row("SELECT path FROM samples WHERE ts = ?1", [now], |r| r.get(0))
            .unwrap();
        assert!(path.as_deref().is_some_and(|p| p.contains(".jpg")));
    }
}
