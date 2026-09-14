use rusqlite::{params, Connection, OptionalExtension};

use gamelife_core::judge::{Dominant, JudgeOutput};
use gamelife_core::ledger::{
    admin_xp_key, support_xp_key, tick_keys_for_credited, tick_keys_for_discount, RewardEvent,
};
use gamelife_core::task::ListRole;
use gamelife_core::types::ActivitySeconds;
use gamelife_core::DayStatDelta;
use gamelife_core::GOLD_DAY_SECS;

use crate::db::{insert_ledger, settlement_mode, SettlementMode};
use crate::db_error::{map_rusqlite, DbOpError};

/// Put one reward event where the current settlement mode says it belongs.
///
/// Both destinations share the same primary key and both treat
/// `AlreadyApplied` as success, so a slot judged twice — or two devices racing
/// to settle the same slot (§7.3) — cannot double-pay. The reward *computation*
/// never consults the mode; only this function does.
fn record_reward(
    conn: &Connection,
    mode: SettlementMode,
    day: &str,
    slot_start: i64,
    ev: &RewardEvent,
) -> Result<(), DbOpError> {
    let result = match mode {
        SettlementMode::Immediate => insert_ledger(conn, &ev.key, day, ev.coin, ev.xp),
        SettlementMode::Deferred => conn
            .execute(
                "INSERT INTO pending_rewards
                   (reward_event_key, day, slot_start, ts, coin_delta, xp_delta)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    ev.key,
                    day,
                    slot_start,
                    crate::db::now_unix(),
                    ev.coin,
                    ev.xp
                ],
            )
            .map(|_| ())
            .map_err(map_rusqlite),
    };
    match result {
        Ok(()) => Ok(()),
        // The only error that may be swallowed: the same event applied twice.
        Err(DbOpError::AlreadyApplied) => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn resolve_slot(
    conn: &mut Connection,
    day: &str,
    slot_start: i64,
    output: &JudgeOutput,
    credited_before: i64,
    early_coins: i64,
    deltas: &[DayStatDelta],
) -> Result<(), DbOpError> {
    let tx = conn.transaction().map_err(map_rusqlite)?;
    let mode = settlement_mode(&tx);

    let prior = slot_status(&tx, day, slot_start)?;
    if matches!(prior.as_deref(), Some("final" | "unknown")) {
        tx.commit().map_err(map_rusqlite)?;
        return Ok(());
    }
    let stats_already_written = prior.as_deref() == Some("pending_review");

    if output.pending {
        upsert_slot(&tx, day, slot_start, output, "pending_review", 0)?;
        if !stats_already_written {
            write_day_stats(&tx, day, deltas)?;
        }
        tx.commit().map_err(map_rusqlite)?;
        return Ok(());
    }

    let credited = output.credited_core_seconds;
    if !upsert_slot(&tx, day, slot_start, output, "final", credited)? {
        tx.commit().map_err(map_rusqlite)?;
        return Ok(());
    }

    if !stats_already_written {
        write_day_stats(&tx, day, deltas)?;
    }

    let after = credited_before + credited;
    for ev in tick_keys_for_credited(day, credited_before, after) {
        record_reward(&tx, mode, day, slot_start, &ev)?;
    }

    if credited_before < 900 && after >= 900 && early_coins > 0 {
        let ev = RewardEvent {
            key: format!("early_start:{day}"),
            coin: early_coins,
            xp: 0,
        };
        record_reward(&tx, mode, day, slot_start, &ev)?;
    }

    let gold_day = i64::try_from(GOLD_DAY_SECS).unwrap_or(28800);
    if credited_before < gold_day {
        let side_before = discounted_seconds_before(&tx, day, slot_start, "credited_side_seconds")?;
        let chore_before =
            discounted_seconds_before(&tx, day, slot_start, "credited_chore_seconds")?;
        for ev in tick_keys_for_discount(
            day,
            ListRole::Side,
            side_before,
            side_before + output.credited_side_seconds,
        ) {
            record_reward(&tx, mode, day, slot_start, &ev)?;
        }
        for ev in tick_keys_for_discount(
            day,
            ListRole::Chore,
            chore_before,
            chore_before + output.credited_chore_seconds,
        ) {
            record_reward(&tx, mode, day, slot_start, &ev)?;
        }
    }

    if after < gold_day && output.credited_side_seconds == 0 && output.credited_chore_seconds == 0 {
        if output.dominant == Dominant::ResearchSupport {
            let ev = support_xp_key(day, slot_start);
            record_reward(&tx, mode, day, slot_start, &ev)?;
        }
        if output.dominant == Dominant::Admin && admin_xp_slots_today(&tx, day, mode)? < 4 {
            let ev = admin_xp_key(day, slot_start);
            record_reward(&tx, mode, day, slot_start, &ev)?;
        }
    }

    tx.commit().map_err(map_rusqlite)?;
    Ok(())
}

fn slot_status(conn: &Connection, day: &str, slot_start: i64) -> Result<Option<String>, DbOpError> {
    Ok(conn
        .query_row(
            "SELECT status FROM slots WHERE day=?1 AND slot_start=?2",
            params![day, slot_start],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(map_rusqlite)?
        .flatten())
}

fn write_day_stats(conn: &Connection, day: &str, deltas: &[DayStatDelta]) -> Result<(), DbOpError> {
    for delta in deltas {
        conn.execute(
            "INSERT INTO app_day_stats (
               day, app, bundle_id, samples, idle_seconds, core, support, admin, side,
               distraction, away, unobserved, protected
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
             ON CONFLICT(day, app, bundle_id) DO UPDATE SET
               samples = samples + excluded.samples,
               idle_seconds = idle_seconds + excluded.idle_seconds,
               core = core + excluded.core,
               support = support + excluded.support,
               admin = admin + excluded.admin,
               side = side + excluded.side,
               distraction = distraction + excluded.distraction,
               away = away + excluded.away,
               unobserved = unobserved + excluded.unobserved,
               protected = protected + excluded.protected",
            params![
                day,
                delta.app,
                delta.bundle_id,
                delta.samples,
                delta.idle_seconds,
                delta.core,
                delta.support,
                delta.admin,
                delta.side,
                delta.distraction,
                delta.away,
                delta.unobserved,
                delta.protected,
            ],
        )
        .map_err(map_rusqlite)?;

        if let Some(host) = delta.host.as_deref().filter(|h| !h.is_empty()) {
            conn.execute(
                "INSERT INTO host_day_stats (
                   day, host, samples, core, support, admin, side, distraction
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                 ON CONFLICT(day, host) DO UPDATE SET
                   samples = samples + excluded.samples,
                   core = core + excluded.core,
                   support = support + excluded.support,
                   admin = admin + excluded.admin,
                   side = side + excluded.side,
                   distraction = distraction + excluded.distraction",
                params![
                    day,
                    host,
                    delta.samples,
                    delta.core,
                    delta.support,
                    delta.admin,
                    delta.side,
                    delta.distraction,
                ],
            )
            .map_err(map_rusqlite)?;
        }
    }
    Ok(())
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
            "INSERT INTO slots (day, slot_start, category, status, activity_json, credited_core_seconds, credited_side_seconds, credited_chore_seconds, observed_seconds, used_vision)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(day, slot_start) DO UPDATE SET
           category=excluded.category,
           status=excluded.status,
           activity_json=excluded.activity_json,
           credited_core_seconds=excluded.credited_core_seconds,
           credited_side_seconds=excluded.credited_side_seconds,
           credited_chore_seconds=excluded.credited_chore_seconds,
           observed_seconds=excluded.observed_seconds,
           used_vision=excluded.used_vision
         WHERE slots.status IS NULL OR slots.status NOT IN ('final', 'unknown')",
            params![
                day,
                slot_start,
                dominant_category(output.dominant),
                status,
                activity_json(&output.activity),
                credited,
                output.credited_side_seconds,
                output.credited_chore_seconds,
                output.observed_seconds,
                output.used_vision as i64,
            ],
        )
        .map_err(map_rusqlite)?;
    Ok(n > 0)
}

fn discounted_seconds_before(
    conn: &Connection,
    day: &str,
    slot_start: i64,
    column: &str,
) -> Result<i64, DbOpError> {
    let sql = match column {
        "credited_side_seconds" | "credited_chore_seconds" => {
            format!(
                "SELECT COALESCE(SUM({column}), 0) FROM slots WHERE day = ?1 AND slot_start != ?2"
            )
        }
        _ => return Ok(0),
    };
    conn.query_row(&sql, params![day, slot_start], |r| r.get(0))
        .map_err(map_rusqlite)
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

/// How many admin slots have already been paid (or, when deferring, already
/// recorded) today. The cap is four a day, so this has to count whichever
/// destination the mode is using — counting an empty ledger while deferring
/// would pay every admin slot the day contains.
///
/// With several devices the day can still exceed four, because each machine
/// only sees its own rows; the settler applies the authoritative day-level cap
/// when it pays (§7.5).
fn admin_xp_slots_today(
    conn: &Connection,
    day: &str,
    mode: SettlementMode,
) -> Result<i64, DbOpError> {
    let table = match mode {
        SettlementMode::Immediate => "ledger",
        SettlementMode::Deferred => "pending_rewards",
    };
    let prefix = format!("xp_admin:{day}:%");
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE reward_event_key LIKE ?1"),
        params![prefix],
        |r| r.get(0),
    )
    .map_err(map_rusqlite)
}

fn activity_json(a: &ActivitySeconds) -> String {
    format!(
        r#"{{"core":{},"support":{},"admin":{},"side":{},"distraction":{},"away":{},"unobserved":{}}}"#,
        a.core, a.support, a.admin, a.side, a.distraction, a.away, a.unobserved
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
            credited_side_seconds: 0,
            credited_chore_seconds: 0,
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

    fn events_of(conn: &Connection, table: &str) -> Vec<(String, i64, i64)> {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT reward_event_key, coin_delta, xp_delta FROM {table}
                 ORDER BY reward_event_key"
            ))
            .unwrap();
        let rows: Vec<(String, i64, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        rows
    }

    fn db_in_mode(mode: SettlementMode) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        crate::db::set_settlement_mode(&conn, mode).unwrap();
        conn
    }

    /// The whole point of the two modes: deferral changes *when* a reward lands,
    /// never *what* it is. If the two diverged, a day settled on a two-device
    /// install would pay a different amount than the same day on a one-device
    /// install, and the ledger could not be trusted to mean one thing.
    #[test]
    fn deferring_records_exactly_what_the_ledger_would_have_paid() {
        let day = "2026-09-10";
        let mut immediate = db_in_mode(SettlementMode::Immediate);
        let mut deferred = db_in_mode(SettlementMode::Deferred);

        let mut credited_before = 0i64;
        for i in 0..6i64 {
            let output = core_output(900);
            let start = i * 900;
            // `early_coins` only bites on the first slot, which is the point.
            resolve_slot(&mut immediate, day, start, &output, credited_before, 8, &[]).unwrap();
            resolve_slot(&mut deferred, day, start, &output, credited_before, 8, &[]).unwrap();
            credited_before += 900;
        }

        // The per-slot keys have their own branches and must defer identically.
        let support = JudgeOutput {
            dominant: Dominant::ResearchSupport,
            ..core_output(900)
        };
        let admin = JudgeOutput {
            dominant: Dominant::Admin,
            ..core_output(900)
        };
        for (slot_start, output) in [(6 * 900, &support), (7 * 900, &admin)] {
            resolve_slot(&mut immediate, day, slot_start, output, credited_before, 0, &[]).unwrap();
            resolve_slot(&mut deferred, day, slot_start, output, credited_before, 0, &[]).unwrap();
        }

        let paid = events_of(&immediate, "ledger");
        assert!(
            paid.len() > 10,
            "the fixture must actually pay something, got {paid:?}"
        );
        assert_eq!(paid, events_of(&deferred, "pending_rewards"));

        // ...and neither mode writes to the other's destination.
        assert!(events_of(&deferred, "ledger").is_empty());
        assert!(events_of(&immediate, "pending_rewards").is_empty());
    }

    #[test]
    fn a_deferred_slot_recorded_twice_does_not_duplicate_its_intents() {
        let day = "2026-09-10";
        let mut conn = db_in_mode(SettlementMode::Deferred);
        let output = core_output(900);

        resolve_slot(&mut conn, day, 0, &output, 0, 0, &[]).unwrap();
        let first = events_of(&conn, "pending_rewards");
        resolve_slot(&mut conn, day, 0, &output, 0, 0, &[]).unwrap();

        assert_eq!(events_of(&conn, "pending_rewards"), first);
    }

    /// Deferring must not stop the slot itself from being written: the local
    /// database is still the record of what this machine observed.
    #[test]
    fn deferring_still_finalises_the_slot() {
        let day = "2026-09-10";
        let mut conn = db_in_mode(SettlementMode::Deferred);
        resolve_slot(&mut conn, day, 0, &core_output(900), 0, 0, &[]).unwrap();

        let (status, credited): (String, i64) = conn
            .query_row(
                "SELECT status, credited_core_seconds FROM slots WHERE day=?1 AND slot_start=0",
                params![day],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "final");
        assert_eq!(credited, 900);
    }

    #[test]
    fn double_resolve_same_slot_does_not_duplicate_coin_ticks() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let output = core_output(900);
        resolve_slot(&mut conn, day, 0, &output, 0, 0, &[]).unwrap();
        let count1 = validated_coin_count(&conn);
        resolve_slot(&mut conn, day, 0, &output, 0, 0, &[]).unwrap();
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
        resolve_slot(&mut conn, day, slot_start, &output, 0, 0, &[]).unwrap();
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
            credited_side_seconds: 0,
            credited_chore_seconds: 0,
            observed_seconds: 600,
            used_vision: true,
            pending: true,
        };
        resolve_slot(&mut conn, day, 0, &output, 0, 0, &[]).unwrap();
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
        resolve_slot(&mut conn, day, 0, &output, 600, 8, &[]).unwrap();
        let coin: i64 = conn
            .query_row(
                "SELECT coin_delta FROM ledger WHERE reward_event_key=?1",
                params![format!("early_start:{day}")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(coin, 8);
    }

    #[test]
    fn side_seconds_insert_discount_ticks() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let mut output = core_output(0);
        output.dominant = Dominant::SideProject;
        output.credited_side_seconds = 1500;
        output.observed_seconds = 1500;
        resolve_slot(&mut conn, "2026-09-11", 0, &output, 0, 0, &[]).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ledger WHERE reward_event_key='validated_side_coin:2026-09-11:1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    fn core_app_delta(host: Option<&str>) -> DayStatDelta {
        DayStatDelta {
            app: "Cursor".into(),
            bundle_id: String::new(),
            host: host.map(str::to_string),
            samples: 1,
            idle_seconds: 0,
            core: 15,
            ..Default::default()
        }
    }

    #[test]
    fn resolve_writes_app_and_host_day_stats() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-13";
        let output = core_output(15);
        let deltas = [core_app_delta(Some("arxiv.org"))];
        resolve_slot(&mut conn, day, 0, &output, 0, 0, &deltas).unwrap();
        let core: i64 = conn
            .query_row(
                "SELECT core FROM app_day_stats WHERE day=?1 AND app=?2 AND bundle_id=?3",
                params![day, "Cursor", ""],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(core, 15);
        let host_core: i64 = conn
            .query_row(
                "SELECT core FROM host_day_stats WHERE day=?1 AND host=?2",
                params![day, "arxiv.org"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(host_core, 15);
        let n_app: i64 = conn
            .query_row("SELECT COUNT(*) FROM app_day_stats", [], |r| r.get(0))
            .unwrap();
        let n_host: i64 = conn
            .query_row("SELECT COUNT(*) FROM host_day_stats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_app, 1);
        assert_eq!(n_host, 1);
    }

    #[test]
    fn pending_also_writes_app_day_stats() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-13";
        let output = JudgeOutput {
            dominant: Dominant::PendingReview,
            activity: ActivitySeconds::default(),
            credited_core_seconds: 0,
            credited_side_seconds: 0,
            credited_chore_seconds: 0,
            observed_seconds: 600,
            used_vision: true,
            pending: true,
        };
        let deltas = [core_app_delta(None)];
        resolve_slot(&mut conn, day, 0, &output, 0, 0, &deltas).unwrap();
        let core: i64 = conn
            .query_row(
                "SELECT core FROM app_day_stats WHERE day=?1 AND app=?2",
                params![day, "Cursor"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(core, 15);
        let n_host: i64 = conn
            .query_row("SELECT COUNT(*) FROM host_day_stats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_host, 0);
    }

    #[test]
    fn pending_then_final_does_not_double_app_stats() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-13";
        let pending = JudgeOutput {
            dominant: Dominant::PendingReview,
            activity: ActivitySeconds::default(),
            credited_core_seconds: 0,
            credited_side_seconds: 0,
            credited_chore_seconds: 0,
            observed_seconds: 600,
            used_vision: true,
            pending: true,
        };
        let deltas = [core_app_delta(None)];
        resolve_slot(&mut conn, day, 0, &pending, 0, 0, &deltas).unwrap();
        let output = core_output(15);
        resolve_slot(&mut conn, day, 0, &output, 0, 0, &deltas).unwrap();
        let core: i64 = conn
            .query_row(
                "SELECT core FROM app_day_stats WHERE day=?1 AND app=?2",
                params![day, "Cursor"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(core, 15);
        let samples: i64 = conn
            .query_row(
                "SELECT samples FROM app_day_stats WHERE day=?1 AND app=?2",
                params![day, "Cursor"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(samples, 1);
    }

    #[test]
    fn two_slots_accumulate_same_app_row() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-13";
        let output = core_output(15);
        let deltas = [core_app_delta(None)];
        resolve_slot(&mut conn, day, 0, &output, 0, 0, &deltas).unwrap();
        resolve_slot(&mut conn, day, 900, &output, 15, 0, &deltas).unwrap();
        let (samples, core): (i64, i64) = conn
            .query_row(
                "SELECT samples, core FROM app_day_stats WHERE day=?1 AND app=?2",
                params![day, "Cursor"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(samples, 2);
        assert_eq!(core, 30);
    }
}
