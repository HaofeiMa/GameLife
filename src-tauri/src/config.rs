use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use gamelife_core::{default_reading_apps, default_trusted_apps, CategoryGuides};

use crate::scheduler::{app_support_dir, ScreenshotRetention};

pub const PROVIDER_OPENCODE_GO: &str = "opencode-go";
pub const PROVIDER_OPENAI: &str = "openai";
pub const PROVIDER_CUSTOM: &str = "custom";
pub const PROVIDER_CODEX: &str = "codex";
pub const PROVIDER_NONE: &str = "none";
pub const KIND_CUSTOM: &str = "custom";
pub const KIND_CODEX: &str = "codex";
pub const DEFAULT_CODEX_MODEL: &str = "gpt-5.4";

pub const THEME_SYSTEM: &str = "system";

pub const SCOPE_AGGREGATE: &str = "aggregate";
pub const SCOPE_SAMPLES: &str = "samples";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VisionProviderSettings {
    pub id: String,
    /// `"custom"` | `"codex"`. Empty means a pre-migration preset.
    #[serde(default)]
    pub kind: String,
    pub base_url: String,
    pub model: String,
}

/// Cloud backup. Never part of the policy snapshot: two devices with different
/// remote targets must not cut a new `policy_versions` row.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    pub enabled: bool,
    pub target: String,
    pub url: String,
    pub username: String,
    /// S3 only: path-style bucket. Unused by WebDAV.
    pub bucket: String,
    /// S3 only: `auto` for Cloudflare R2.
    pub region: String,
    pub remote_path: String,
    pub interval_minutes: i64,
    pub scope: String,
    pub keep_snapshots: i64,
    pub device_label: String,
    /// Hours after a local day ends before stragglers stop blocking settlement.
    /// Missing or non-positive values fall back to 36 (§9 / T16).
    #[serde(default = "default_settle_grace_hours")]
    pub settle_grace_hours: i64,
}

fn default_settle_grace_hours() -> i64 {
    36
}

/// Anything non-positive (and the 0 that a blank number input produces) must
/// not disable the grace period — that would settle a two-device day the
/// moment it ended, which is the hostage situation §3.4 exists to avoid.
pub fn normalize_settle_grace_hours(h: i64) -> i64 {
    if h <= 0 {
        default_settle_grace_hours()
    } else {
        h
    }
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            target: "webdav".into(),
            url: String::new(),
            username: String::new(),
            bucket: String::new(),
            region: "auto".into(),
            remote_path: "gamelife".into(),
            interval_minutes: 60,
            scope: SCOPE_AGGREGATE.into(),
            keep_snapshots: 7,
            device_label: String::new(),
            settle_grace_hours: default_settle_grace_hours(),
        }
    }
}

/// `aggregate` is the default and the fallback: anything unrecognised must
/// narrow the upload, never widen it.
pub fn normalize_scope(s: &str) -> &'static str {
    match s {
        SCOPE_SAMPLES => SCOPE_SAMPLES,
        _ => SCOPE_AGGREGATE,
    }
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
    #[serde(default = "default_true")]
    pub silent_start: bool,
    #[serde(default)]
    pub admin_apps: Vec<String>,
    #[serde(default)]
    pub category_guides: CategoryGuides,
    #[serde(default, skip_serializing)]
    pub ticktick_client_id: String,
    #[serde(default, skip_serializing)]
    pub ticktick_project_roles: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing)]
    pub ticktick_column_roles: std::collections::BTreeMap<String, String>,
    /// UI appearance: system | light | dark. Normalised on the frontend by
    /// `normalizeThemePreference` in src/lib/theme.ts.
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub sync: SyncSettings,
}

fn default_primary_provider() -> String {
    PROVIDER_OPENCODE_GO.into()
}

fn default_fallback_provider() -> String {
    PROVIDER_CODEX.into()
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    THEME_SYSTEM.into()
}

pub fn show_window_on_launch(silent_start: bool) -> bool {
    !silent_start
}

pub fn default_vision_providers() -> Vec<VisionProviderSettings> {
    vec![
        VisionProviderSettings {
            id: PROVIDER_OPENCODE_GO.into(),
            kind: KIND_CUSTOM.into(),
            base_url: "https://opencode.ai/zen/go/v1".into(),
            model: "deepseek-v4-flash-vision-exp".into(),
        },
        VisionProviderSettings {
            id: PROVIDER_CODEX.into(),
            kind: KIND_CODEX.into(),
            base_url: String::new(),
            model: DEFAULT_CODEX_MODEL.into(),
        },
    ]
}

pub fn is_codex_provider(p: &VisionProviderSettings) -> bool {
    let kind = p.kind.trim();
    if kind == KIND_CODEX {
        return true;
    }
    if kind == KIND_CUSTOM {
        return false;
    }
    p.id == PROVIDER_OPENAI || p.id == PROVIDER_CODEX
}

fn mapped_provider_id(id: &str) -> &str {
    if id == PROVIDER_OPENAI {
        PROVIDER_CODEX
    } else {
        id
    }
}

fn as_codex(p: &VisionProviderSettings) -> VisionProviderSettings {
    let model = if p.model.trim().is_empty() || p.model == "gpt-4o-mini" {
        DEFAULT_CODEX_MODEL
    } else {
        p.model.as_str()
    };
    VisionProviderSettings {
        id: PROVIDER_CODEX.into(),
        kind: KIND_CODEX.into(),
        base_url: String::new(),
        model: model.into(),
    }
}

fn as_custom(p: &VisionProviderSettings) -> VisionProviderSettings {
    VisionProviderSettings {
        id: p.id.clone(),
        kind: KIND_CUSTOM.into(),
        base_url: p.base_url.clone(),
        model: p.model.clone(),
    }
}

fn normalize_one(p: &VisionProviderSettings) -> VisionProviderSettings {
    if is_codex_provider(p) {
        as_codex(p)
    } else {
        as_custom(p)
    }
}

fn kinds_already_set(providers: &[VisionProviderSettings]) -> bool {
    !providers.is_empty() && providers.iter().all(|p| !p.kind.trim().is_empty())
}

/// Rebuild the ordered API panel list from an old primary/fallback + three-slot config.
pub fn migrate_vision_providers(
    providers: &[VisionProviderSettings],
    primary: &str,
    fallback: &str,
) -> Vec<VisionProviderSettings> {
    if kinds_already_set(providers) {
        return providers.to_vec();
    }
    let mut rest: Vec<VisionProviderSettings> = providers
        .iter()
        .map(normalize_one)
        .filter(|p| {
            if p.kind != KIND_CUSTOM || p.id != PROVIDER_CUSTOM {
                return true;
            }
            !p.base_url.trim().is_empty() || !p.model.trim().is_empty()
        })
        .collect();
    let mut seen_codex = false;
    rest.retain(|p| {
        if p.kind != KIND_CODEX {
            return true;
        }
        if seen_codex {
            return false;
        }
        seen_codex = true;
        true
    });
    let mut ordered = Vec::new();
    for wanted in [mapped_provider_id(primary), mapped_provider_id(fallback)] {
        if wanted.is_empty() || wanted == PROVIDER_NONE {
            continue;
        }
        if let Some(i) = rest.iter().position(|p| p.id == wanted) {
            ordered.push(rest.remove(i));
        }
    }
    ordered.append(&mut rest);
    ordered
}

pub fn normalize_vision_providers(settings: &mut AppSettings) {
    settings.vision_providers = migrate_vision_providers(
        &settings.vision_providers,
        &settings.primary_provider,
        &settings.fallback_provider,
    );
    settings.primary_provider = settings
        .vision_providers
        .first()
        .map(|p| p.id.clone())
        .unwrap_or_default();
    settings.fallback_provider = settings
        .vision_providers
        .get(1)
        .map(|p| p.id.clone())
        .unwrap_or_else(|| PROVIDER_NONE.into());
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
        silent_start: true,
        admin_apps: vec![],
        category_guides: CategoryGuides::default(),
        ticktick_client_id: String::new(),
        ticktick_project_roles: std::collections::BTreeMap::new(),
        ticktick_column_roles: std::collections::BTreeMap::new(),
        theme: default_theme(),
        sync: SyncSettings::default(),
    }
}

pub fn with_vision_defaults(mut settings: AppSettings) -> AppSettings {
    if settings.vision_providers.is_empty() {
        settings.vision_providers = default_vision_providers();
    }
    normalize_vision_providers(&mut settings);
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
        if let Ok(mut s) = serde_json::from_str::<AppSettings>(&data) {
            s.sync.settle_grace_hours =
                normalize_settle_grace_hours(s.sync.settle_grace_hours);
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

pub fn policy_snapshot_json(settings: &AppSettings) -> String {
    serde_json::json!({
        "trusted_apps": settings.trusted_apps,
        "distraction_rules": settings.distraction_rules,
        "side_project_rules": settings.side_project_rules,
        "reading_apps": settings.reading_apps,
        "never_capture_apps": settings.never_capture_apps,
        "admin_apps": settings.admin_apps,
        "category_guides": settings.category_guides,
    })
    .to_string()
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
    fn old_config_json_gets_custom_then_codex_defaults() {
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
        assert_eq!(s.fallback_provider, PROVIDER_CODEX);
        assert_eq!(s.vision_providers.len(), 2);
        assert_eq!(s.vision_providers[0].id, PROVIDER_OPENCODE_GO);
        assert_eq!(s.vision_providers[0].kind, KIND_CUSTOM);
        assert_eq!(s.vision_providers[1].id, PROVIDER_CODEX);
        assert_eq!(s.vision_providers[1].kind, KIND_CODEX);
        let go = &s.vision_providers[0];
        assert_eq!(go.base_url, "https://opencode.ai/zen/go/v1");
        assert_eq!(go.model, "deepseek-v4-flash-vision-exp");
    }

    #[test]
    fn old_three_slot_config_drops_empty_custom_and_promotes_openai_to_codex() {
        let parsed: AppSettings = serde_json::from_str(
            r#"{
            "screenshotRetention":"none",
            "sampleKeepDays":7,
            "loginAtStartup":true,
            "trustedApps":[],
            "distractionRules":[],
            "sideProjectRules":[],
            "readingApps":[],
            "neverCaptureApps":[],
            "primaryProvider":"opencode-go",
            "fallbackProvider":"openai",
            "visionProviders":[
                {"id":"opencode-go","baseUrl":"https://opencode.ai/zen/go/v1","model":"deepseek-v4-flash-vision-exp"},
                {"id":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o-mini"},
                {"id":"custom","baseUrl":"","model":""}
            ]
        }"#,
        )
        .unwrap();
        let s = with_vision_defaults(parsed);
        assert_eq!(
            s.vision_providers
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            vec!["opencode-go", "codex"]
        );
        assert_eq!(s.vision_providers[1].kind, KIND_CODEX);
        assert!(s.vision_providers[1].base_url.is_empty());
    }

    #[test]
    fn already_kinded_list_keeps_user_order() {
        let parsed: AppSettings = serde_json::from_str(
            r#"{
            "screenshotRetention":"none",
            "sampleKeepDays":7,
            "loginAtStartup":true,
            "trustedApps":[],
            "distractionRules":[],
            "sideProjectRules":[],
            "readingApps":[],
            "neverCaptureApps":[],
            "primaryProvider":"opencode-go",
            "fallbackProvider":"codex",
            "visionProviders":[
                {"id":"codex","kind":"codex","baseUrl":"","model":"gpt-5.4"},
                {"id":"opencode-go","kind":"custom","baseUrl":"https://opencode.ai/zen/go/v1","model":"x"}
            ]
        }"#,
        )
        .unwrap();
        let s = with_vision_defaults(parsed);
        assert_eq!(s.vision_providers[0].id, PROVIDER_CODEX);
        assert_eq!(s.vision_providers[1].id, PROVIDER_OPENCODE_GO);
        assert_eq!(s.primary_provider, PROVIDER_CODEX);
        assert_eq!(s.fallback_provider, PROVIDER_OPENCODE_GO);
    }

    #[test]
    fn old_config_json_defaults_rail_labels_true() {
        let parsed: AppSettings = serde_json::from_str(
            r#"{"screenshotRetention":"none","sampleKeepDays":7,"loginAtStartup":true,"trustedApps":[],"distractionRules":[],"sideProjectRules":[],"readingApps":[],"neverCaptureApps":[]}"#,
        )
        .unwrap();
        assert!(parsed.show_rail_labels);
        assert!(parsed.silent_start);
        assert!(parsed.admin_apps.is_empty());
        assert!(parsed.ticktick_client_id.is_empty());
    }

    #[test]
    fn silent_start_hides_window_on_launch() {
        assert!(!show_window_on_launch(true));
        assert!(show_window_on_launch(false));
    }

    #[test]
    fn policy_snapshot_ignores_ticktick_client_id() {
        let a = default_settings();
        let mut b = default_settings();
        b.ticktick_client_id = "changed".into();
        b.primary_provider = "openai".into();
        assert_eq!(policy_snapshot_json(&a), policy_snapshot_json(&b));
    }

    #[test]
    fn old_config_json_without_sync_gets_disabled_defaults() {
        let parsed: AppSettings = serde_json::from_str(
            r#"{"screenshotRetention":"none","sampleKeepDays":7,"loginAtStartup":true,"trustedApps":[],"distractionRules":[],"sideProjectRules":[],"readingApps":[],"neverCaptureApps":[]}"#,
        )
        .unwrap();
        assert!(!parsed.sync.enabled);
        assert_eq!(parsed.sync.target, "webdav");
        assert_eq!(parsed.sync.scope, SCOPE_AGGREGATE);
        assert_eq!(parsed.sync.interval_minutes, 60);
        assert_eq!(parsed.sync.keep_snapshots, 7);
        assert_eq!(parsed.sync.remote_path, "gamelife");
        assert!(parsed.sync.bucket.is_empty());
        assert_eq!(parsed.sync.region, "auto");
        assert_eq!(parsed.sync.settle_grace_hours, 36);
    }

    #[test]
    fn normalize_settle_grace_hours_rejects_non_positive() {
        assert_eq!(normalize_settle_grace_hours(36), 36);
        assert_eq!(normalize_settle_grace_hours(12), 12);
        assert_eq!(normalize_settle_grace_hours(0), 36);
        assert_eq!(normalize_settle_grace_hours(-8), 36);
    }

    #[test]
    fn unknown_scope_falls_back_to_aggregate() {
        assert_eq!(normalize_scope("samples"), SCOPE_SAMPLES);
        assert_eq!(normalize_scope("everything"), SCOPE_AGGREGATE);
        assert_eq!(normalize_scope(""), SCOPE_AGGREGATE);
        assert_eq!(normalize_scope("Aggregate"), SCOPE_AGGREGATE);
    }

    #[test]
    fn sync_settings_are_not_part_of_the_policy_snapshot() {
        let a = default_settings();
        let mut b = default_settings();
        b.sync.enabled = true;
        b.sync.url = "https://dav.example.com".into();
        b.sync.scope = SCOPE_SAMPLES.into();
        b.sync.settle_grace_hours = 12;
        assert_eq!(policy_snapshot_json(&a), policy_snapshot_json(&b));
    }

    #[test]
    fn sync_settings_roundtrip_through_camel_case_json() {
        let mut s = SyncSettings::default();
        s.enabled = true;
        s.url = "https://dav.example.com/remote.php/dav/files/me/".into();
        s.interval_minutes = 15;
        s.keep_snapshots = 3;
        s.device_label = "MacBook".into();
        s.settle_grace_hours = 12;
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"remotePath\""));
        assert!(json.contains("\"intervalMinutes\""));
        assert!(json.contains("\"keepSnapshots\""));
        assert!(json.contains("\"deviceLabel\""));
        assert!(json.contains("\"settleGraceHours\""));
        assert_eq!(serde_json::from_str::<SyncSettings>(&json).unwrap(), s);
    }
}
