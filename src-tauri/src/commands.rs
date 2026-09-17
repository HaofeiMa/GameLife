use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use gamelife_core::shop::{tray_entertainment_minutes, Wish, WishKind};
use gamelife_core::types::ActivitySeconds;
use gamelife_core::{
    align_range, can_delete_list, clear_schedule, distraction_runs, first_core_hour,
    format_estimated_minutes, hit_rate, is_lock_screen_app, is_weekday, matched_quest_index,
    matches_app_identity, parse_list_role_strict, parse_task_line, remind_offsets_ok, notes_ok,
    spawn_after_complete, streak_at_risk, sum_activity, validate_lists, wow_delta,
    xp_shop_unlocked, ParseContext, Policy, QuestDraft, RepeatRule, Task, TaskList, TaskListError,
    TaskRange, CHEST_SECS, GOLD_DAY_SECS, PRESET_MAINLINE_ID, SLOT_SECS,
};

use crate::config::{
    load_settings, retention_from_str, save_settings as write_settings_file, AppSettings,
};
use crate::db::{
    archive_wish as db_archive_wish, insert_wish as db_insert_wish, list_role_sql,
    load_active_session, load_task_lists, load_tasks, migrate, open, redeem as db_redeem,
    update_wish as db_update_wish,
};
use crate::db_error::DbOpError;
use crate::keychain::{
    get_provider_api_key, set_openai_api_key, set_provider_api_key as write_provider_api_key,
};
use crate::macos;
use crate::sampler::PauseControl;
use crate::scheduler::{
    continue_previous_workday_for_day, day_str_for_ts, default_screenshot_retention,
    end_of_local_day, list_freeze_candidates, load_policy, load_quests_for_day, previous_quest_day,
    review_pending_slot, sampling_allowed, save_quests_for_day, start_of_named_day, streak_from_db,
};

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn map_db_err(e: DbOpError) -> String {
    match e {
        DbOpError::Rejected(code) => code,
        DbOpError::AlreadyApplied => "already_applied".into(),
        other => format!("{other:?}"),
    }
}

fn ping_task_notifications() {
    crate::task_notify::sync_now();
}

fn with_db_err<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut Connection) -> Result<T, DbOpError>,
{
    let path = crate::db::app_db_path().ok_or_else(|| "home dir".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create db dir: {e}"))?;
    }
    let mut conn = open(&path).map_err(|e| format!("{e:?}"))?;
    migrate(&conn).map_err(|e| format!("{e:?}"))?;
    crate::scheduler::seed_default_policy_if_needed(&conn).map_err(|e| format!("{e:?}"))?;
    f(&mut conn).map_err(map_db_err)
}

fn with_db<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut Connection) -> Result<T, DbOpError>,
{
    let path = crate::db::app_db_path().ok_or_else(|| "home dir".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create db dir: {e}"))?;
    }
    let mut conn = open(&path).map_err(|e| format!("{e:?}"))?;
    migrate(&conn).map_err(|e| format!("{e:?}"))?;
    crate::scheduler::seed_default_policy_if_needed(&conn).map_err(|e| format!("{e:?}"))?;
    f(&mut conn).map_err(|e| format!("{e:?}"))
}

pub fn run_end_today() -> Result<(), String> {
    with_db(|conn| crate::scheduler::end_today(conn, now_secs(), default_screenshot_retention()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChestGold {
    pub unlocked: bool,
    pub have: i64,
    pub need: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotActivityMinutes {
    pub core: i64,
    pub support: i64,
    pub admin: i64,
    pub side: i64,
    pub distraction: i64,
    pub away: i64,
    pub unobserved: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodaySlot {
    pub start: i64,
    pub dominant: String,
    pub credited_minutes: i64,
    pub activity: SlotActivityMinutes,
    pub activity_summary: String,
    pub pending: bool,
    #[serde(rename = "final")]
    pub is_final: bool,
    pub task_snapshot_json: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestView {
    pub text: String,
    pub evidence: Vec<String>,
    pub hero: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntertainmentView {
    pub name: String,
    pub ends_at: i64,
    pub remaining_secs: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveWindow {
    pub app: String,
    pub title: String,
    pub document_path: Option<String>,
    pub url: Option<String>,
    pub matched_quest_index: Option<usize>,
    pub trusted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndedEntertainmentView {
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerTailRow {
    pub key: String,
    pub coin: i64,
    pub xp: i64,
    pub ts: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedemptionView {
    pub id: String,
    pub name: String,
    pub ts: i64,
    pub duration_minutes: Option<i64>,
    pub kind: String,
    pub spent: i64,
    pub status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppTopRow {
    pub name: String,
    pub minutes: i64,
    pub dominant: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayView {
    pub day: String,
    pub quests: Vec<QuestView>,
    pub live: Option<LiveWindow>,
    pub previous_workday: Option<String>,
    pub credited_seconds: i64,
    pub credited_label: String,
    pub coins_today: i64,
    pub xp_today: i64,
    pub xp_shop_unlocked: bool,
    pub chest: ChestGold,
    pub gold: ChestGold,
    pub streak: u32,
    pub at_risk: bool,
    pub freeze_candidates: Vec<String>,
    pub default_freeze_date: Option<String>,
    pub first_core_label: Option<String>,
    pub slots: Vec<TodaySlot>,
    pub gold_day: bool,
    pub active_entertainment: Option<EntertainmentView>,
    pub ended_entertainment: Option<EndedEntertainmentView>,
    pub ledger_tail: Vec<LedgerTailRow>,
    pub lists: Vec<TaskListView>,
    pub tasks: Vec<TaskView>,
    pub coin_balance: i64,
    pub activity: SlotActivityMinutes,
    pub app_top: Vec<AppTopRow>,
    pub pending_count: i64,
    /// True when settlement is deferred: `coins_today` / `xp_today` are a
    /// preview of what this machine's final slots would pay, not ledger rows.
    pub rewards_pending: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanMark {
    pub start: i64,
    pub end: i64,
    pub title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayView {
    pub day: String,
    pub day_start: i64,
    pub tasks: Vec<TaskView>,
    pub slots: Vec<TodaySlot>,
    pub activity: SlotActivityMinutes,
    pub app_top: Vec<AppTopRow>,
    pub pending_count: i64,
    pub plan_marks: Vec<PlanMark>,
    pub day_tasks: Vec<DayTaskView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayTaskView {
    pub id: String,
    pub title: String,
    pub role: String,
    pub start: i64,
    pub end: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskListView {
    pub id: String,
    pub name: String,
    pub sort: i64,
    pub role: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    pub id: String,
    pub list_id: String,
    pub title: String,
    pub done: bool,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub range: Option<String>,
    #[serde(default)]
    pub sort: i64,
    #[serde(default)]
    pub repeat: String,
    #[serde(default)]
    pub remind_offsets: Vec<i64>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskBoardView {
    pub lists: Vec<TaskListView>,
    pub tasks: Vec<TaskView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedTaskView {
    pub title: String,
    pub list_id: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub parse_ok: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekView {
    pub core: i64,
    pub support: i64,
    pub admin: i64,
    pub side: i64,
    pub distraction: i64,
    pub away: i64,
    pub unobserved: i64,
    pub pending_review: i64,
    pub wishes: Vec<WishView>,
    pub coin_balance: i64,
    pub xp_today: i64,
    pub xp_shop_unlocked: bool,
    pub active_entertainment: Option<EntertainmentView>,
    pub ended_entertainment: Option<EndedEntertainmentView>,
    pub redemptions: Vec<RedemptionView>,
    pub by_day: Vec<WeekDayRow>,
    pub by_hour: Vec<WeekHourRow>,
    pub core_label: String,
    pub credited_today_minutes: i64,
    pub core_hours: f64,
    pub wow_core_delta_minutes: Option<i64>,
    pub distraction_observed_ratio: f64,
    pub pending_over_resolved: f64,
    pub days_ge_6h: i64,
    pub days_ge_8h: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthDayCell {
    pub day: String,
    pub credited_core: i64,
    pub is_weekend: bool,
    pub is_future: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthReportView {
    pub days: Vec<MonthDayCell>,
    pub activity: SlotActivityMinutes,
    pub coins_earned: i64,
    pub coins_spent: i64,
    pub xp_earned: i64,
    pub gold_days: i64,
    pub freeze_count: i64,
    pub completed_days: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RhythmStartHour {
    pub day: String,
    pub hour: Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RhythmReportView {
    pub start_hours: Vec<RhythmStartHour>,
    pub rate_6h: f64,
    pub rate_8h: f64,
    pub distraction_run_count: i64,
    pub distraction_run_slots: i64,
    pub peak_hours: Vec<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppReportRow {
    pub name: String,
    pub minutes: i64,
    pub dominant: String,
    pub listed_as: String,
    pub filed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostReportRow {
    pub host: String,
    pub minutes: i64,
    pub dominant: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppReportView {
    pub apps: Vec<AppReportRow>,
    pub newcomers: Vec<String>,
    pub hosts: Vec<HostReportRow>,
    pub protected_minutes: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekDayRow {
    pub day: String,
    pub core: i64,
    pub side: i64,
    pub chore: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekHourRow {
    pub hour: i32,
    pub core: i64,
    pub observed: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WishView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub price: i64,
    pub duration_minutes: Option<i64>,
}

#[derive(Deserialize, Default)]
struct ActivityJson {
    core: i64,
    support: i64,
    admin: i64,
    side: i64,
    distraction: i64,
    away: i64,
    unobserved: i64,
}

impl ActivityJson {
    fn into_activity(self) -> ActivitySeconds {
        ActivitySeconds {
            core: self.core,
            support: self.support,
            admin: self.admin,
            side: self.side,
            distraction: self.distraction,
            away: self.away,
            unobserved: self.unobserved,
        }
    }
}

fn parse_activity_json(raw: Option<String>) -> ActivitySeconds {
    let Some(raw) = raw else {
        return ActivitySeconds::default();
    };
    serde_json::from_str::<ActivityJson>(&raw)
        .map(|a| a.into_activity())
        .unwrap_or_default()
}

fn secs_to_minutes(secs: i64) -> i64 {
    secs.max(0) / 60
}

fn activity_to_minutes(a: &ActivitySeconds) -> SlotActivityMinutes {
    SlotActivityMinutes {
        core: secs_to_minutes(a.core),
        support: secs_to_minutes(a.support),
        admin: secs_to_minutes(a.admin),
        side: secs_to_minutes(a.side),
        distraction: secs_to_minutes(a.distraction),
        away: secs_to_minutes(a.away),
        unobserved: secs_to_minutes(a.unobserved),
    }
}

fn activity_summary(a: &ActivitySeconds) -> String {
    let mut parts = Vec::new();
    let mut push = |label: &str, secs: i64| {
        let m = secs_to_minutes(secs);
        if m > 0 {
            parts.push(format!("{m}m {label}"));
        }
    };
    push("Core", a.core);
    push("Support", a.support);
    push("Admin", a.admin);
    push("Side", a.side);
    push("Distraction", a.distraction);
    push("Away", a.away);
    push("Unobserved", a.unobserved);
    if parts.is_empty() {
        "—".into()
    } else {
        parts.join(" · ")
    }
}

pub fn tray_tooltip_for_today_db() -> Result<String, String> {
    with_db(|conn| {
        let now = now_secs();
        let day = day_str_for_ts(now);
        tray_tooltip_for_today(conn, &day, now)
    })
}

pub fn tray_tooltip_for_today(conn: &Connection, day: &str, now: i64) -> Result<String, DbOpError> {
    let credited_seconds: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(credited_core_seconds), 0) FROM slots
             WHERE day = ?1 AND status = 'final'",
            params![day],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let mut label = format!("{} / 8h", format_estimated_minutes(credited_seconds));
    if let Some((name, _ends_at, remaining)) = load_active_session(conn, now)? {
        if let Some(mins) = tray_entertainment_minutes(remaining) {
            label = format!("{label} · {name} {mins}m");
        }
    }
    Ok(label)
}

fn load_active_entertainment(
    conn: &Connection,
    now: i64,
) -> Result<Option<EntertainmentView>, DbOpError> {
    Ok(
        load_active_session(conn, now)?.map(|(name, ends_at, remaining_secs)| EntertainmentView {
            name,
            ends_at,
            remaining_secs,
        }),
    )
}

fn load_ended_entertainment(
    conn: &Connection,
    now: i64,
    has_active: bool,
) -> Result<Option<EndedEntertainmentView>, DbOpError> {
    if has_active {
        return Ok(None);
    }
    conn.query_row(
        "SELECT name FROM entertainment_sessions
         WHERE ends_at <= ?1 ORDER BY ends_at DESC LIMIT 1",
        [now],
        |r| r.get(0),
    )
    .optional()
    .map_err(crate::db_error::map_rusqlite)
    .map(|name| name.map(|name| EndedEntertainmentView { name }))
}

fn load_ledger_tail(conn: &Connection, day: &str) -> Result<Vec<LedgerTailRow>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT reward_event_key, coin_delta, xp_delta, ts FROM ledger
             WHERE day = ?1 ORDER BY ts, reward_event_key",
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let rows = stmt
        .query_map(params![day], |r| {
            Ok(LedgerTailRow {
                key: r.get(0)?,
                coin: r.get(1)?,
                xp: r.get(2)?,
                ts: r.get(3)?,
            })
        })
        .map_err(crate::db_error::map_rusqlite)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(crate::db_error::map_rusqlite)
}

fn load_redemptions(conn: &Connection, now: i64) -> Result<Vec<RedemptionView>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT r.redemption_id, COALESCE(r.name, w.name, ''), r.ts, r.duration_minutes,
                    COALESCE(ABS(l.coin_delta), 0), COALESCE(ABS(l.xp_delta), 0), s.ends_at
             FROM redemptions r
             LEFT JOIN wishes w ON r.wish_id = w.id
             LEFT JOIN ledger l ON l.reward_event_key = 'shop_spend:' || r.redemption_id
             LEFT JOIN entertainment_sessions s ON s.redemption_id = r.redemption_id
             ORDER BY r.ts DESC LIMIT 30",
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<i64>>(6)?,
            ))
        })
        .map_err(crate::db_error::map_rusqlite)?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, ts, duration_minutes, coin_spent, xp_spent, ends_at) =
            row.map_err(crate::db_error::map_rusqlite)?;
        let energy = duration_minutes.is_some();
        let status = if energy {
            if ends_at.unwrap_or(0) > now {
                "进行中".into()
            } else {
                "已结束".into()
            }
        } else {
            "—".into()
        };
        out.push(RedemptionView {
            id,
            name,
            ts,
            duration_minutes,
            kind: if energy {
                "energy".into()
            } else {
                "coin".into()
            },
            spent: if energy { xp_spent } else { coin_spent },
            status,
        });
    }
    Ok(out)
}

fn week_start_for(date: NaiveDate) -> NaiveDate {
    let offset = date.weekday().num_days_from_monday();
    date - chrono::Duration::days(offset as i64)
}

fn day_str(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn parse_anchor(anchor: &str) -> Result<NaiveDate, DbOpError> {
    NaiveDate::parse_from_str(anchor, "%Y-%m-%d")
        .map_err(|_| DbOpError::Rejected("bad_anchor".into()))
}

#[derive(Clone, Copy)]
enum ReportSpan {
    Week,
    Month,
}

fn parse_report_kind(kind: &str) -> Result<ReportSpan, DbOpError> {
    match kind {
        "week" => Ok(ReportSpan::Week),
        "month" => Ok(ReportSpan::Month),
        _ => Err(DbOpError::Rejected("bad_kind".into())),
    }
}

fn span_bounds(kind: ReportSpan, anchor: NaiveDate) -> (NaiveDate, NaiveDate) {
    match kind {
        ReportSpan::Week => {
            let start = week_start_for(anchor);
            (start, start + chrono::Duration::days(6))
        }
        ReportSpan::Month => {
            let start = NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1).unwrap_or(anchor);
            let end = if anchor.month() == 12 {
                NaiveDate::from_ymd_opt(anchor.year() + 1, 1, 1).unwrap()
                    - chrono::Duration::days(1)
            } else {
                NaiveDate::from_ymd_opt(anchor.year(), anchor.month() + 1, 1).unwrap()
                    - chrono::Duration::days(1)
            };
            (start, end)
        }
    }
}

struct RangeSlot {
    day: String,
    slot_start: i64,
    activity: ActivitySeconds,
    status: String,
    observed: i64,
    credited_core: i64,
    category: String,
}

fn load_slots_in_range(
    conn: &Connection,
    start: &str,
    end: &str,
) -> Result<Vec<RangeSlot>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT day, slot_start, activity_json, status, COALESCE(observed_seconds, 0),
                    COALESCE(credited_core_seconds, 0), COALESCE(category, '')
             FROM slots WHERE day >= ?1 AND day <= ?2 ORDER BY slot_start",
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let rows = stmt
        .query_map(params![start, end], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .map_err(crate::db_error::map_rusqlite)?;
    let mut slots = Vec::new();
    for row in rows {
        let (day, slot_start, json, status, observed, credited_core, category) =
            row.map_err(crate::db_error::map_rusqlite)?;
        slots.push(RangeSlot {
            day,
            slot_start,
            activity: parse_activity_json(json),
            status: status.unwrap_or_default(),
            observed,
            credited_core,
            category,
        });
    }
    Ok(slots)
}

fn is_pending_status(status: &str) -> bool {
    status == "pending_review" || status == "unknown"
}

fn is_final_status(status: &str) -> bool {
    status == "final"
}

/// Insert a `false` breaker when slots are not adjacent (Δ ≠ 900s) or the day
/// changes, so a weekend / missing row cannot glue two entertainment runs.
fn distraction_run_flags(slots: &[RangeSlot]) -> Vec<bool> {
    let slot_secs = i64::try_from(SLOT_SECS).unwrap_or(900);
    let mut flags = Vec::new();
    let mut prev: Option<(&str, i64)> = None;
    for slot in slots {
        if let Some((prev_day, prev_start)) = prev {
            if slot.day != prev_day || slot.slot_start - prev_start != slot_secs {
                flags.push(false);
            }
        }
        flags.push(slot.category == "distraction");
        prev = Some((slot.day.as_str(), slot.slot_start));
    }
    flags
}

fn activity_observed_secs(total: &ActivitySeconds) -> i64 {
    total.core + total.support + total.admin + total.side + total.away + total.distraction
}

fn distraction_observed_ratio(total: &ActivitySeconds) -> f64 {
    let observed = activity_observed_secs(total);
    if observed <= 0 {
        0.0
    } else {
        total.distraction as f64 / observed as f64
    }
}

/// 待复核槽 / 已决议槽. pending / max(pending+final, 1) so the ratio cannot be NaN.
fn pending_over_resolved(pending: i64, final_slots: i64) -> f64 {
    pending as f64 / (pending + final_slots).max(1) as f64
}

fn chest_secs() -> i64 {
    i64::try_from(CHEST_SECS).unwrap_or(21600)
}

fn gold_secs() -> i64 {
    i64::try_from(GOLD_DAY_SECS).unwrap_or(28800)
}

fn weekday_credited_slice(
    start: NaiveDate,
    end: NaiveDate,
    today: NaiveDate,
    credited: &BTreeMap<String, i64>,
) -> Vec<i64> {
    let mut out = Vec::new();
    let mut d = start;
    while d <= end {
        if is_weekday(d) && d <= today {
            out.push(*credited.get(&day_str(d)).unwrap_or(&0));
        }
        d += chrono::Duration::days(1);
    }
    out
}

fn credited_by_day(slots: &[RangeSlot]) -> BTreeMap<String, i64> {
    let mut map = BTreeMap::new();
    for slot in slots {
        if is_final_status(&slot.status) {
            *map.entry(slot.day.clone()).or_insert(0) += slot.credited_core;
        }
    }
    map
}

fn range_credited_and_count(
    conn: &Connection,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<(i64, i64), DbOpError> {
    let start_s = day_str(start);
    let end_s = day_str(end);
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM slots WHERE day >= ?1 AND day <= ?2",
            params![start_s, end_s],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let credited: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(credited_core_seconds), 0) FROM slots
             WHERE day >= ?1 AND day <= ?2 AND status = 'final'",
            params![start_s, end_s],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    Ok((credited, count))
}

fn load_day_activity(conn: &Connection, day: &str) -> Result<ActivitySeconds, DbOpError> {
    let mut stmt = conn
        .prepare("SELECT activity_json FROM slots WHERE day = ?1")
        .map_err(crate::db_error::map_rusqlite)?;
    let rows = stmt
        .query_map(params![day], |r| r.get::<_, Option<String>>(0))
        .map_err(crate::db_error::map_rusqlite)?;
    let mut acts = Vec::new();
    for row in rows {
        acts.push(parse_activity_json(
            row.map_err(crate::db_error::map_rusqlite)?,
        ));
    }
    Ok(sum_activity(&acts))
}

fn dominant_category(
    core: i64,
    support: i64,
    admin: i64,
    side: i64,
    distraction: i64,
    away: i64,
) -> String {
    let mut best = ("", 0i64);
    for (name, secs) in [
        ("core", core),
        ("support", support),
        ("admin", admin),
        ("side", side),
        ("distraction", distraction),
        ("away", away),
    ] {
        if secs > best.1 {
            best = (name, secs);
        }
    }
    if best.1 <= 0 {
        String::new()
    } else {
        best.0.to_string()
    }
}

/// The list an app's *name* is filed in, for the 当日应用 card. Only the lists the judge
/// still reads: `trusted_apps` and `reading_apps` no longer decide anything, so labelling
/// an app 主线 or 阅读 would be a claim the engine no longer makes.
fn filed_list_for(app: &str, bundle_id: &str, policy: &Policy) -> String {
    let bid = if bundle_id.is_empty() {
        None
    } else {
        Some(bundle_id)
    };
    if matches_app_identity(app, bid, &policy.side_project_rules) {
        return "side".into();
    }
    if matches_app_identity(app, bid, &policy.admin_apps) {
        return "admin".into();
    }
    if matches_app_identity(app, bid, &policy.distraction_rules) {
        return "entertainment".into();
    }
    if matches_app_identity(app, bid, &policy.never_capture_apps) {
        return "never_capture".into();
    }
    String::new()
}

struct AppAgg {
    name: String,
    minutes: i64,
    dominant: String,
    /// The list this app's time belongs to: what the rules decided, or the filing when
    /// no rule fired.
    listed_as: String,
    /// Whether the app's own name sits in `listed_as`, as opposed to a title / URL rule
    /// having matched it. Only a filed app can be re-filed from the report table.
    filed: bool,
    /// No list and no classified seconds — the ones worth asking the user to classify.
    unruled: bool,
}

/// The list that actually governed this app's time, read off the seconds the engine put
/// in each bucket.
///
/// Identity matching alone cannot answer this: `distraction_rules` and
/// `side_project_rules` match window titles and URLs, so `"bilibili.com"` in the
/// entertainment list will never match the app name `"Google Chrome"` — yet Chrome's
/// afternoon *was* entertainment. Reporting 未列入 there was not just uninformative, it
/// invited the user to file "Google Chrome" under 娱乐, which would make every Chrome
/// window entertainment. Ties go to the rule that sits earlier in `hint_sample`.
fn ruled_list(admin: i64, side: i64, distraction: i64) -> Option<&'static str> {
    let mut best: Option<(i64, &'static str)> = None;
    for (secs, key) in [
        (distraction, "entertainment"),
        (admin, "admin"),
        (side, "side"),
    ] {
        if secs > 0 && best.is_none_or(|(top, _)| secs > top) {
            best = Some((secs, key));
        }
    }
    best.map(|(_, key)| key)
}

fn load_app_aggregates(
    conn: &Connection,
    start: &str,
    end: &str,
    policy: &Policy,
) -> Result<(Vec<AppAgg>, i64), DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT app, bundle_id, core, support, admin, side, distraction, away, unobserved, protected
             FROM app_day_stats WHERE day >= ?1 AND day <= ?2",
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let rows = stmt
        .query_map(params![start, end], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, i64>(8)?,
                r.get::<_, i64>(9)?,
            ))
        })
        .map_err(crate::db_error::map_rusqlite)?;
    let mut grouped: BTreeMap<String, (i64, i64, i64, i64, i64, i64, Vec<String>)> =
        BTreeMap::new();
    let mut protected_total = 0i64;
    for row in rows {
        let (app, bundle_id, core, support, admin, side, distraction, away, _unobserved, protected) =
            row.map_err(crate::db_error::map_rusqlite)?;
        if is_lock_screen_app(&app, Some(bundle_id.as_str())) {
            continue;
        }
        protected_total += protected;
        let entry = grouped.entry(app).or_insert((0, 0, 0, 0, 0, 0, Vec::new()));
        entry.0 += core;
        entry.1 += support;
        entry.2 += admin;
        entry.3 += side;
        entry.4 += distraction;
        entry.5 += away;
        entry.6.push(bundle_id);
    }
    let mut apps = Vec::new();
    for (name, (core, support, admin, side, distraction, away, bundles)) in grouped {
        let mut filed = String::new();
        for b in &bundles {
            filed = filed_list_for(&name, b, policy);
            if !filed.is_empty() {
                break;
            }
        }
        if filed.is_empty() {
            filed = filed_list_for(&name, "", policy);
        }
        let ruled = ruled_list(admin, side, distraction);
        let listed_as = ruled.map(str::to_string).unwrap_or_else(|| filed.clone());
        let cat_secs = core + support + admin + side + distraction + away;
        apps.push(AppAgg {
            name,
            minutes: secs_to_minutes(cat_secs),
            dominant: dominant_category(core, support, admin, side, distraction, away),
            filed: !filed.is_empty() && filed == listed_as,
            unruled: listed_as.is_empty() && cat_secs == 0,
            listed_as,
        });
    }
    apps.sort_by(|a, b| b.minutes.cmp(&a.minutes).then(a.name.cmp(&b.name)));
    Ok((apps, protected_total))
}

fn load_app_top(conn: &Connection, day: &str, limit: usize) -> Result<Vec<AppTopRow>, DbOpError> {
    let policy = load_policy(conn)?;
    let (mut apps, _) = load_app_aggregates(conn, day, day, &policy)?;
    apps.truncate(limit);
    Ok(apps
        .into_iter()
        .map(|a| AppTopRow {
            name: a.name,
            minutes: a.minutes,
            dominant: a.dominant,
        })
        .collect())
}

fn load_host_rows(
    conn: &Connection,
    start: &str,
    end: &str,
) -> Result<Vec<HostReportRow>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT host, SUM(core), SUM(support), SUM(admin), SUM(side), SUM(distraction)
             FROM host_day_stats WHERE day >= ?1 AND day <= ?2 GROUP BY host",
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let rows = stmt
        .query_map(params![start, end], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .map_err(crate::db_error::map_rusqlite)?;
    let mut hosts = Vec::new();
    for row in rows {
        let (host, core, support, admin, side, distraction) =
            row.map_err(crate::db_error::map_rusqlite)?;
        let minutes = secs_to_minutes(core + support + admin + side + distraction);
        hosts.push(HostReportRow {
            host,
            minutes,
            dominant: dominant_category(core, support, admin, side, distraction, 0),
        });
    }
    hosts.sort_by(|a, b| b.minutes.cmp(&a.minutes).then(a.host.cmp(&b.host)));
    hosts.truncate(15);
    Ok(hosts)
}

fn empty_hour_rows() -> Vec<WeekHourRow> {
    (0..24)
        .map(|hour| WeekHourRow {
            hour,
            core: 0,
            observed: 0,
        })
        .collect()
}

fn fill_hour_rows(slots: &[RangeSlot]) -> Vec<WeekHourRow> {
    let mut by_hour = empty_hour_rows();
    for slot in slots {
        if let Some(hour_row) = by_hour.get_mut(local_hour(slot.slot_start) as usize) {
            hour_row.core += slot.activity.core;
            hour_row.observed += slot.observed;
        }
    }
    by_hour
}

fn peak_hours_from(by_hour: &[WeekHourRow]) -> Vec<i32> {
    let mut ranked: Vec<(f64, i32)> = by_hour
        .iter()
        .filter(|h| h.observed > 0)
        .map(|h| (h.core as f64 / h.observed as f64, h.hour))
        .collect();
    ranked.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    ranked.into_iter().take(3).map(|(_, hour)| hour).collect()
}

fn resolved_activities(slots: &[RangeSlot]) -> Vec<ActivitySeconds> {
    slots
        .iter()
        .filter(|s| !is_pending_status(&s.status))
        .map(|s| s.activity.clone())
        .collect()
}

fn build_today(conn: &Connection, day: &str, now: i64) -> Result<TodayView, DbOpError> {
    let quests = load_quests_for_day(conn, day)?;
    let quest_views: Vec<QuestView> = quests
        .iter()
        .map(|q| QuestView {
            text: q.text.clone(),
            evidence: q.evidence.clone(),
            hero: q.hero,
        })
        .collect();
    let previous_workday = previous_quest_day(conn, day)?;
    let policy = load_policy(conn)?;
    let live = conn
        .query_row(
            "SELECT app, title, document_path, url, bundle_id FROM samples
             WHERE day = ?1 ORDER BY ts DESC LIMIT 1",
            params![day],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(crate::db_error::map_rusqlite)?
        .map(|(app, title, document_path, url, bundle_id)| {
            let quest_match =
                matched_quest_index(&title, document_path.as_deref(), url.as_deref(), &quests);
            let trusted = matches_app_identity(&app, bundle_id.as_deref(), &policy.trusted_apps);
            LiveWindow {
                app,
                title,
                document_path,
                url,
                matched_quest_index: quest_match,
                trusted,
            }
        });
    let credited_seconds: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(credited_core_seconds), 0) FROM slots
             WHERE day = ?1 AND status = 'final'",
            params![day],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let credited_minutes = secs_to_minutes(credited_seconds);
    let coins_today: i64;
    let xp_today: i64;
    let rewards_pending: bool;
    if crate::db::settlement_mode(conn) == crate::db::SettlementMode::Deferred {
        let events = crate::settle::day_events(day, &crate::settle::canonical_slots_on(conn, day)?);
        coins_today = events.iter().map(|e| e.coin).sum();
        xp_today = events.iter().map(|e| e.xp).sum();
        rewards_pending = true;
    } else {
        coins_today = conn
            .query_row(
                "SELECT COALESCE(SUM(coin_delta), 0) FROM ledger WHERE day = ?1",
                params![day],
                |r| r.get(0),
            )
            .map_err(crate::db_error::map_rusqlite)?;
        xp_today = conn
            .query_row(
                "SELECT COALESCE(SUM(xp_delta), 0) FROM ledger WHERE day = ?1",
                params![day],
                |r| r.get(0),
            )
            .map_err(crate::db_error::map_rusqlite)?;
        rewards_pending = false;
    }
    let chest_need = secs_to_minutes(i64::try_from(CHEST_SECS).unwrap_or(21600));
    let gold_need = secs_to_minutes(i64::try_from(GOLD_DAY_SECS).unwrap_or(28800));
    let streak = streak_from_db(conn)?;
    let settled = conn
        .query_row(
            "SELECT COUNT(*) FROM days WHERE day = ?1 AND settled_at IS NOT NULL",
            params![day],
            |r| r.get::<_, i64>(0),
        )
        .map_err(crate::db_error::map_rusqlite)?
        > 0;
    let at_risk = {
        let weekday = NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .map(is_weekday)
            .unwrap_or(false);
        streak_at_risk(
            weekday,
            settled,
            streak,
            credited_seconds,
            i64::try_from(CHEST_SECS).unwrap_or(21600),
        )
    };
    let first_core_label = first_core_label_for_day(conn, day, credited_seconds);
    let slots = load_today_slots(conn, day)?;
    let gold_day = credited_seconds >= i64::try_from(GOLD_DAY_SECS).unwrap_or(28800);
    let freeze_candidates = list_freeze_candidates(conn)?;
    let default_freeze_date = freeze_candidates.first().cloned();
    let active_entertainment = load_active_entertainment(conn, now)?;
    let ended_entertainment = load_ended_entertainment(conn, now, active_entertainment.is_some())?;
    let ledger_tail = load_ledger_tail(conn, day)?;
    let lists = task_list_views(conn)?;
    let tasks = task_views(conn)?;
    let coin_balance: i64 = conn
        .query_row("SELECT COALESCE(SUM(coin_delta), 0) FROM ledger", [], |r| {
            r.get(0)
        })
        .map_err(crate::db_error::map_rusqlite)?;
    let activity = activity_to_minutes(&load_day_activity(conn, day)?);
    let app_top = load_app_top(conn, day, 5)?;
    let pending_count = slots.iter().filter(|s| s.pending).count() as i64;
    Ok(TodayView {
        day: day.to_string(),
        quests: quest_views,
        live,
        previous_workday,
        credited_seconds,
        credited_label: format_estimated_minutes(credited_seconds),
        coins_today,
        xp_today,
        xp_shop_unlocked: xp_shop_unlocked(credited_seconds),
        chest: ChestGold {
            unlocked: credited_seconds >= i64::try_from(CHEST_SECS).unwrap_or(21600),
            have: credited_minutes,
            need: chest_need,
        },
        gold: ChestGold {
            unlocked: gold_day,
            have: credited_minutes,
            need: gold_need,
        },
        streak,
        at_risk,
        freeze_candidates,
        default_freeze_date,
        first_core_label,
        slots,
        gold_day,
        active_entertainment,
        ended_entertainment,
        ledger_tail,
        lists,
        tasks,
        coin_balance,
        activity,
        app_top,
        pending_count,
        rewards_pending,
    })
}

fn first_core_label_for_day(conn: &Connection, day: &str, credited_seconds: i64) -> Option<String> {
    if credited_seconds < 900 {
        return None;
    }
    let key = format!("early_start:{day}");
    let coin: i64 = conn
        .query_row(
            "SELECT COALESCE(coin_delta, 0) FROM ledger WHERE reward_event_key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
        .map_err(crate::db_error::map_rusqlite)
        .unwrap_or(None)
        .unwrap_or(0);
    if coin > 0 {
        return Some("早开始".into());
    }
    None
}

fn load_today_slots(conn: &Connection, day: &str) -> Result<Vec<TodaySlot>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT slot_start, category, COALESCE(credited_core_seconds, 0), status, activity_json, task_snapshot_json
             FROM slots WHERE day = ?1 ORDER BY slot_start",
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let rows = stmt
        .query_map(params![day], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(crate::db_error::map_rusqlite)?;
    let mut slots = Vec::new();
    for row in rows {
        let (start, category, credited, status, activity_json, task_snapshot_json) =
            row.map_err(crate::db_error::map_rusqlite)?;
        let status = status.unwrap_or_default();
        let pending = status == "pending_review";
        let is_final = status == "final" || status == "unknown";
        let activity_secs = parse_activity_json(activity_json);
        let activity = activity_to_minutes(&activity_secs);
        slots.push(TodaySlot {
            start,
            dominant: category.unwrap_or_else(|| "unknown".into()),
            credited_minutes: secs_to_minutes(credited),
            activity,
            activity_summary: activity_summary(&activity_secs),
            pending,
            is_final,
            task_snapshot_json,
        });
    }
    Ok(slots)
}

fn tasks_overlapping_day(
    conn: &Connection,
    day_start: i64,
    day_end: i64,
) -> Result<Vec<TaskView>, DbOpError> {
    Ok(load_tasks(conn)?
        .into_iter()
        .filter(|t| match (t.start, t.end) {
            (Some(s), Some(e)) => s < day_end && e > day_start,
            _ => false,
        })
        .map(task_to_view)
        .collect())
}

fn load_plan_marks(
    conn: &Connection,
    day_start: i64,
    day_end: i64,
) -> Result<Vec<PlanMark>, DbOpError> {
    Ok(load_day_tasks(conn, day_start, day_end)?
        .into_iter()
        .map(|t| PlanMark {
            start: t.start,
            end: t.end,
            title: t.title,
        })
        .collect())
}

fn load_day_tasks(
    conn: &Connection,
    day_start: i64,
    day_end: i64,
) -> Result<Vec<DayTaskView>, DbOpError> {
    let lists = load_task_lists(conn)?;
    let mut out = Vec::new();
    for task in load_tasks(conn)? {
        if task.done {
            continue;
        }
        let (Some(start), Some(end)) = (task.start, task.end) else {
            continue;
        };
        if end <= start || start >= day_end || end <= day_start {
            continue;
        }
        let Some(list) = lists.iter().find(|l| l.id == task.list_id) else {
            continue;
        };
        out.push(DayTaskView {
            id: task.id,
            title: task.title,
            role: list_role_sql(list.role).into(),
            start,
            end,
        });
    }
    out.sort_by_key(|t| (t.start, t.end, t.title.clone()));
    Ok(out)
}

fn build_day_view(conn: &Connection, day: &str) -> Result<DayView, DbOpError> {
    let day_start = start_of_named_day(day).ok_or_else(|| DbOpError::Rejected("bad_day".into()))?;
    let day_end = end_of_local_day(day_start);
    let slots = load_today_slots(conn, day)?;
    let pending_count = slots.iter().filter(|s| s.pending).count() as i64;
    let plan_marks = load_plan_marks(conn, day_start, day_end)?;
    let day_tasks = load_day_tasks(conn, day_start, day_end)?;
    Ok(DayView {
        day: day.to_string(),
        day_start,
        tasks: tasks_overlapping_day(conn, day_start, day_end)?,
        slots,
        activity: activity_to_minutes(&load_day_activity(conn, day)?),
        app_top: load_app_top(conn, day, 5)?,
        pending_count,
        plan_marks,
        day_tasks,
    })
}

fn load_wishes(conn: &Connection) -> Result<Vec<WishView>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, kind, price, duration_minutes FROM wishes
             WHERE COALESCE(archived, 0) = 0 ORDER BY name",
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(WishView {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                price: r.get(3)?,
                duration_minutes: r.get(4)?,
            })
        })
        .map_err(crate::db_error::map_rusqlite)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(crate::db_error::map_rusqlite)
}

fn local_hour(ts: i64) -> i32 {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.hour() as i32)
        .unwrap_or(0)
}

/// When settlement is deferred, activity reports read the owner-filtered
/// merged view. Wallet fields (balance, today's ledger, live sessions) stay
/// on the live database, where the settler writes them.
fn open_merged_stats(live: &Connection) -> Option<Connection> {
    if crate::db::settlement_mode(live) != crate::db::SettlementMode::Deferred {
        return None;
    }
    let path = crate::sync::merged_db_path()?;
    if !path.is_file() {
        return None;
    }
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

fn stats_db<'a>(live: &'a Connection, merged: Option<&'a Connection>) -> &'a Connection {
    merged.unwrap_or(live)
}

/// `anchor` picks which week to report; `today` stays the anchor for the
/// wallet fields (credited today, XP today, shop unlock, live sessions),
/// which are always "now" regardless of the week being looked at.
#[cfg(test)]
fn build_week(
    conn: &Connection,
    anchor: &str,
    today: &str,
    now: i64,
) -> Result<WeekView, DbOpError> {
    build_week_from(conn, anchor, today, now, None)
}

fn build_week_from(
    conn: &Connection,
    anchor: &str,
    today: &str,
    now: i64,
    stats: Option<&Connection>,
) -> Result<WeekView, DbOpError> {
    let today_date = NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .map_err(|e| DbOpError::Fatal(e.to_string()))?;
    let anchor_date = NaiveDate::parse_from_str(anchor, "%Y-%m-%d")
        .map_err(|e| DbOpError::Fatal(e.to_string()))?;
    let week_start = week_start_for(anchor_date);
    let week_start_str = day_str(week_start);
    // A past week runs Mon–Sun; the current week stops at today, so a
    // future day is never counted as a missed day.
    let week_end = week_start + chrono::Duration::days(6);
    let range_end = if week_end < today_date {
        week_end
    } else {
        today_date
    };
    let range_end_str = day_str(range_end);
    let mut by_day: Vec<WeekDayRow> = (0..7)
        .map(|i| {
            let day = week_start + chrono::Duration::days(i);
            WeekDayRow {
                day: day_str(day),
                core: 0,
                side: 0,
                chore: 0,
            }
        })
        .collect();
    let slots = load_slots_in_range(stats_db(conn, stats), &week_start_str, &range_end_str)?;
    let by_hour = fill_hour_rows(&slots);
    let mut activities = Vec::new();
    let mut pending_review_secs = 0i64;
    let mut pending_slots = 0i64;
    let mut final_slots = 0i64;
    for slot in &slots {
        if is_pending_status(&slot.status) {
            pending_review_secs += slot.observed;
            pending_slots += 1;
            continue;
        }
        if is_final_status(&slot.status) {
            final_slots += 1;
        }
        activities.push(slot.activity.clone());
        if let Some(day_row) = by_day.iter_mut().find(|d| d.day == slot.day) {
            day_row.core += slot.activity.core;
            day_row.side += slot.activity.side;
            day_row.chore += slot.activity.admin;
        }
    }
    let total = sum_activity(&activities);
    for day_row in &mut by_day {
        day_row.core = secs_to_minutes(day_row.core);
        day_row.side = secs_to_minutes(day_row.side);
        day_row.chore = secs_to_minutes(day_row.chore);
    }
    let credited_today: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(credited_core_seconds), 0) FROM slots
             WHERE day = ?1 AND status = 'final'",
            params![today],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let xp_today: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(xp_delta), 0) FROM ledger WHERE day = ?1",
            params![today],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let coin_balance: i64 = conn
        .query_row("SELECT COALESCE(SUM(coin_delta), 0) FROM ledger", [], |r| {
            r.get(0)
        })
        .map_err(crate::db_error::map_rusqlite)?;
    let wishes = load_wishes(conn)?;
    let active_entertainment = load_active_entertainment(conn, now)?;
    let ended_entertainment = load_ended_entertainment(conn, now, active_entertainment.is_some())?;
    let redemptions = load_redemptions(conn, now)?;
    let this_credited: i64 = slots
        .iter()
        .filter(|s| is_final_status(&s.status))
        .map(|s| s.credited_core)
        .sum();
    let last_start = week_start - chrono::Duration::days(7);
    let last_end = week_start - chrono::Duration::days(1);
    let (last_credited, last_count) =
        range_credited_and_count(stats_db(conn, stats), last_start, last_end)?;
    let wow_core_delta_minutes = if last_count == 0 {
        None
    } else {
        wow_delta(this_credited / 60, last_credited / 60)
    };
    let credited_map = credited_by_day(&slots);
    let weekday_credited = weekday_credited_slice(week_start, range_end, today_date, &credited_map);
    Ok(WeekView {
        core: secs_to_minutes(total.core),
        support: secs_to_minutes(total.support),
        admin: secs_to_minutes(total.admin),
        side: secs_to_minutes(total.side),
        distraction: secs_to_minutes(total.distraction),
        away: secs_to_minutes(total.away),
        unobserved: secs_to_minutes(total.unobserved),
        pending_review: secs_to_minutes(pending_review_secs),
        wishes,
        coin_balance,
        xp_today,
        xp_shop_unlocked: xp_shop_unlocked(credited_today),
        active_entertainment,
        ended_entertainment,
        redemptions,
        by_day,
        by_hour,
        core_label: format_estimated_minutes(total.core),
        credited_today_minutes: secs_to_minutes(credited_today),
        core_hours: this_credited as f64 / 3600.0,
        wow_core_delta_minutes,
        distraction_observed_ratio: distraction_observed_ratio(&total),
        pending_over_resolved: pending_over_resolved(pending_slots, final_slots),
        days_ge_6h: weekday_credited
            .iter()
            .filter(|&&s| s >= chest_secs())
            .count() as i64,
        days_ge_8h: weekday_credited
            .iter()
            .filter(|&&s| s >= gold_secs())
            .count() as i64,
    })
}

fn month_start(year: i32, month: u32) -> Result<NaiveDate, DbOpError> {
    NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| DbOpError::Rejected("bad_month".into()))
}

#[cfg(test)]
fn build_month_report(
    conn: &Connection,
    year: i32,
    month: i32,
    today: &str,
) -> Result<MonthReportView, DbOpError> {
    build_month_report_from(conn, year, month, today, None)
}

fn build_month_report_from(
    conn: &Connection,
    year: i32,
    month: i32,
    today: &str,
    stats: Option<&Connection>,
) -> Result<MonthReportView, DbOpError> {
    if !(1..=12).contains(&month) {
        return Err(DbOpError::Rejected("bad_month".into()));
    }
    let start = month_start(year, month as u32)?;
    let (span_start, end) = span_bounds(ReportSpan::Month, start);
    let start_s = day_str(span_start);
    let end_s = day_str(end);
    let slots = load_slots_in_range(stats_db(conn, stats), &start_s, &end_s)?;
    let total = sum_activity(&resolved_activities(&slots));
    let credited_map = credited_by_day(&slots);
    let mut days = Vec::new();
    let mut d = span_start;
    while d <= end {
        let key = day_str(d);
        days.push(MonthDayCell {
            day: key.clone(),
            credited_core: *credited_map.get(&key).unwrap_or(&0),
            is_weekend: !is_weekday(d),
            is_future: key.as_str() > today,
        });
        d += chrono::Duration::days(1);
    }
    let gold = gold_secs();
    let gold_days = credited_map.values().filter(|&&s| s >= gold).count() as i64;
    let coins_earned: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(CASE WHEN coin_delta > 0 THEN coin_delta ELSE 0 END), 0)
             FROM ledger WHERE day >= ?1 AND day <= ?2",
            params![start_s, end_s],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let coins_spent: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(CASE WHEN coin_delta < 0 THEN -coin_delta ELSE 0 END), 0)
             FROM ledger WHERE day >= ?1 AND day <= ?2",
            params![start_s, end_s],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let xp_earned: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(CASE WHEN xp_delta > 0 THEN xp_delta ELSE 0 END), 0)
             FROM ledger WHERE day >= ?1 AND day <= ?2",
            params![start_s, end_s],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let freeze_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM freeze_uses WHERE protected_date >= ?1 AND protected_date <= ?2",
            params![start_s, end_s],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let completed_days: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM days WHERE outcome = 'completed' AND day >= ?1 AND day <= ?2",
            params![start_s, end_s],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    Ok(MonthReportView {
        days,
        activity: activity_to_minutes(&total),
        coins_earned,
        coins_spent,
        xp_earned,
        gold_days,
        freeze_count,
        completed_days,
    })
}

#[cfg(test)]
fn build_rhythm_report(
    conn: &Connection,
    kind: &str,
    anchor: &str,
    today: &str,
) -> Result<RhythmReportView, DbOpError> {
    build_rhythm_report_from(conn, kind, anchor, today, None)
}

fn build_rhythm_report_from(
    conn: &Connection,
    kind: &str,
    anchor: &str,
    today: &str,
    stats: Option<&Connection>,
) -> Result<RhythmReportView, DbOpError> {
    let kind = parse_report_kind(kind)?;
    let anchor_date = parse_anchor(anchor)?;
    let today_date = parse_anchor(today)?;
    let (start, end) = span_bounds(kind, anchor_date);
    let start_s = day_str(start);
    let end_s = day_str(end);
    let slots = load_slots_in_range(stats_db(conn, stats), &start_s, &end_s)?;
    let credited_map = credited_by_day(&slots);
    let mut starts_by_day: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for slot in &slots {
        if slot.credited_core > 0 {
            starts_by_day
                .entry(slot.day.clone())
                .or_default()
                .push(slot.slot_start);
        }
    }
    let mut start_hours = Vec::new();
    let mut d = start;
    while d <= end {
        let include = match kind {
            ReportSpan::Week => true,
            ReportSpan::Month => is_weekday(d),
        };
        if include {
            let key = day_str(d);
            let hour = starts_by_day.get(&key).and_then(|starts| {
                let mut ordered = starts.clone();
                ordered.sort_unstable();
                let day_start = start_of_named_day(&key)?;
                first_core_hour(&ordered, day_start)
            });
            start_hours.push(RhythmStartHour { day: key, hour });
        }
        d += chrono::Duration::days(1);
    }
    let weekday_credited = weekday_credited_slice(start, end, today_date, &credited_map);
    let flags = distraction_run_flags(&slots);
    let (run_count, run_slots) = distraction_runs(&flags);
    let by_hour = fill_hour_rows(&slots);
    Ok(RhythmReportView {
        start_hours,
        rate_6h: hit_rate(&weekday_credited, chest_secs()),
        rate_8h: hit_rate(&weekday_credited, gold_secs()),
        distraction_run_count: run_count as i64,
        distraction_run_slots: run_slots,
        peak_hours: peak_hours_from(&by_hour),
    })
}

#[cfg(test)]
fn build_app_report(
    conn: &Connection,
    kind: &str,
    anchor: &str,
) -> Result<AppReportView, DbOpError> {
    build_app_report_from(conn, kind, anchor, None)
}

fn build_app_report_from(
    conn: &Connection,
    kind: &str,
    anchor: &str,
    stats: Option<&Connection>,
) -> Result<AppReportView, DbOpError> {
    let kind = parse_report_kind(kind)?;
    let anchor_date = parse_anchor(anchor)?;
    let (start, end) = span_bounds(kind, anchor_date);
    let start_s = day_str(start);
    let end_s = day_str(end);
    let policy = load_policy(conn)?;
    let stats = stats_db(conn, stats);
    let (apps, protected_total) = load_app_aggregates(stats, &start_s, &end_s, &policy)?;
    let newcomers: Vec<String> = apps
        .iter()
        .filter(|a| a.unruled)
        .map(|a| a.name.clone())
        .collect();
    let rows = apps
        .into_iter()
        .map(|a| AppReportRow {
            name: a.name,
            minutes: a.minutes,
            dominant: a.dominant,
            listed_as: a.listed_as,
            filed: a.filed,
        })
        .collect();
    Ok(AppReportView {
        apps: rows,
        newcomers,
        hosts: load_host_rows(stats, &start_s, &end_s)?,
        protected_minutes: secs_to_minutes(protected_total),
    })
}

#[tauri::command]
pub fn get_today() -> Result<TodayView, String> {
    with_db(|conn| {
        let now = now_secs();
        let day = day_str_for_ts(now);
        build_today(conn, &day, now)
    })
}

#[tauri::command]
pub fn get_day_view(day: String) -> Result<DayView, String> {
    with_db_err(|conn| build_day_view(conn, &day))
}

#[tauri::command]
pub fn get_week(anchor: Option<String>) -> Result<WeekView, String> {
    with_db(|conn| {
        let now = now_secs();
        let today = day_str_for_ts(now);
        let anchor = anchor.as_deref().unwrap_or(&today);
        let merged = open_merged_stats(conn);
        build_week_from(conn, anchor, &today, now, merged.as_ref())
    })
}

#[tauri::command]
pub fn get_month_report(year: i32, month: i32) -> Result<MonthReportView, String> {
    with_db_err(|conn| {
        let today = day_str_for_ts(now_secs());
        let merged = open_merged_stats(conn);
        build_month_report_from(conn, year, month, &today, merged.as_ref())
    })
}

#[tauri::command]
pub fn get_rhythm_report(kind: String, anchor: String) -> Result<RhythmReportView, String> {
    with_db_err(|conn| {
        let today = day_str_for_ts(now_secs());
        let merged = open_merged_stats(conn);
        build_rhythm_report_from(conn, &kind, &anchor, &today, merged.as_ref())
    })
}

#[tauri::command]
pub fn get_app_report(kind: String, anchor: String) -> Result<AppReportView, String> {
    with_db_err(|conn| {
        let merged = open_merged_stats(conn);
        build_app_report_from(conn, &kind, &anchor, merged.as_ref())
    })
}

fn task_list_views(conn: &Connection) -> Result<Vec<TaskListView>, DbOpError> {
    Ok(load_task_lists(conn)?
        .into_iter()
        .map(|l| TaskListView {
            id: l.id,
            name: l.name,
            sort: l.sort,
            role: list_role_sql(l.role).into(),
        })
        .collect())
}

fn task_views(conn: &Connection) -> Result<Vec<TaskView>, DbOpError> {
    Ok(load_tasks(conn)?.into_iter().map(task_to_view).collect())
}

fn task_to_view(task: Task) -> TaskView {
    TaskView {
        id: task.id,
        list_id: task.list_id,
        title: task.title,
        done: task.done,
        start: task.start,
        end: task.end,
        range: match task.range {
            Some(TaskRange::Week) => Some("week".into()),
            Some(TaskRange::Month) => Some("month".into()),
            None => None,
        },
        sort: task.sort,
        repeat: crate::db::repeat_sql(task.repeat).into(),
        remind_offsets: task.remind_offsets,
        notes: task.notes,
    }
}

fn view_to_task(view: &TaskView) -> Task {
    Task {
        id: view.id.clone(),
        list_id: view.list_id.clone(),
        title: view.title.clone(),
        done: view.done,
        start: view.start,
        end: view.end,
        range: match view.range.as_deref() {
            Some("week") => Some(TaskRange::Week),
            Some("month") => Some(TaskRange::Month),
            _ => None,
        },
        sort: view.sort,
        repeat: crate::db::parse_repeat(&view.repeat),
        remind_offsets: view.remind_offsets.clone(),
        notes: view.notes.clone(),
    }
}

fn persist_task(conn: &Connection, task: &Task) -> Result<(), DbOpError> {
    let range = match task.range {
        Some(TaskRange::Week) => Some("week"),
        Some(TaskRange::Month) => Some("month"),
        None => None,
    };
    let remind_json = serde_json::to_string(&task.remind_offsets).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO tasks (id, list_id, title, done, start, end, range, sort, repeat, remind_json, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
           list_id=excluded.list_id,
           title=excluded.title,
           done=excluded.done,
           start=excluded.start,
           end=excluded.end,
           range=excluded.range,
           sort=excluded.sort,
           repeat=excluded.repeat,
           remind_json=excluded.remind_json,
           notes=excluded.notes",
        params![
            task.id,
            task.list_id,
            task.title.trim(),
            task.done as i64,
            task.start,
            task.end,
            range,
            task.sort,
            crate::db::repeat_sql(task.repeat),
            remind_json,
            task.notes,
        ],
    )
    .map_err(crate::db_error::map_rusqlite)?;
    Ok(())
}

fn new_task_id() -> String {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).expect("rng");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn next_list_sort(conn: &Connection, list_id: &str) -> Result<i64, DbOpError> {
    let max: Option<i64> = conn
        .query_row(
            "SELECT MAX(sort) FROM tasks WHERE list_id = ?1",
            params![list_id],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    Ok(max.unwrap_or(-1) + 1)
}

fn upsert_task_in(conn: &Connection, task: TaskView) -> Result<(), DbOpError> {
    let title = task.title.trim();
    if title.is_empty() {
        return Err(DbOpError::Rejected("empty_title".into()));
    }
    let mut stored = view_to_task(&task);
    stored.title = title.to_string();
    if !remind_offsets_ok(&stored.remind_offsets) {
        return Err(DbOpError::Rejected("bad_remind".into()));
    }
    if !notes_ok(&stored.notes) {
        return Err(DbOpError::Rejected("notes_too_long".into()));
    }
    if stored.start.is_none() && stored.repeat != RepeatRule::None {
        return Err(DbOpError::Rejected("repeat_needs_schedule".into()));
    }
    let existing = load_tasks(conn)?;
    let is_new = stored.id.trim().is_empty() || !existing.iter().any(|t| t.id == stored.id);
    if stored.id.trim().is_empty() {
        stored.id = new_task_id();
    }
    if is_new && stored.sort == 0 {
        stored.sort = next_list_sort(conn, &stored.list_id)?;
    }
    persist_task(conn, &stored)?;
    Ok(())
}

#[tauri::command]
pub fn list_task_board() -> Result<TaskBoardView, String> {
    with_db_err(|conn| {
        Ok(TaskBoardView {
            lists: task_list_views(conn)?,
            tasks: task_views(conn)?,
        })
    })
}

#[tauri::command]
pub fn upsert_task(task: TaskView) -> Result<(), String> {
    let out = with_db_err(|conn| upsert_task_in(conn, task));
    if out.is_ok() {
        ping_task_notifications();
    }
    out
}

#[tauri::command]
pub fn toggle_task_done(id: String, done: bool) -> Result<(), String> {
    let out = with_db_err(|conn| toggle_task_done_in(conn, &id, done, now_secs()));
    if out.is_ok() {
        ping_task_notifications();
    }
    out
}

/// Completing a repeating task inserts the next occurrence. Uncomplete only
/// flips this row — it does not delete a spawned successor.
fn toggle_task_done_in(conn: &Connection, id: &str, done: bool, now: i64) -> Result<(), DbOpError> {
    let mut tasks = load_tasks(conn)?;
    let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
        return Err(DbOpError::Fatal("task missing".into()));
    };
    let was_done = task.done;
    task.done = done;
    let stored = task.clone();
    persist_task(conn, &stored)?;
    if done && !was_done {
        let day = day_str_for_ts(now);
        let today_start = start_of_named_day(&day).unwrap_or(0);
        let sort = next_list_sort(conn, &stored.list_id)?;
        if let Some(next) = spawn_after_complete(&stored, new_task_id(), today_start, sort) {
            persist_task(conn, &next)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn reorder_task(id: String, list_id: String, sort: i64) -> Result<(), String> {
    let out = with_db_err(|conn| {
        let lists = load_task_lists(conn)?;
        if !lists.iter().any(|l| l.id == list_id) {
            return Err(DbOpError::Rejected("list_missing".into()));
        }
        let n = conn
            .execute(
                "UPDATE tasks SET list_id = ?1, sort = ?2 WHERE id = ?3",
                params![list_id, sort, id],
            )
            .map_err(crate::db_error::map_rusqlite)?;
        if n == 0 {
            return Err(DbOpError::Fatal("task missing".into()));
        }
        Ok(())
    });
    if out.is_ok() {
        ping_task_notifications();
    }
    out
}

#[tauri::command]
pub fn duplicate_task(id: String) -> Result<TaskView, String> {
    let out = with_db_err(|conn| duplicate_task_in(conn, &id));
    if out.is_ok() {
        ping_task_notifications();
    }
    out
}

fn duplicate_task_in(conn: &Connection, id: &str) -> Result<TaskView, DbOpError> {
    let tasks = load_tasks(conn)?;
    let Some(src) = tasks.iter().find(|t| t.id == id) else {
        return Err(DbOpError::Fatal("task missing".into()));
    };
    let mut copy = src.clone();
    copy.id = new_task_id();
    copy.done = false;
    copy.sort = next_list_sort(conn, &copy.list_id)?;
    persist_task(conn, &copy)?;
    Ok(task_to_view(copy))
}

#[tauri::command(rename = "parse_task_line")]
pub fn parse_task_line_cmd(
    line: String,
    current_list_id: Option<String>,
) -> Result<ParsedTaskView, String> {
    with_db(|conn| {
        let lists = load_task_lists(conn)?;
        let current = current_list_id
            .filter(|id| lists.iter().any(|l| l.id == *id))
            .unwrap_or_else(|| PRESET_MAINLINE_ID.to_string());
        let now = chrono::Local::now().fixed_offset();
        let parsed = parse_task_line(
            &line,
            &ParseContext {
                now,
                lists: &lists,
                current_list_id: &current,
                default_list_id: PRESET_MAINLINE_ID,
            },
        );
        Ok(ParsedTaskView {
            title: parsed.title,
            list_id: parsed.list_id,
            start: parsed.start,
            end: parsed.end,
            parse_ok: parsed.parse_ok,
        })
    })
}

#[tauri::command]
pub fn create_list(name: String, role: String) -> Result<TaskListView, String> {
    with_db_err(|conn| {
        let name = name.trim();
        if name.is_empty() {
            return Err(DbOpError::Rejected("empty_name".into()));
        }
        let role = parse_list_role_strict(&role)
            .ok_or_else(|| DbOpError::Rejected("invalid_role".into()))?;
        let mut lists = load_task_lists(conn)?;
        let sort = lists.iter().map(|l| l.sort).max().unwrap_or(-1) + 1;
        let list = TaskList {
            id: format!("list-{}", now_secs()),
            name: name.to_string(),
            sort,
            role,
        };
        lists.push(list.clone());
        validate_lists(&lists).map_err(|_| DbOpError::Rejected("invalid_lists".into()))?;
        conn.execute(
            "INSERT INTO task_lists (id, name, sort, role) VALUES (?1, ?2, ?3, ?4)",
            params![list.id, list.name, list.sort, list_role_sql(list.role)],
        )
        .map_err(crate::db_error::map_rusqlite)?;
        Ok(TaskListView {
            id: list.id,
            name: list.name,
            sort: list.sort,
            role: list_role_sql(list.role).into(),
        })
    })
}

#[tauri::command]
pub fn rename_list(id: String, name: String) -> Result<(), String> {
    with_db_err(|conn| {
        let name = name.trim();
        if name.is_empty() {
            return Err(DbOpError::Rejected("empty_name".into()));
        }
        let n = conn
            .execute(
                "UPDATE task_lists SET name = ?1 WHERE id = ?2",
                params![name, id],
            )
            .map_err(crate::db_error::map_rusqlite)?;
        if n == 0 {
            return Err(DbOpError::Fatal("list missing".into()));
        }
        Ok(())
    })
}

#[tauri::command]
pub fn delete_list(id: String) -> Result<(), String> {
    with_db_err(|conn| {
        let lists = load_task_lists(conn)?;
        let tasks = load_tasks(conn)?;
        can_delete_list(&lists, &tasks, &id).map_err(|e| match e {
            TaskListError::PresetLocked => DbOpError::Rejected("preset_locked".into()),
            TaskListError::NotEmpty => DbOpError::Rejected("list_not_empty".into()),
            TaskListError::NoMainline => DbOpError::Rejected("no_mainline".into()),
            TaskListError::MissingList => DbOpError::Rejected("list_missing".into()),
            other => DbOpError::Rejected(format!("{other:?}")),
        })?;
        conn.execute("DELETE FROM task_lists WHERE id = ?1", params![id])
            .map_err(crate::db_error::map_rusqlite)?;
        Ok(())
    })
}

#[tauri::command]
pub fn delete_task(id: String) -> Result<(), String> {
    let out = with_db_err(|conn| {
        let n = conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])
            .map_err(crate::db_error::map_rusqlite)?;
        if n == 0 {
            return Err(DbOpError::Fatal("task missing".into()));
        }
        Ok(())
    });
    if out.is_ok() {
        ping_task_notifications();
    }
    out
}

#[tauri::command]
pub fn move_task(id: String, list_id: String) -> Result<(), String> {
    with_db_err(|conn| {
        let lists = load_task_lists(conn)?;
        if !lists.iter().any(|l| l.id == list_id) {
            return Err(DbOpError::Rejected("list_missing".into()));
        }
        let n = conn
            .execute(
                "UPDATE tasks SET list_id = ?1 WHERE id = ?2",
                params![list_id, id],
            )
            .map_err(crate::db_error::map_rusqlite)?;
        if n == 0 {
            return Err(DbOpError::Fatal("task missing".into()));
        }
        Ok(())
    })
}

#[tauri::command]
pub fn reschedule_task(id: String, start: Option<i64>, end: Option<i64>) -> Result<(), String> {
    let out = with_db_err(|conn| {
        let mut tasks = load_tasks(conn)?;
        let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
            return Err(DbOpError::Fatal("task missing".into()));
        };
        match (start, end) {
            (None, None) => clear_schedule(task),
            (Some(s), Some(e)) => {
                let (s, e) = align_range(s, e);
                task.start = Some(s);
                task.end = Some(e);
            }
            _ => return Err(DbOpError::Rejected("need_start_and_end".into())),
        }
        if task.start.is_none() && task.repeat != RepeatRule::None {
            return Err(DbOpError::Rejected("repeat_needs_schedule".into()));
        }
        let stored = task.clone();
        persist_task(conn, &stored)?;
        Ok(())
    });
    if out.is_ok() {
        ping_task_notifications();
    }
    out
}

#[tauri::command]
pub fn set_quests(quests: Vec<QuestDraft>) -> Result<(), String> {
    with_db(|conn| {
        let day = day_str_for_ts(now_secs());
        if !sampling_allowed(conn, &day)? {
            return Err(DbOpError::Fatal("day ended".into()));
        }
        save_quests_for_day(conn, &day, quests, now_secs())
    })
}

#[tauri::command]
pub fn continue_previous_workday() -> Result<String, String> {
    with_db(|conn| {
        let day = day_str_for_ts(now_secs());
        if !sampling_allowed(conn, &day)? {
            return Err(DbOpError::Fatal("day ended".into()));
        }
        continue_previous_workday_for_day(conn, &day, now_secs())
    })
}

#[tauri::command]
pub fn review_slot(day: String, slot_start: i64, category: String) -> Result<(), String> {
    let retention = retention_from_str(&load_settings().screenshot_retention);
    with_db(|conn| review_pending_slot(conn, &day, slot_start, &category, retention))
}

pub fn report_misclassification_db(
    conn: &Connection,
    day: &str,
    slot_start: i64,
    note: &str,
    ts: i64,
) -> Result<(), DbOpError> {
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM slots WHERE day = ?1 AND slot_start = ?2",
            params![day, slot_start],
            |r| r.get(0),
        )
        .optional()
        .map_err(crate::db_error::map_rusqlite)?;
    let status = status.ok_or_else(|| DbOpError::Fatal("slot missing".into()))?;
    if status != "final" && status != "unknown" {
        return Err(DbOpError::Fatal("slot not final".into()));
    }
    conn.execute(
        "INSERT INTO misclassification_reports (day, slot_start, note, ts) VALUES (?1, ?2, ?3, ?4)",
        params![day, slot_start, note, ts],
    )
    .map_err(crate::db_error::map_rusqlite)?;
    Ok(())
}

#[tauri::command]
pub fn report_misclassification(day: String, slot_start: i64, note: String) -> Result<(), String> {
    with_db(|conn| report_misclassification_db(conn, &day, slot_start, &note, now_secs()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub accessibility: bool,
    pub screen_recording: bool,
    pub process_name: String,
    pub process_path: String,
}

/// What the running platform can do, so the UI stops guessing from the user
/// agent — a Linux app under XWayland looks like a working X11 session and is
/// not one.
#[tauri::command]
pub fn observation_status() -> crate::observe::ObservationStatus {
    crate::observe::status()
}

/// macOS-only: which system permissions this process has been granted.
#[tauri::command]
pub fn get_permission_status() -> PermissionStatus {
    PermissionStatus {
        accessibility: macos::accessibility_granted(),
        screen_recording: macos::screen_recording_granted(),
        process_name: macos::current_process_label(),
        process_path: macos::current_process_path(),
    }
}

fn load_wish_for_redeem(conn: &Connection, wish_id: &str) -> Result<(Wish, String), DbOpError> {
    let row = conn
        .query_row(
            "SELECT id, name, kind, price, duration_minutes, COALESCE(archived, 0)
             FROM wishes WHERE id = ?1",
            params![wish_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(crate::db_error::map_rusqlite)?
        .ok_or_else(|| DbOpError::Rejected("wish_missing".into()))?;
    let (id, name, kind, price, duration, archived) = row;
    if archived != 0 {
        return Err(DbOpError::Rejected("wish_archived".into()));
    }
    let wish = Wish {
        id,
        kind: if kind == "coin" {
            WishKind::Coin
        } else {
            WishKind::Xp {
                duration_minutes: duration,
            }
        },
        price,
    };
    Ok((wish, name))
}

#[tauri::command]
pub fn request_screen_recording() -> bool {
    macos::request_screen_recording()
}

#[tauri::command]
pub fn open_privacy_settings(kind: String) -> Result<(), String> {
    // These are `x-apple.systempreferences:` URLs — there is no pane to
    // deep-link to and no permission to grant off macOS, so say so rather than
    // shell out to an opener that cannot do anything with the URL.
    if !cfg!(target_os = "macos") {
        return Err("privacy settings: macOS only".into());
    }
    let url = match kind.as_str() {
        "accessibility" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        "screen" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
        _ => return Err("unknown pane".into()),
    };
    crate::platform::open_url(url)
}

#[tauri::command]
pub fn redeem(wish_id: String, redemption_id: String) -> Result<(), String> {
    with_db_err(|conn| {
        let day = day_str_for_ts(now_secs());
        let credited_today: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(credited_core_seconds), 0) FROM slots
                 WHERE day = ?1 AND status = 'final'",
                params![day],
                |r| r.get(0),
            )
            .map_err(crate::db_error::map_rusqlite)?;
        let (wish, name) = load_wish_for_redeem(conn, &wish_id)?;
        db_redeem(
            conn,
            credited_today,
            &day,
            &wish,
            &name,
            now_secs(),
            &redemption_id,
        )
    })
}

#[tauri::command]
pub fn create_wish(
    id: String,
    name: String,
    kind: String,
    price: i64,
    duration_minutes: Option<i64>,
) -> Result<(), String> {
    with_db_err(|conn| db_insert_wish(conn, &id, &name, &kind, price, duration_minutes))
}

#[tauri::command]
pub fn update_wish(
    id: String,
    name: String,
    price: i64,
    duration_minutes: Option<i64>,
) -> Result<(), String> {
    with_db_err(|conn| db_update_wish(conn, &id, &name, price, duration_minutes))
}

#[tauri::command]
pub fn archive_wish(id: String) -> Result<(), String> {
    with_db_err(|conn| db_archive_wish(conn, &id))
}

#[tauri::command]
pub fn end_today(_pause: State<'_, PauseControl>) -> Result<(), String> {
    run_end_today()
}

#[tauri::command]
pub fn freeze(protected_date: String) -> Result<(), String> {
    with_db(|conn| crate::scheduler::freeze_day(conn, &protected_date, now_secs()))
}

#[tauri::command]
pub fn get_settings() -> Result<AppSettings, String> {
    Ok(load_settings())
}

#[tauri::command]
pub fn save_settings(settings: AppSettings, update_policy: Option<bool>) -> Result<(), String> {
    let mut settings = settings;
    crate::config::normalize_vision_providers(&mut settings);
    let prev = load_settings();
    write_settings_file(&settings)?;
    if !prev.task_notifications && settings.task_notifications {
        crate::macos::request_authorization();
    }
    crate::macos::apply_login_at_startup(settings.login_at_startup);
    ping_task_notifications();
    if !update_policy.unwrap_or(true) {
        return Ok(());
    }
    with_db(|conn| {
        let json = crate::config::policy_snapshot_json(&settings);
        let ts = now_secs();
        conn.execute(
            "INSERT INTO policy_versions (json, created_at) VALUES (?1, ?2)",
            params![json, ts],
        )
        .map_err(crate::db_error::map_rusqlite)?;
        Ok(())
    })
}

#[tauri::command]
pub fn set_api_key(key: String) -> Result<(), String> {
    set_openai_api_key(&key)
}

#[tauri::command]
pub fn has_api_key() -> Result<bool, String> {
    let status = provider_key_status()?;
    Ok(status.codex_logged_in || status.keys.values().any(|v| *v))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKeyStatus {
    pub keys: BTreeMap<String, bool>,
    pub codex_logged_in: bool,
}

#[tauri::command]
pub fn set_provider_api_key(provider: String, key: String) -> Result<(), String> {
    let id = provider.trim();
    if id.is_empty() {
        return Err("unknown provider".into());
    }
    if id == crate::config::PROVIDER_CODEX || id == crate::config::PROVIDER_OPENAI {
        return Err("Codex 使用本机命令行授权，不需要 API Key".into());
    }
    write_provider_api_key(id, &key)
}

#[tauri::command]
pub fn provider_key_status() -> Result<ProviderKeyStatus, String> {
    let settings = load_settings();
    let mut keys = BTreeMap::new();
    for p in &settings.vision_providers {
        if crate::config::is_codex_provider(p) {
            continue;
        }
        keys.insert(p.id.clone(), get_provider_api_key(&p.id).is_ok());
    }
    Ok(ProviderKeyStatus {
        keys,
        codex_logged_in: crate::codex_auth::logged_in(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTestResult {
    pub ok: bool,
    pub preview: String,
}

/// `async` on purpose: Tauri runs sync commands on the main thread, so a
/// blocking HTTP call in one freezes the webview for its whole duration.
/// `reqwest::blocking` must still run on `spawn_blocking` — calling it on
/// the Tokio worker deadlocks and the 设置 button never comes back.
#[tauri::command]
pub async fn test_vision_provider(provider: Option<String>) -> Result<ProviderTestResult, String> {
    tauri::async_runtime::spawn_blocking(move || test_vision_provider_blocking(provider))
        .await
        .map_err(|e| e.to_string())?
}

fn test_vision_provider_blocking(provider: Option<String>) -> Result<ProviderTestResult, String> {
    let settings = load_settings();
    let id = provider
        .unwrap_or_else(|| settings.primary_provider.clone())
        .trim()
        .to_string();
    let spec = settings
        .vision_providers
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or_else(|| "未找到该提供商配置".to_string())?;
    let Some(endpoint) = crate::vision::endpoint_from_provider(
        &spec,
        &|pid: &str| get_provider_api_key(pid).ok(),
        &crate::codex_auth::load_session(),
    ) else {
        if crate::config::is_codex_provider(&spec) {
            return Err("本机还没有 Codex 登录。请在终端运行 codex login 后点刷新。".into());
        }
        if spec.base_url.trim().is_empty() || spec.model.trim().is_empty() {
            return Err("请先填写 Base URL 和模型".into());
        }
        return Err("还没有保存 API Key".into());
    };
    match crate::text_ai::call_provider_test(&endpoint) {
        Ok(body) => Ok(ProviderTestResult {
            ok: true,
            preview: crate::text_ai::preview_provider_reply(&body),
        }),
        Err(e) => Err(crate::text_ai::text_ai_error_message(e).into()),
    }
}

// ---- Cloud backup ---------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusView {
    pub enabled: bool,
    pub target: String,
    pub url: String,
    pub scope: String,
    pub interval_minutes: i64,
    pub last_at: Option<i64>,
    pub last_ok: bool,
    pub last_error: String,
    pub snapshot_bytes: u64,
    pub device_id: String,
    pub devices: Vec<crate::sync::DeviceEntry>,
}

fn sync_meta(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM app_meta WHERE key = ?1",
        params![key],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

fn sync_status_from(conn: &Connection, settings: &AppSettings) -> Result<SyncStatusView, String> {
    let device_id = crate::sync::device_id(conn).map_err(|e| e.to_string())?;
    let last_at = sync_meta(conn, "sync_last_at").and_then(|s| s.parse::<i64>().ok());
    let last_error = sync_meta(conn, "sync_last_error").unwrap_or_default();
    let outcome: Option<crate::sync::SyncOutcome> =
        sync_meta(conn, "sync_last_result").and_then(|s| serde_json::from_str(&s).ok());

    Ok(SyncStatusView {
        enabled: settings.sync.enabled,
        target: settings.sync.target.clone(),
        url: settings.sync.url.clone(),
        scope: crate::config::normalize_scope(&settings.sync.scope).to_string(),
        interval_minutes: settings.sync.interval_minutes,
        last_at,
        last_ok: last_at.is_some() && last_error.is_empty(),
        last_error,
        snapshot_bytes: outcome.as_ref().map(|o| o.snapshot_bytes).unwrap_or(0),
        device_id,
        devices: outcome.map(|o| o.devices).unwrap_or_default(),
    })
}

fn sync_conn() -> Result<Connection, String> {
    let path = crate::db::app_db_path().ok_or_else(|| "home dir".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create db dir: {e}"))?;
    }
    let conn = open(&path).map_err(|e| format!("{e:?}"))?;
    migrate(&conn).map_err(|e| format!("{e:?}"))?;
    Ok(conn)
}

/// Cached status only. Opening 设置 must not hit the network.
#[tauri::command]
pub fn sync_status() -> Result<SyncStatusView, String> {
    let settings = load_settings();
    let conn = sync_conn()?;
    sync_status_from(&conn, &settings)
}

#[tauri::command]
pub async fn sync_now_cmd() -> Result<SyncStatusView, String> {
    tauri::async_runtime::spawn_blocking(sync_now_blocking)
        .await
        .map_err(|e| e.to_string())?
}

fn sync_now_blocking() -> Result<SyncStatusView, String> {
    let settings = load_settings();
    crate::sync::sync_now(&settings.sync).map_err(|e| e.to_string())?;
    let conn = sync_conn()?;
    sync_status_from(&conn, &settings)
}

/// Writes and deletes a probe object: that proves the credentials, the write
/// permission and the path all work, which a read-only probe would not.
#[tauri::command]
pub async fn sync_test_connection() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(sync_test_connection_blocking)
        .await
        .map_err(|e| e.to_string())?
}

fn sync_test_connection_blocking() -> Result<String, String> {
    use crate::sync::SyncError;

    let settings = load_settings();
    let target = crate::sync::target_from_settings(&settings.sync).map_err(|e| e.to_string())?;
    let base = crate::sync::base_dir(&settings.sync);
    let probe = format!("{base}/.gamelife-probe");

    match target.put(&probe, b"ok") {
        Ok(()) => {}
        Err(SyncError::Remote(404)) => {
            return Err(format!("远端目录不存在，请先创建 {base}"));
        }
        Err(SyncError::Auth) => return Err("远端认证失败，请检查账号与凭据".into()),
        Err(e) => return Err(e.to_string()),
    }
    let _ = target.delete(&probe);
    Ok(format!("连接正常，可写入 {base}"))
}

#[tauri::command]
pub fn sync_set_credentials(password: String) -> Result<(), String> {
    let settings = load_settings();
    let slot = if settings.sync.target == "s3" {
        "sync-s3-secret"
    } else {
        "sync-webdav-password"
    };
    let path = crate::keychain::secrets_path()?;
    crate::keychain::set_in(&path, slot, &password)
}

#[tauri::command]
pub async fn sync_list_devices() -> Result<Vec<crate::sync::DeviceEntry>, String> {
    tauri::async_runtime::spawn_blocking(sync_list_devices_blocking)
        .await
        .map_err(|e| e.to_string())?
}

fn sync_list_devices_blocking() -> Result<Vec<crate::sync::DeviceEntry>, String> {
    use crate::sync::SyncError;

    let settings = load_settings();
    let target = crate::sync::target_from_settings(&settings.sync).map_err(|e| e.to_string())?;
    let path = format!("{}/devices.json", crate::sync::base_dir(&settings.sync));
    match target.get(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("devices.json: {e}")),
        Err(SyncError::Remote(404)) => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

/// Stages the restored database next to the live one. It deliberately does not
/// swap it in — the sampler must not have its database replaced underneath it.
#[tauri::command]
pub async fn sync_restore(device_id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || sync_restore_blocking(device_id))
        .await
        .map_err(|e| e.to_string())?
}

fn sync_restore_blocking(device_id: String) -> Result<String, String> {
    let settings = load_settings();
    let dir = crate::platform::app_support_dir().ok_or_else(|| "home dir".to_string())?;
    let path = crate::sync::restore_from_target(&settings.sync, &device_id, &dir)
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{archive_wish, insert_ledger, insert_wish, migrate};
    use gamelife_core::ListRole;

    #[test]
    fn create_list_requires_strict_role() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        assert!(parse_list_role_strict("custom").is_none());
        assert_eq!(parse_list_role_strict("mainline"), Some(ListRole::Mainline));
    }

    #[test]
    fn delete_task_removes_row() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tasks (id, list_id, title, done, start, end)
             VALUES ('t1','list-mainline','x',0, NULL, NULL)",
            [],
        )
        .unwrap();
        let n = conn
            .execute("DELETE FROM tasks WHERE id = ?1", params!["t1"])
            .unwrap();
        assert_eq!(n, 1);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    fn sample_view(id: &str, title: &str) -> TaskView {
        TaskView {
            id: id.into(),
            list_id: PRESET_MAINLINE_ID.into(),
            title: title.into(),
            done: false,
            start: None,
            end: None,
            range: None,
            sort: 0,
            repeat: "none".into(),
            remind_offsets: vec![],
            notes: String::new(),
        }
    }

    #[test]
    fn completing_daily_inserts_one_future() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let today = start_of_named_day("2026-09-15").expect("named day");
        let now = today + 10 * 3600;
        persist_task(
            &conn,
            &Task {
                id: "a".into(),
                list_id: PRESET_MAINLINE_ID.into(),
                title: "写方法节".into(),
                done: false,
                start: Some(today - 86400 + 10 * 3600),
                end: Some(today - 86400 + 11 * 3600),
                range: None,
                sort: 0,
                repeat: RepeatRule::Daily,
                remind_offsets: vec![],
                notes: "指标".into(),
            },
        )
        .unwrap();
        toggle_task_done_in(&conn, "a", true, now).unwrap();
        let tasks = load_tasks(&conn).unwrap();
        let old = tasks.iter().find(|t| t.id == "a").unwrap();
        assert!(old.done);
        let spawned = tasks.iter().find(|t| t.id != "a").expect("spawned next");
        assert!(!spawned.done);
        assert!(spawned.start.unwrap() >= today);
        assert_eq!(spawned.repeat, RepeatRule::Daily);
        assert_eq!(spawned.notes, "指标");

        toggle_task_done_in(&conn, "a", false, now).unwrap();
        let after = load_tasks(&conn).unwrap();
        assert_eq!(after.len(), 2);
        assert!(!after.iter().find(|t| t.id == "a").unwrap().done);
    }

    #[test]
    fn upsert_more_than_twenty_open_tasks_ok() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        for i in 0..21 {
            upsert_task_in(&conn, sample_view(&format!("t{i}"), "x")).unwrap();
        }
        assert_eq!(load_tasks(&conn).unwrap().len(), 21);
    }

    #[test]
    fn upsert_rejects_notes_over_8192_bytes() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let mut ok = sample_view("t1", "x");
        ok.notes = "a".repeat(8192);
        upsert_task_in(&conn, ok).unwrap();
        let mut bad = sample_view("t2", "y");
        bad.notes = "a".repeat(8193);
        let err = upsert_task_in(&conn, bad).unwrap_err();
        assert_eq!(err, DbOpError::Rejected("notes_too_long".into()));
    }

    #[test]
    fn duplicate_task_copies_notes() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let mut src = sample_view("src", "x");
        src.notes = "地点：A301".into();
        upsert_task_in(&conn, src).unwrap();
        let copy = duplicate_task_in(&conn, "src").unwrap();
        assert_ne!(copy.id, "src");
        assert_eq!(copy.notes, "地点：A301");
        assert!(!copy.done);
    }

    #[test]
    fn load_wish_for_redeem_rejects_archived_and_missing() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        insert_wish(&conn, "w1", "视频", "xp", 10, Some(30)).unwrap();
        archive_wish(&conn, "w1").unwrap();

        let archived = load_wish_for_redeem(&conn, "w1").unwrap_err();
        assert_eq!(archived, DbOpError::Rejected("wish_archived".into()));
        assert_eq!(map_db_err(archived), "wish_archived");

        let missing = load_wish_for_redeem(&conn, "missing").unwrap_err();
        assert_eq!(missing, DbOpError::Rejected("wish_missing".into()));
        assert_eq!(map_db_err(missing), "wish_missing");
    }

    #[test]
    fn report_misclassification_does_not_change_ledger_sum() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        let slot_start = 1_700_000_000i64;
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, credited_core_seconds, observed_seconds)
             VALUES (?1, ?2, 'final', 900, 900)",
            params![day, slot_start],
        )
        .unwrap();
        insert_ledger(&conn, "validated_coin:2026-09-10:1", day, 5, 10).unwrap();
        insert_ledger(&conn, "validated_coin:2026-09-10:2", day, 3, 0).unwrap();
        let sum_before: i64 = conn
            .query_row("SELECT COALESCE(SUM(coin_delta), 0) FROM ledger", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(sum_before, 8);

        report_misclassification_db(&conn, day, slot_start, "wrong category", 99).unwrap();

        let sum_after: i64 = conn
            .query_row("SELECT COALESCE(SUM(coin_delta), 0) FROM ledger", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(sum_after, sum_before);
        let reports: i64 = conn
            .query_row("SELECT COUNT(*) FROM misclassification_reports", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(reports, 1);
    }

    #[test]
    fn build_today_accepts_open_slot_with_null_credits() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_status)
             VALUES ('2026-09-11', 1789091100, 'Scheduled')",
            [],
        )
        .unwrap();
        let view = build_today(&conn, "2026-09-11", 1_789_091_100)
            .expect("open slot must not fail get_today");
        assert_eq!(view.slots.len(), 1);
        assert_eq!(view.slots[0].credited_minutes, 0);
        assert!(!view.slots[0].is_final);
        assert!(!view.rewards_pending);
    }

    #[test]
    fn build_today_deferred_previews_unpaid_rewards() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        crate::db::set_settlement_mode(&conn, crate::db::SettlementMode::Deferred).unwrap();
        let day = "2026-09-11";
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, credited_core_seconds, category)
             VALUES (?1, 1789091100, 'final', 900, 'core_research')",
            params![day],
        )
        .unwrap();
        let view = build_today(&conn, day, 1_789_091_100).unwrap();
        assert!(view.rewards_pending);
        assert_eq!(view.coins_today, 1);
        assert_eq!(view.xp_today, 10);
        assert_eq!(view.coin_balance, 0);
    }

    #[test]
    fn build_today_immediate_reads_ledger_not_a_preview() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, credited_core_seconds, category)
             VALUES (?1, 1789091100, 'final', 900, 'core_research')",
            params![day],
        )
        .unwrap();
        insert_ledger(&conn, "validated_coin:2026-09-11:1", day, 1, 0).unwrap();
        let view = build_today(&conn, day, 1_789_091_100).unwrap();
        assert!(!view.rewards_pending);
        assert_eq!(view.coins_today, 1);
        assert_eq!(view.xp_today, 0);
    }

    #[test]
    fn build_week_reads_activity_from_the_stats_conn() {
        let live = Connection::open_in_memory().unwrap();
        migrate(&live).unwrap();
        let stats = Connection::open_in_memory().unwrap();
        migrate(&stats).unwrap();
        let day = "2026-09-11";
        let day_start = start_of_named_day(day).unwrap();
        let json = r#"{"core":1800,"support":0,"admin":0,"side":0,"distraction":0,"away":0,"unobserved":0}"#;
        stats
            .execute(
                "INSERT INTO slots (day, slot_start, status, observed_seconds, activity_json)
                 VALUES (?1, ?2, 'final', 1800, ?3)",
                params![day, day_start + 10 * 3600, json],
            )
            .unwrap();
        let view = build_week_from(&live, day, day, day_start + 10 * 3600, Some(&stats)).unwrap();
        assert_eq!(view.core, 30);
        let empty = build_week(&live, day, day, day_start + 10 * 3600).unwrap();
        assert_eq!(empty.core, 0);
    }

    #[test]
    fn build_day_view_includes_overlapping_tasks_and_slots() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        let day_start = start_of_named_day(day).unwrap();
        persist_task(
            &conn,
            &Task {
                id: "t1".into(),
                list_id: PRESET_MAINLINE_ID.into(),
                title: "HDP".into(),
                done: false,
                start: Some(day_start + 9 * 3600),
                end: Some(day_start + 11 * 3600),
                range: None,
                sort: 0,
                repeat: RepeatRule::None,
                remind_offsets: vec![],
            notes: String::new(),
            },
        )
        .unwrap();
        persist_task(
            &conn,
            &Task {
                id: "t2".into(),
                list_id: PRESET_MAINLINE_ID.into(),
                title: "inbox".into(),
                done: false,
                start: None,
                end: None,
                range: None,
                sort: 0,
                repeat: RepeatRule::None,
                remind_offsets: vec![],
            notes: String::new(),
            },
        )
        .unwrap();
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, credited_core_seconds)
             VALUES (?1, ?2, 'pending_review', 0)",
            params![day, day_start + 9 * 3600],
        )
        .unwrap();
        let view = build_day_view(&conn, day).unwrap();
        assert_eq!(view.tasks.len(), 1);
        assert_eq!(view.tasks[0].title, "HDP");
        assert_eq!(view.slots.len(), 1);
        assert!(view.slots[0].pending);
        let err = build_day_view(&conn, "not-a-day").err().expect("bad day");
        match err {
            DbOpError::Rejected(code) => assert_eq!(code, "bad_day"),
            other => panic!("expected bad_day, got {other:?}"),
        }
    }

    #[test]
    fn build_week_accepts_open_slot_with_null_observed() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO slots (day, slot_start, capture_status)
             VALUES ('2026-09-11', 1789091100, 'Scheduled')",
            [],
        )
        .unwrap();
        let view = build_week(&conn, "2026-09-11", "2026-09-11", 1_789_091_100)
            .expect("open slot must not fail get_week");
        assert_eq!(view.unobserved, 0);
        assert_eq!(view.by_day.len(), 7);
        assert_eq!(view.by_hour.len(), 24);
    }

    /// 2026-09-07 is a Monday and 2026-09-13 the Sunday that closes it.
    #[test]
    fn build_week_anchor_selects_that_week_only() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let json = r#"{"core":1800,"support":0,"admin":0,"side":0,"distraction":0,"away":0,"unobserved":0}"#;
        for day in ["2026-09-09", "2026-09-16"] {
            let start = start_of_named_day(day).unwrap() + 10 * 3600;
            conn.execute(
                "INSERT INTO slots (day, slot_start, status, observed_seconds, activity_json)
                 VALUES (?1, ?2, 'final', 1800, ?3)",
                params![day, start, json],
            )
            .unwrap();
        }

        // Today sits in the *later* week; the anchor must still pick the earlier one.
        let earlier = build_week(&conn, "2026-09-09", "2026-09-16", 1_789_500_000).unwrap();
        assert_eq!(earlier.core, 30);
        assert_eq!(earlier.by_day.first().unwrap().day, "2026-09-07");
        assert_eq!(earlier.by_day.last().unwrap().day, "2026-09-13");

        let later = build_week(&conn, "2026-09-16", "2026-09-16", 1_789_500_000).unwrap();
        assert_eq!(later.core, 30);
        assert_eq!(later.by_day.first().unwrap().day, "2026-09-14");
    }

    /// The current week stops at today so a future day is never a missed day;
    /// once the week is over the same anchor sees all seven days.
    #[test]
    fn build_week_current_week_stops_at_today() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let json = r#"{"core":28800,"support":0,"admin":0,"side":0,"distraction":0,"away":0,"unobserved":0}"#;
        let saturday = "2026-09-12";
        let start = start_of_named_day(saturday).unwrap() + 10 * 3600;
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, observed_seconds, activity_json)
             VALUES (?1, ?2, 'final', 28800, ?3)",
            params![saturday, start, json],
        )
        .unwrap();

        // Today is Friday 2026-09-11: the Saturday is still in the future.
        let live = build_week(&conn, "2026-09-11", "2026-09-11", 1_789_400_000).unwrap();
        assert_eq!(live.core, 0);

        // Same week, read after it finished.
        let whole = build_week(&conn, "2026-09-11", "2026-09-20", 1_789_400_000).unwrap();
        assert_eq!(whole.core, 480);
    }

    #[test]
    fn build_week_aggregates_activity_by_day_and_hour() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        let day_start = start_of_named_day(day).unwrap();
        let slot_start = day_start + 10 * 3600;
        let json = r#"{"core":1800,"support":0,"admin":600,"side":900,"distraction":0,"away":0,"unobserved":0}"#;
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, observed_seconds, activity_json)
             VALUES (?1, ?2, 'final', 3600, ?3)",
            params![day, slot_start, json],
        )
        .unwrap();
        let view = build_week(&conn, day, day, slot_start).unwrap();
        assert_eq!(view.core, 30);
        assert_eq!(view.side, 15);
        assert_eq!(view.admin, 10);
        let friday = view
            .by_day
            .iter()
            .find(|d| d.day == day)
            .expect("friday row");
        assert_eq!(friday.core, 30);
        assert_eq!(friday.side, 15);
        assert_eq!(friday.chore, 10);
        let hour = view.by_hour.iter().find(|h| h.hour == 10).expect("10:00");
        assert_eq!(hour.core, 1800);
        assert_eq!(hour.observed, 3600);
        assert!(view.core_label.contains("30m"));
    }

    #[test]
    fn load_redemptions_reports_kind_spent_and_status() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        insert_wish(&conn, "w1", "奶茶", "coin", 32, None).unwrap();
        insert_wish(&conn, "w2", "B站", "xp", 20, Some(45)).unwrap();
        conn.execute(
            "INSERT INTO redemptions (redemption_id, wish_id, ts, name, duration_minutes)
             VALUES ('r1', 'w1', 10, '奶茶', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ledger (reward_event_key, day, ts, coin_delta, xp_delta)
             VALUES ('shop_spend:r1', '2026-09-11', 10, -32, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO redemptions (redemption_id, wish_id, ts, name, duration_minutes)
             VALUES ('r2', 'w2', 20, 'B站', 45)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ledger (reward_event_key, day, ts, coin_delta, xp_delta)
             VALUES ('shop_spend:r2', '2026-09-11', 20, 0, -20)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entertainment_sessions (redemption_id, wish_id, name, started_at, ends_at)
             VALUES ('r2', 'w2', 'B站', 20, 100)",
            [],
        )
        .unwrap();
        let rows = load_redemptions(&conn, 50).unwrap();
        assert_eq!(rows[0].name, "B站");
        assert_eq!(rows[0].kind, "energy");
        assert_eq!(rows[0].spent, 20);
        assert_eq!(rows[0].status, "进行中");
        assert_eq!(rows[1].kind, "coin");
        assert_eq!(rows[1].spent, 32);
        assert_eq!(rows[1].status, "—");
        let ended = load_redemptions(&conn, 200).unwrap();
        assert_eq!(ended[0].status, "已结束");
    }

    #[test]
    fn load_redemptions_keeps_the_last_30() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        insert_wish(&conn, "w1", "奶茶", "coin", 32, None).unwrap();
        for i in 1..=31 {
            conn.execute(
                "INSERT INTO redemptions (redemption_id, wish_id, ts, name, duration_minutes)
                 VALUES (?1, 'w1', ?2, '奶茶', NULL)",
                params![format!("r{i}"), i],
            )
            .unwrap();
        }
        let rows = load_redemptions(&conn, 0).unwrap();
        assert_eq!(rows.len(), 30);
        assert_eq!(rows[0].id, "r31");
        assert_eq!(rows[29].id, "r2");
        assert!(rows.iter().all(|r| r.id != "r1"));
    }

    #[test]
    fn permission_status_reports_process_identity() {
        let status = get_permission_status();
        assert!(!status.process_name.is_empty());
        assert!(!status.process_path.is_empty());
    }

    #[test]
    fn today_live_matches_latest_sample() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        save_quests_for_day(
            &conn,
            day,
            vec![QuestDraft {
                text: "HDP".into(),
                evidence: vec!["HDP".into()],
                hero: true,
            }],
            1,
        )
        .unwrap();
        crate::sampler::insert_sample(
            &conn,
            10,
            day,
            "Cursor",
            "train.py — HDP",
            None,
            None,
            None,
            1,
            false,
            false,
            false,
        )
        .unwrap();
        let view = build_today(&conn, day, 10).unwrap();
        assert_eq!(view.quests[0].text, "HDP");
        assert_eq!(view.live.as_ref().unwrap().matched_quest_index, Some(0));
        assert!(view.live.as_ref().unwrap().trusted);
    }

    #[test]
    fn today_live_null_without_samples() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let view = build_today(&conn, "2026-09-11", 0).unwrap();
        assert!(view.live.is_none());
        assert!(view.previous_workday.is_none());
    }

    #[test]
    fn weekend_today_is_not_at_risk() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO days (day, settled_at, outcome) VALUES ('2026-09-11', 1, 'completed')",
            [],
        )
        .unwrap();
        let view = build_today(&conn, "2026-09-13", 0).unwrap();
        assert!(!view.at_risk);
        assert_eq!(view.streak, 1);
    }

    #[test]
    fn tray_appends_entertainment_minutes() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-10";
        insert_ledger(&conn, "validated_xp:2026-09-10:1", day, 0, 50).unwrap();
        let now = 1_700_000_000i64;
        let wish = Wish {
            id: "video".into(),
            kind: WishKind::Xp {
                duration_minutes: Some(30),
            },
            price: 10,
        };
        crate::db::redeem(&mut conn, 3600, day, &wish, "视频", now, "r1").unwrap();
        let label = tray_tooltip_for_today(&conn, day, now + 12 * 60).unwrap();
        assert!(label.ends_with(" · 视频 18m"), "{label}");
        let label_done = tray_tooltip_for_today(&conn, day, now + 30 * 60).unwrap();
        assert!(!label_done.contains("视频"), "{label_done}");
    }

    #[test]
    fn build_week_mixed_slot_keeps_core_and_side() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        let day_start = start_of_named_day(day).unwrap();
        let json = r#"{"core":480,"support":0,"admin":0,"side":420,"distraction":0,"away":0,"unobserved":0}"#;
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, observed_seconds, activity_json, credited_core_seconds, category)
             VALUES (?1, ?2, 'final', 900, ?3, 480, 'core_research')",
            params![day, day_start + 10 * 3600, json],
        )
        .unwrap();
        let view = build_week(&conn, day, day, day_start + 10 * 3600).unwrap();
        assert_eq!(view.core, 8);
        assert_eq!(view.side, 7);
        assert_ne!(view.core, 15);
        assert!(view.pending_over_resolved.is_finite());
        assert_eq!(view.pending_over_resolved, 0.0);
        assert!(view.wow_core_delta_minutes.is_none());
        assert!((view.core_hours - 480.0 / 3600.0).abs() < 1e-9);
        assert_eq!(view.distraction_observed_ratio, 0.0);
        assert_eq!(view.days_ge_6h, 0);
        assert_eq!(view.days_ge_8h, 0);
    }

    #[test]
    fn build_today_fills_app_top_pending_and_activity() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        let day_start = start_of_named_day(day).unwrap();
        let json = r#"{"core":480,"support":0,"admin":0,"side":420,"distraction":0,"away":0,"unobserved":0}"#;
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, observed_seconds, activity_json, credited_core_seconds)
             VALUES (?1, ?2, 'pending_review', 900, ?3, 0)",
            params![day, day_start + 9 * 3600, json],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_day_stats (day, app, bundle_id, samples, idle_seconds, core, support, admin, side, distraction, away, unobserved, protected)
             VALUES (?1, 'Cursor', '', 32, 0, 480, 0, 0, 0, 0, 0, 0, 0)",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_day_stats (day, app, bundle_id, samples, idle_seconds, core, support, admin, side, distraction, away, unobserved, protected)
             VALUES (?1, 'WeirdApp', '', 4, 0, 0, 0, 0, 0, 60, 0, 0, 0)",
            params![day],
        )
        .unwrap();
        let view = build_today(&conn, day, day_start).unwrap();
        assert_eq!(view.pending_count, 1);
        assert_eq!(view.activity.core, 8);
        assert_eq!(view.activity.side, 7);
        assert_eq!(view.app_top.len(), 2);
        assert_eq!(view.app_top[0].name, "Cursor");
        assert_eq!(view.app_top[0].minutes, 8);
        assert_eq!(view.app_top[0].dominant, "core");
    }

    #[test]
    fn build_today_omits_lock_screen_from_app_top() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        conn.execute(
            "INSERT INTO app_day_stats (day, app, bundle_id, samples, idle_seconds, core, support, admin, side, distraction, away, unobserved, protected)
             VALUES (?1, 'Cursor', '', 32, 0, 480, 0, 0, 0, 0, 0, 0, 0)",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_day_stats (day, app, bundle_id, samples, idle_seconds, core, support, admin, side, distraction, away, unobserved, protected)
             VALUES (?1, 'loginwindow', 'com.apple.loginwindow', 40, 0, 0, 0, 0, 0, 0, 600, 0, 0)",
            params![day],
        )
        .unwrap();
        let view = build_today(&conn, day, start_of_named_day(day).unwrap()).unwrap();
        assert_eq!(view.app_top.len(), 1);
        assert_eq!(view.app_top[0].name, "Cursor");
    }

    #[test]
    fn build_month_rhythm_and_app_reports() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        let day_start = start_of_named_day(day).unwrap();
        let json = r#"{"core":480,"support":0,"admin":0,"side":420,"distraction":0,"away":0,"unobserved":0}"#;
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, observed_seconds, activity_json, credited_core_seconds, category)
             VALUES (?1, ?2, 'final', 900, ?3, 28800, 'core_research')",
            params![day, day_start + 8 * 3600, json],
        )
        .unwrap();
        for i in 0..3 {
            conn.execute(
                "INSERT INTO slots (day, slot_start, status, observed_seconds, activity_json, credited_core_seconds, category)
                 VALUES (?1, ?2, 'final', 900, ?3, 0, 'distraction')",
                params![
                    day,
                    day_start + 12 * 3600 + i * 900,
                    r#"{"core":0,"support":0,"admin":0,"side":0,"distraction":900,"away":0,"unobserved":0}"#
                ],
            )
            .unwrap();
        }
        insert_ledger(&conn, "coin:1", day, 10, 5).unwrap();
        conn.execute(
            "INSERT INTO ledger (reward_event_key, day, ts, coin_delta, xp_delta)
             VALUES ('shop_spend:r1', ?1, 10, -4, 0)",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO freeze_uses (protected_date) VALUES (?1)",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO days (day, settled_at, outcome) VALUES (?1, 1, 'completed')",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_day_stats (day, app, bundle_id, samples, idle_seconds, core, support, admin, side, distraction, away, unobserved, protected)
             VALUES (?1, 'Cursor', '', 32, 0, 480, 0, 0, 0, 0, 0, 0, 0)",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_day_stats (day, app, bundle_id, samples, idle_seconds, core, support, admin, side, distraction, away, unobserved, protected)
             VALUES (?1, 'Mystery', '', 4, 0, 0, 0, 0, 0, 60, 0, 0, 120)",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_day_stats (day, app, bundle_id, samples, idle_seconds, core, support, admin, side, distraction, away, unobserved, protected)
             VALUES (?1, 'ChatGPT', '', 8, 0, 0, 0, 0, 0, 0, 0, 0, 0)",
            params![day],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO host_day_stats (day, host, samples, core, support, admin, side, distraction)
             VALUES (?1, 'arxiv.org', 10, 150, 0, 0, 0, 0)",
            params![day],
        )
        .unwrap();

        let month = build_month_report(&conn, 2026, 9, day).unwrap();
        assert_eq!(month.activity.core, 8);
        assert_eq!(month.activity.side, 7);
        assert_eq!(month.activity.distraction, 45);
        assert_eq!(month.coins_earned, 10);
        assert_eq!(month.coins_spent, 4);
        assert_eq!(month.xp_earned, 5);
        assert_eq!(month.gold_days, 1);
        assert_eq!(month.freeze_count, 1);
        assert_eq!(month.completed_days, 1);
        let friday = month.days.iter().find(|d| d.day == day).unwrap();
        assert!(!friday.is_weekend);
        assert!(!friday.is_future);
        assert_eq!(friday.credited_core, 28800);
        let saturday = month.days.iter().find(|d| d.day == "2026-09-12").unwrap();
        assert!(saturday.is_weekend);
        let future = month.days.iter().find(|d| d.day == "2026-09-13").unwrap();
        assert!(future.is_future);

        let rhythm = build_rhythm_report(&conn, "week", day, day).unwrap();
        assert_eq!(rhythm.start_hours.len(), 7);
        let fri = rhythm.start_hours.iter().find(|h| h.day == day).unwrap();
        assert_eq!(fri.hour, Some(8));
        assert!((rhythm.rate_8h - 0.2).abs() < 1e-9);
        assert_eq!(rhythm.distraction_run_count, 1);
        assert_eq!(rhythm.distraction_run_slots, 3);
        assert!(!rhythm.peak_hours.is_empty());

        let apps = build_app_report(&conn, "week", day).unwrap();
        assert_eq!(apps.protected_minutes, 2);
        // Only the app whose time went nowhere needs filing: Mystery's seconds were
        // entertainment, which the report now knows even though no list names it.
        let newcomers = &apps.newcomers;
        assert!(newcomers.iter().any(|n| n == "ChatGPT"));
        assert!(!newcomers.iter().any(|n| n == "Mystery"));
        assert!(!newcomers.iter().any(|n| n == "Cursor"));
        // Cursor is in the default policy's trusted_apps, which no longer decides
        // anything, so it is deliberately not labelled 主线 any more.
        let cursor = apps.apps.iter().find(|a| a.name == "Cursor").unwrap();
        assert_eq!(cursor.listed_as, "");
        let mystery = apps.apps.iter().find(|a| a.name == "Mystery").unwrap();
        assert_eq!(mystery.listed_as, "entertainment");
        assert!(!mystery.filed);
        let pol = gamelife_core::default_v01();
        assert_eq!(filed_list_for("1Password", "", &pol), "never_capture");
        assert_eq!(filed_list_for("Cursor", "", &pol), "");
        // Chrome's minutes are distraction seconds, so it reports 娱乐 even though no
        // list contains the app name — and it is not offered as re-filable.
        assert_eq!(ruled_list(0, 0, 45), Some("entertainment"));
        assert_eq!(ruled_list(300, 0, 45), Some("admin"));
        assert_eq!(ruled_list(0, 0, 0), None);
        assert_eq!(cursor.minutes, 8);
        assert_eq!(apps.hosts[0].host, "arxiv.org");
        assert_eq!(apps.hosts[0].minutes, 2);
    }

    #[test]
    fn rhythm_does_not_join_friday_and_monday_distraction_runs() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let friday = "2026-09-11";
        let monday = "2026-09-14";
        let friday_start = start_of_named_day(friday).unwrap();
        let monday_start = start_of_named_day(monday).unwrap();
        let json = r#"{"core":0,"support":0,"admin":0,"side":0,"distraction":900,"away":0,"unobserved":0}"#;
        for i in 0..3 {
            conn.execute(
                "INSERT INTO slots (day, slot_start, status, observed_seconds, activity_json, credited_core_seconds, category)
                 VALUES (?1, ?2, 'final', 900, ?3, 0, 'distraction')",
                params![friday, friday_start + 21 * 3600 + i * 900, json],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO slots (day, slot_start, status, observed_seconds, activity_json, credited_core_seconds, category)
                 VALUES (?1, ?2, 'final', 900, ?3, 0, 'distraction')",
                params![monday, monday_start + 8 * 3600 + i * 900, json],
            )
            .unwrap();
        }
        let rhythm = build_rhythm_report(&conn, "month", monday, monday).unwrap();
        assert_eq!(rhythm.distraction_run_count, 2);
        assert_eq!(rhythm.distraction_run_slots, 6);
        assert_ne!(
            (rhythm.distraction_run_count, rhythm.distraction_run_slots),
            (1, 6),
            "weekend gap must not become one 6-slot run"
        );
    }

    #[test]
    fn build_day_view_overlapping_timed_tasks_become_plan_marks() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let day = "2026-09-11";
        let day_start = start_of_named_day(day).unwrap();
        persist_task(
            &conn,
            &Task {
                id: "tt-a".into(),
                list_id: PRESET_MAINLINE_ID.into(),
                title: "A".into(),
                done: false,
                start: Some(day_start + 10 * 3600),
                end: Some(day_start + 11 * 3600),
                range: None,
                sort: 0,
                repeat: RepeatRule::None,
                remind_offsets: vec![],
            notes: String::new(),
            },
        )
        .unwrap();
        persist_task(
            &conn,
            &Task {
                id: "tt-out".into(),
                list_id: PRESET_MAINLINE_ID.into(),
                title: "Out".into(),
                done: false,
                start: Some(day_start - 5 * 3600),
                end: Some(day_start - 4 * 3600),
                range: None,
                sort: 0,
                repeat: RepeatRule::None,
                remind_offsets: vec![],
            notes: String::new(),
            },
        )
        .unwrap();
        persist_task(
            &conn,
            &Task {
                id: "tt-done".into(),
                list_id: PRESET_MAINLINE_ID.into(),
                title: "Done".into(),
                done: true,
                start: Some(day_start + 12 * 3600),
                end: Some(day_start + 13 * 3600),
                range: None,
                sort: 0,
                repeat: RepeatRule::None,
                remind_offsets: vec![],
            notes: String::new(),
            },
        )
        .unwrap();
        let snapshot = r#"[{"id":"tt-a","title":"A","role":"mainline"}]"#;
        conn.execute(
            "INSERT INTO slots (day, slot_start, status, credited_core_seconds, task_snapshot_json)
             VALUES (?1, ?2, 'pending_review', 0, ?3)",
            params![day, day_start + 10 * 3600, snapshot],
        )
        .unwrap();
        let view = build_day_view(&conn, day).unwrap();
        assert_eq!(view.plan_marks.len(), 1);
        assert_eq!(view.plan_marks[0].title, "A");
        assert_eq!(view.plan_marks[0].start, day_start + 10 * 3600);
        assert_eq!(view.plan_marks[0].end, day_start + 11 * 3600);
        assert_eq!(view.day_tasks.len(), 1);
        assert_eq!(view.day_tasks[0].title, "A");
        assert_eq!(view.day_tasks[0].role, "mainline");
        assert_eq!(view.day_tasks[0].id, "tt-a");
    }
}
