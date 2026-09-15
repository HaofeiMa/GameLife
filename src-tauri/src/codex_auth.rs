use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const CODEX_BACKEND: &str = "https://chatgpt.com/backend-api/codex";
pub const CODEX_ORIGINATOR: &str = "gamelife";
const REFRESH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexSession {
    ChatGpt {
        access_token: String,
        refresh_token: String,
        account_id: String,
    },
    ApiKey {
        api_key: String,
    },
}

#[derive(Deserialize)]
struct AuthFile {
    #[serde(default)]
    auth_mode: Option<String>,
    #[serde(default, alias = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,
    #[serde(default)]
    tokens: Option<AuthTokens>,
}

#[derive(Deserialize)]
struct AuthTokens {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

pub fn default_auth_path() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("CODEX_HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed).join("auth.json"));
        }
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".codex").join("auth.json"))
}

pub fn chatgpt_account_id_from_jwt(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value
        .get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(|id| id.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn session_from_json(raw: &str) -> Option<CodexSession> {
    let file: AuthFile = serde_json::from_str(raw).ok()?;
    let api_key = file
        .openai_api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let mode = file
        .auth_mode
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if mode == "apikey" || (api_key.is_some() && file.tokens.is_none()) {
        return api_key.map(|k| CodexSession::ApiKey {
            api_key: k.to_string(),
        });
    }
    let tokens = file.tokens?;
    let access = tokens.access_token.as_deref().map(str::trim).filter(|s| !s.is_empty())?;
    let refresh = tokens
        .refresh_token
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    let account = tokens
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| chatgpt_account_id_from_jwt(access))
        .or_else(|| {
            tokens
                .id_token
                .as_deref()
                .and_then(chatgpt_account_id_from_jwt)
        })?;
    Some(CodexSession::ChatGpt {
        access_token: access.to_string(),
        refresh_token: refresh,
        account_id: account,
    })
}

pub fn load_session_from_path(path: &Path) -> Option<CodexSession> {
    let raw = fs::read_to_string(path).ok()?;
    session_from_json(&raw)
}

pub fn load_session() -> Option<CodexSession> {
    load_session_from_path(&default_auth_path()?)
}

pub fn logged_in_at(path: &Path) -> bool {
    load_session_from_path(path).is_some()
}

pub fn logged_in() -> bool {
    default_auth_path()
        .map(|p| logged_in_at(&p))
        .unwrap_or(false)
}

#[derive(serde::Serialize)]
struct RefreshRequest {
    client_id: String,
    grant_type: &'static str,
    refresh_token: String,
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
}

/// Refresh ChatGPT tokens. Writes the new access token back into `auth.json` when possible.
pub fn refresh_chatgpt_session(path: &Path, session: &CodexSession) -> Option<CodexSession> {
    let CodexSession::ChatGpt {
        refresh_token,
        account_id,
        ..
    } = session
    else {
        return None;
    };
    if refresh_token.is_empty() {
        return None;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()?;
    let resp = client
        .post(REFRESH_TOKEN_URL)
        .json(&RefreshRequest {
            client_id: OAUTH_CLIENT_ID.into(),
            grant_type: "refresh_token",
            refresh_token: refresh_token.clone(),
        })
        .send()
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: RefreshResponse = resp.json().ok()?;
    let access = body.access_token?.trim().to_string();
    if access.is_empty() {
        return None;
    }
    let new_refresh = body
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(refresh_token)
        .to_string();
    let account = chatgpt_account_id_from_jwt(&access)
        .or_else(|| {
            body.id_token
                .as_deref()
                .and_then(chatgpt_account_id_from_jwt)
        })
        .unwrap_or_else(|| account_id.clone());
    let next = CodexSession::ChatGpt {
        access_token: access.clone(),
        refresh_token: new_refresh.clone(),
        account_id: account,
    };
    if let Ok(raw) = fs::read_to_string(path) {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(obj) = value.as_object_mut() {
                if let Some(tokens) = obj.get_mut("tokens").and_then(|t| t.as_object_mut()) {
                    tokens.insert("access_token".into(), serde_json::Value::String(access));
                    tokens.insert(
                        "refresh_token".into(),
                        serde_json::Value::String(new_refresh),
                    );
                    if let Some(id) = body.id_token {
                        tokens.insert("id_token".into(), serde_json::Value::String(id));
                    }
                }
                obj.insert(
                    "last_refresh".into(),
                    serde_json::Value::String(chrono::Utc::now().to_rfc3339()),
                );
                if let Ok(out) = serde_json::to_string_pretty(&value) {
                    let _ = fs::write(path, out);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
                    }
                }
            }
        }
    }
    Some(next)
}

pub fn sse_output_text(body: &str) -> Option<String> {
    let mut chunks = String::new();
    let mut completed = String::new();
    for block in body.split("\n\n") {
        let mut data = String::new();
        for line in block.lines() {
            let line = line.trim_end();
            if let Some(rest) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest.trim_start());
            }
        }
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&data) else {
            continue;
        };
        let kind = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if kind == "response.output_text.delta" {
            if let Some(delta) = value.get("delta").and_then(|d| d.as_str()) {
                chunks.push_str(delta);
            }
        }
        if kind == "response.completed" || kind == "response.output_text.done" {
            if let Some(text) = value
                .pointer("/response/output/0/content/0/text")
                .and_then(|t| t.as_str())
            {
                completed = text.to_string();
            } else if let Some(text) = value.get("text").and_then(|t| t.as_str()) {
                completed = text.to_string();
            }
        }
    }
    if !chunks.is_empty() {
        Some(chunks)
    } else if !completed.is_empty() {
        Some(completed)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt_for(account: &str) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = serde_json::json!({
            "https://api.openai.com/auth": { "chatgpt_account_id": account }
        });
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.to_string());
        format!("{header}.{payload_b64}.sig")
    }

    #[test]
    fn reads_chatgpt_session_from_auth_json() {
        let token = jwt_for("acct-jwt");
        let raw = serde_json::json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "access_token": token,
                "refresh_token": "rt",
                "account_id": "acct-file"
            }
        })
        .to_string();
        match session_from_json(&raw) {
            Some(CodexSession::ChatGpt {
                account_id,
                refresh_token,
                ..
            }) => {
                assert_eq!(account_id, "acct-file");
                assert_eq!(refresh_token, "rt");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn falls_back_to_jwt_account_id() {
        let token = jwt_for("acct-jwt");
        let raw = serde_json::json!({
            "auth_mode": "chatgpt",
            "tokens": { "access_token": token, "refresh_token": "rt" }
        })
        .to_string();
        match session_from_json(&raw) {
            Some(CodexSession::ChatGpt { account_id, .. }) => {
                assert_eq!(account_id, "acct-jwt");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn reads_api_key_login() {
        let raw = r#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk-live"}"#;
        match session_from_json(raw) {
            Some(CodexSession::ApiKey { api_key }) => assert_eq!(api_key, "sk-live"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn missing_tokens_is_logged_out() {
        assert!(session_from_json(r#"{"auth_mode":"chatgpt"}"#).is_none());
    }

    #[test]
    fn concatenates_sse_output_text_deltas() {
        let body = "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"{\\\"ok\\\":\"}\n\n\
event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\" true}\"}\n\n\
event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n";
        assert_eq!(sse_output_text(body).as_deref(), Some("{\"ok\": true}"));
    }

    #[test]
    fn load_session_from_path_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let token = jwt_for("acct-jwt");
        fs::write(
            &path,
            serde_json::json!({
                "auth_mode": "chatgpt",
                "tokens": { "access_token": token, "account_id": "acct-file" }
            })
            .to_string(),
        )
        .unwrap();
        assert!(logged_in_at(&path));
        assert!(matches!(
            load_session_from_path(&path),
            Some(CodexSession::ChatGpt { .. })
        ));
    }
}
