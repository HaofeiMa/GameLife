use rusqlite::{params, Connection, OptionalExtension};

use gamelife_core::judge::{Dominant, JudgeOutput};
use gamelife_core::ledger::tick_keys_for_credited;
use gamelife_core::types::ActivitySeconds;

use crate::db::insert_ledger;
use crate::db_error::{map_rusqlite, DbOpError};

pub fn resolve_slot(
    conn: &mut Connection,
    day: &str,
    slot_start: i64,
    output: &JudgeOutput,
    credited_before: i64,
    early_coins: i64,
) -> Result<(), DbOpError> {
    let tx = conn.transaction().map_err(map_rusqlite)?;

    if slot_is_immutable(&tx, day, slot_start)? {
        tx.commit().map_err(map_rusqlite)?;
        return Ok(());
    }

    if output.pending {
        upsert_slot(&tx, day, slot_start, output, "pending_review", 0)?;
        tx.commit().map_err(map_rusqlite)?;
        return Ok(());
    }

    let credited = output.credited_core_seconds;
    if !upsert_slot(&tx, day, slot_start, output, "final", credited)? {
        tx.commit().map_err(map_rusqlite)?;
        return Ok(());
    }

    let after = credited_before + credited;
    for ev in tick_keys_for_credited(day, credited_before, after) {
        if let Err(e) = insert_ledger(&tx, &ev.key, day, ev.coin, ev.xp) {
            if e != DbOpError::AlreadyApplied {
                return Err(e);
            }
        }
    }

    if credited_before < 900 && after >= 900 {
        let key = format!("early_start:{day}");
        if let Err(e) = insert_ledger(&tx, &key, day, early_coins, 0) {
            if e != DbOpError::AlreadyApplied {
                return Err(e);
            }
        }
    }

    tx.commit().map_err(map_rusqlite)?;
    Ok(())
}

fn slot_is_immutable(conn: &Connection, day: &str, slot_start: i64) -> Result<bool, DbOpError> {
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM slots WHERE day=?1 AND slot_start=?2",
            params![day, slot_start],
            |r| r.get(0),
        )
        .optional()
        .map_err(map_rusqlite)?;
    Ok(matches!(status.as_deref(), Some("final" | "unknown")))
}

fn upsert_slot(
    conn: &Connection,
    day: &str,
    slot_start: i64,
    output: &JudgeOutput,
    status: &str,
    credited: i64,
) -> Result<bool, DbOpError> {
    let n = conn
        .execute(
            "INSERT INTO slots (day, slot_start, category, status, activity_json, credited_core_seconds, observed_seconds, used_vision)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(day, slot_start) DO UPDATE SET
           category=excluded.category,
           status=excluded.status,
           activity_json=excluded.activity_json,
           credited_core_seconds=excluded.credited_core_seconds,
           observed_seconds=excluded.observed_seconds,
           used_vision=excluded.used_vision
         WHERE slots.status NOT IN ('final', 'unknown')",
            params![
                day,
                slot_start,
                dominant_category(output.dominant),
                status,
                activity_json(&output.activity),
                credited,
                output.observed_seconds,
                output.used_vision as i64,
            ],
        )
        .map_err(map_rusqlite)?;
    Ok(n > 0)
}

fn dominant_category(d: Dominant) -> &'static str {
    match d {
        Dominant::CoreResearch => "core_research",
        Dominant::ResearchSupport => "research_support",
        Dominant::Admin => "admin",
        Dominant::SideProject => "side_project",
        Dominant::Distraction => "distraction",
        Dominant::BreakAway => "break_away",
        Dominant::Unobserved => "unobserved",
        Dominant::PendingReview => "pending_review",
        Dominant::Unknown => "unknown",
    }
}

fn activity_json(a: &ActivitySeconds) -> String {
    format!(
        r#"{{"core":{},"support":{},"admin":{},"side":{},"distraction":{},"away":{},"unobserved":{}}}"#,
        a.core,
        a.support,
        a.admin,
        a.side,
        a.distraction,
        a.away,
        a.unobserved
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use rusqlite::Connection;

    fn core_output(credited: i64) -> JudgeOutput {
        JudgeOutput {
            dominant: Dominant::CoreResearch,
            activity: ActivitySeconds {
                core: credited,
                ..Default::default()
            },
            credited_core_seconds: credited,
            observed_seconds: credited,
            used_vision: false,
            pending: false,
        }
    }

    fn validated_coin_count(conn: &Connection) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM ledger WHERE reward_event_key LIKE 'validated_coin:%'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn double_resolve_same_slot_does_not_duplicate_coin_ticks() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let output = core_output(900);
        resolve_slot(&mut conn, day, 0, &output, 0, 0).unwrap();
        let count1 = validated_coin_count(&conn);
        resolve_slot(&mut conn, day, 0, &output, 0, 0).unwrap();
        let count2 = validated_coin_count(&conn);
        assert_eq!(count1, count2);
        assert_eq!(count1, 1);
    }

    /// Pre-existing `final` slot: resolve is a no-op (guarded inside tx; UPSERT WHERE skips overwrite).
    #[test]
    fn final_slot_rejects_higher_credited() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let slot_start = 900i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, credited_core_seconds, observed_seconds, used_vision)
             VALUES (?1,?2,'final',300,300,0)",
            params![day, slot_start],
        )
        .unwrap();
        let output = core_output(900);
        resolve_slot(&mut conn, day, slot_start, &output, 0, 0).unwrap();
        let credited: i64 = conn
            .query_row(
                "SELECT credited_core_seconds FROM slots WHERE day=?1 AND slot_start=?2",
                params![day, slot_start],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(credited, 300);
    }

    #[test]
    fn pending_writes_zero_credit_without_ticks() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let output = JudgeOutput {
            dominant: Dominant::PendingReview,
            activity: ActivitySeconds::default(),
            credited_core_seconds: 0,
            observed_seconds: 600,
            used_vision: true,
            pending: true,
        };
        resolve_slot(&mut conn, day, 0, &output, 0, 0).unwrap();
        let (status, credited): (String, i64) = conn
            .query_row(
                "SELECT status, credited_core_seconds FROM slots WHERE day=?1 AND slot_start=?2",
                params![day, 0i64],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "pending_review");
        assert_eq!(credited, 0);
        assert_eq!(validated_coin_count(&conn), 0);
    }

    #[test]
    fn crossing_fifteen_minutes_inserts_early_start() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let output = core_output(300);
        resolve_slot(&mut conn, day, 0, &output, 600, 8).unwrap();
        let coin: i64 = conn
            .query_row(
                "SELECT coin_delta FROM ledger WHERE reward_event_key=?1",
                params![format!("early_start:{day}")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(coin, 8);
    }
}
