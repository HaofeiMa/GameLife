use keyring::Entry;

pub const KEYCHAIN_ACCOUNT: &str = "GameLife";
pub const KEYCHAIN_SERVICE: &str = "ma.haofei.gamelife.openai";
pub const KEYCHAIN_SERVICE_OPENCODE_GO: &str = "ma.haofei.gamelife.opencode-go";
pub const KEYCHAIN_SERVICE_CUSTOM: &str = "ma.haofei.gamelife.custom";

pub fn service_for_provider(id: &str) -> &'static str {
    match id {
        "opencode-go" => KEYCHAIN_SERVICE_OPENCODE_GO,
        "custom" => KEYCHAIN_SERVICE_CUSTOM,
        _ => KEYCHAIN_SERVICE,
    }
}

pub fn get_provider_api_key(provider: &str) -> Result<String, String> {
    let entry = Entry::new(service_for_provider(provider), KEYCHAIN_ACCOUNT)
        .map_err(|e| format!("keychain entry: {e}"))?;
    entry
        .get_password()
        .map_err(|e| format!("keychain read: {e}"))
}

pub fn set_provider_api_key(provider: &str, key: &str) -> Result<(), String> {
    let entry = Entry::new(service_for_provider(provider), KEYCHAIN_ACCOUNT)
        .map_err(|e| format!("keychain entry: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("keychain write: {e}"))
}

pub fn get_openai_api_key() -> Result<String, String> {
    get_provider_api_key("openai")
}

pub fn set_openai_api_key(key: &str) -> Result<(), String> {
    set_provider_api_key("openai", key)
}

pub fn delete_openai_api_key() -> Result<(), String> {
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|e| format!("keychain entry: {e}"))?;
    entry
        .delete_credential()
        .map_err(|e| format!("keychain delete: {e}"))
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
        assert_eq!(service_for_provider("unknown"), KEYCHAIN_SERVICE);
    }
}
