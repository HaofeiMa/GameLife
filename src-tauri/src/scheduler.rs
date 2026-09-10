use chrono::{Local, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};

use gamelife_core::{
    capture_on_resume, heartbeat_unobserved, schedule_capture, slot_end_exclusive, slot_start,
    CaptureStatus,
};

use crate::db::migrate;
use crate::db_error::{map_rusqlite, DbOpError};

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
    if secure || locked || paused {
        return true;
    }
    gamelife_core::builtin_never_capture()
        .iter()
        .any(|n| app.contains(n))
}

/// At/after scheduled time: skip conditions → Skipped; past scheduled without capture → Missed.
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
    let next = if should_skip_capture(app, secure, locked, paused) {
        CaptureStatus::Skipped
    } else if now > scheduled_at {
        CaptureStatus::Missed
    } else {
        CaptureStatus::Scheduled
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use rusqlite::Connection;

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
}
