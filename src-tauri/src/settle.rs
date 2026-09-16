//! Day-level settlement of deferred rewards (§7.5.2).
//!
//! With more than one device registered, `resolve_slot` stops paying as each
//! slot finalises and the day's rewards are paid later — once the day's
//! canonical slot sequence is known. This module owns that later moment.
//!
//! **Why the day is recomputed rather than replayed.** The intuitive design is
//! "record the events now, pay them once the day is ready". §7.5.1 records why
//! that is wrong: the cumulative keys (`validated_coin:<day>:<i>`,
//! `validated_xp:<day>:<i>`, `ladder:<label>:<day>`, and the side/chore
//! variants) are functions of the day's *total* credited seconds, and a device
//! only knows its own share. Two devices each observing half a day both start
//! their cumulative at zero, emit the same keys, and the unique key silently
//! swallows the second half — half a day worked, half paid, no error.
//!
//! So the settler walks the canonical sequence instead. The rules are not
//! re-implemented here: `day_events` calls the same `gamelife_core` functions
//! `resolve_slot` calls. Only the input changes, from "the slots this device
//! happened to observe" to "the day".
//!
//! **Who pays.** Every device settles the whole day, not its own slots. It
//! must: the watermark advances once per day, so if a device only paid its own
//! slots, a day whose owner was offline past the grace period would have those
//! slots unpaid forever. Paying the whole day is safe because every device
//! derives the same sequence from the same merged rows, so they compute the
//! same keys and amounts; the first one to run wins and the rest get
//! `AlreadyApplied` (§7.3).

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};

use gamelife_core::ledger::{
    admin_xp_key, support_xp_key, tick_keys_for_credited, tick_keys_for_discount, RewardEvent,
};
use gamelife_core::task::ListRole;
use gamelife_core::{COIN_TICK_SECS, GOLD_DAY_SECS};

use crate::db::{
    insert_ledger, meta_get, meta_set, settlement_mode, SettlementMode, SETTLED_THROUGH_KEY,
};
use crate::db_error::{map_rusqlite, DbOpError};
use crate::scheduler::yesterday_str_for_ts;
use crate::sync::{day_is_ready, DeviceEntry, SnapshotCoverage};

/// The `admin` slots a day may pay XP for. A day-level rule, so the day-level
/// actor applies it (§7.5.2 point 4) — the per-slot path enforces the same cap
/// by counting the day's ledger rows as it goes.
const ADMIN_XP_SLOTS_PER_DAY: usize = 4;

/// How many days one run may settle. The watermark is normally within a day of
/// the clock, so this only matters after the app has been closed for a while;
/// the rest catch up on the next run rather than turning one tick into an
/// unbounded loop.
const MAX_DAYS_PER_RUN: usize = 60;

/// One slot of a day's canonical sequence — the row §7.2 kept for a
/// `(day, slot_start)`, whichever device owns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalSlot {
    pub slot_start: i64,
    pub category: String,
    pub credited_core_seconds: i64,
    pub credited_side_seconds: i64,
    pub credited_chore_seconds: i64,
    /// The early-start bonus this slot earned, stored at judgment time because
    /// it cannot be recomputed from the merged view (§7.5.2 point 3).
    pub early_coins: i64,
}

/// Everything a day pays, in the order it would have been paid had one device
/// observed the whole day.
///
/// This mirrors `resolve::resolve_slot` branch for branch. That is the point:
/// the day's worth of rewards has one definition, and deferring only changes
/// who runs it and over what input. A second, "batched" version of the rules
/// would be a second thing to keep in step.
pub fn day_events(day: &str, slots: &[CanonicalSlot]) -> Vec<RewardEvent> {
    let gold_day = i64::try_from(GOLD_DAY_SECS).unwrap_or(28800);
    let early_start_secs = i64::try_from(COIN_TICK_SECS).unwrap_or(900);

    let mut events = Vec::new();
    let mut core_before = 0i64;
    let mut side_before = 0i64;
    let mut chore_before = 0i64;
    let mut admin_paid = 0usize;

    for slot in slots {
        let credited = slot.credited_core_seconds;
        let after = core_before + credited;

        events.extend(tick_keys_for_credited(day, core_before, after));

        if core_before < early_start_secs && after >= early_start_secs && slot.early_coins > 0 {
            events.push(RewardEvent {
                key: format!("early_start:{day}"),
                coin: slot.early_coins,
                xp: 0,
            });
        }

        if core_before < gold_day {
            events.extend(tick_keys_for_discount(
                day,
                ListRole::Side,
                side_before,
                side_before + slot.credited_side_seconds,
            ));
            events.extend(tick_keys_for_discount(
                day,
                ListRole::Chore,
                chore_before,
                chore_before + slot.credited_chore_seconds,
            ));
        }

        if after < gold_day && slot.credited_side_seconds == 0 && slot.credited_chore_seconds == 0 {
            if slot.category == "research_support" {
                events.push(support_xp_key(day, slot.slot_start));
            }
            if slot.category == "admin" && admin_paid < ADMIN_XP_SLOTS_PER_DAY {
                events.push(admin_xp_key(day, slot.slot_start));
                admin_paid += 1;
            }
        }

        core_before = after;
        // Accumulated unconditionally: the per-slot path reads these off the
        // slots table, so they include every earlier final slot whether or not
        // the branch that consults them ran.
        side_before += slot.credited_side_seconds;
        chore_before += slot.credited_chore_seconds;
    }

    events
}

/// Read one day's canonical sequence out of the merged view.
///
/// `Ok(None)` means the merged view is not there at all — a different fact
/// from `Ok(Some(vec![]))`, which means the day genuinely has no final slots.
/// The first is "cannot settle yet"; the second is "nothing to pay".
///
/// Opened read-only, which is what §5.2 says the merged view is: a derived
/// artefact that sampling, judgment and settlement never write to.
pub fn read_canonical_day(
    merged: &Path,
    day: &str,
) -> Result<Option<Vec<CanonicalSlot>>, DbOpError> {
    if !merged.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(merged, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(map_rusqlite)?;
    Ok(Some(canonical_slots_on(&conn, day)?))
}

pub fn canonical_slots_on(conn: &Connection, day: &str) -> Result<Vec<CanonicalSlot>, DbOpError> {
    // `final` only, mirroring `credited_before_slot`: a slot still awaiting
    // review, or one the day's close converted to `unknown`, contributes
    // nothing to the day and pays nothing.
    let mut stmt = conn
        .prepare(
            "SELECT slot_start, COALESCE(category, ''), COALESCE(credited_core_seconds, 0),
                    COALESCE(credited_side_seconds, 0), COALESCE(credited_chore_seconds, 0),
                    COALESCE(early_coins, 0)
               FROM slots
              WHERE day = ?1 AND status = 'final'
              ORDER BY slot_start ASC",
        )
        .map_err(map_rusqlite)?;
    let rows = stmt
        .query_map(params![day], |r| {
            Ok(CanonicalSlot {
                slot_start: r.get(0)?,
                category: r.get(1)?,
                credited_core_seconds: r.get(2)?,
                credited_side_seconds: r.get(3)?,
                credited_chore_seconds: r.get(4)?,
                early_coins: r.get(5)?,
            })
        })
        .map_err(map_rusqlite)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(map_rusqlite)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettleReport {
    /// Days paid by this run, oldest first.
    pub settled: Vec<String>,
    /// The first day that stopped the walk, and why. Later days are not
    /// attempted: a day that is not ready usually means a device is behind,
    /// and the days after it are likely missing that device's data too —
    /// settling them would apply §3.3's ownership rule to exactly the view
    /// `day_is_ready` exists to reject.
    pub blocked: Option<(String, String)>,
    /// How many reward events were inserted (repeats are not counted).
    pub events: usize,
}

/// Pay every day that has come due, oldest first, and advance the watermark.
///
/// Called from the sync flow, after the merged view has been rebuilt — the
/// settler's whole input is that view, so a failed rebuild must not be
/// followed by settlement. That ordering is also why nothing needs to be
/// recorded at judgment time: an unsettled day simply stays unsettled, and
/// the watermark does not move, so no reward can be lost to a bad network.
///
/// `snapshots` is what this run actually **read**, not what the registry
/// claims exists. The two differ exactly when a device is registered but its
/// snapshot could not be fetched, and that is the case where settling would
/// apply §3.3's ownership rule to a view missing that device — the thing
/// `day_is_ready` exists to prevent. Passing the registry's `last_seen`
/// straight through would have said "covered" for a device whose data never
/// arrived.
pub fn settle_due_days(
    conn: &mut Connection,
    merged: &Path,
    devices: &[DeviceEntry],
    snapshots: &[SnapshotCoverage],
    now: i64,
    grace_hours: i64,
) -> Result<SettleReport, DbOpError> {
    let mut report = SettleReport::default();

    // A one-device install settles as it goes and has nothing to catch up on.
    // The guard is here rather than at the call site because it is the same
    // fact as `day_is_ready`'s short circuit, and it is not optional: running
    // the settler on a single-device install would re-derive days the
    // immediate path already paid.
    if settlement_mode(conn) != SettlementMode::Deferred {
        return Ok(report);
    }

    let yesterday = yesterday_str_for_ts(now);
    let mut from = settled_through(conn, &yesterday)?;

    for _ in 0..MAX_DAYS_PER_RUN {
        let Some(day) = next_day(&from) else { break };
        if day > yesterday {
            break;
        }

        if !day_is_ready(&day, devices, snapshots, now, grace_hours) {
            report.blocked = Some((day.clone(), "waiting for every device".into()));
            break;
        }

        let Some(slots) = read_canonical_day(merged, &day)? else {
            report.blocked = Some((day, "no merged view yet".into()));
            break;
        };

        let events = day_events(&day, &slots);
        let tx = conn.transaction().map_err(map_rusqlite)?;
        let mut inserted = 0usize;
        for ev in &events {
            match insert_ledger(&tx, &ev.key, &day, ev.coin, ev.xp) {
                Ok(()) => inserted += 1,
                // Already paid — by this device on an earlier run, or by the
                // other device settling the same day first (§7.3).
                Err(DbOpError::AlreadyApplied) => {}
                Err(e) => return Err(e),
            }
        }
        meta_set(&tx, SETTLED_THROUGH_KEY, &day)?;
        tx.commit().map_err(map_rusqlite)?;

        report.events += inserted;
        from = day.clone();
        report.settled.push(day);
    }

    Ok(report)
}

/// Where settlement has got to.
///
/// Absent means this install has never settled a day in deferred mode. The
/// honest starting point is then *yesterday*: every earlier day was already
/// paid by the immediate path (§7.5.2 point 7), and today cannot be settled
/// yet in any case.
///
/// It is written rather than merely derived from the clock. A default that
/// slid forward with `now` would answer "yesterday" again tomorrow and the day
/// it was computed for would never be settled at all.
fn settled_through(conn: &Connection, yesterday: &str) -> Result<String, DbOpError> {
    if let Some(v) = meta_get(conn, SETTLED_THROUGH_KEY)? {
        return Ok(v);
    }
    meta_set(conn, SETTLED_THROUGH_KEY, yesterday)?;
    Ok(yesterday.to_string())
}

/// The calendar day after `day`, or `None` if `day` is not a date.
///
/// Day keys are `YYYY-MM-DD`, so their lexical order is their chronological
/// order; this only has to get the arithmetic right.
fn next_day(day: &str) -> Option<String> {
    let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    Some(date.succ_opt()?.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use crate::config::SyncSettings;
    use crate::db::{migrate, set_settlement_mode, DEVICE_ID_KEY};
    use crate::scheduler::start_of_named_day;
    use crate::sync::{
        coverage_from, rebuild_merged, remote_dir, update_registry, DeviceEntry, RemoteTarget,
        SyncError,
    };

    // ---- the pure half: what a day is worth ------------------------------

    fn slot(slot_start: i64, category: &str, core: i64) -> CanonicalSlot {
        CanonicalSlot {
            slot_start,
            category: category.into(),
            credited_core_seconds: core,
            credited_side_seconds: 0,
            credited_chore_seconds: 0,
            early_coins: 0,
        }
    }

    fn keys(events: &[RewardEvent]) -> Vec<String> {
        events.iter().map(|e| e.key.clone()).collect()
    }

    /// Quarter-hour coin ticks only — the ladder rungs pay coins too, and they
    /// are counted by their own test.
    fn coin_ticks(events: &[RewardEvent]) -> usize {
        events
            .iter()
            .filter(|e| e.key.starts_with("validated_coin:"))
            .count()
    }

    /// §7.5.1's failure case, as a property of the pure function: a day's
    /// cumulative keys are functions of the day's *total*, so two devices each
    /// computing their own half emit the *same* keys — and the unique key then
    /// swallows one of them. The union is a half-day's worth, not a day's.
    #[test]
    fn two_halves_of_a_day_produce_the_same_keys_rather_than_the_day_s_keys() {
        let day = "2026-09-10";
        let whole: Vec<CanonicalSlot> = (0..8)
            .map(|i| slot(i * 900, "core_research", 900))
            .collect();

        let all = day_events(day, &whole);
        let morning = day_events(day, &whole[..4]);
        let afternoon = day_events(day, &whole[4..]);

        assert_eq!(
            coin_ticks(&all),
            8,
            "2h of core is 8 quarter-hour coin ticks"
        );
        assert_eq!(coin_ticks(&morning), 4);
        assert_eq!(coin_ticks(&afternoon), 4);

        // The two halves agree key for key, because each starts its cumulative
        // at zero. Paying both through one `reward_event_key` therefore pays
        // four coins for eight quarter-hours.
        assert_eq!(keys(&morning), keys(&afternoon));

        let mut union: Vec<String> = keys(&morning);
        union.extend(keys(&afternoon));
        union.sort();
        union.dedup();
        assert_eq!(union.len(), keys(&morning).len());
        assert!(union.len() < keys(&all).len());
    }

    #[test]
    fn a_split_day_still_pays_every_key_exactly_once() {
        let day = "2026-09-10";
        let whole: Vec<CanonicalSlot> = (0..8)
            .map(|i| slot(i * 900, "core_research", 900))
            .collect();

        let mut keys_all = keys(&day_events(day, &whole));
        keys_all.sort();
        keys_all.dedup();
        assert_eq!(keys_all.len(), day_events(day, &whole).len());
    }

    /// The ladder is a day-level threshold, so it must be crossed once for the
    /// day rather than once per device. Two devices at two hours each: the day
    /// reaches 4h and owes the 2h rung, while each half alone reaches only 1h
    /// and owes nothing — so a per-device computation loses the rung entirely.
    #[test]
    fn the_ladder_is_crossed_once_for_the_day() {
        let day = "2026-09-10";
        let whole = vec![
            slot(0, "core_research", 3600),
            slot(3600, "core_research", 3600),
        ];

        let ladders = |events: &[RewardEvent]| -> Vec<String> {
            keys(events)
                .into_iter()
                .filter(|k| k.starts_with("ladder:"))
                .collect()
        };

        assert_eq!(
            ladders(&day_events(day, &whole)),
            vec![format!("ladder:2h:{day}")]
        );
        assert!(ladders(&day_events(day, &whole[..1])).is_empty());
        assert!(ladders(&day_events(day, &whole[1..])).is_empty());
    }

    /// §7.5.2 point 4: four `admin` slots a day, decided in `slot_start` order
    /// so every device truncates the same way.
    #[test]
    fn the_admin_cap_is_applied_in_slot_order() {
        let day = "2026-09-10";
        let slots: Vec<CanonicalSlot> = (0..6).map(|i| slot(i * 900, "admin", 0)).collect();

        let events = day_events(day, &slots);
        let admins: Vec<String> = keys(&events)
            .into_iter()
            .filter(|k| k.starts_with("xp_admin:"))
            .collect();
        assert_eq!(admins.len(), 4);
        assert_eq!(
            admins,
            vec![
                format!("xp_admin:{day}:0"),
                format!("xp_admin:{day}:900"),
                format!("xp_admin:{day}:1800"),
                format!("xp_admin:{day}:2700"),
            ]
        );
    }

    /// An empty day pays nothing and is still a day that got settled.
    #[test]
    fn a_day_with_no_slots_pays_nothing() {
        assert!(day_events("2026-09-10", &[]).is_empty());
    }

    #[test]
    fn the_next_day_rolls_over_months_and_years() {
        assert_eq!(next_day("2026-09-10").unwrap(), "2026-09-11");
        assert_eq!(next_day("2026-09-30").unwrap(), "2026-10-01");
        assert_eq!(next_day("2026-12-31").unwrap(), "2027-01-01");
        assert_eq!(next_day("not-a-day"), None);
    }

    // ---- the whole path: snapshots -> merged view -> ledger ---------------

    /// An in-memory remote. `sync.rs` has one of its own for its own tests;
    /// this module needs its own so the settler can be exercised against a
    /// merged view built by the real `rebuild_merged` rather than a hand-made
    /// table — the settler reads columns off that view, and a hand-made one
    /// would not catch the view losing them.
    #[derive(Default)]
    struct MemoryTarget {
        store: Mutex<BTreeMap<String, Vec<u8>>>,
    }

    impl RemoteTarget for MemoryTarget {
        fn put(&self, path: &str, bytes: &[u8]) -> Result<(), SyncError> {
            self.store
                .lock()
                .unwrap()
                .insert(path.to_string(), bytes.to_vec());
            Ok(())
        }
        fn get(&self, path: &str) -> Result<Vec<u8>, SyncError> {
            self.store
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or(SyncError::Remote(404))
        }
        fn list(&self, prefix: &str) -> Result<Vec<String>, SyncError> {
            Ok(self
                .store
                .lock()
                .unwrap()
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect())
        }
        fn delete(&self, path: &str) -> Result<(), SyncError> {
            self.store.lock().unwrap().remove(path);
            Ok(())
        }
    }

    /// A live database, an empty remote, and a scratch directory.
    struct Fixture {
        _dir: tempfile::TempDir,
        target: MemoryTarget,
        settings: SyncSettings,
        merged: std::path::PathBuf,
        scratch: std::path::PathBuf,
        live: Connection,
        registry: Vec<DeviceEntry>,
    }

    impl Fixture {
        /// The live database starts in deferred mode with the watermark set, as
        /// it would be on a device that has been sharing a wallet for a while.
        fn new(settled_through: Option<&str>) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let scratch = dir.path().join("scratch");
            let merged = dir.path().join("merged.db");
            std::fs::create_dir_all(&scratch).unwrap();

            let live = Connection::open(dir.path().join("live.db")).unwrap();
            migrate(&live).unwrap();
            set_settlement_mode(&live, SettlementMode::Deferred).unwrap();
            if let Some(day) = settled_through {
                meta_set(&live, SETTLED_THROUGH_KEY, day).unwrap();
            }

            Self {
                _dir: dir,
                target: MemoryTarget::default(),
                settings: SyncSettings::default(),
                merged,
                scratch,
                live,
                registry: Vec::new(),
            }
        }

        /// Publish one device's snapshot the way `sync_now` would: rows in a
        /// real database, uploaded under the device's own remote directory,
        /// with the registry entry that carries its `last_seen`.
        fn upload(&mut self, device: &str, last_seen: i64, rows: &[String]) {
            let path = self.scratch.join(format!("snap-{device}.db"));
            let _ = std::fs::remove_file(&path);
            {
                let conn = Connection::open(&path).unwrap();
                migrate(&conn).unwrap();
                meta_set(&conn, DEVICE_ID_KEY, device).unwrap();
                for sql in rows {
                    conn.execute_batch(sql).unwrap();
                }
            }
            let bytes = std::fs::read(&path).unwrap();
            self.target
                .put(
                    &format!("{}/latest.db", remote_dir(&self.settings, device)),
                    &bytes,
                )
                .unwrap();

            let entry = DeviceEntry {
                device_id: device.into(),
                label: device.into(),
                platform: "macos".into(),
                last_seen,
            };
            update_registry(&mut self.registry, &entry);
            self.target
                .put(
                    "gamelife/devices.json",
                    &serde_json::to_vec(&self.registry).unwrap(),
                )
                .unwrap();
        }

        fn rebuild(&self) -> crate::sync::MergedReport {
            rebuild_merged(&self.target, &self.settings, &self.merged, &self.scratch).unwrap()
        }

        /// Settle with the coverage the sync flow would build: every device
        /// whose snapshot this run actually read. In this fixture every upload
        /// is readable, so that is the whole registry — the tests that care
        /// about the difference pass coverage explicitly.
        fn settle(&mut self, now: i64, grace_hours: i64) -> SettleReport {
            let devices = self.registry.clone();
            let read: Vec<String> = devices.iter().map(|d| d.device_id.clone()).collect();
            let snapshots = coverage_from(&devices, &read);
            settle_due_days(
                &mut self.live,
                &self.merged,
                &devices,
                &snapshots,
                now,
                grace_hours,
            )
            .unwrap()
        }

        fn ledger_keys(&self) -> Vec<String> {
            let mut stmt = self
                .live
                .prepare("SELECT reward_event_key FROM ledger ORDER BY reward_event_key")
                .unwrap();
            let rows: Vec<String> = stmt
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            rows
        }

        fn watermark(&self) -> Option<String> {
            meta_get(&self.live, SETTLED_THROUGH_KEY).unwrap()
        }
    }

    fn final_slot(day: &str, slot_start: i64, core: i64, early_coins: i64) -> String {
        format!(
            "INSERT INTO slots (day, slot_start, category, status, observed_seconds,
                                credited_core_seconds, early_coins)
             VALUES ('{day}', {slot_start}, 'core_research', 'final', {core}, {core}, {early_coins});"
        )
    }

    /// The headline case, end to end. Two devices, each observing half of
    /// 2026-09-10. Before §7.5.1's correction each would have computed its own
    /// half-day and the unique key would have kept four quarter-hours out of
    /// eight. The merged view is what makes the day's real total visible.
    #[test]
    fn two_devices_each_observing_half_a_day_pay_the_whole_day() {
        let mut f = Fixture::new(Some("2026-09-09"));
        let day = "2026-09-10";
        let end_of_day = start_of_named_day("2026-09-11").unwrap();
        let now = start_of_named_day("2026-09-12").unwrap();

        let morning: Vec<String> = (0..4)
            .map(|i| final_slot(day, i * 900, 900, if i == 0 { 8 } else { 0 }))
            .collect();
        let afternoon: Vec<String> = (4..8).map(|i| final_slot(day, i * 900, 900, 0)).collect();
        f.upload("aaa", end_of_day, &morning);
        f.upload("bbb", end_of_day, &afternoon);
        f.rebuild();

        let report = f.settle(now, 36);

        assert_eq!(report.settled, vec![day.to_string()]);
        // The walk then reaches 2026-09-11, which neither device has uploaded
        // a snapshot covering yet — so it stops there rather than settling a
        // day whose ownership rule would be applied to an incomplete view.
        assert_eq!(
            report.blocked.as_ref().map(|(d, _)| d.as_str()),
            Some("2026-09-11")
        );
        let keys = f.ledger_keys();
        let coins = keys
            .iter()
            .filter(|k| k.starts_with("validated_coin:"))
            .count();
        assert_eq!(coins, 8, "the day is worth 2h, not the half one device saw");
        let rung = format!("ladder:2h:{day}");
        assert_eq!(
            keys.iter().filter(|k| **k == rung).count(),
            1,
            "the 2h rung belongs to the day: {keys:?}"
        );
        // The early-start bonus came off the owner's slot row, because the
        // merged view has no `samples` to recompute it from.
        let bonus: i64 = f
            .live
            .query_row(
                "SELECT coin_delta FROM ledger WHERE reward_event_key = ?1",
                params![format!("early_start:{day}")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bonus, 8);
        assert_eq!(f.watermark().as_deref(), Some(day));
    }

    /// The other device settling first must not make this one pay again — and
    /// the watermark must not let it try.
    #[test]
    fn settling_twice_pays_once() {
        let mut f = Fixture::new(Some("2026-09-09"));
        let day = "2026-09-10";
        let end_of_day = start_of_named_day("2026-09-11").unwrap();
        let now = start_of_named_day("2026-09-12").unwrap();

        f.upload("aaa", end_of_day, &[final_slot(day, 0, 900, 0)]);
        f.upload("bbb", end_of_day, &[final_slot(day, 900, 900, 0)]);
        f.rebuild();

        let first = f.settle(now, 36);
        let after_first = f.ledger_keys();
        assert!(!first.settled.is_empty());

        let second = f.settle(now, 36);
        assert!(second.settled.is_empty(), "the watermark must stop a rerun");
        assert_eq!(second.events, 0);
        assert_eq!(f.ledger_keys(), after_first);
    }

    /// A re-run against a *stale* watermark is the real test of §7.3: the
    /// second pass recomputes the same keys, and the unique key makes every
    /// one of them a no-op rather than a second payment.
    #[test]
    fn recomputing_a_settled_day_adds_nothing() {
        let mut f = Fixture::new(Some("2026-09-09"));
        let day = "2026-09-10";
        let end_of_day = start_of_named_day("2026-09-11").unwrap();
        let now = start_of_named_day("2026-09-12").unwrap();

        f.upload("aaa", end_of_day, &[final_slot(day, 0, 900, 0)]);
        f.upload("bbb", end_of_day, &[final_slot(day, 900, 900, 0)]);
        f.rebuild();
        f.settle(now, 36);
        let after_first = f.ledger_keys();
        assert!(!after_first.is_empty());

        // Roll the watermark back, as a hand-edited or restored database might.
        meta_set(&f.live, SETTLED_THROUGH_KEY, "2026-09-09").unwrap();
        let again = f.settle(now, 36);

        assert_eq!(again.settled, vec![day.to_string()]);
        assert_eq!(again.events, 0, "every event was already in the ledger");
        assert_eq!(f.ledger_keys(), after_first);
    }

    /// §3.4: a day waits for every registered device, and a day that is not
    /// ready holds back the days after it.
    #[test]
    fn a_day_waits_for_the_other_device_and_blocks_the_days_after_it() {
        let mut f = Fixture::new(Some("2026-09-09"));
        let end_of_d = start_of_named_day("2026-09-11").unwrap();
        // `bbb` last uploaded before either day ended.
        let stale = end_of_d - 3600;
        // Well inside the grace period, so coverage is what decides.
        let now = start_of_named_day("2026-09-12").unwrap();

        f.upload(
            "aaa",
            end_of_d + 7200,
            &[final_slot("2026-09-10", 0, 900, 0)],
        );
        f.upload("bbb", stale, &[final_slot("2026-09-10", 900, 900, 0)]);
        f.upload(
            "aaa",
            end_of_d + 7200,
            &[final_slot("2026-09-11", 0, 900, 0)],
        );
        f.rebuild();

        let report = f.settle(now, 36);

        assert!(report.settled.is_empty());
        assert_eq!(
            report.blocked.as_ref().map(|(d, _)| d.as_str()),
            Some("2026-09-10")
        );
        assert!(f.ledger_keys().is_empty());
        assert_eq!(f.watermark().as_deref(), Some("2026-09-09"));
    }

    /// ...but the grace period stops a missing device from holding the ledger
    /// hostage forever (§3.4).
    #[test]
    fn the_grace_period_settles_without_the_missing_device() {
        let mut f = Fixture::new(Some("2026-09-09"));
        let end_of_d = start_of_named_day("2026-09-11").unwrap();
        // 72h after the day ended, comfortably past the 36h grace.
        let now = start_of_named_day("2026-09-14").unwrap();

        f.upload(
            "aaa",
            end_of_d + 7200,
            &[final_slot("2026-09-10", 0, 900, 0)],
        );
        f.upload("bbb", end_of_d - 3600, &[]);
        f.rebuild();

        let report = f.settle(now, 36);

        // 2026-09-10 settles without `bbb`, and the empty 2026-09-11 after it
        // settles too — it is equally past grace and has nothing to pay.
        assert_eq!(
            report.settled,
            vec!["2026-09-10".to_string(), "2026-09-11".to_string()]
        );
        assert_eq!(f.watermark().as_deref(), Some("2026-09-11"));
    }

    /// A single-device install settles as it goes. Running the settler there
    /// would re-derive days the immediate path already paid.
    #[test]
    fn the_settler_does_nothing_when_settlement_is_immediate() {
        let mut f = Fixture::new(Some("2026-09-09"));
        set_settlement_mode(&f.live, SettlementMode::Immediate).unwrap();
        let day = "2026-09-10";
        let end_of_day = start_of_named_day("2026-09-11").unwrap();

        f.upload("aaa", end_of_day, &[final_slot(day, 0, 900, 0)]);
        f.rebuild();

        let report = f.settle(start_of_named_day("2026-09-12").unwrap(), 36);

        assert_eq!(report, SettleReport::default());
        assert!(f.ledger_keys().is_empty());
    }

    /// §7.5.2 point 7: the watermark starts at yesterday — every earlier day
    /// was already paid by the immediate path — and it is *written*, so the
    /// day it was computed for is not skipped tomorrow.
    #[test]
    fn a_fresh_deferred_install_starts_its_watermark_at_yesterday() {
        let mut f = Fixture::new(None);
        let day = "2026-09-10";
        let end_of_day = start_of_named_day("2026-09-11").unwrap();
        let now = start_of_named_day("2026-09-12").unwrap();

        f.upload("aaa", end_of_day, &[final_slot(day, 0, 900, 0)]);
        f.upload("bbb", end_of_day, &[]);
        f.rebuild();

        let report = f.settle(now, 36);

        assert!(report.settled.is_empty());
        assert_eq!(
            f.watermark().as_deref(),
            Some("2026-09-11"),
            "the watermark must be written, not re-derived each run"
        );
        assert!(f.ledger_keys().is_empty());

        // Tomorrow the same day is behind the watermark and still not settled:
        // that is the accepted cost of not backfilling (§7.5.2 point 8).
        let later = start_of_named_day("2026-09-13").unwrap();
        assert!(f.settle(later, 36).settled.is_empty());
    }

    /// No merged view means no canonical sequence, so no settlement — and the
    /// watermark must not move, or the day would be lost.
    #[test]
    fn a_missing_merged_view_blocks_without_moving_the_watermark() {
        let mut f = Fixture::new(Some("2026-09-09"));
        let now = start_of_named_day("2026-09-12").unwrap();

        let report = f.settle(now, 36);

        assert!(report.settled.is_empty());
        assert_eq!(
            report.blocked.as_ref().map(|(d, _)| d.as_str()),
            Some("2026-09-10")
        );
        assert_eq!(f.watermark().as_deref(), Some("2026-09-09"));
    }

    /// The settler reads the merged view read-only; §5.2 makes it a derived
    /// artefact, and a settlement that wrote to it would make the next rebuild
    /// a repair job.
    #[test]
    fn settling_leaves_the_merged_view_untouched() {
        let mut f = Fixture::new(Some("2026-09-09"));
        let day = "2026-09-10";
        let end_of_day = start_of_named_day("2026-09-11").unwrap();

        f.upload("aaa", end_of_day, &[final_slot(day, 0, 900, 0)]);
        f.upload("bbb", end_of_day, &[final_slot(day, 900, 900, 0)]);
        f.rebuild();
        let before = std::fs::read(&f.merged).unwrap();

        f.settle(start_of_named_day("2026-09-12").unwrap(), 36);

        assert_eq!(std::fs::read(&f.merged).unwrap(), before);
    }
}
