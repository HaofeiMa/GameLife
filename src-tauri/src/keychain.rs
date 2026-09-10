use keyring::Entry;

pub const KEYCHAIN_SERVICE: &str = "ma.haofei.gamelife.openai";
pub const KEYCHAIN_ACCOUNT: &str = "GameLife";

pub fn get_openai_api_key() -> Result<String, String> {
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|e| format!("keychain entry: {e}"))?;
    entry
        .get_password()
        .map_err(|e| format!("keychain read: {e}"))
}

pub fn set_openai_api_key(key: &str) -> Result<(), String> {
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|e| format!("keychain entry: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("keychain write: {e}"))
}

pub fn delete_openai_api_key() -> Result<(), String> {
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|e| format!("keychain entry: {e}"))?;
    entry
        .delete_credential()
        .map_err(|e| format!("keychain delete: {e}"))
}
