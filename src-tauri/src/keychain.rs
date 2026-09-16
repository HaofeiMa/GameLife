use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const KEYCHAIN_ACCOUNT: &str = "GameLife";
pub const KEYCHAIN_SERVICE: &str = "openai";
pub const KEYCHAIN_SERVICE_OPENCODE_GO: &str = "opencode-go";
pub const KEYCHAIN_SERVICE_CUSTOM: &str = "custom";

static SECRETS_LOCK: Mutex<()> = Mutex::new(());

pub fn service_for_provider(id: &str) -> String {
    match id {
        "openai" => KEYCHAIN_SERVICE.to_string(),
        other => other.to_string(),
    }
}

pub fn secrets_path() -> Result<PathBuf, String> {
    let dir = crate::scheduler::app_support_dir().ok_or_else(|| "home dir".to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
    Ok(dir.join("secrets.json"))
}

pub fn load_map(path: &Path) -> Result<BTreeMap<String, String>, String> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let data = fs::read_to_string(path).map_err(|e| format!("read secrets: {e}"))?;
    if data.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    let value: Value = serde_json::from_str(&data).map_err(|e| format!("secrets json: {e}"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| "secrets json: expected object".to_string())?;
    let mut map = BTreeMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            map.insert(k.clone(), s.to_string());
        }
    }
    Ok(map)
}

pub fn save_map(path: &Path, map: &BTreeMap<String, String>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(map).map_err(|e| format!("json: {e}"))?;
    fs::write(path, json).map_err(|e| format!("write secrets: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("chmod: {e}"))?;
    }
    Ok(())
}

pub fn get_in(path: &Path, slot: &str) -> Result<String, String> {
    let map = load_map(path)?;
    map.get(slot)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("secret missing: {slot}"))
}

pub fn set_in(path: &Path, slot: &str, value: &str) -> Result<(), String> {
    let mut map = load_map(path)?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        map.remove(slot);
    } else {
        map.insert(slot.to_string(), trimmed.to_string());
    }
    save_map(path, &map)
}

pub fn delete_in(path: &Path, slot: &str) -> Result<(), String> {
    let mut map = load_map(path)?;
    map.remove(slot);
    save_map(path, &map)
}

fn with_store<T>(f: impl FnOnce(&Path) -> Result<T, String>) -> Result<T, String> {
    let _guard = SECRETS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = secrets_path()?;
    f(&path)
}

pub fn get_provider_api_key(provider: &str) -> Result<String, String> {
    with_store(|path| get_in(path, &service_for_provider(provider)))
}

pub fn set_provider_api_key(provider: &str, key: &str) -> Result<(), String> {
    with_store(|path| set_in(path, &service_for_provider(provider), key))
}

pub fn get_openai_api_key() -> Result<String, String> {
    get_provider_api_key("openai")
}

pub fn set_openai_api_key(key: &str) -> Result<(), String> {
    set_provider_api_key("openai", key)
}

pub fn delete_openai_api_key() -> Result<(), String> {
    with_store(|path| delete_in(path, KEYCHAIN_SERVICE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_map_to_distinct_keychain_services() {
        assert_eq!(
            service_for_provider("opencode-go"),
            KEYCHAIN_SERVICE_OPENCODE_GO
        );
        assert_eq!(service_for_provider("openai"), KEYCHAIN_SERVICE);
        assert_eq!(service_for_provider("custom"), KEYCHAIN_SERVICE_CUSTOM);
        assert_eq!(service_for_provider("unknown"), "unknown");
    }

    #[test]
    fn file_store_roundtrip_keeps_sibling_slots() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        assert!(get_in(&path, "openai").is_err());
        set_in(&path, "openai", "sk-test").unwrap();
        assert_eq!(get_in(&path, "openai").unwrap(), "sk-test");
        set_in(&path, "opencode-go", "oc-test").unwrap();
        assert_eq!(get_in(&path, "openai").unwrap(), "sk-test");
        assert_eq!(get_in(&path, "opencode-go").unwrap(), "oc-test");
        delete_in(&path, "openai").unwrap();
        assert!(get_in(&path, "openai").is_err());
        assert_eq!(get_in(&path, "opencode-go").unwrap(), "oc-test");
    }

    #[test]
    fn file_store_treats_blank_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        set_in(&path, "openai", "  ").unwrap();
        assert!(get_in(&path, "openai").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn file_store_is_mode_600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        set_in(&path, "openai", "sk-test").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
