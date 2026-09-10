use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use tauri::State;

use crate::db::{migrate, open};
use crate::db_error::DbOpError;
use crate::sampler::PauseControl;
use crate::scheduler::{default_screenshot_retention, end_today, freeze_day};

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
    with_db(|conn| end_today(conn, now_secs(), default_screenshot_retention()))
}

#[tauri::command]
pub fn end_today_cmd(_pause: State<'_, PauseControl>) -> Result<(), String> {
    run_end_today()
}

#[tauri::command]
pub fn freeze_cmd(protected_date: String) -> Result<(), String> {
    with_db(|conn| freeze_day(conn, &protected_date, now_secs()))
}
