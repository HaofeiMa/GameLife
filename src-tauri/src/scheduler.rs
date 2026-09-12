use std::path::{Path, PathBuf};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

use gamelife_core::judge::{Dominant, JudgeOutput, VisionResult};
use gamelife_core::types::{ActivitySeconds, Hint};
use gamelife_core::{
    activity_summary_for_vision, analyze_slot_evidence, builtin_never_capture,
    builtin_side_project_rules, can_use_freeze, capture_on_resume, credited_core_spans,
    default_distraction_rules, default_v01, early_start_anchor, early_start_coins_for_local_secs,
    heartbeat_unobserved, hint_sample, is_weekday, judge_slot, judgment_tasks, matches_app_identity,
    new_milestones, normalize_quest_list, parse_quest_versions_json, parse_task_snapshot_json,
    quest_list_has_evidence, recompute_streak, schedule_capture, slot_end_exclusive, slot_start,
    snapshot_of, spans_for_slot, vision_quest_label, apply_task_match, parse_task_match_json,
    CaptureContext, CaptureStatus, CategoryGuides, DayOutcome, JudgeInput, Policy, Quest,
    QuestDraft, QuestListError,
    Sample, TASK_MATCH_MIN, TaskListError, TaskSnapshot, VisionContext, CHEST_SECS,
};

use crate::db::{app_db_path, insert_ledger, load_task_lists, load_tasks, migrate, open};
use crate::db_error::{map_rusqlite, DbOpError};
use crate::resolve::resolve_slot;
use crate::text_ai::{call_text_task_match, sample_summary_lines, SampleLine};
use crate::vision;

const LOW_INPUT_IDLE_SECS: i64 = 180;
const AWAY_DOMINANT_SECS: i64 = 600;
const STRONG_CORE_AUTO_SECS: i64 = 780;
const SIDE_DISTRACTION_DOMINANT_SECS: i64 = 300;
const SIDE_DISTRACTION_MAX_FOR_AUTO_CORE: i64 = 60;

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

pub fn retention_ttl_secs(retention: ScreenshotRetention) -> Option<i64> {
    match retention {
        ScreenshotRetention::None => None,
        ScreenshotRetention::Hours24 => Some(24 * 3600),
        ScreenshotRetention::Days3 => Some(3 * 86400),
        ScreenshotRetention::Days14 => Some(14 * 86400),
    }
}

pub fn is_screenshot_expired(mtime: i64, now: i64, retention: ScreenshotRetention) -> bool {
    retention_ttl_secs(retention).is_some_and(|ttl| now - mtime > ttl)
}

/// Delete `samples` rows older than `keep_days` (default 7 in settings).
pub fn purge_old_samples(conn: &Connection, keep_days: i64, now: i64) -> Result<u64, DbOpError> {
    if keep_days <= 0 {
        return Ok(0);
    }
    let cutoff = now - keep_days * 86400;
    let deleted = conn
        .execute("DELETE FROM samples WHERE ts < ?1", params![cutoff])
        .map_err(map_rusqlite)?;
    Ok(deleted as u64)
}

pub fn purge_expired_screenshots(
    conn: &Connection,
    retention: ScreenshotRetention,
    now: i64,
) -> Result<(), DbOpError> {
    let Some(ttl) = retention_ttl_secs(retention) else {
        return Ok(());
    };
    let Some(dir) = screenshots_dir() else {
        return Ok(());
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let mtime = modified
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if now - mtime > ttl {
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    let path_str = path.to_string_lossy().to_string();
                    conn.execute(
                        "UPDATE slots SET screenshot_path = NULL, capture_context_json = NULL
                         WHERE screenshot_path = ?1",
                        params![path_str],
                    )
                    .map_err(map_rusqlite)?;
                }
                Err(_) => {}
            }
        }
    }
    Ok(())
}

/// Never Capture / secure input / lock / pause → Skipped; else Scheduled.
pub fn decide_capture(
    app: &str,
    bundle_id: Option<&str>,
    secure: bool,
    locked: bool,
    paused: bool,
    never: &[String],
) -> CaptureStatus {
    if secure || locked || paused || matches_app_identity(app, bundle_id, never) {
        CaptureStatus::Skipped
    } else {
        CaptureStatus::Scheduled
    }
}

pub fn app_support_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support/GameLife"))
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
    grounded_strong_core: i64,
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
        && grounded_strong_core >= STRONG_CORE_AUTO_SECS
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
    snapshots: &[TaskSnapshot],
    slot_start: i64,
    slot_end: i64,
) -> (ActivitySeconds, i64, i64) {
    let ev = analyze_slot_evidence(samples, policy, quests, snapshots, slot_start, slot_end);
    (
        ev.activity,
        ev.strong_core_seconds,
        ev.reading_bridge_seconds,
    )
}

pub fn maybe_vision_for_gray_zone(
    metadata_decidable: bool,
    capture: CaptureStatus,
    screenshot_path: &Path,
    ctx: VisionContext,
    never: &[String],
    primary: Option<&vision::VisionEndpoint>,
    fallback: Option<&vision::VisionEndpoint>,
) -> Option<VisionResult> {
    if metadata_decidable || capture != CaptureStatus::Captured {
        return None;
    }
    vision::analyze_screenshot_with_fallback(screenshot_path, primary, fallback, ctx, never).ok()
}

pub fn apply_screenshot_retention(path: &Path, retention: ScreenshotRetention) {
    match retention {
        ScreenshotRetention::None => {
            let _ = std::fs::remove_file(path);
        }
        ScreenshotRetention::Hours24 | ScreenshotRetention::Days3 | ScreenshotRetention::Days14 => {
        }
    }
}

fn clear_capture_columns(conn: &Connection, day: &str, slot_start: i64) -> Result<(), DbOpError> {
    conn.execute(
        "UPDATE slots SET screenshot_path = NULL, capture_context_json = NULL
         WHERE day = ?1 AND slot_start = ?2",
        params![day, slot_start],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

pub fn apply_capture_retention(
    conn: &Connection,
    day: &str,
    slot_start: i64,
    retention: ScreenshotRetention,
    slot_status: &str,
) -> Result<(), DbOpError> {
    if slot_status == "pending_review" {
        return Ok(());
    }
    if slot_status != "final" && slot_status != "unknown" {
        return Ok(());
    }
    match retention {
        ScreenshotRetention::None => {
            let path: Option<String> = conn
                .query_row(
                    "SELECT screenshot_path FROM slots WHERE day = ?1 AND slot_start = ?2",
                    params![day, slot_start],
                    |r| r.get(0),
                )
                .optional()
                .map_err(map_rusqlite)?
                .flatten();
            if let Some(p) = path {
                match std::fs::remove_file(&p) {
                    Ok(()) => clear_capture_columns(conn, day, slot_start)?,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        clear_capture_columns(conn, day, slot_start)?;
                    }
                    Err(_) => {}
                }
            } else {
                clear_capture_columns(conn, day, slot_start)?;
            }
        }
        ScreenshotRetention::Hours24 | ScreenshotRetention::Days3 | ScreenshotRetention::Days14 => {
        }
    }
    Ok(())
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
    let quest_vid: Option<i64> = conn
        .query_row(
            "SELECT id FROM quest_versions WHERE day = ?1 ORDER BY id DESC LIMIT 1",
            params![day],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    let policy_vid: Option<i64> = conn
        .query_row(
            "SELECT id FROM policy_versions ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    let snapshot_json = pin_task_snapshot_json(conn, day);
    conn.execute(
        "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status, quest_version_id, policy_version_id, credited_core_seconds, observed_seconds, task_snapshot_json)
         VALUES (?1, ?2, ?3, 'Scheduled', ?4, ?5, 0, 0, ?6)
         ON CONFLICT(day, slot_start) DO NOTHING",
        params![day, slot_start_ts, scheduled, quest_vid, policy_vid, snapshot_json],
    )
    .map_err(map_rusqlite)?;
    conn.execute(
        "UPDATE slots SET
           quest_version_id = COALESCE(quest_version_id, ?3),
           policy_version_id = COALESCE(policy_version_id, ?4),
           task_snapshot_json = COALESCE(task_snapshot_json, ?5)
         WHERE day = ?1 AND slot_start = ?2",
        params![day, slot_start_ts, quest_vid, policy_vid, snapshot_json],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

fn pin_task_snapshot_json(conn: &Connection, day: &str) -> String {
    let Some(day_start) = start_of_named_day(day) else {
        return "[]".into();
    };
    let day_end = end_of_local_day(day_start);
    let lists = match load_task_lists(conn) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("load_task_lists failed while pinning snapshot: {e:?}");
            return "[]".into();
        }
    };
    let tasks = match load_tasks(conn) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("load_tasks failed while pinning snapshot: {e:?}");
            return "[]".into();
        }
    };
    let selected = match judgment_tasks(&tasks, &lists, day_start, day_end) {
        Ok(v) => v,
        Err(TaskListError::TooManyJudgment) => {
            eprintln!("too many judgment tasks for {day}; pinning empty snapshot");
            return "[]".into();
        }
        Err(e) => {
            eprintln!("judgment_tasks failed for {day}: {e:?}");
            return "[]".into();
        }
    };
    let snaps = snapshot_of(&selected, &lists);
    serde_json::to_string(&snaps).unwrap_or_else(|_| "[]".into())
}

pub fn start_of_named_day(day: &str) -> Option<i64> {
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .single()
        .map(|dt| dt.timestamp())
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
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(map_rusqlite)?
        .flatten();
    Ok(matches!(status.as_deref(), Some("final" | "unknown")))
}

pub fn add_unobserved_secs(
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
pub fn fill_heartbeat_unobserved(
    conn: &Connection,
    last_heartbeat: i64,
    now: i64,
) -> Result<(), DbOpError> {
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
                if let Ok(date) = parse_day(&day) {
                    if !is_weekday(date) {
                        t = overlap_end;
                        continue;
                    }
                }
                let rng = slot_rng(&day, ss);
                add_unobserved_secs(conn, &day, ss, secs, rng)?;
            }
            t = overlap_end;
        }
    }
    Ok(())
}

pub fn startup_from_heartbeat(
    conn: &mut Connection,
    now: i64,
    retention: ScreenshotRetention,
) -> Result<(), DbOpError> {
    migrate(conn)?;
    if let Some(last) = read_heartbeat_ts(conn)? {
        fill_heartbeat_unobserved(conn, last, now)?;
    }
    apply_capture_on_resume(conn, now)?;
    finalize_ended_open_slots(conn, now, retention)?;
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
    decide_capture(app, None, secure, locked, paused, &builtin_never_capture())
        == CaptureStatus::Skipped
}

/// At/after scheduled time: skip → Skipped; else attempt frontmost capture → Captured or Missed.
/// `capture_context` is invoked only when a capture is actually due.
pub fn tick_capture(
    conn: &Connection,
    day: &str,
    slot_start_ts: i64,
    now: i64,
    locked: bool,
    paused: bool,
    screen_recording: bool,
    capture_context: impl Fn() -> CaptureContext,
    capture_fn: impl Fn(&Path) -> Result<(), ()>,
) -> Result<(), DbOpError> {
    tick_capture_impl(
        conn,
        day,
        slot_start_ts,
        now,
        locked,
        paused,
        screen_recording,
        capture_context,
        capture_fn,
    )
}

fn tick_capture_impl(
    conn: &Connection,
    day: &str,
    slot_start_ts: i64,
    now: i64,
    locked: bool,
    paused: bool,
    screen_recording: bool,
    capture_context: impl Fn() -> CaptureContext,
    capture_fn: impl Fn(&Path) -> Result<(), ()>,
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
    if !screen_recording || locked || paused {
        conn.execute(
            "UPDATE slots SET capture_status = ?1 WHERE day = ?2 AND slot_start = ?3",
            params![
                capture_status_to_str(CaptureStatus::Skipped),
                day,
                slot_start_ts
            ],
        )
        .map_err(map_rusqlite)?;
        return Ok(());
    }

    let mut ctx = capture_context();
    ctx.document_path = ctx
        .document_path
        .as_deref()
        .and_then(gamelife_core::normalize_document_path);
    let policy = load_policy(conn)?;
    let never = merged_never_capture(&policy);
    if decide_capture(
        &ctx.app,
        ctx.bundle_id.as_deref(),
        ctx.secure_input,
        locked,
        paused,
        &never,
    ) == CaptureStatus::Skipped
    {
        conn.execute(
            "UPDATE slots SET capture_status = ?1 WHERE day = ?2 AND slot_start = ?3",
            params![
                capture_status_to_str(CaptureStatus::Skipped),
                day,
                slot_start_ts
            ],
        )
        .map_err(map_rusqlite)?;
        return Ok(());
    }

    let next = if let Some(path) = screenshot_path_for(day, slot_start_ts, now) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if capture_fn(&path).is_ok() {
            let json = serde_json::to_string(&ctx)
                .map_err(|e| DbOpError::Fatal(format!("capture_context json: {e}")))?;
            conn.execute(
                "UPDATE slots SET
                    screenshot_path = ?1,
                    captured_at = ?2,
                    capture_context_json = ?3,
                    capture_status = ?4
                 WHERE day = ?5 AND slot_start = ?6",
                params![
                    path.to_string_lossy().to_string(),
                    now,
                    json,
                    capture_status_to_str(CaptureStatus::Captured),
                    day,
                    slot_start_ts,
                ],
            )
            .map_err(map_rusqlite)?;
            return Ok(());
        }
        CaptureStatus::Missed
    } else {
        CaptureStatus::Missed
    };
    conn.execute(
        "UPDATE slots SET capture_status = ?1 WHERE day = ?2 AND slot_start = ?3",
        params![capture_status_to_str(next), day, slot_start_ts],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

fn app_meta_get(conn: &Connection, key: &str) -> Result<Option<String>, DbOpError> {
    conn.query_row(
        "SELECT value FROM app_meta WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
    .map_err(map_rusqlite)
}

fn app_meta_set(conn: &Connection, key: &str, value: &str) -> Result<(), DbOpError> {
    conn.execute(
        "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

fn user_policy_lists_empty(json: &str) -> bool {
    let Ok(parsed) = serde_json::from_str::<PolicyJson>(json) else {
        return false;
    };
    parsed.trusted_apps.as_ref().is_none_or(|v| v.is_empty())
        && parsed.reading_apps.as_ref().is_none_or(|v| v.is_empty())
        && parsed
            .distraction_rules
            .as_ref()
            .is_none_or(|v| v.is_empty())
        && parsed
            .side_project_rules
            .as_ref()
            .is_none_or(|v| v.is_empty())
}

/// Seed V0.1 policy, then D2 once. After `policy_seed_version=2`, a cleared list stays empty.
pub fn seed_default_policy_if_needed(conn: &Connection) -> Result<(), DbOpError> {
    migrate(conn)?;
    let seeded = app_meta_get(conn, "policy_seed_version")?
        .as_deref()
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    if seeded >= 2 {
        return Ok(());
    }
    if seeded == 0 {
        let latest: Option<String> = conn
            .query_row(
                "SELECT json FROM policy_versions ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(map_rusqlite)?;
        let insert_default = match latest.as_deref() {
            None => true,
            Some(json) => user_policy_lists_empty(json),
        };
        if insert_default {
            let json = serde_json::to_string(&default_v01())
                .map_err(|e| DbOpError::Fatal(format!("policy seed json: {e}")))?;
            conn.execute(
                "INSERT INTO policy_versions (json, created_at) VALUES (?1, ?2)",
                params![json, now_secs()],
            )
            .map_err(map_rusqlite)?;
        }
        app_meta_set(conn, "policy_seed_version", "2")?;
        return Ok(());
    }

    let latest: Option<String> = conn
        .query_row(
            "SELECT json FROM policy_versions ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    if let Some(json) = latest {
        let mut policy: Policy = serde_json::from_str(&json)
            .map_err(|e| DbOpError::Fatal(format!("policy json: {e}")))?;
        if policy.distraction_rules.is_empty() {
            policy.distraction_rules = default_distraction_rules();
            let new_json = serde_json::to_string(&policy)
                .map_err(|e| DbOpError::Fatal(format!("policy seed json: {e}")))?;
            conn.execute(
                "INSERT INTO policy_versions (json, created_at) VALUES (?1, ?2)",
                params![new_json, now_secs()],
            )
            .map_err(map_rusqlite)?;
        }
    }
    app_meta_set(conn, "policy_seed_version", "2")?;
    Ok(())
}

#[derive(Deserialize)]
struct PolicyJson {
    trusted_apps: Option<Vec<String>>,
    distraction_rules: Option<Vec<String>>,
    side_project_rules: Option<Vec<String>>,
    reading_apps: Option<Vec<String>>,
    never_capture_apps: Option<Vec<String>>,
    #[serde(default)]
    admin_apps: Option<Vec<String>>,
    #[serde(default)]
    category_guides: Option<CategoryGuides>,
}

pub fn load_policy(conn: &Connection) -> Result<Policy, DbOpError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT json FROM policy_versions ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    let Some(json) = json else {
        return Ok(default_v01());
    };
    let parsed: PolicyJson =
        serde_json::from_str(&json).map_err(|e| DbOpError::Fatal(format!("policy json: {e}")))?;
    Ok(Policy {
        trusted_apps: parsed.trusted_apps.unwrap_or_default(),
        distraction_rules: parsed.distraction_rules.unwrap_or_default(),
        side_project_rules: parsed
            .side_project_rules
            .unwrap_or_else(builtin_side_project_rules),
        reading_apps: parsed.reading_apps.unwrap_or_default(),
        never_capture_apps: parsed
            .never_capture_apps
            .unwrap_or_else(builtin_never_capture),
        admin_apps: parsed.admin_apps.unwrap_or_default(),
        category_guides: parsed.category_guides.unwrap_or_default(),
    })
}

fn merged_never_capture(policy: &Policy) -> Vec<String> {
    let mut out = builtin_never_capture();
    for app in &policy.never_capture_apps {
        if !out.iter().any(|b| b.eq_ignore_ascii_case(app)) {
            out.push(app.clone());
        }
    }
    out
}

fn slot_version_ids(
    conn: &Connection,
    day: &str,
    slot_start: i64,
) -> Result<(Option<i64>, Option<i64>), DbOpError> {
    Ok(conn
        .query_row(
            "SELECT quest_version_id, policy_version_id FROM slots WHERE day = ?1 AND slot_start = ?2",
            params![day, slot_start],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(map_rusqlite)?
        .unwrap_or((None, None)))
}

pub fn load_quests_for_version(
    conn: &Connection,
    day: &str,
    quest_version_id: Option<i64>,
) -> Result<Vec<Quest>, DbOpError> {
    let json: Option<String> = if let Some(id) = quest_version_id {
        conn.query_row(
            "SELECT json FROM quest_versions WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?
    } else {
        None
    };
    let json = match json {
        Some(j) => j,
        None => {
            return load_quests_for_day(conn, day);
        }
    };
    match parse_quest_versions_json(&json) {
        Ok(quests) => Ok(quests),
        Err(e) => {
            eprintln!("quest json parse failed: {e}");
            Ok(vec![])
        }
    }
}

pub fn load_policy_for_version(
    conn: &Connection,
    policy_version_id: Option<i64>,
) -> Result<Policy, DbOpError> {
    let json: Option<String> = if let Some(id) = policy_version_id {
        conn.query_row(
            "SELECT json FROM policy_versions WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?
    } else {
        None
    };
    let Some(json) = json else {
        return load_policy(conn);
    };
    let parsed: PolicyJson =
        serde_json::from_str(&json).map_err(|e| DbOpError::Fatal(format!("policy json: {e}")))?;
    Ok(Policy {
        trusted_apps: parsed.trusted_apps.unwrap_or_default(),
        distraction_rules: parsed.distraction_rules.unwrap_or_default(),
        side_project_rules: parsed
            .side_project_rules
            .unwrap_or_else(builtin_side_project_rules),
        reading_apps: parsed.reading_apps.unwrap_or_default(),
        never_capture_apps: parsed
            .never_capture_apps
            .unwrap_or_else(builtin_never_capture),
        admin_apps: parsed.admin_apps.unwrap_or_default(),
        category_guides: parsed.category_guides.unwrap_or_default(),
    })
}

pub fn load_quests_for_day(conn: &Connection, day: &str) -> Result<Vec<Quest>, DbOpError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT json FROM quest_versions WHERE day = ?1 ORDER BY id DESC LIMIT 1",
            params![day],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    let Some(json) = json else {
        return Ok(vec![]);
    };
    match parse_quest_versions_json(&json) {
        Ok(quests) => Ok(quests),
        Err(e) => {
            eprintln!("quest json parse failed: {e}");
            Ok(vec![])
        }
    }
}

pub fn save_quests_for_day(
    conn: &Connection,
    day: &str,
    drafts: Vec<QuestDraft>,
    now: i64,
) -> Result<(), DbOpError> {
    let quests = normalize_quest_list(drafts).map_err(|e| match e {
        QuestListError::TooMany => DbOpError::Fatal("at most 3 quests".into()),
        QuestListError::EmptyText => DbOpError::Fatal("quest text empty".into()),
    })?;
    let json_rows = quests
        .iter()
        .map(|q| {
            serde_json::json!({
                "text": q.text,
                "evidence": q.evidence,
                "hero": q.hero,
            })
        })
        .collect::<Vec<_>>();
    let json = serde_json::to_string(&json_rows)
        .map_err(|e| DbOpError::Fatal(e.to_string()))?;
    conn.execute(
        "INSERT INTO quest_versions (day, json, created_at) VALUES (?1, ?2, ?3)",
        params![day, json, now],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

fn previous_nonempty_quest_snapshot(
    conn: &Connection,
    today: &str,
) -> Result<Option<(String, Vec<Quest>)>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT day, json FROM quest_versions WHERE day < ?1 ORDER BY day DESC, id DESC",
        )
        .map_err(map_rusqlite)?;
    let rows = stmt
        .query_map(params![today], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(map_rusqlite)?;
    for row in rows {
        let (day, json) = row.map_err(map_rusqlite)?;
        match parse_quest_versions_json(&json) {
            Ok(quests) if !quests.is_empty() => return Ok(Some((day, quests))),
            _ => {}
        }
    }
    Ok(None)
}

pub fn previous_quest_day(conn: &Connection, today: &str) -> Result<Option<String>, DbOpError> {
    Ok(previous_nonempty_quest_snapshot(conn, today)?.map(|(day, _)| day))
}

pub fn continue_previous_workday_for_day(
    conn: &Connection,
    today: &str,
    now: i64,
) -> Result<String, DbOpError> {
    let (from_day, quests) = previous_nonempty_quest_snapshot(conn, today)?
        .ok_or_else(|| DbOpError::Fatal("no previous quests".into()))?;
    let drafts = quests
        .into_iter()
        .map(|q| QuestDraft {
            text: q.text,
            evidence: q.evidence,
            hero: q.hero,
        })
        .collect();
    save_quests_for_day(conn, today, drafts, now)?;
    Ok(from_day)
}

fn credited_before_slot(conn: &Connection, day: &str, slot_start: i64) -> Result<i64, DbOpError> {
    conn.query_row(
        "SELECT COALESCE(SUM(credited_core_seconds), 0) FROM slots
         WHERE day = ?1 AND slot_start < ?2 AND status = 'final'",
        params![day, slot_start],
        |r| r.get(0),
    )
    .map_err(map_rusqlite)
}

fn load_samples_for_slot(
    conn: &Connection,
    day: &str,
    slot_start: i64,
    slot_end: i64,
) -> Result<Vec<Sample>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT ts, app, title, url, document_path, bundle_id, idle_seconds, locked, paused, secure_input
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
                document_path: r.get(4)?,
                bundle_id: r.get(5)?,
                idle_seconds: r.get(6)?,
                screen_locked: r.get::<_, i64>(7)? != 0,
                paused: r.get::<_, i64>(8)? != 0,
                secure_input: r.get::<_, i64>(9)? != 0,
            })
        })
        .map_err(map_rusqlite)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(map_rusqlite)
}

fn load_slot_task_snapshots(
    conn: &Connection,
    day: &str,
    slot_start: i64,
) -> Result<Vec<TaskSnapshot>, DbOpError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT task_snapshot_json FROM slots WHERE day = ?1 AND slot_start = ?2",
            params![day, slot_start],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(map_rusqlite)?
        .flatten();
    match json.as_deref() {
        Some(raw) if !raw.trim().is_empty() => match parse_task_snapshot_json(raw) {
            Ok(tasks) => Ok(tasks),
            Err(e) => {
                eprintln!("task_snapshot_json parse failed for {day}@{slot_start}: {e}");
                Ok(vec![])
            }
        },
        _ => Ok(vec![]),
    }
}

fn hints_for_slot(samples: &[Sample], policy: &Policy, quests: &[Quest]) -> Vec<Hint> {
    let mut last_core_interaction_ts: Option<i64> = None;
    let mut hints = Vec::with_capacity(samples.len());
    for sample in samples {
        let hint = hint_sample(sample, policy, quests, last_core_interaction_ts);
        if sample.idle_seconds < LOW_INPUT_IDLE_SECS && hint == Hint::CoreCandidate {
            last_core_interaction_ts = Some(sample.ts);
        }
        hints.push(hint);
    }
    hints
}

fn credited_spans_for_final_slot(
    conn: &Connection,
    day: &str,
    slot_start: i64,
    slot_end: i64,
    credited_limit: i64,
    include_verified_unsure: bool,
) -> Result<Vec<(i64, i64)>, DbOpError> {
    if credited_limit <= 0 {
        return Ok(vec![]);
    }
    let (quest_vid, policy_vid) = slot_version_ids(conn, day, slot_start)?;
    let policy = load_policy_for_version(conn, policy_vid)?;
    let quests = load_quests_for_version(conn, day, quest_vid)?;
    let samples = load_samples_for_slot(conn, day, slot_start, slot_end)?;
    let hints = hints_for_slot(&samples, &policy, &quests);
    let spans = spans_for_slot(&samples, &hints, slot_start, slot_end);
    Ok(credited_core_spans(
        &samples,
        &spans,
        credited_limit,
        include_verified_unsure,
    ))
}

/// Compute early-start coins when `credited_before + current_credited` first crosses 900.
pub fn compute_early_coins(
    conn: &Connection,
    day: &str,
    through_slot_start: i64,
    through_slot_end: i64,
    current_credited: i64,
    current_include_verified: bool,
) -> Result<i64, DbOpError> {
    let credited_before = credited_before_slot(conn, day, through_slot_start)?;
    let after = credited_before + current_credited;
    if credited_before >= 900 || after < 900 {
        return Ok(0);
    }

    let mut all_spans: Vec<(i64, i64)> = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT slot_start, credited_core_seconds FROM slots
             WHERE day = ?1 AND slot_start < ?2 AND status = 'final'
             ORDER BY slot_start",
        )
        .map_err(map_rusqlite)?;
    let rows = stmt
        .query_map(params![day, through_slot_start], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(map_rusqlite)?;
    for row in rows {
        let (ss, credited) = row.map_err(map_rusqlite)?;
        let se = slot_end_exclusive(ss);
        all_spans.extend(credited_spans_for_final_slot(
            conn, day, ss, se, credited, false,
        )?);
    }

    let (quest_vid, policy_vid) = slot_version_ids(conn, day, through_slot_start)?;
    let policy = load_policy_for_version(conn, policy_vid)?;
    let quests = load_quests_for_version(conn, day, quest_vid)?;
    let samples = load_samples_for_slot(conn, day, through_slot_start, through_slot_end)?;
    let hints = hints_for_slot(&samples, &policy, &quests);
    let spans = spans_for_slot(&samples, &hints, through_slot_start, through_slot_end);
    all_spans.extend(credited_core_spans(
        &samples,
        &spans,
        current_credited,
        current_include_verified,
    ));

    let Some(anchor) = early_start_anchor(&all_spans) else {
        return Ok(0);
    };
    let local_secs = anchor - start_of_local_day(anchor);
    Ok(early_start_coins_for_local_secs(local_secs))
}

pub fn finalize_ended_open_slots(
    conn: &mut Connection,
    now: i64,
    retention: ScreenshotRetention,
) -> Result<(), DbOpError> {
    let rows: Vec<(String, i64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT day, slot_start FROM slots
             WHERE (status IS NULL OR status NOT IN ('final', 'unknown'))
               AND slot_start + 900 <= ?1
             ORDER BY day, slot_start",
            )
            .map_err(map_rusqlite)?;
        let mapped = stmt
            .query_map(params![now], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(map_rusqlite)?;
        mapped.filter_map(|r| r.ok()).collect()
    };
    for (day, slot_start) in rows {
        if slot_is_final(conn, &day, slot_start)? {
            continue;
        }
        let slot_end = slot_end_exclusive(slot_start);
        if slot_end > now {
            continue;
        }
        let credited_before = credited_before_slot(conn, &day, slot_start)?;
        finalize_slot_end(conn, &day, slot_start, slot_end, retention, credited_before)?;
    }
    Ok(())
}

fn spawn_async_finalize(
    day: String,
    slot_start: i64,
    slot_end: i64,
    retention: ScreenshotRetention,
) {
    let Some(db_path) = app_db_path() else {
        return;
    };
    thread::spawn(move || {
        let mut conn = match open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("async finalize: open db failed: {e:?}");
                return;
            }
        };
        let credited_before = credited_before_slot(&conn, &day, slot_start).unwrap_or(0);
        if let Err(e) = finalize_slot_end(
            &mut conn,
            &day,
            slot_start,
            slot_end,
            retention,
            credited_before,
        ) {
            eprintln!("async finalize failed: {e:?}");
        }
    });
}

/// Finalize the previous slot when the sampler crosses a slot (or day) boundary.
pub fn maybe_finalize_previous_slot(
    conn: &mut Connection,
    prev_day: Option<&str>,
    prev_slot: Option<i64>,
    current_day: &str,
    current_slot: i64,
    retention: ScreenshotRetention,
) -> Result<(), DbOpError> {
    let (Some(prev_day), Some(prev_slot)) = (prev_day, prev_slot) else {
        return Ok(());
    };
    if prev_day == current_day && prev_slot == current_slot {
        return Ok(());
    }
    if slot_is_final(conn, prev_day, prev_slot)? {
        return Ok(());
    }
    let slot_end = if prev_day == current_day {
        current_slot
    } else {
        slot_end_exclusive(prev_slot)
    };
    spawn_async_finalize(prev_day.to_string(), prev_slot, slot_end, retention);
    Ok(())
}

/// Slot end: strong metadata → no upload; gray zone + Captured → vision HTTP; then judge + resolve.
pub fn finalize_slot_end(
    conn: &mut Connection,
    day: &str,
    slot_start: i64,
    slot_end: i64,
    retention: ScreenshotRetention,
    credited_before: i64,
) -> Result<(), DbOpError> {
    if slot_is_final(conn, day, slot_start)? {
        return Ok(());
    }

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
    let (quest_vid, policy_vid) = slot_version_ids(conn, day, slot_start)?;
    let policy = load_policy_for_version(conn, policy_vid)?;
    let quests = load_quests_for_version(conn, day, quest_vid)?;
    let tasks = load_slot_task_snapshots(conn, day, slot_start)?;
    let actual = slot_end - slot_start;
    let evidence = analyze_slot_evidence(&samples, &policy, &quests, &tasks, slot_start, slot_end);
    let decidable = metadata_decidable(
        &evidence.activity,
        evidence.strong_core_seconds,
        evidence.grounded_strong_core_seconds,
        evidence.reading_bridge_seconds,
        actual,
        !quest_list_has_evidence(&quests),
    );

    let (screenshot_path, capture_json): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT screenshot_path, capture_context_json FROM slots WHERE day = ?1 AND slot_start = ?2",
            params![day, slot_start],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(map_rusqlite)?
        .unwrap_or((None, None));

    let capture_ctx = match capture_json.as_deref() {
        Some(json) => match serde_json::from_str::<CaptureContext>(json) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("capture_context_json parse failed for {day}@{slot_start}: {e}");
                None
            }
        },
        None => None,
    };

    let never = merged_never_capture(&policy);
    let settings = crate::config::load_settings();
    let (primary, fallback) = vision::endpoints_from_settings(&settings, |id| {
        crate::keychain::get_provider_api_key(id).ok()
    });

    let mut output = judge_slot(JudgeInput {
        slot_start,
        slot_end,
        samples: &samples,
        quests: &quests,
        tasks: &tasks,
        policy: &policy,
        capture,
        vision: None,
        manual_core: None,
    });

    let lines: Vec<SampleLine> = samples
        .iter()
        .map(|s| SampleLine {
            app: s.app.clone(),
            title: s.window_title.clone(),
            url: s.url.clone(),
            document_path: s.document_path.clone(),
            idle_seconds: s.idle_seconds,
            protected: s.secure_input
                || matches_app_identity(&s.app, s.bundle_id.as_deref(), &never),
        })
        .collect();
    let summary = sample_summary_lines(&lines);
    let gray = output.pending
        || matches!(
            output.dominant,
            Dominant::PendingReview | Dominant::Unknown
        );
    let mut matched_text = false;
    if gray && !tasks.is_empty() && !summary.is_empty() {
        if let Some(ep) = primary.as_ref().or(fallback.as_ref()) {
            if let Ok(raw) = call_text_task_match(ep, &tasks, &summary) {
                if let Ok(Some(m)) = parse_task_match_json(&raw, &tasks) {
                    if m.confidence >= TASK_MATCH_MIN {
                        output = apply_task_match(output, &evidence, &m);
                        matched_text = true;
                    }
                }
            }
        }
    }

    if !matched_text && gray {
        let vision = match (screenshot_path.as_ref(), capture_ctx) {
            (Some(path), Some(capture_ctx)) => {
                let ctx = VisionContext {
                    slot_start,
                    slot_end,
                    quests: quests.iter().map(vision_quest_label).collect(),
                    capture: capture_ctx,
                    activity_summary: activity_summary_for_vision(&evidence, &samples),
                };
                maybe_vision_for_gray_zone(
                    decidable,
                    capture,
                    Path::new(path),
                    ctx,
                    &never,
                    primary.as_ref(),
                    fallback.as_ref(),
                )
            }
            _ => None,
        };
        if vision.is_some() {
            output = judge_slot(JudgeInput {
                slot_start,
                slot_end,
                samples: &samples,
                quests: &quests,
                tasks: &tasks,
                policy: &policy,
                capture,
                vision,
                manual_core: None,
            });
        }
    }

    let include_verified = output.used_vision || output.credited_core_seconds > 0;
    let early_coins = compute_early_coins(
        conn,
        day,
        slot_start,
        slot_end,
        output.credited_core_seconds,
        include_verified,
    )
    .unwrap_or(0);

    resolve_slot(conn, day, slot_start, &output, credited_before, early_coins)?;

    let status = slot_status(conn, day, slot_start)?.unwrap_or_default();
    apply_capture_retention(conn, day, slot_start, retention, &status)?;
    purge_expired_screenshots(conn, retention, now_secs())?;
    Ok(())
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn start_of_local_day(ts: i64) -> i64 {
    let dt = Local.timestamp_opt(ts, 0).single().unwrap();
    let date = dt.date_naive();
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .unwrap()
        .timestamp()
}

pub fn yesterday_str_for_ts(now: i64) -> String {
    let today_start = start_of_local_day(now);
    day_str_for_ts(today_start - 1)
}

fn parse_day(day: &str) -> Result<NaiveDate, DbOpError> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map_err(|e| DbOpError::Fatal(format!("day parse: {e}")))
}

fn next_weekday(date: NaiveDate) -> NaiveDate {
    let mut d = date.succ_opt().expect("date");
    while !is_weekday(d) {
        d = d.succ_opt().expect("date");
    }
    d
}

fn day_is_settled(conn: &Connection, day: &str) -> Result<bool, DbOpError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM days WHERE day = ?1 AND settled_at IS NOT NULL",
            params![day],
            |r| r.get(0),
        )
        .map_err(map_rusqlite)?;
    Ok(count > 0)
}

fn day_total_credited(conn: &Connection, day: &str) -> Result<i64, DbOpError> {
    conn.query_row(
        "SELECT COALESCE(SUM(credited_core_seconds), 0) FROM slots
         WHERE day = ?1 AND status = 'final'",
        params![day],
        |r| r.get(0),
    )
    .map_err(map_rusqlite)
}

fn outcome_from_str(s: &str) -> Option<DayOutcome> {
    match s {
        "completed" => Some(DayOutcome::Completed),
        "protected" => Some(DayOutcome::Protected),
        "failed" => Some(DayOutcome::Failed),
        _ => None,
    }
}

fn load_settled_days_newest_first(
    conn: &Connection,
) -> Result<Vec<(NaiveDate, DayOutcome)>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT day, outcome FROM days
             WHERE settled_at IS NOT NULL AND outcome IS NOT NULL
             ORDER BY day DESC",
        )
        .map_err(map_rusqlite)?;
    let rows = stmt
        .query_map([], |r| {
            let day: String = r.get(0)?;
            let outcome: String = r.get(1)?;
            Ok((day, outcome))
        })
        .map_err(map_rusqlite)?;
    let mut days = Vec::new();
    for row in rows {
        let (day, outcome) = row.map_err(map_rusqlite)?;
        let date = parse_day(&day)?;
        let Some(outcome) = outcome_from_str(&outcome) else {
            continue;
        };
        days.push((date, outcome));
    }
    Ok(days)
}

pub fn streak_from_db(conn: &Connection) -> Result<u32, DbOpError> {
    Ok(recompute_streak(&load_settled_days_newest_first(conn)?))
}

fn apply_streak_milestones(
    conn: &Connection,
    trigger_day: &str,
    old_streak: u32,
    new_streak: u32,
) -> Result<(), DbOpError> {
    for milestone in new_milestones(old_streak, new_streak) {
        let key = format!("streak_milestone:{milestone}");
        if let Err(e) = insert_ledger(conn, &key, trigger_day, 0, 0) {
            if e != DbOpError::AlreadyApplied {
                return Err(e);
            }
        }
    }
    Ok(())
}

fn convert_pending_to_unknown(conn: &Connection, day: &str) -> Result<(), DbOpError> {
    conn.execute(
        "UPDATE slots SET status = 'unknown', category = 'unknown'
         WHERE day = ?1 AND status = 'pending_review'",
        params![day],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

fn slot_status(
    conn: &Connection,
    day: &str,
    slot_start_ts: i64,
) -> Result<Option<String>, DbOpError> {
    Ok(conn
        .query_row(
            "SELECT status FROM slots WHERE day = ?1 AND slot_start = ?2",
            params![day, slot_start_ts],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(map_rusqlite)?
        .flatten())
}

fn finalize_yesterday_last_slot(
    conn: &mut Connection,
    yesterday: &str,
    day_end: i64,
    retention: ScreenshotRetention,
) -> Result<(), DbOpError> {
    let last_ss = slot_start(day_end - 1);
    if slot_is_final(conn, yesterday, last_ss)? {
        return Ok(());
    }
    if slot_status(conn, yesterday, last_ss)? == Some("pending_review".into()) {
        return Ok(());
    }
    let credited_before = credited_before_slot(conn, yesterday, last_ss)?;
    finalize_slot_end(
        conn,
        yesterday,
        last_ss,
        day_end,
        retention,
        credited_before,
    )
}

/// Settle a calendar day: pending→unknown, compute outcome, idempotent.
pub fn settle_day(
    conn: &mut Connection,
    day: &str,
    settled_at: i64,
    retention: ScreenshotRetention,
) -> Result<(), DbOpError> {
    migrate(conn)?;
    if day_is_settled(conn, day)? {
        return Ok(());
    }
    let old_streak = streak_from_db(conn)?;
    convert_pending_to_unknown(conn, day)?;
    let mut stmt = conn
        .prepare(
            "SELECT slot_start FROM slots
             WHERE day = ?1 AND status = 'unknown'
               AND (screenshot_path IS NOT NULL OR capture_context_json IS NOT NULL)",
        )
        .map_err(map_rusqlite)?;
    let starts: Vec<i64> = stmt
        .query_map(params![day], |r| r.get(0))
        .map_err(map_rusqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_rusqlite)?;
    drop(stmt);
    for ss in starts {
        apply_capture_retention(conn, day, ss, retention, "unknown")?;
    }
    let credited = day_total_credited(conn, day)?;
    let outcome = if credited >= i64::try_from(CHEST_SECS).unwrap_or(i64::MAX) {
        "completed"
    } else {
        "failed"
    };
    let existing: Option<String> = conn
        .query_row(
            "SELECT outcome FROM days WHERE day = ?1",
            params![day],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    let final_outcome = if existing.as_deref() == Some("protected") {
        "protected"
    } else {
        outcome
    };
    conn.execute(
        "INSERT INTO days (day, settled_at, outcome) VALUES (?1, ?2, ?3)
         ON CONFLICT(day) DO UPDATE SET settled_at = excluded.settled_at, outcome = excluded.outcome
         WHERE days.settled_at IS NULL",
        params![day, settled_at, final_outcome],
    )
    .map_err(map_rusqlite)?;
    let new_streak = streak_from_db(conn)?;
    apply_streak_milestones(conn, day, old_streak, new_streak)?;
    Ok(())
}

/// At local 00:00:00 resolve yesterday's last slot; at 00:00:05+ settle yesterday.
pub fn midnight_tick(
    conn: &mut Connection,
    now: i64,
    retention: ScreenshotRetention,
) -> Result<(), DbOpError> {
    migrate(conn)?;
    let today_start = start_of_local_day(now);
    let yesterday = yesterday_str_for_ts(now);
    finalize_yesterday_last_slot(conn, &yesterday, today_start, retention)?;
    if now >= today_start + 5 {
        settle_day(conn, &yesterday, now, retention)?;
    }
    Ok(())
}

fn load_freeze_dates(conn: &Connection) -> Result<Vec<NaiveDate>, DbOpError> {
    let mut stmt = conn
        .prepare("SELECT protected_date FROM freeze_uses ORDER BY protected_date")
        .map_err(map_rusqlite)?;
    let rows = stmt
        .query_map([], |r| {
            let s: String = r.get(0)?;
            NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })
        .map_err(map_rusqlite)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(map_rusqlite)
}

fn next_weekday_settled(conn: &Connection, after: NaiveDate) -> Result<bool, DbOpError> {
    let next = next_weekday(after);
    let day = next.format("%Y-%m-%d").to_string();
    let settled: Option<i64> = conn
        .query_row(
            "SELECT settled_at FROM days WHERE day = ?1",
            params![day],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    Ok(settled.is_some())
}

/// Whether a settled failed day is still inside the freeze window with quota remaining.
pub fn freeze_eligible(conn: &Connection, protected_date: &str) -> Result<bool, DbOpError> {
    migrate(conn)?;
    let protected = parse_day(protected_date)?;
    if !day_is_settled(conn, protected_date)? {
        return Ok(false);
    }
    let outcome: Option<String> = conn
        .query_row(
            "SELECT outcome FROM days WHERE day = ?1",
            params![protected_date],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    if outcome.as_deref() != Some("failed") {
        return Ok(false);
    }
    if next_weekday_settled(conn, protected)? {
        return Ok(false);
    }
    let used = load_freeze_dates(conn)?;
    Ok(can_use_freeze(&used, protected))
}

/// Newest-first failed settled days that can still be frozen.
pub fn list_freeze_candidates(conn: &Connection) -> Result<Vec<String>, DbOpError> {
    migrate(conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT day FROM days
             WHERE settled_at IS NOT NULL AND outcome = 'failed'
             ORDER BY day DESC",
        )
        .map_err(map_rusqlite)?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(map_rusqlite)?;
    let mut out = Vec::new();
    for row in rows {
        let day = row.map_err(map_rusqlite)?;
        if freeze_eligible(conn, &day)? {
            out.push(day);
        }
    }
    Ok(out)
}

/// Freeze a failed settled day; quota by protected_date's calendar month.
pub fn freeze_day(conn: &mut Connection, protected_date: &str, _now: i64) -> Result<(), DbOpError> {
    migrate(conn)?;
    let protected = parse_day(protected_date)?;
    if !day_is_settled(conn, protected_date)? {
        return Err(DbOpError::Fatal("day not settled".into()));
    }
    let outcome: Option<String> = conn
        .query_row(
            "SELECT outcome FROM days WHERE day = ?1",
            params![protected_date],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    match outcome.as_deref() {
        Some("failed") => {}
        Some("protected") => return Ok(()),
        _ => return Err(DbOpError::Fatal("cannot freeze".into())),
    }
    if next_weekday_settled(conn, protected)? {
        return Err(DbOpError::Fatal("freeze window closed".into()));
    }
    let used = load_freeze_dates(conn)?;
    if !can_use_freeze(&used, protected) {
        return Err(DbOpError::Fatal("freeze quota".into()));
    }
    let old_streak = streak_from_db(conn)?;
    let tx = conn.transaction().map_err(map_rusqlite)?;
    tx.execute(
        "INSERT INTO freeze_uses (protected_date) VALUES (?1)",
        params![protected_date],
    )
    .map_err(map_rusqlite)?;
    tx.execute(
        "UPDATE days SET outcome = 'protected' WHERE day = ?1",
        params![protected_date],
    )
    .map_err(map_rusqlite)?;
    tx.commit().map_err(map_rusqlite)?;
    let new_streak = streak_from_db(conn)?;
    apply_streak_milestones(conn, protected_date, old_streak, new_streak)?;
    Ok(())
}

fn mark_scheduled_capture_missed(
    conn: &Connection,
    day: &str,
    slot_start_ts: i64,
) -> Result<(), DbOpError> {
    conn.execute(
        "UPDATE slots SET capture_status = 'Missed'
         WHERE day = ?1 AND slot_start = ?2 AND capture_status = 'Scheduled'",
        params![day, slot_start_ts],
    )
    .map_err(map_rusqlite)?;
    Ok(())
}

/// End today: half slot, missed capture, resolve, stop sampling via settle.
pub fn end_today(
    conn: &mut Connection,
    now: i64,
    retention: ScreenshotRetention,
) -> Result<(), DbOpError> {
    migrate(conn)?;
    let day = day_str_for_ts(now);
    if day_is_settled(conn, &day)? {
        return Ok(());
    }
    let ss = gamelife_core::slot_start(now);
    mark_scheduled_capture_missed(conn, &day, ss)?;
    let credited_before = credited_before_slot(conn, &day, ss)?;
    finalize_slot_end(conn, &day, ss, now, retention, credited_before)?;
    settle_day(conn, &day, now, retention)
}

pub fn sampling_allowed(conn: &Connection, day: &str) -> Result<bool, DbOpError> {
    if day_is_settled(conn, day)? {
        return Ok(false);
    }
    let date = parse_day(day)?;
    Ok(is_weekday(date))
}

fn category_to_dominant(category: &str) -> Dominant {
    match category {
        "core_research" => Dominant::CoreResearch,
        "research_support" => Dominant::ResearchSupport,
        "admin" => Dominant::Admin,
        "side_project" => Dominant::SideProject,
        "distraction" => Dominant::Distraction,
        "break_away" => Dominant::BreakAway,
        _ => Dominant::Unknown,
    }
}

/// Human review of a pending slot; economically final slots stay final via resolve_slot guard.
pub fn review_pending_slot(
    conn: &mut Connection,
    day: &str,
    slot_start: i64,
    category: &str,
    retention: ScreenshotRetention,
) -> Result<(), DbOpError> {
    migrate(conn)?;
    let status = slot_status(conn, day, slot_start)?;
    if status.as_deref() != Some("pending_review") {
        return Err(DbOpError::Fatal("slot not pending".into()));
    }
    let slot_end = slot_end_exclusive(slot_start);
    let samples = load_samples_for_slot(conn, day, slot_start, slot_end)?;
    let (quest_vid, policy_vid) = slot_version_ids(conn, day, slot_start)?;
    let policy = load_policy_for_version(conn, policy_vid)?;
    let quests = load_quests_for_version(conn, day, quest_vid)?;
    let tasks = load_slot_task_snapshots(conn, day, slot_start)?;
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
    let credited_before = credited_before_slot(conn, day, slot_start)?;
    let output = if category == "core_research" {
        let mut judged = judge_slot(JudgeInput {
            slot_start,
            slot_end,
            samples: &samples,
            quests: &quests,
            tasks: &tasks,
            policy: &policy,
            capture,
            vision: None,
            manual_core: Some(true),
        });
        judged.pending = false;
        judged
    } else {
        let judged = judge_slot(JudgeInput {
            slot_start,
            slot_end,
            samples: &samples,
            quests: &quests,
            tasks: &tasks,
            policy: &policy,
            capture,
            vision: None,
            manual_core: None,
        });
        let mut activity = judged.activity;
        match category {
            "research_support" => {
                activity.support = judged.observed_seconds;
            }
            "admin" => {
                activity.admin = judged.observed_seconds;
            }
            _ => {}
        }
        JudgeOutput {
            dominant: category_to_dominant(category),
            activity,
            credited_core_seconds: 0,
            credited_side_seconds: 0,
            credited_chore_seconds: 0,
            observed_seconds: judged.observed_seconds,
            used_vision: false,
            pending: false,
        }
    };
    let early_coins = compute_early_coins(
        conn,
        day,
        slot_start,
        slot_end,
        output.credited_core_seconds,
        output.used_vision,
    )
    .unwrap_or(0);
    resolve_slot(conn, day, slot_start, &output, credited_before, early_coins)?;
    let status = slot_status(conn, day, slot_start)?.unwrap_or_default();
    apply_capture_retention(conn, day, slot_start, retention, &status)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use gamelife_core::{builtin_never_capture, CategoryGuides};
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
            decide_capture("1Password", None, false, false, false, &never),
            CaptureStatus::Skipped
        );
    }

    #[test]
    fn decide_capture_scheduled_for_normal_app() {
        let never = builtin_never_capture();
        assert_eq!(
            decide_capture("Cursor", None, false, false, false, &never),
            CaptureStatus::Scheduled
        );
    }

    #[test]
    fn decide_capture_skips_when_paused() {
        let never = builtin_never_capture();
        assert_eq!(
            decide_capture("Cursor", None, false, false, true, &never),
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
        assert!(metadata_decidable(&activity, 800, 800, 0, 900, false));
    }

    #[test]
    fn metadata_not_decidable_gray_zone() {
        let activity = ActivitySeconds {
            side: 100,
            distraction: 50,
            ..Default::default()
        };
        assert!(!metadata_decidable(&activity, 200, 200, 0, 900, false));
    }

    #[test]
    fn metadata_not_decidable_for_title_only_strong_core() {
        let activity = ActivitySeconds {
            core: 800,
            side: 0,
            distraction: 30,
            ..Default::default()
        };
        assert!(!metadata_decidable(&activity, 800, 0, 0, 900, false));
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

    fn capture_ctx(app: &str) -> CaptureContext {
        CaptureContext {
            app: app.into(),
            bundle_id: None,
            title: String::new(),
            document_path: None,
            url: None,
            secure_input: false,
        }
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
        tick_capture(
            &conn,
            day,
            ss,
            200,
            false,
            false,
            true,
            || capture_ctx("Cursor"),
            |_| Err(()),
        )
        .unwrap();
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
        let calls = std::cell::Cell::new(0u32);
        tick_capture(
            &conn,
            day,
            ss,
            100,
            false,
            true,
            true,
            || {
                calls.set(calls.get() + 1);
                capture_ctx("Cursor")
            },
            |_| Err(()),
        )
        .unwrap();
        let status: String = conn
            .query_row(
                "SELECT capture_status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "Skipped");
        assert_eq!(calls.get(), 0);
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
        tick_capture(
            &conn,
            day,
            ss,
            100,
            false,
            false,
            true,
            || capture_ctx("1Password"),
            |_| Err(()),
        )
        .unwrap();
        let status: String = conn
            .query_row(
                "SELECT capture_status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "Skipped");
        let path: Option<String> = conn
            .query_row(
                "SELECT screenshot_path FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        let json: Option<String> = conn
            .query_row(
                "SELECT capture_context_json FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert!(path.is_none());
        assert!(json.is_none());
    }

    #[test]
    fn tick_capture_err_is_missed_without_json() {
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
        tick_capture_impl(
            &conn,
            day,
            ss,
            100,
            false,
            false,
            true,
            || capture_ctx("Cursor"),
            |_| Err(()),
        )
        .unwrap();
        let status: String = conn
            .query_row(
                "SELECT capture_status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "Missed");
        let path: Option<String> = conn
            .query_row(
                "SELECT screenshot_path FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        let json: Option<String> = conn
            .query_row(
                "SELECT capture_context_json FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert!(path.is_none());
        assert!(json.is_none());
    }

    #[test]
    fn tick_capture_marks_captured_when_capture_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        unsafe {
            std::env::set_var("HOME", &home);
        }

        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        let sample_ts = 90i64;
        let capture_ts = 100i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status)
             VALUES (?1, ?2, 100, 'Scheduled')",
            params![day, ss],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO samples (ts, day, app, title, document_path, idle_seconds, locked, paused)
             VALUES (?1, ?2, 'Cursor', 'train.py — HDP', '/Users/me/HDP/train.py', 0, 0, 0)",
            params![sample_ts, day],
        )
        .unwrap();
        tick_capture_impl(
            &conn,
            day,
            ss,
            capture_ts,
            false,
            false,
            true,
            || CaptureContext {
                app: "WeChat".into(),
                bundle_id: None,
                title: "chat".into(),
                document_path: None,
                url: None,
                secure_input: false,
            },
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
        let shot: String = conn
            .query_row(
                "SELECT screenshot_path FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert!(shot.contains(".jpg"));
        let json: String = conn
            .query_row(
                "SELECT capture_context_json FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        let ctx: CaptureContext = serde_json::from_str(&json).unwrap();
        assert_eq!(ctx.app, "WeChat");
        let doc: String = conn
            .query_row(
                "SELECT document_path FROM samples WHERE ts = ?1",
                [sample_ts],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(doc, "/Users/me/HDP/train.py");
        let sample_path: Option<String> = conn
            .query_row("SELECT path FROM samples WHERE ts = ?1", [sample_ts], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(sample_path.is_none());
        let extra_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
            .unwrap();
        assert_eq!(extra_rows, 1);
    }

    #[test]
    fn tick_capture_before_schedule_does_not_call_capture_context() {
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
        let calls = std::cell::Cell::new(0u32);
        tick_capture(
            &conn,
            day,
            ss,
            50,
            false,
            false,
            true,
            || {
                calls.set(calls.get() + 1);
                capture_ctx("Cursor")
            },
            |_| Err(()),
        )
        .unwrap();
        assert_eq!(calls.get(), 0);
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
    fn finalize_does_not_treat_sample_path_as_screenshot() {
        let dir = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("HOME", dir.path());
        }

        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        let shot = screenshots_dir().unwrap().join("test.jpg");
        std::fs::create_dir_all(shot.parent().unwrap()).unwrap();
        std::fs::write(&shot, b"x").unwrap();

        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status)
             VALUES (?1, ?2, 50, 'Captured')",
            params![day, ss],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO samples (ts, day, app, title, path, idle_seconds, locked, paused)
             VALUES (60, ?1, 'Isaac Sim', 'robot', ?2, 2, 0, 0)",
            params![day, shot.to_string_lossy().to_string()],
        )
        .unwrap();
        for i in 1..30 {
            conn.execute(
                "INSERT INTO samples (ts, day, app, title, idle_seconds, locked, paused)
                 VALUES (?1, ?2, 'Isaac Sim', 'robot', 2, 0, 0)",
                params![60 + i * 15, day],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at)
             VALUES (?1, '[{\"text\":\"robot\",\"keywords\":[\"robot\"]}]', 1)",
            params![day],
        )
        .unwrap();

        finalize_slot_end(&mut conn, day, ss, 900, ScreenshotRetention::None, 0).unwrap();

        let status: String = conn
            .query_row(
                "SELECT status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "pending_review");
        assert!(
            shot.exists(),
            "samples.path must not be used as screenshot evidence or cleanup target"
        );
    }

    fn isaac_capture_json() -> String {
        serde_json::to_string(&CaptureContext {
            app: "Isaac Sim".into(),
            bundle_id: None,
            title: "robot".into(),
            document_path: None,
            url: None,
            secure_input: false,
        })
        .unwrap()
    }

    fn mainline_snapshot_json() -> &'static str {
        r#"[{"id":"t1","title":"paper","role":"mainline"}]"#
    }

    fn pin_mainline_snapshot(conn: &Connection, day: &str, slot_start: i64) {
        conn.execute(
            "UPDATE slots SET task_snapshot_json = ?1 WHERE day = ?2 AND slot_start = ?3",
            params![mainline_snapshot_json(), day, slot_start],
        )
        .unwrap();
    }

    #[test]
    fn finalize_isaac_screenshot_without_api_key_is_pending() {
        let dir = tempfile::tempdir().unwrap();
        let shot = dir.path().join("isaac.jpg");
        std::fs::write(&shot, b"x").unwrap();

        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status, screenshot_path, capture_context_json)
             VALUES (?1, ?2, 50, 'Captured', ?3, ?4)",
            params![day, ss, shot.to_string_lossy().to_string(), isaac_capture_json()],
        )
        .unwrap();
        for i in 0..30 {
            conn.execute(
                "INSERT INTO samples (ts, day, app, title, idle_seconds, locked, paused)
                 VALUES (?1, ?2, 'Isaac Sim', 'robot', 2, 0, 0)",
                params![i * 15, day],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at)
             VALUES (?1, '[{\"text\":\"robot\",\"keywords\":[\"robot\"]}]', 1)",
            params![day],
        )
        .unwrap();

        finalize_slot_end(&mut conn, day, ss, 900, ScreenshotRetention::Hours24, 0).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "pending_review");
        let jpg_in_samples: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM samples WHERE path IS NOT NULL AND path LIKE '%.jpg'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(jpg_in_samples, 0);
    }

    #[test]
    fn finalize_corrupt_capture_json_is_pending_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let shot = dir.path().join("slot.jpg");
        std::fs::write(&shot, b"x").unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status, screenshot_path, capture_context_json)
             VALUES (?1, ?2, 50, 'Captured', ?3, '{not json')",
            params![day, ss, shot.to_string_lossy().to_string()],
        )
        .unwrap();
        for i in 0..30 {
            conn.execute(
                "INSERT INTO samples (ts, day, app, title, idle_seconds, locked, paused)
                 VALUES (?1, ?2, 'Isaac Sim', 'robot', 2, 0, 0)",
                params![i * 15, day],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at)
             VALUES (?1, '[{\"text\":\"robot\",\"keywords\":[\"robot\"]}]', 1)",
            params![day],
        )
        .unwrap();
        finalize_slot_end(&mut conn, day, ss, 900, ScreenshotRetention::Hours24, 0).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "pending_review");
    }

    #[test]
    fn finalize_without_capture_context_does_not_invent_app() {
        let dir = tempfile::tempdir().unwrap();
        let shot = dir.path().join("cursor.jpg");
        std::fs::write(&shot, b"x").unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status, screenshot_path)
             VALUES (?1, ?2, 50, 'Captured', ?3)",
            params![day, ss, shot.to_string_lossy().to_string()],
        )
        .unwrap();
        for i in 0..30 {
            conn.execute(
                "INSERT INTO samples (ts, day, app, title, idle_seconds, locked, paused)
                 VALUES (?1, ?2, 'Cursor', 'lib.rs', 2, 0, 0)",
                params![i * 15, day],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at)
             VALUES (?1, '[{\"text\":\"paper\",\"keywords\":[\"lib.rs\"]}]', 1)",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO policy_versions (json, created_at)
             VALUES ('{\"trusted_apps\":[\"Cursor\"],\"distraction_rules\":[],\"side_project_rules\":[],\"reading_apps\":[],\"never_capture_apps\":[]}', 1)",
            [],
        )
        .unwrap();
        finalize_slot_end(&mut conn, day, ss, 900, ScreenshotRetention::Hours24, 0).unwrap();
        let used: i64 = conn
            .query_row(
                "SELECT used_vision FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(used, 0);
    }

    #[test]
    fn previous_quest_day_skips_empty_json() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at) VALUES ('2026-09-10', '[]', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at) VALUES ('2026-09-09', ?1, 1)",
            rusqlite::params![r#"[{"text":"HDP","evidence":["HDP"],"hero":true}]"#],
        )
        .unwrap();
        assert_eq!(
            previous_quest_day(&conn, "2026-09-11").unwrap().as_deref(),
            Some("2026-09-09")
        );
    }

    #[test]
    fn continue_copies_hero_and_evidence() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at) VALUES ('2026-09-10', ?1, 1)",
            rusqlite::params![r#"[{"text":"HDP","evidence":["HDP"],"hero":true}]"#],
        )
        .unwrap();
        let from = continue_previous_workday_for_day(&conn, "2026-09-11", 2).unwrap();
        assert_eq!(from, "2026-09-10");
        let qs = load_quests_for_day(&conn, "2026-09-11").unwrap();
        assert_eq!(qs[0].text, "HDP");
        assert_eq!(qs[0].evidence, vec!["HDP".to_string()]);
        assert!(qs[0].hero);
    }

    #[test]
    fn continue_without_history_errors() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let err = continue_previous_workday_for_day(&conn, "2026-09-11", 1).unwrap_err();
        assert!(format!("{err:?}").contains("no previous quests"));
    }

    #[test]
    fn save_quests_writes_evidence_and_hero() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        save_quests_for_day(
            &conn,
            day,
            vec![gamelife_core::QuestDraft {
                text: "Finish HDP tactile ablation".into(),
                evidence: vec!["HDP".into(), "a".into()],
                hero: true,
            }],
            1,
        )
        .unwrap();
        let json: String = conn
            .query_row(
                "SELECT json FROM quest_versions WHERE day = ?1",
                rusqlite::params![day],
                |r| r.get(0),
            )
            .unwrap();
        assert!(json.contains("\"evidence\""));
        assert!(json.contains("HDP"));
        assert!(!json.contains("\"a\""));
        let qs = load_quests_for_day(&conn, day).unwrap();
        assert_eq!(qs[0].text, "Finish HDP tactile ablation");
        assert_eq!(qs[0].evidence, vec!["HDP".to_string()]);
        assert!(qs[0].hero);
    }

    #[test]
    fn load_legacy_keywords_json() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at) VALUES (?1, ?2, 1)",
            rusqlite::params![day, r#"[{"text":"robot","keywords":["robot"]}]"#],
        )
        .unwrap();
        let qs = load_quests_for_day(&conn, day).unwrap();
        assert_eq!(qs[0].evidence, vec!["robot".to_string()]);
        assert!(qs[0].hero);
    }

    #[test]
    fn load_quests_and_policy_from_db() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at)
             VALUES (?1, '[{\"text\":\"HDP\",\"keywords\":[\"HDP\"]}]', 1)",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO policy_versions (json, created_at)
             VALUES ('{\"trusted_apps\":[\"Cursor\"],\"distraction_rules\":[],\"side_project_rules\":[],\"reading_apps\":[],\"never_capture_apps\":[]}', 1)",
            [],
        )
        .unwrap();
        let quests = load_quests_for_day(&conn, day).unwrap();
        assert_eq!(quests.len(), 1);
        assert_eq!(quests[0].text, "HDP");
        let policy = load_policy(&conn).unwrap();
        assert_eq!(policy.trusted_apps, vec!["Cursor".to_string()]);
    }

    #[test]
    fn seed_empty_db_marks_version_2_and_has_d2() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        seed_default_policy_if_needed(&conn).unwrap();
        let policy = load_policy(&conn).unwrap();
        assert!(policy.trusted_apps.iter().any(|a| a == "Cursor"));
        assert!(policy.distraction_rules.iter().any(|r| r == "youtube.com"));
        let version: String = conn
            .query_row(
                "SELECT value FROM app_meta WHERE key = 'policy_seed_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, "2");
    }

    #[test]
    fn seed_v1_empty_distraction_inserts_d2_row() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let old = Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec!["Preview".into()],
            never_capture_apps: builtin_never_capture(),
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        conn.execute(
            "INSERT INTO policy_versions (json, created_at) VALUES (?1, 1)",
            params![serde_json::to_string(&old).unwrap()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_meta (key, value) VALUES ('policy_seed_version', '1')",
            [],
        )
        .unwrap();
        seed_default_policy_if_needed(&conn).unwrap();
        let policy = load_policy(&conn).unwrap();
        assert!(policy.distraction_rules.iter().any(|r| r == "youtube.com"));
        assert_eq!(policy.trusted_apps, vec!["Cursor".to_string()]);
        let version: String = conn
            .query_row(
                "SELECT value FROM app_meta WHERE key = 'policy_seed_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, "2");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policy_versions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn seed_v1_custom_distraction_not_replaced() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let old = Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec!["reddit.com".into()],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: builtin_never_capture(),
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        conn.execute(
            "INSERT INTO policy_versions (json, created_at) VALUES (?1, 1)",
            params![serde_json::to_string(&old).unwrap()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_meta (key, value) VALUES ('policy_seed_version', '1')",
            [],
        )
        .unwrap();
        seed_default_policy_if_needed(&conn).unwrap();
        let policy = load_policy(&conn).unwrap();
        assert_eq!(policy.distraction_rules, vec!["reddit.com".to_string()]);
        let version: String = conn
            .query_row(
                "SELECT value FROM app_meta WHERE key = 'policy_seed_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, "2");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policy_versions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn seed_v2_empty_distraction_not_refilled() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let empty = Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: builtin_never_capture(),
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        conn.execute(
            "INSERT INTO policy_versions (json, created_at) VALUES (?1, 1)",
            params![serde_json::to_string(&empty).unwrap()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_meta (key, value) VALUES ('policy_seed_version', '2')",
            [],
        )
        .unwrap();
        seed_default_policy_if_needed(&conn).unwrap();
        let policy = load_policy(&conn).unwrap();
        assert!(policy.distraction_rules.is_empty());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policy_versions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn seed_does_not_restore_cleared_user_lists() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        seed_default_policy_if_needed(&conn).unwrap();
        let empty = Policy {
            trusted_apps: vec![],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        conn.execute(
            "INSERT INTO policy_versions (json, created_at) VALUES (?1, 2)",
            params![serde_json::to_string(&empty).unwrap()],
        )
        .unwrap();
        seed_default_policy_if_needed(&conn).unwrap();
        let policy = load_policy(&conn).unwrap();
        assert!(policy.trusted_apps.is_empty());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policy_versions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn seed_load_policy_falls_back_to_default_v01_when_no_rows() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let policy = load_policy(&conn).unwrap();
        assert!(policy.trusted_apps.iter().any(|a| a == "Cursor"));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policy_versions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn apply_screenshot_retention_none_deletes_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shot.jpg");
        std::fs::write(&path, b"x").unwrap();
        apply_screenshot_retention(&path, ScreenshotRetention::None);
        assert!(!path.exists());
    }

    #[test]
    fn apply_screenshot_retention_hours24_keeps_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shot.jpg");
        std::fs::write(&path, b"x").unwrap();
        apply_screenshot_retention(&path, ScreenshotRetention::Hours24);
        assert!(path.exists());
    }

    fn slot_capture_cols(
        conn: &Connection,
        day: &str,
        ss: i64,
    ) -> (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) {
        conn.query_row(
            "SELECT screenshot_path, capture_context_json, capture_status, status
             FROM slots WHERE day = ?1 AND slot_start = ?2",
            params![day, ss],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
    }

    #[test]
    fn pending_slot_keeps_screenshot_and_json() {
        let dir = tempfile::tempdir().unwrap();
        let shot = dir.path().join("pending.jpg");
        std::fs::write(&shot, b"x").unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status, screenshot_path, capture_context_json)
             VALUES (?1, ?2, 50, 'Captured', ?3, ?4)",
            params![day, ss, shot.to_string_lossy().to_string(), isaac_capture_json()],
        )
        .unwrap();
        for i in 0..30 {
            conn.execute(
                "INSERT INTO samples (ts, day, app, title, idle_seconds, locked, paused)
                 VALUES (?1, ?2, 'Isaac Sim', 'robot', 2, 0, 0)",
                params![i * 15, day],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at)
             VALUES (?1, '[{\"text\":\"robot\",\"keywords\":[\"robot\"]}]', 1)",
            params![day],
        )
        .unwrap();
        finalize_slot_end(&mut conn, day, ss, 900, ScreenshotRetention::None, 0).unwrap();
        let (path, json, capture_status, status) = slot_capture_cols(&conn, day, ss);
        assert_eq!(status.as_deref(), Some("pending_review"));
        assert_eq!(capture_status.as_deref(), Some("Captured"));
        assert!(path.is_some());
        assert!(json.is_some());
        assert!(shot.exists());
    }

    #[test]
    fn finalize_final_none_clears_capture_context() {
        let dir = tempfile::tempdir().unwrap();
        let shot = dir.path().join("final.jpg");
        std::fs::write(&shot, b"x").unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        let json = serde_json::to_string(&CaptureContext {
            app: "Cursor".into(),
            bundle_id: None,
            title: "main.tex".into(),
            document_path: Some("/paper/main.tex".into()),
            url: None,
            secure_input: false,
        })
        .unwrap();
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status, screenshot_path, capture_context_json)
             VALUES (?1, ?2, 50, 'Captured', ?3, ?4)",
            params![day, ss, shot.to_string_lossy().to_string(), json],
        )
        .unwrap();
        for i in 0..58 {
            conn.execute(
                "INSERT INTO samples (ts, day, app, title, document_path, idle_seconds, locked, paused)
                 VALUES (?1, ?2, 'Cursor', 'main.tex', '/paper/main.tex', 2, 0, 0)",
                params![i * 15, day],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at)
             VALUES (?1, '[{\"text\":\"paper\",\"keywords\":[\"main.tex\"]}]', 1)",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO policy_versions (json, created_at)
             VALUES ('{\"trusted_apps\":[\"Cursor\"],\"distraction_rules\":[],\"side_project_rules\":[],\"reading_apps\":[],\"never_capture_apps\":[]}', 1)",
            [],
        )
        .unwrap();
        pin_mainline_snapshot(&conn, day, ss);
        finalize_slot_end(&mut conn, day, ss, 900, ScreenshotRetention::None, 0).unwrap();
        let (path, json, capture_status, status) = slot_capture_cols(&conn, day, ss);
        assert_eq!(status.as_deref(), Some("final"));
        assert_eq!(capture_status.as_deref(), Some("Captured"));
        assert!(path.is_none());
        assert!(json.is_none());
        assert!(!shot.exists());
    }

    #[test]
    fn settle_day_clears_capture_after_pending_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let shot = dir.path().join("settle.jpg");
        std::fs::write(&shot, b"x").unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let ss = 0i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, capture_status, screenshot_path, capture_context_json, credited_core_seconds, observed_seconds, used_vision)
             VALUES (?1, ?2, 'pending_review', 'Captured', ?3, ?4, 0, 600, 0)",
            params![day, ss, shot.to_string_lossy().to_string(), isaac_capture_json()],
        )
        .unwrap();
        settle_day(&mut conn, day, 1, ScreenshotRetention::None).unwrap();
        let (path, json, capture_status, status) = slot_capture_cols(&conn, day, ss);
        assert_eq!(status.as_deref(), Some("unknown"));
        assert_eq!(capture_status.as_deref(), Some("Captured"));
        assert!(path.is_none());
        assert!(json.is_none());
        assert!(!shot.exists());
    }

    #[test]
    fn is_screenshot_expired_respects_ttl() {
        let now = 1_000_000i64;
        assert!(!is_screenshot_expired(
            now - 3600,
            now,
            ScreenshotRetention::Hours24
        ));
        assert!(is_screenshot_expired(
            now - 86401,
            now,
            ScreenshotRetention::Hours24
        ));
        assert!(is_screenshot_expired(
            now - 86400 * 4,
            now,
            ScreenshotRetention::Days3
        ));
        assert!(!is_screenshot_expired(
            now - 86400,
            now,
            ScreenshotRetention::Days14
        ));
    }

    #[test]
    fn purge_expired_screenshots_removes_old_files() {
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("HOME", dir.path()) };
        let shots = screenshots_dir().unwrap();
        std::fs::create_dir_all(&shots).unwrap();
        let old = shots.join("old.jpg");
        std::fs::write(&old, b"x").unwrap();
        let now = 1_000_000i64;
        let old_mtime = now - 86400 * 5;
        let _ = filetime::set_file_mtime(&old, filetime::FileTime::from_unix_time(old_mtime, 0));
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO slots (day, slot_start, screenshot_path, capture_context_json, capture_status)
             VALUES ('2026-09-10', 0, ?1, '{}', 'Captured')",
            params![old.to_string_lossy().to_string()],
        )
        .unwrap();
        purge_expired_screenshots(&conn, ScreenshotRetention::Days3, now).unwrap();
        assert!(!old.exists());
        let path: Option<String> = conn
            .query_row(
                "SELECT screenshot_path FROM slots WHERE day = '2026-09-10' AND slot_start = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let json: Option<String> = conn
            .query_row(
                "SELECT capture_context_json FROM slots WHERE day = '2026-09-10' AND slot_start = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(path.is_none());
        assert!(json.is_none());
    }

    #[test]
    fn maybe_finalize_previous_slot_on_boundary() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status)
             VALUES (?1, 0, 50, 'Missed')",
            params![day],
        )
        .unwrap();
        for i in 0..20 {
            conn.execute(
                "INSERT INTO samples (ts, day, app, title, idle_seconds, locked, paused)
                 VALUES (?1, ?2, 'Isaac Sim', 'robot', 2, 0, 0)",
                params![i * 15, day],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at)
             VALUES (?1, '[{\"text\":\"robot\",\"keywords\":[\"robot\"]}]', 1)",
            params![day],
        )
        .unwrap();

        finalize_slot_end(&mut conn, day, 0, 900, ScreenshotRetention::None, 0).unwrap();

        let status: String = conn
            .query_row(
                "SELECT status FROM slots WHERE day = ?1 AND slot_start = 0",
                params![day],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "pending_review");
    }

    fn day_start_ts(day: &str) -> i64 {
        use chrono::NaiveDate;
        let d = NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
        Local
            .from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
            .unwrap()
            .timestamp()
    }

    #[test]
    fn midnight_resolves_last_slot_and_settles_day() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let yesterday = "2026-09-09";
        let today_start = day_start_ts("2026-09-10");
        let last_ss = slot_start(today_start - 1);
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, credited_core_seconds, observed_seconds, used_vision, activity_json)
             VALUES (?1, ?2, 'pending_review', 0, 600, 1, '{\"core\":0,\"support\":0,\"admin\":0,\"side\":0,\"distraction\":0,\"away\":0,\"unobserved\":600}')",
            params![yesterday, last_ss],
        )
        .unwrap();

        midnight_tick(&mut conn, today_start + 5, ScreenshotRetention::None).unwrap();

        let status: String = conn
            .query_row(
                "SELECT status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![yesterday, last_ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "unknown");
        let settled_at: i64 = conn
            .query_row(
                "SELECT settled_at FROM days WHERE day = ?1",
                params![yesterday],
                |r| r.get(0),
            )
            .unwrap();
        assert!(settled_at > 0);

        let outcome_before: String = conn
            .query_row(
                "SELECT outcome FROM days WHERE day = ?1",
                params![yesterday],
                |r| r.get(0),
            )
            .unwrap();
        settle_day(
            &mut conn,
            yesterday,
            today_start + 10,
            ScreenshotRetention::None,
        )
        .unwrap();
        let outcome_after: String = conn
            .query_row(
                "SELECT outcome FROM days WHERE day = ?1",
                params![yesterday],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(outcome_before, outcome_after);
        assert_eq!(outcome_after, "failed");
    }

    #[test]
    fn freeze_march_31_from_april_1_uses_march_quota() {
        use chrono::NaiveDate;
        use gamelife_core::{can_use_freeze, freeze_month_key, freeze_quota_used};

        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let protected = "2026-03-31";
        let now = day_start_ts("2026-04-01") + 3600;
        conn.execute(
            "INSERT INTO days (day, settled_at, outcome) VALUES (?1, ?2, 'failed')",
            params![protected, now - 100],
        )
        .unwrap();

        freeze_day(&mut conn, protected, now).unwrap();

        let row: String = conn
            .query_row("SELECT protected_date FROM freeze_uses", [], |r| r.get(0))
            .unwrap();
        assert_eq!(row, "2026-03-31");
        let outcome: String = conn
            .query_row(
                "SELECT outcome FROM days WHERE day = ?1",
                params![protected],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(outcome, "protected");

        let d = NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();
        let used = vec![d];
        assert_eq!(freeze_month_key(d), "2026-03");
        assert_eq!(freeze_quota_used(&used, "2026-03"), 1);
        assert!(!can_use_freeze(&used, d));
    }

    #[test]
    fn settle_completed_when_credited_reaches_chest() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let chest = i64::try_from(CHEST_SECS).unwrap();
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, credited_core_seconds, observed_seconds, used_vision)
             VALUES (?1, 0, 'final', ?2, ?2, 0)",
            params![day, chest],
        )
        .unwrap();
        settle_day(&mut conn, day, 1, ScreenshotRetention::None).unwrap();
        let outcome: String = conn
            .query_row(
                "SELECT outcome FROM days WHERE day = ?1",
                params![day],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(outcome, "completed");
    }

    #[test]
    fn settle_preserves_existing_protected_outcome() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        conn.execute(
            "INSERT INTO days (day, settled_at, outcome) VALUES (?1, NULL, 'protected')",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, credited_core_seconds, observed_seconds, used_vision)
             VALUES (?1, 0, 'final', 100, 100, 0)",
            params![day],
        )
        .unwrap();
        settle_day(&mut conn, day, 1, ScreenshotRetention::None).unwrap();
        let outcome: String = conn
            .query_row(
                "SELECT outcome FROM days WHERE day = ?1",
                params![day],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(outcome, "protected");
    }

    #[test]
    fn settle_third_weekday_inserts_streak_milestone() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        for (day, ts) in [("2026-09-07", 1), ("2026-09-08", 2)] {
            conn.execute(
                "INSERT INTO days (day, settled_at, outcome) VALUES (?1, ?2, 'completed')",
                params![day, ts],
            )
            .unwrap();
        }
        assert_eq!(streak_from_db(&conn).unwrap(), 2);
        let day = "2026-09-09";
        let chest = i64::try_from(CHEST_SECS).unwrap();
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, credited_core_seconds, observed_seconds, used_vision)
             VALUES (?1, 0, 'final', ?2, ?2, 0)",
            params![day, chest],
        )
        .unwrap();
        settle_day(&mut conn, day, 3, ScreenshotRetention::None).unwrap();
        assert_eq!(streak_from_db(&conn).unwrap(), 3);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ledger WHERE reward_event_key = 'streak_milestone:3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn freeze_restores_streak_and_is_idempotent_on_milestones() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let fri = "2026-09-04";
        let mon = "2026-09-07";
        conn.execute(
            "INSERT INTO days (day, settled_at, outcome) VALUES (?1, 1, 'completed')",
            params![fri],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO days (day, settled_at, outcome) VALUES (?1, 2, 'failed')",
            params![mon],
        )
        .unwrap();
        assert_eq!(streak_from_db(&conn).unwrap(), 0);
        freeze_day(&mut conn, mon, 3).unwrap();
        assert_eq!(streak_from_db(&conn).unwrap(), 2);
        freeze_day(&mut conn, mon, 4).unwrap();
        assert_eq!(streak_from_db(&conn).unwrap(), 2);
    }

    #[test]
    fn end_today_half_slot_missed_capture_and_stops_sampling() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let day_start = day_start_ts(day);
        let now = day_start + 450;
        let ss = slot_start(now);
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status)
             VALUES (?1, ?2, ?3, 'Scheduled')",
            params![day, ss, now + 300],
        )
        .unwrap();
        for i in 0..10 {
            conn.execute(
                "INSERT INTO samples (ts, day, app, title, idle_seconds, locked, paused)
                 VALUES (?1, ?2, 'Isaac Sim', 'robot', 2, 0, 0)",
                params![ss + i * 15, day],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at)
             VALUES (?1, '[{\"text\":\"robot\",\"keywords\":[\"robot\"]}]', 1)",
            params![day],
        )
        .unwrap();

        end_today(&mut conn, now, ScreenshotRetention::None).unwrap();

        let capture: String = conn
            .query_row(
                "SELECT capture_status FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(capture, "Missed");
        let observed: i64 = conn
            .query_row(
                "SELECT observed_seconds FROM slots WHERE day = ?1 AND slot_start = ?2",
                params![day, ss],
                |r| r.get(0),
            )
            .unwrap();
        assert!(observed < 900);
        let settled: i64 = conn
            .query_row(
                "SELECT settled_at FROM days WHERE day = ?1",
                params![day],
                |r| r.get(0),
            )
            .unwrap();
        assert!(settled > 0);
        assert!(!sampling_allowed(&conn, day).unwrap());
    }

    #[test]
    fn sampling_not_allowed_on_weekend() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        assert!(!sampling_allowed(&conn, "2026-09-12").unwrap());
        assert!(sampling_allowed(&conn, "2026-09-11").unwrap());
    }

    #[test]
    fn early_start_coins_from_contiguous_morning_core() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        let day_start = day_start_ts(day);
        let ss = day_start + 8 * 3600 + 25 * 60;
        conn.execute(
            "INSERT INTO policy_versions (json, created_at)
             VALUES ('{\"trusted_apps\":[\"Cursor\"],\"distraction_rules\":[],\"side_project_rules\":[],\"reading_apps\":[],\"never_capture_apps\":[]}', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at)
             VALUES (?1, '[{\"text\":\"paper\",\"keywords\":[\"main.tex\"]}]', 1)",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status, quest_version_id, policy_version_id)
             VALUES (?1, ?2, ?3, 'Missed', 1, 1)",
            params![day, ss, ss + 60],
        )
        .unwrap();
        for i in 0..60 {
            conn.execute(
                "INSERT INTO samples (ts, day, app, title, document_path, idle_seconds, locked, paused)
                 VALUES (?1, ?2, 'Cursor', 'main.tex', '/x/main.tex', 2, 0, 0)",
                params![ss + i * 15, day],
            )
            .unwrap();
        }
        pin_mainline_snapshot(&conn, day, ss);
        finalize_slot_end(&mut conn, day, ss, ss + 900, ScreenshotRetention::None, 0).unwrap();
        let coin: i64 = conn
            .query_row(
                "SELECT coin_delta FROM ledger WHERE reward_event_key = ?1",
                params![format!("early_start:{day}")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(coin, 8);
    }

    #[test]
    fn early_start_no_ledger_when_gap_breaks_anchor() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        let day_start = day_start_ts(day);
        let ss1 = day_start + 8 * 3600 + 25 * 60;
        let ss2 = day_start + 10 * 3600 + 30 * 60;
        conn.execute(
            "INSERT INTO policy_versions (json, created_at)
             VALUES ('{\"trusted_apps\":[\"Cursor\"],\"distraction_rules\":[],\"side_project_rules\":[],\"reading_apps\":[],\"never_capture_apps\":[]}', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at)
             VALUES (?1, '[{\"text\":\"paper\",\"keywords\":[\"main.tex\"]}]', 1)",
            params![day],
        )
        .unwrap();
        for (ss, n) in [(ss1, 4), (ss2, 56)] {
            conn.execute(
                "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status, quest_version_id, policy_version_id, status, credited_core_seconds, observed_seconds, used_vision)
                 VALUES (?1, ?2, ?3, 'Missed', 1, 1, NULL, 0, 0, 0)",
                params![day, ss, ss + 60],
            )
            .unwrap();
            for i in 0..n {
                conn.execute(
                    "INSERT INTO samples (ts, day, app, title, path, idle_seconds, locked, paused)
                     VALUES (?1, ?2, 'Cursor', 'main.tex', '/x/main.tex', 2, 0, 0)",
                    params![ss + i * 15, day],
                )
                .unwrap();
            }
        }
        finalize_slot_end(&mut conn, day, ss1, ss1 + 900, ScreenshotRetention::None, 0).unwrap();
        finalize_slot_end(
            &mut conn,
            day,
            ss2,
            ss2 + 900,
            ScreenshotRetention::None,
            60,
        )
        .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ledger WHERE reward_event_key = ?1",
                params![format!("early_start:{day}")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn purge_old_samples_removes_rows_before_cutoff() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let now = 1_700_000_000i64;
        let old = now - 10 * 86400;
        let recent = now - 2 * 86400;
        conn.execute(
            "INSERT INTO samples (ts, day, idle_seconds, locked, paused) VALUES (?1, '2026-09-01', 0, 0, 0)",
            params![old],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO samples (ts, day, idle_seconds, locked, paused) VALUES (?1, '2026-09-09', 0, 0, 0)",
            params![recent],
        )
        .unwrap();
        let deleted = purge_old_samples(&conn, 7, now).unwrap();
        assert_eq!(deleted, 1);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
