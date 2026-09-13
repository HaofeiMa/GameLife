use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use gamelife_core::{default_reading_apps, default_trusted_apps, CategoryGuides};

use crate::scheduler::{app_support_dir, ScreenshotRetention};

pub const PROVIDER_OPENCODE_GO: &str = "opencode-go";
pub const PROVIDER_OPENAI: &str = "openai";
pub const PROVIDER_CUSTOM: &str = "custom";
pub const PROVIDER_NONE: &str = "none";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VisionProviderSettings {
    pub id: String,
    pub base_url: String,
    pub model: String,
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
    #[serde(default = "default_primary_provider")]
    pub primary_provider: String,
    #[serde(default = "default_fallback_provider")]
    pub fallback_provider: String,
    #[serde(default = "default_vision_providers")]
    pub vision_providers: Vec<VisionProviderSettings>,
    #[serde(default = "default_true")]
    pub show_rail_labels: bool,
    #[serde(default)]
    pub admin_apps: Vec<String>,
    #[serde(default)]
    pub category_guides: CategoryGuides,
    #[serde(default)]
    pub ticktick_client_id: String,
    #[serde(default)]
    pub ticktick_project_roles: std::collections::BTreeMap<String, String>,
}

fn default_primary_provider() -> String {
    PROVIDER_OPENCODE_GO.into()
}

fn default_fallback_provider() -> String {
    PROVIDER_OPENAI.into()
}

fn default_true() -> bool {
    true
}

pub fn default_vision_providers() -> Vec<VisionProviderSettings> {
    vec![
        VisionProviderSettings {
            id: PROVIDER_OPENCODE_GO.into(),
            base_url: "https://opencode.ai/zen/go/v1".into(),
            model: "deepseek-v4-flash-vision-exp".into(),
        },
        VisionProviderSettings {
            id: PROVIDER_OPENAI.into(),
            base_url: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
            model: std::env::var("OPENAI_VISION_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".into()),
        },
        VisionProviderSettings {
            id: PROVIDER_CUSTOM.into(),
            base_url: String::new(),
            model: String::new(),
        },
    ]
}

fn config_path() -> Option<PathBuf> {
    app_support_dir().map(|d| d.join("config.json"))
}

pub fn default_settings() -> AppSettings {
    AppSettings {
        screenshot_retention: "none".into(),
        sample_keep_days: 7,
        login_at_startup: true,
        trusted_apps: default_trusted_apps(),
        distraction_rules: vec![],
        side_project_rules: vec![],
        reading_apps: default_reading_apps(),
        never_capture_apps: vec![],
        primary_provider: default_primary_provider(),
        fallback_provider: default_fallback_provider(),
        vision_providers: default_vision_providers(),
        show_rail_labels: true,
        admin_apps: vec![],
        category_guides: CategoryGuides::default(),
        ticktick_client_id: String::new(),
        ticktick_project_roles: std::collections::BTreeMap::new(),
    }
}

pub fn with_vision_defaults(mut settings: AppSettings) -> AppSettings {
    for preset in default_vision_providers() {
        if !settings
            .vision_providers
            .iter()
            .any(|p| p.id == preset.id)
        {
            settings.vision_providers.push(preset);
        }
    }
    if settings.primary_provider.trim().is_empty() {
        settings.primary_provider = default_primary_provider();
    }
    if settings.fallback_provider.trim().is_empty() {
        settings.fallback_provider = default_fallback_provider();
    }
    settings
}

pub fn load_settings() -> AppSettings {
    let path = config_path();
    let Some(path) = path else {
        return default_settings();
    };
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(s) = serde_json::from_str(&data) {
            return with_vision_defaults(s);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_json_gets_vision_preset_defaults() {
        let json = r#"{
            "screenshotRetention":"none",
            "sampleKeepDays":7,
            "loginAtStartup":true,
            "trustedApps":["Cursor"],
            "distractionRules":[],
            "sideProjectRules":[],
            "readingApps":[],
            "neverCaptureApps":[]
        }"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        let s = with_vision_defaults(parsed);
        assert_eq!(s.trusted_apps, vec!["Cursor"]);
        assert_eq!(s.primary_provider, PROVIDER_OPENCODE_GO);
        assert_eq!(s.fallback_provider, PROVIDER_OPENAI);
        assert!(s
            .vision_providers
            .iter()
            .any(|p| p.id == PROVIDER_OPENCODE_GO));
        assert!(s.vision_providers.iter().any(|p| p.id == PROVIDER_OPENAI));
        assert!(s.vision_providers.iter().any(|p| p.id == PROVIDER_CUSTOM));
        let go = s
            .vision_providers
            .iter()
            .find(|p| p.id == PROVIDER_OPENCODE_GO)
            .unwrap();
        assert_eq!(go.base_url, "https://opencode.ai/zen/go/v1");
        assert_eq!(go.model, "deepseek-v4-flash-vision-exp");
    }

    #[test]
    fn old_config_json_defaults_rail_labels_true() {
        let parsed: AppSettings = serde_json::from_str(
            r#"{"screenshotRetention":"none","sampleKeepDays":7,"loginAtStartup":true,"trustedApps":[],"distractionRules":[],"sideProjectRules":[],"readingApps":[],"neverCaptureApps":[]}"#,
        )
        .unwrap();
        assert!(parsed.show_rail_labels);
        assert!(parsed.admin_apps.is_empty());
        assert!(parsed.ticktick_client_id.is_empty());
    }
}
