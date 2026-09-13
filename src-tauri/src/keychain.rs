use keyring::Entry;

pub const KEYCHAIN_ACCOUNT: &str = "GameLife";
pub const KEYCHAIN_SERVICE: &str = "ma.haofei.gamelife.openai";
pub const KEYCHAIN_SERVICE_OPENCODE_GO: &str = "ma.haofei.gamelife.opencode-go";
pub const KEYCHAIN_SERVICE_CUSTOM: &str = "ma.haofei.gamelife.custom";
pub const KEYCHAIN_SERVICE_TICKTICK_TOKEN: &str = "ma.haofei.gamelife.ticktick";
pub const KEYCHAIN_SERVICE_TICKTICK_SECRET: &str = "ma.haofei.gamelife.ticktick-secret";
pub const KEYCHAIN_ACCOUNT_ACCESS: &str = "access";
pub const KEYCHAIN_ACCOUNT_REFRESH: &str = "refresh";

fn entry(service: &str, account: &str) -> Result<Entry, String> {
    Entry::new(service, account).map_err(|e| format!("keychain entry: {e}"))
}

fn get_password(service: &str, account: &str) -> Result<String, String> {
    entry(service, account)?
        .get_password()
        .map_err(|e| format!("keychain read: {e}"))
}

fn set_password(service: &str, account: &str, value: &str) -> Result<(), String> {
    entry(service, account)?
        .set_password(value)
        .map_err(|e| format!("keychain write: {e}"))
}

fn delete_password(service: &str, account: &str) -> Result<(), String> {
    match entry(service, account)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keychain delete: {e}")),
    }
}

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
    delete_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
}

pub fn get_ticktick_access_token() -> Result<String, String> {
    get_password(KEYCHAIN_SERVICE_TICKTICK_TOKEN, KEYCHAIN_ACCOUNT_ACCESS)
}

pub fn set_ticktick_access_token(v: &str) -> Result<(), String> {
    set_password(KEYCHAIN_SERVICE_TICKTICK_TOKEN, KEYCHAIN_ACCOUNT_ACCESS, v)
}

pub fn get_ticktick_refresh_token() -> Result<String, String> {
    get_password(KEYCHAIN_SERVICE_TICKTICK_TOKEN, KEYCHAIN_ACCOUNT_REFRESH)
}

pub fn set_ticktick_refresh_token(v: &str) -> Result<(), String> {
    set_password(KEYCHAIN_SERVICE_TICKTICK_TOKEN, KEYCHAIN_ACCOUNT_REFRESH, v)
}

pub fn get_ticktick_client_secret() -> Result<String, String> {
    get_password(KEYCHAIN_SERVICE_TICKTICK_SECRET, KEYCHAIN_ACCOUNT)
}

pub fn set_ticktick_client_secret(v: &str) -> Result<(), String> {
    set_password(KEYCHAIN_SERVICE_TICKTICK_SECRET, KEYCHAIN_ACCOUNT, v)
}

pub fn clear_ticktick_tokens() -> Result<(), String> {
    delete_password(KEYCHAIN_SERVICE_TICKTICK_TOKEN, KEYCHAIN_ACCOUNT_ACCESS)?;
    delete_password(KEYCHAIN_SERVICE_TICKTICK_TOKEN, KEYCHAIN_ACCOUNT_REFRESH)
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

    #[test]
    fn ticktick_services_are_distinct() {
        assert_ne!(KEYCHAIN_SERVICE_TICKTICK_TOKEN, KEYCHAIN_SERVICE);
        assert_ne!(
            KEYCHAIN_SERVICE_TICKTICK_SECRET,
            KEYCHAIN_SERVICE_TICKTICK_TOKEN
        );
    }
}
