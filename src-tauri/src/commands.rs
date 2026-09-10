use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use gamelife_core::{
    format_estimated_minutes, sum_activity, xp_shop_unlocked, CHEST_SECS, GOLD_DAY_SECS,
};
use gamelife_core::shop::{Wish, WishKind};
use gamelife_core::types::ActivitySeconds;

use crate::db::{migrate, open, redeem as db_redeem};
use crate::db_error::DbOpError;
use crate::keychain::{get_openai_api_key, set_openai_api_key};
use crate::sampler::PauseControl;
use crate::scheduler::{
    app_support_dir, day_str_for_ts, default_screenshot_retention, list_freeze_candidates,
    load_quests_for_day, review_pending_slot, sampling_allowed, streak_from_db,
    ScreenshotRetention,
};

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
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
pub struct TodaySlot {
    pub start: i64,
    pub dominant: String,
    pub credited_minutes: i64,
    pub pending: bool,
    #[serde(rename = "final")]
    pub is_final: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayView {
    pub day: String,
    pub quests: Vec<String>,
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub screenshot_retention: String,
    pub sample_keep_days: i64,
    pub login_at_startup: bool,
    pub trusted_apps: Vec<String>,
    pub distraction_rules: Vec<String>,
    pub side_project_rules: Vec<String>,
    pub reading_apps: Vec<String>,
    pub never_capture_apps: Vec<String>,
}

fn config_path() -> Option<PathBuf> {
    app_support_dir().map(|d| d.join("config.json"))
}

fn default_settings() -> AppSettings {
    AppSettings {
        screenshot_retention: "none".into(),
        sample_keep_days: 7,
        login_at_startup: true,
        trusted_apps: vec![],
        distraction_rules: vec![],
        side_project_rules: vec![],
        reading_apps: vec![],
        never_capture_apps: vec![],
    }
}

fn load_settings() -> AppSettings {
    let path = config_path();
    let Some(path) = path else {
        return default_settings();
    };
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(s) = serde_json::from_str(&data) {
            return s;
        }
    }
    default_settings()
}

fn save_settings_file(settings: &AppSettings) -> Result<(), String> {
    let dir = app_support_dir().ok_or_else(|| "home dir".to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
    let path = dir.join("config.json");
    let json = serde_json::to_string_pretty(settings).map_err(|e| format!("json: {e}"))?;
    fs::write(path, json).map_err(|e| format!("write config: {e}"))?;
    Ok(())
}

fn retention_from_str(s: &str) -> ScreenshotRetention {
    match s {
        "24h" => ScreenshotRetention::Hours24,
        "3d" => ScreenshotRetention::Days3,
        "14d" => ScreenshotRetention::Days14,
        _ => ScreenshotRetention::None,
    }
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

fn week_start_for(date: NaiveDate) -> NaiveDate {
    let offset = date.weekday().num_days_from_monday();
    date - chrono::Duration::days(offset as i64)
}

fn build_today(conn: &Connection, day: &str) -> Result<TodayView, DbOpError> {
    let quests = load_quests_for_day(conn, day)?;
    let quest_texts = quests.iter().map(|q| q.text.clone()).collect();
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
    let mut stmt = conn
        .prepare(
            "SELECT slot_start, category, credited_core_seconds, status
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
            ))
        })
        .map_err(crate::db_error::map_rusqlite)?;
    let mut slots = Vec::new();
    for row in rows {
        let (start, category, credited, status) = row.map_err(crate::db_error::map_rusqlite)?;
        let status = status.unwrap_or_default();
        let pending = status == "pending_review";
        let is_final = status == "final" || status == "unknown";
        slots.push(TodaySlot {
            start,
            dominant: category.unwrap_or_else(|| "unknown".into()),
            credited_minutes: secs_to_minutes(credited),
            pending,
            is_final,
        });
    }
    let gold_day = credited_seconds >= i64::try_from(GOLD_DAY_SECS).unwrap_or(28800);
    let freeze_candidates = list_freeze_candidates(conn)?;
    let default_freeze_date = freeze_candidates.first().cloned();
    Ok(TodayView {
        day: day.to_string(),
        quests: quest_texts,
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
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ledger WHERE reward_event_key = ?1",
            params![key],
            |r| r.get(0),
        )
        .map_err(crate::db_error::map_rusqlite)
        .unwrap_or(0);
    if exists > 0 {
        return Some("Early start".into());
    }
    None
}

fn load_wishes(conn: &Connection) -> Result<Vec<WishView>, DbOpError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, kind, price, duration_minutes FROM wishes ORDER BY name",
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

fn build_week(conn: &Connection, today: &str) -> Result<WeekView, DbOpError> {
    let today_date =
        NaiveDate::parse_from_str(today, "%Y-%m-%d").map_err(|e| DbOpError::Fatal(e.to_string()))?;
    let week_start = week_start_for(today_date);
    let week_start_str = week_start.format("%Y-%m-%d").to_string();
    let mut stmt = conn
        .prepare(
            "SELECT activity_json, status, observed_seconds FROM slots
             WHERE day >= ?1 AND day <= ?2",
        )
        .map_err(crate::db_error::map_rusqlite)?;
    let rows = stmt
        .query_map(params![week_start_str, today], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(crate::db_error::map_rusqlite)?;
    let mut activities = Vec::new();
    let mut pending_review_secs = 0i64;
    for row in rows {
        let (json, status, observed) = row.map_err(crate::db_error::map_rusqlite)?;
        let status = status.unwrap_or_default();
        if status == "pending_review" || status == "unknown" {
            pending_review_secs += observed;
        } else {
            activities.push(parse_activity_json(json));
        }
    }
    let total = sum_activity(&activities);
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
    })
}

#[tauri::command]
pub fn get_today() -> Result<TodayView, String> {
    with_db(|conn| {
        let day = day_str_for_ts(now_secs());
        build_today(conn, &day)
    })
}

#[tauri::command]
pub fn get_week() -> Result<WeekView, String> {
    with_db(|conn| {
        let day = day_str_for_ts(now_secs());
        build_week(conn, &day)
    })
}

#[tauri::command]
pub fn set_quests(quests: Vec<String>) -> Result<(), String> {
    if quests.len() > 3 {
        return Err("at most 3 quests".into());
    }
    with_db(|conn| {
        let day = day_str_for_ts(now_secs());
        if !sampling_allowed(conn, &day)? {
            return Err(DbOpError::Fatal("day ended".into()));
        }
        let json = serde_json::to_string(
            &quests
                .into_iter()
                .map(|text| serde_json::json!({ "text": text, "keywords": [] }))
                .collect::<Vec<_>>(),
        )
        .map_err(|e| DbOpError::Fatal(e.to_string()))?;
        let ts = now_secs();
        conn.execute(
            "INSERT INTO quest_versions (day, json, created_at) VALUES (?1, ?2, ?3)",
            params![day, json, ts],
        )
        .map_err(crate::db_error::map_rusqlite)?;
        Ok(())
    })
}

#[tauri::command]
pub fn review_slot(day: String, slot_start: i64, category: String) -> Result<(), String> {
    let retention = retention_from_str(&load_settings().screenshot_retention);
    with_db(|conn| review_pending_slot(conn, &day, slot_start, &category, retention))
}

#[tauri::command]
pub fn report_misclassification(day: String, slot_start: i64, note: String) -> Result<(), String> {
    with_db(|conn| {
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
            params![day, slot_start, note, now_secs()],
        )
        .map_err(crate::db_error::map_rusqlite)?;
        Ok(())
    })
}

#[tauri::command]
pub fn redeem(wish_id: String, redemption_id: String) -> Result<(), String> {
    with_db(|conn| {
        let day = day_str_for_ts(now_secs());
        let credited_today: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(credited_core_seconds), 0) FROM slots
                 WHERE day = ?1 AND status = 'final'",
                params![day],
                |r| r.get(0),
            )
            .map_err(crate::db_error::map_rusqlite)?;
        let row = conn
            .query_row(
                "SELECT id, kind, price, duration_minutes FROM wishes WHERE id = ?1",
                params![wish_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(crate::db_error::map_rusqlite)?
            .ok_or_else(|| DbOpError::Fatal("wish missing".into()))?;
        let (id, kind, price, duration) = row;
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
        db_redeem(conn, credited_today, &day, &wish, &redemption_id)
    })
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
    save_settings_file(&settings)?;
    with_db(|conn| {
        let json = serde_json::json!({
            "trusted_apps": settings.trusted_apps,
            "distraction_rules": settings.distraction_rules,
            "side_project_rules": settings.side_project_rules,
            "reading_apps": settings.reading_apps,
            "never_capture_apps": settings.never_capture_apps,
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
