use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use gamelife_core::{
    format_estimated_minutes, judgment_tasks, matched_quest_index, matches_app_identity,
    parse_task_line, sum_activity, validate_lists, xp_shop_unlocked, ListRole, ParseContext, QuestDraft,
    Task, TaskList, TaskRange, CHEST_SECS, GOLD_DAY_SECS, PRESET_MAINLINE_ID,
};
use gamelife_core::shop::{tray_entertainment_minutes, Wish, WishKind};
use gamelife_core::types::ActivitySeconds;

use crate::config::{load_settings, retention_from_str, save_settings as write_settings_file, AppSettings};
use crate::ticktick::TickTickHttp;
use crate::db::{
    archive_wish as db_archive_wish, insert_wish as db_insert_wish, list_role_sql,
    load_active_session, load_task_lists, load_tasks, migrate, open, redeem as db_redeem,
    update_wish as db_update_wish,
};
use crate::db_error::DbOpError;
use crate::keychain::{
    get_openai_api_key, get_provider_api_key, set_openai_api_key,
    set_provider_api_key as write_provider_api_key,
};
use crate::macos;
use crate::sampler::PauseControl;
use crate::scheduler::{
    continue_previous_workday_for_day, day_str_for_ts, default_screenshot_retention,
    list_freeze_candidates, load_policy, load_quests_for_day, previous_quest_day,
    review_pending_slot, sampling_allowed, save_quests_for_day, start_of_named_day, end_of_local_day,
    streak_from_db,
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
    with_db(|conn| {
        crate::scheduler::end_today(conn, now_secs(), default_screenshot_retention())
    })
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayView {
    pub day: String,
    pub day_start: i64,
    pub tasks: Vec<TaskView>,
    pub slots: Vec<TodaySlot>,
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

pub fn tray_tooltip_for_today(
    conn: &Connection,
    day: &str,
    now: i64,
) -> Result<String, DbOpError> {
    let credited_seconds: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(credited_core_seconds), 0) FROM slots
             WHERE day = ?1 AND status = 'final'",
            params![day],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let mut label = format!(
        "{} / 8h",
        format_estimated_minutes(credited_seconds)
    );
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
    Ok(load_active_session(conn, now)?.map(|(name, ends_at, remaining_secs)| EntertainmentView {
        name,
        ends_at,
        remaining_secs,
    }))
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
             ORDER BY r.ts DESC LIMIT 20",
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
            kind: if energy { "energy".into() } else { "coin".into() },
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
            let quest_match = matched_quest_index(
                &title,
                document_path.as_deref(),
                url.as_deref(),
                &quests,
            );
            let trusted = matches_app_identity(
                &app,
                bundle_id.as_deref(),
                &policy.trusted_apps,
            );
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
    let coins_today: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(coin_delta), 0) FROM ledger WHERE day = ?1",
            params![day],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let xp_today: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(xp_delta), 0) FROM ledger WHERE day = ?1",
            params![day],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let chest_need = secs_to_minutes(i64::try_from(CHEST_SECS).unwrap_or(21600));
    let gold_need = secs_to_minutes(i64::try_from(GOLD_DAY_SECS).unwrap_or(28800));
    let streak = streak_from_db(conn)?;
    let settled = conn
        .query_row(
            "SELECT COUNT(*) FROM days WHERE day = ?1 AND settled_at IS NOT NULL",
            params![day],
            |r| r.get::<_, i64>(0),
        )
        .map_err(crate::db_error::map_rusqlite)? > 0;
    let at_risk = !settled
        && streak > 0
        && credited_seconds < i64::try_from(CHEST_SECS).unwrap_or(21600);
    let first_core_label = first_core_label_for_day(conn, day, credited_seconds);
    let slots = load_today_slots(conn, day)?;
    let gold_day = credited_seconds >= i64::try_from(GOLD_DAY_SECS).unwrap_or(28800);
    let freeze_candidates = list_freeze_candidates(conn)?;
    let default_freeze_date = freeze_candidates.first().cloned();
    let active_entertainment = load_active_entertainment(conn, now)?;
    let ended_entertainment =
        load_ended_entertainment(conn, now, active_entertainment.is_some())?;
    let ledger_tail = load_ledger_tail(conn, day)?;
    let lists = task_list_views(conn)?;
    let tasks = task_views(conn)?;
    let coin_balance: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(coin_delta), 0) FROM ledger",
            [],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
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
    })
}

fn first_core_label_for_day(
    conn: &Connection,
    day: &str,
    credited_seconds: i64,
) -> Option<String> {
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

fn build_day_view(conn: &Connection, day: &str) -> Result<DayView, DbOpError> {
    let day_start =
        start_of_named_day(day).ok_or_else(|| DbOpError::Rejected("bad_day".into()))?;
    let day_end = end_of_local_day(day_start);
    Ok(DayView {
        day: day.to_string(),
        day_start,
        tasks: tasks_overlapping_day(conn, day_start, day_end)?,
        slots: load_today_slots(conn, day)?,
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

fn build_week(conn: &Connection, today: &str, now: i64) -> Result<WeekView, DbOpError> {
    let today_date =
        NaiveDate::parse_from_str(today, "%Y-%m-%d").map_err(|e| DbOpError::Fatal(e.to_string()))?;
    let week_start = week_start_for(today_date);
    let week_start_str = week_start.format("%Y-%m-%d").to_string();
    let mut by_day: Vec<WeekDayRow> = (0..7)
        .map(|i| {
            let day = week_start + chrono::Duration::days(i);
            WeekDayRow {
                day: day.format("%Y-%m-%d").to_string(),
                core: 0,
                side: 0,
                chore: 0,
            }
        })
        .collect();
    let mut by_hour: Vec<WeekHourRow> = (0..24)
        .map(|hour| WeekHourRow {
            hour,
            core: 0,
            observed: 0,
        })
        .collect();
    let mut stmt = conn
        .prepare(
            "SELECT day, slot_start, activity_json, status, COALESCE(observed_seconds, 0) FROM slots
             WHERE day >= ?1 AND day <= ?2",
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let rows = stmt
        .query_map(params![week_start_str, today], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .map_err(crate::db_error::map_rusqlite)?;
    let mut activities = Vec::new();
    let mut pending_review_secs = 0i64;
    for row in rows {
        let (day, slot_start, json, status, observed) = row.map_err(crate::db_error::map_rusqlite)?;
        let status = status.unwrap_or_default();
        let activity = parse_activity_json(json);
        if let Some(hour_row) = by_hour.get_mut(local_hour(slot_start) as usize) {
            hour_row.core += activity.core;
            hour_row.observed += observed;
        }
        if status == "pending_review" || status == "unknown" {
            pending_review_secs += observed;
            continue;
        }
        activities.push(activity.clone());
        if let Some(day_row) = by_day.iter_mut().find(|d| d.day == day) {
            day_row.core += activity.core;
            day_row.side += activity.side;
            day_row.chore += activity.admin;
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
        .query_row(
            "SELECT COALESCE(SUM(coin_delta), 0) FROM ledger",
            [],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let wishes = load_wishes(conn)?;
    let active_entertainment = load_active_entertainment(conn, now)?;
    let ended_entertainment =
        load_ended_entertainment(conn, now, active_entertainment.is_some())?;
    let redemptions = load_redemptions(conn, now)?;
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
    })
}

#[tauri::command]
pub fn get_today() -> Result<TodayView, String> {
    with_db(|conn| {
        crate::scheduler::maybe_refresh_ticktick_cache(conn);
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
pub fn get_week() -> Result<WeekView, String> {
    with_db(|conn| {
        let now = now_secs();
        let day = day_str_for_ts(now);
        build_week(conn, &day, now)
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
    }
}

fn persist_task(conn: &Connection, task: &Task) -> Result<(), DbOpError> {
    let range = match task.range {
        Some(TaskRange::Week) => Some("week"),
        Some(TaskRange::Month) => Some("month"),
        None => None,
    };
    conn.execute(
        "INSERT INTO tasks (id, list_id, title, done, start, end, range)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
           list_id=excluded.list_id,
           title=excluded.title,
           done=excluded.done,
           start=excluded.start,
           end=excluded.end,
           range=excluded.range",
        params![
            task.id,
            task.list_id,
            task.title.trim(),
            task.done as i64,
            task.start,
            task.end,
            range,
        ],
    )
    .map_err(crate::db_error::map_rusqlite)?;
    Ok(())
}

#[tauri::command]
pub fn list_tasks() -> Result<TodayView, String> {
    get_today()
}

#[tauri::command]
pub fn upsert_task(task: TaskView) -> Result<(), String> {
    with_db_err(|conn| {
        let title = task.title.trim();
        if title.is_empty() {
            return Err(DbOpError::Rejected("empty_title".into()));
        }
        let mut stored = view_to_task(&task);
        stored.title = title.to_string();
        if stored.id.trim().is_empty() {
            stored.id = format!("task-{}", now_secs());
        }
        let lists = load_task_lists(conn)?;
        let mut tasks = load_tasks(conn)?;
        if let Some(existing) = tasks.iter_mut().find(|t| t.id == stored.id) {
            *existing = stored.clone();
        } else {
            tasks.push(stored.clone());
        }
        let day = day_str_for_ts(now_secs());
        if let Some(day_start) = start_of_named_day(&day) {
            let day_end = end_of_local_day(day_start);
            if let Err(gamelife_core::TaskListError::TooManyJudgment) =
                judgment_tasks(&tasks, &lists, day_start, day_end)
            {
                return Err(DbOpError::Rejected("too_many_judgment_tasks".into()));
            }
        }
        persist_task(conn, &stored)?;
        Ok(())
    })
}

#[tauri::command]
pub fn toggle_task_done(id: String, done: bool) -> Result<(), String> {
    with_db_err(|conn| {
        let n = conn
            .execute(
                "UPDATE tasks SET done = ?1 WHERE id = ?2",
                params![done as i64, id],
            )
            .map_err(crate::db_error::map_rusqlite)?;
        if n == 0 {
            return Err(DbOpError::Fatal("task missing".into()));
        }
        Ok(())
    })
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
pub fn create_list(name: String) -> Result<TaskListView, String> {
    with_db_err(|conn| {
        let name = name.trim();
        if name.is_empty() {
            return Err(DbOpError::Rejected("empty_name".into()));
        }
        let mut lists = load_task_lists(conn)?;
        let sort = lists.iter().map(|l| l.sort).max().unwrap_or(-1) + 1;
        let list = TaskList {
            id: format!("list-{}", now_secs()),
            name: name.to_string(),
            sort,
            role: ListRole::Custom,
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
pub fn save_settings(settings: AppSettings) -> Result<(), String> {
    write_settings_file(&settings)?;
    with_db(|conn| {
        let json = serde_json::json!({
            "trusted_apps": settings.trusted_apps,
            "distraction_rules": settings.distraction_rules,
            "side_project_rules": settings.side_project_rules,
            "reading_apps": settings.reading_apps,
            "never_capture_apps": settings.never_capture_apps,
            "admin_apps": settings.admin_apps,
            "category_guides": settings.category_guides,
        });
        let ts = now_secs();
        conn.execute(
            "INSERT INTO policy_versions (json, created_at) VALUES (?1, ?2)",
            params![json.to_string(), ts],
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
    Ok(get_openai_api_key().is_ok())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKeyStatus {
    pub opencode_go: bool,
    pub openai: bool,
    pub custom: bool,
}

#[tauri::command]
pub fn set_provider_api_key(provider: String, key: String) -> Result<(), String> {
    match provider.as_str() {
        "opencode-go" | "openai" | "custom" => write_provider_api_key(&provider, &key),
        _ => Err("unknown provider".into()),
    }
}

#[tauri::command]
pub fn provider_key_status() -> Result<ProviderKeyStatus, String> {
    Ok(ProviderKeyStatus {
        opencode_go: get_provider_api_key("opencode-go").is_ok(),
        openai: get_openai_api_key().is_ok(),
        custom: get_provider_api_key("custom").is_ok(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickTickStatus {
    pub connected: bool,
    pub last_sync: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickTickAuthorize {
    pub authorize_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickTickProjectView {
    pub id: String,
    pub name: String,
    pub role: String,
}

#[tauri::command]
pub fn ticktick_status() -> Result<TickTickStatus, String> {
    let connected = crate::keychain::get_ticktick_access_token().is_ok();
    let last_sync = with_db(|conn| crate::ticktick::cache_fetched_at_max(conn)).ok().flatten();
    Ok(TickTickStatus {
        connected,
        last_sync,
    })
}

#[tauri::command]
pub fn ticktick_set_client_secret(secret: String) -> Result<(), String> {
    crate::keychain::set_ticktick_client_secret(&secret)
}

#[tauri::command]
pub fn ticktick_begin_oauth() -> Result<TickTickAuthorize, String> {
    let client_id = load_settings().ticktick_client_id.trim().to_string();
    if client_id.is_empty() {
        return Err("missing ticktick client id".into());
    }
    let verifier = crate::ticktick::pkce_verifier();
    let challenge = crate::ticktick::pkce_challenge(&verifier);
    crate::ticktick::store_pkce_verifier(verifier);
    let url = format!(
        "{}?client_id={}&redirect_uri=http%3A%2F%2F127.0.0.1%3A18789%2Fcallback&response_type=code&scope=tasks:read&code_challenge={}&code_challenge_method=S256",
        crate::ticktick::TICKTICK_AUTHORIZE,
        client_id,
        challenge
    );
    Ok(TickTickAuthorize { authorize_url: url })
}

#[tauri::command]
pub fn ticktick_finish_oauth(callback_url: String) -> Result<(), String> {
    let code = crate::ticktick::oauth_code_from_callback(&callback_url)?;
    let verifier = crate::ticktick::take_pkce_verifier().ok_or("missing pkce verifier")?;
    let client_id = load_settings().ticktick_client_id;
    let secret = crate::keychain::get_ticktick_client_secret()?;
    let body = crate::ticktick::ReqwestTickTick.post_form(
        crate::ticktick::TICKTICK_TOKEN,
        &[
            ("client_id", client_id.as_str()),
            ("client_secret", secret.as_str()),
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", crate::ticktick::TICKTICK_REDIRECT),
            ("code_verifier", verifier.as_str()),
        ],
    )?;
    let (access, refresh) = crate::ticktick::parse_token_response(&body)?;
    crate::keychain::set_ticktick_access_token(&access)?;
    if let Some(refresh) = refresh {
        crate::keychain::set_ticktick_refresh_token(&refresh)?;
    }
    Ok(())
}

#[tauri::command]
pub fn ticktick_disconnect() -> Result<(), String> {
    crate::keychain::clear_ticktick_tokens()?;
    with_db(|conn| {
        conn.execute("DELETE FROM ticktick_cache", [])
            .map_err(crate::db_error::map_rusqlite)?;
        Ok(())
    })
}

#[tauri::command]
pub fn ticktick_sync() -> Result<(), String> {
    let now = now_secs();
    let access = crate::keychain::get_ticktick_access_token()?;
    let roles = load_settings().ticktick_project_roles;
    with_db(|conn| {
        if let Some(until) = crate::ticktick::ticktick_backoff_until(conn)? {
            if now < until {
                return Err(DbOpError::Rejected("ticktick_backoff".into()));
            }
        }
        match crate::ticktick::sync_projects(
            &crate::ticktick::ReqwestTickTick,
            conn,
            &roles,
            &access,
            now,
        ) {
            Ok(_) => Ok(()),
            Err(e) if e == "429" => {
                crate::ticktick::set_ticktick_backoff(conn, now + 60)?;
                Err(DbOpError::Rejected("ticktick_429".into()))
            }
            Err(e) => Err(DbOpError::Fatal(e)),
        }
    })
}

#[tauri::command]
pub fn ticktick_list_projects() -> Result<Vec<TickTickProjectView>, String> {
    let access = crate::keychain::get_ticktick_access_token()?;
    let roles = load_settings().ticktick_project_roles;
    let json = crate::ticktick::ReqwestTickTick
        .get_json(
            &format!("{}/project", crate::ticktick::TICKTICK_API),
            Some(&access),
        )
        .map_err(|e| e)?;
    let projects: Vec<serde_json::Value> =
        serde_json::from_str(&json).map_err(|e| format!("projects json: {e}"))?;
    Ok(projects
        .into_iter()
        .filter_map(|p| {
            let id = p.get("id")?.as_str()?.to_string();
            let name = p
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let role = roles.get(&id).cloned().unwrap_or_else(|| "ignore".into());
            Some(TickTickProjectView { id, name, role })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{archive_wish, insert_ledger, insert_wish, migrate};

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
        let view =
            build_today(&conn, "2026-09-11", 1_789_091_100).expect("open slot must not fail get_today");
        assert_eq!(view.slots.len(), 1);
        assert_eq!(view.slots[0].credited_minutes, 0);
        assert!(!view.slots[0].is_final);
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
        let view =
            build_week(&conn, "2026-09-11", 1_789_091_100).expect("open slot must not fail get_week");
        assert_eq!(view.unobserved, 0);
        assert_eq!(view.by_day.len(), 7);
        assert_eq!(view.by_hour.len(), 24);
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
        let view = build_week(&conn, day, slot_start).unwrap();
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
}
