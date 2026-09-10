use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::scheduler::{app_support_dir, ScreenshotRetention};

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

pub fn default_settings() -> AppSettings {
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

pub fn load_settings() -> AppSettings {
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

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let dir = app_support_dir().ok_or_else(|| "home dir".to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
    let path = dir.join("config.json");
    let json = serde_json::to_string_pretty(settings).map_err(|e| format!("json: {e}"))?;
    fs::write(path, json).map_err(|e| format!("write config: {e}"))?;
    Ok(())
}

pub fn retention_from_str(s: &str) -> ScreenshotRetention {
    match s {
        "24h" => ScreenshotRetention::Hours24,
        "3d" => ScreenshotRetention::Days3,
        "14d" => ScreenshotRetention::Days14,
        _ => ScreenshotRetention::None,
    }
}
