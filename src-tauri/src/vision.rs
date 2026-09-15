use std::io::Cursor;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD, Engine};
use gamelife_core::judge::{parse_vision_json, VisionMatchContext, VisionResult};
use gamelife_core::{
    build_vision_prompt, sanitize_vision_context, SanitizedVisionContext, VisionContext,
};
use image::imageops::FilterType;
use image::GenericImageView;

use crate::codex_auth::{
    default_auth_path, load_session, load_session_from_path, refresh_chatgpt_session,
    sse_output_text, CodexSession, CODEX_BACKEND, CODEX_ORIGINATOR,
};

pub const VISION_TIMEOUT_SECS: u64 = 20;
pub const CODEX_TIMEOUT_SECS: u64 = 45;
pub const JPEG_MAX_LONG_EDGE: u32 = 1280;
pub const USER_AGENT: &str = "GameLife/0.2";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EndpointKind {
    Compat,
    Codex,
}

#[derive(Clone, Debug)]
pub struct VisionEndpoint {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub kind: EndpointKind,
    pub account_id: String,
}

impl VisionEndpoint {
    pub fn compat(base_url: String, model: String, api_key: String) -> Self {
        Self {
            base_url,
            model,
            api_key,
            kind: EndpointKind::Compat,
            account_id: String::new(),
        }
    }

    pub fn usable(&self) -> bool {
        if self.model.trim().is_empty() || self.api_key.trim().is_empty() {
            return false;
        }
        match self.kind {
            EndpointKind::Compat => !self.base_url.trim().is_empty(),
            EndpointKind::Codex => !self.account_id.trim().is_empty(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum VisionCallError {
    Transport,
    Client,
    Parse,
}

/// Resize in-memory image so long edge ≤ `JPEG_MAX_LONG_EDGE` and return JPEG bytes.
pub fn jpeg_bytes_for_upload(image_bytes: &[u8]) -> Result<Vec<u8>, ()> {
    let img = image::load_from_memory(image_bytes).map_err(|_| ())?;
    let (w, h) = img.dimensions();
    let long = w.max(h);
    let resized = if long > JPEG_MAX_LONG_EDGE {
        let scale = JPEG_MAX_LONG_EDGE as f32 / long as f32;
        let nw = (w as f32 * scale).round().max(1.0) as u32;
        let nh = (h as f32 * scale).round().max(1.0) as u32;
        img.resize(nw, nh, FilterType::Lanczos3)
    } else {
        img
    };
    let mut out = Vec::new();
    resized
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Jpeg)
        .map_err(|_| ())?;
    Ok(out)
}

pub fn read_and_prepare_jpeg(path: &Path) -> Result<Vec<u8>, ()> {
    let bytes = std::fs::read(path).map_err(|_| ())?;
    jpeg_bytes_for_upload(&bytes)
}

fn openai_base_url() -> String {
    std::env::var("OPENAI_BASE_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1".to_string())
}

fn openai_model() -> String {
    std::env::var("OPENAI_VISION_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string())
}

pub fn endpoints_from_settings(
    settings: &crate::config::AppSettings,
    key_for: impl Fn(&str) -> Option<String>,
) -> Vec<VisionEndpoint> {
    endpoints_from_settings_with_codex(settings, key_for, load_session())
}

pub fn endpoints_from_settings_with_codex(
    settings: &crate::config::AppSettings,
    key_for: impl Fn(&str) -> Option<String>,
    codex: Option<CodexSession>,
) -> Vec<VisionEndpoint> {
    settings
        .vision_providers
        .iter()
        .filter_map(|p| endpoint_from_provider(p, &key_for, &codex))
        .collect()
}

pub fn endpoint_from_provider(
    spec: &crate::config::VisionProviderSettings,
    key_for: &impl Fn(&str) -> Option<String>,
    codex: &Option<CodexSession>,
) -> Option<VisionEndpoint> {
    if spec.model.trim().is_empty() {
        return None;
    }
    if crate::config::is_codex_provider(spec) {
        return match codex {
            Some(CodexSession::ChatGpt {
                access_token,
                account_id,
                ..
            }) => Some(VisionEndpoint {
                base_url: CODEX_BACKEND.into(),
                model: spec.model.clone(),
                api_key: access_token.clone(),
                kind: EndpointKind::Codex,
                account_id: account_id.clone(),
            }),
            Some(CodexSession::ApiKey { api_key }) => Some(VisionEndpoint::compat(
                "https://api.openai.com/v1".into(),
                spec.model.clone(),
                api_key.clone(),
            )),
            None => None,
        };
    }
    let api_key = key_for(&spec.id)?;
    if api_key.trim().is_empty() || spec.base_url.trim().is_empty() {
        return None;
    }
    Some(VisionEndpoint::compat(
        spec.base_url.clone(),
        spec.model.clone(),
        api_key,
    ))
}

pub fn complete_json(
    endpoint: &VisionEndpoint,
    prompt: &str,
    jpeg_bytes: Option<&[u8]>,
) -> Result<String, VisionCallError> {
    if !endpoint.usable() || prompt.trim().is_empty() {
        return Err(VisionCallError::Client);
    }
    match endpoint.kind {
        EndpointKind::Compat => post_compat(endpoint, prompt, jpeg_bytes),
        EndpointKind::Codex => post_codex(endpoint, prompt, jpeg_bytes, true),
    }
}

fn post_compat(
    endpoint: &VisionEndpoint,
    prompt: &str,
    jpeg_bytes: Option<&[u8]>,
) -> Result<String, VisionCallError> {
    let url = format!(
        "{}/chat/completions",
        endpoint.base_url.trim_end_matches('/')
    );
    let content = match jpeg_bytes {
        Some(jpeg) => {
            let b64 = STANDARD.encode(jpeg);
            serde_json::json!([
                { "type": "text", "text": prompt },
                { "type": "image_url", "image_url": { "url": format!("data:image/jpeg;base64,{b64}") } }
            ])
        }
        None => serde_json::json!(prompt),
    };
    let body = serde_json::json!({
        "model": endpoint.model,
        "response_format": { "type": "json_object" },
        "messages": [{ "role": "user", "content": content }]
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(VISION_TIMEOUT_SECS))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|_| VisionCallError::Transport)?;
    let resp = client
        .post(&url)
        .header("x-opencode-session", "gamelife")
        .bearer_auth(&endpoint.api_key)
        .json(&body)
        .send()
        .map_err(|_| VisionCallError::Transport)?;
    let status = resp.status();
    if status.is_server_error() {
        return Err(VisionCallError::Transport);
    }
    if !status.is_success() {
        return Err(VisionCallError::Client);
    }
    let json: serde_json::Value = resp.json().map_err(|_| VisionCallError::Parse)?;
    json["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or(VisionCallError::Parse)
}

fn post_codex(
    endpoint: &VisionEndpoint,
    prompt: &str,
    jpeg_bytes: Option<&[u8]>,
    retry_on_401: bool,
) -> Result<String, VisionCallError> {
    let url = format!("{}/responses", endpoint.base_url.trim_end_matches('/'));
    let mut content = vec![serde_json::json!({"type": "input_text", "text": prompt})];
    if let Some(jpeg) = jpeg_bytes {
        let b64 = STANDARD.encode(jpeg);
        content.push(serde_json::json!({
            "type": "input_image",
            "image_url": format!("data:image/jpeg;base64,{b64}")
        }));
    }
    let body = serde_json::json!({
        "model": endpoint.model,
        "stream": true,
        "store": false,
        "input": [{
            "type": "message",
            "role": "user",
            "content": content
        }]
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(CODEX_TIMEOUT_SECS))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|_| VisionCallError::Transport)?;
    let resp = client
        .post(&url)
        .header("ChatGPT-Account-ID", &endpoint.account_id)
        .header("OpenAI-Beta", "responses=experimental")
        .header("originator", CODEX_ORIGINATOR)
        .header("Accept", "text/event-stream")
        .bearer_auth(&endpoint.api_key)
        .json(&body)
        .send()
        .map_err(|_| VisionCallError::Transport)?;
    let status = resp.status();
    if status.as_u16() == 401 && retry_on_401 {
        if let Some(path) = default_auth_path() {
            if let Some(session) = load_session_from_path(&path) {
                if let Some(CodexSession::ChatGpt {
                    access_token,
                    account_id,
                    ..
                }) = refresh_chatgpt_session(&path, &session)
                {
                    let mut retry = endpoint.clone();
                    retry.api_key = access_token;
                    retry.account_id = account_id;
                    return post_codex(&retry, prompt, jpeg_bytes, false);
                }
            }
        }
        return Err(VisionCallError::Client);
    }
    if status.is_server_error() {
        return Err(VisionCallError::Transport);
    }
    if !status.is_success() {
        return Err(VisionCallError::Client);
    }
    let text = resp.text().map_err(|_| VisionCallError::Parse)?;
    sse_output_text(&text).ok_or(VisionCallError::Parse)
}

/// POST OpenAI-compatible chat completions with a JPEG screenshot; vision failures return `Err`.
pub fn call_vision_endpoint(
    jpeg_bytes: &[u8],
    endpoint: &VisionEndpoint,
    sanitized: &SanitizedVisionContext,
    match_context: Option<VisionMatchContext>,
) -> Result<VisionResult, VisionCallError> {
    let prompt = build_vision_prompt(sanitized);
    let content = complete_json(endpoint, &prompt, Some(jpeg_bytes))?;
    parse_vision_json(&content, match_context).map_err(|_| VisionCallError::Parse)
}

/// Only timeout / network / HTTP 5xx on the first usable endpoint try the next.
pub fn analyze_with_chain(
    jpeg_bytes: &[u8],
    chain: &[VisionEndpoint],
    sanitized: &SanitizedVisionContext,
    match_context: Option<VisionMatchContext>,
) -> Result<VisionResult, ()> {
    let usable: Vec<&VisionEndpoint> = chain.iter().filter(|ep| ep.usable()).collect();
    if usable.is_empty() {
        return Err(());
    }
    for (i, ep) in usable.iter().enumerate() {
        match call_vision_endpoint(jpeg_bytes, ep, sanitized, match_context.clone()) {
            Ok(v) => return Ok(v),
            Err(VisionCallError::Transport) => continue,
            Err(VisionCallError::Client | VisionCallError::Parse) => {
                if i == 0 {
                    return Err(());
                }
                return Err(());
            }
        }
    }
    Err(())
}

pub fn analyze_with_fallback(
    jpeg_bytes: &[u8],
    primary: Option<&VisionEndpoint>,
    fallback: Option<&VisionEndpoint>,
    sanitized: &SanitizedVisionContext,
    match_context: Option<VisionMatchContext>,
) -> Result<VisionResult, ()> {
    let chain: Vec<VisionEndpoint> = [primary, fallback]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    analyze_with_chain(jpeg_bytes, &chain, sanitized, match_context)
}

/// POST OpenAI-compatible chat completions with a JPEG screenshot; vision failures return `Err`.
pub fn call_vision_api(
    jpeg_bytes: &[u8],
    api_key: &str,
    sanitized: &SanitizedVisionContext,
    match_context: Option<VisionMatchContext>,
) -> Result<VisionResult, ()> {
    analyze_with_fallback(
        jpeg_bytes,
        Some(&VisionEndpoint::compat(
            openai_base_url(),
            openai_model(),
            api_key.to_string(),
        )),
        None,
        sanitized,
        match_context,
    )
}

pub fn analyze_screenshot(
    path: &Path,
    api_key: &str,
    ctx: VisionContext,
    never_capture: &[String],
) -> Result<VisionResult, ()> {
    analyze_screenshot_with_fallback(
        path,
        Some(&VisionEndpoint::compat(
            openai_base_url(),
            openai_model(),
            api_key.to_string(),
        )),
        None,
        ctx,
        never_capture,
    )
}

pub fn analyze_screenshot_with_chain(
    path: &Path,
    chain: &[VisionEndpoint],
    ctx: VisionContext,
    never_capture: &[String],
) -> Result<VisionResult, ()> {
    let match_context = Some(VisionMatchContext {
        app: ctx.capture.app.clone(),
        title: ctx.capture.title.clone(),
        document_path: ctx.capture.document_path.clone(),
    });
    let sanitized = sanitize_vision_context(ctx, never_capture).map_err(|_| ())?;
    let jpeg = read_and_prepare_jpeg(path)?;
    analyze_with_chain(&jpeg, chain, &sanitized, match_context)
}

pub fn analyze_screenshot_with_fallback(
    path: &Path,
    primary: Option<&VisionEndpoint>,
    fallback: Option<&VisionEndpoint>,
    ctx: VisionContext,
    never_capture: &[String],
) -> Result<VisionResult, ()> {
    let chain: Vec<VisionEndpoint> = [primary, fallback]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    analyze_screenshot_with_chain(path, &chain, ctx, never_capture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_core_research_maps_to_wants_core() {
        let json = r#"{"category":"core_research","confidence":0.91,"reason":"sim viewport"}"#;
        let ctx = Some(VisionMatchContext {
            app: "Isaac Sim".into(),
            title: "robot".into(),
            document_path: None,
        });
        let v = parse_vision_json(json, ctx.clone()).unwrap();
        assert!(v.wants_core);
        assert!((v.confidence - 0.91).abs() < 1e-6);
        assert_eq!(v.match_context.as_ref().map(|c| c.app.as_str()), Some("Isaac Sim"));
    }

    #[test]
    fn parse_distraction_maps_to_not_wants_core() {
        let json = r#"{"category":"distraction","confidence":0.8,"reason":"social feed"}"#;
        let ctx = Some(VisionMatchContext {
            app: "Safari".into(),
            title: "".into(),
            document_path: None,
        });
        let v = parse_vision_json(json, ctx).unwrap();
        assert!(!v.wants_core);
    }

    #[test]
    fn parse_rejects_unknown_category() {
        let json = r#"{"category":"unknown","confidence":0.5,"reason":""}"#;
        assert!(parse_vision_json(json, None).is_err());
    }

    #[test]
    fn jpeg_resize_caps_long_edge() {
        let img = image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_pixel(2000, 1000, image::Rgba([0, 0, 0, 255])),
        );
        let mut raw = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut raw),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
        let jpeg = jpeg_bytes_for_upload(&raw).unwrap();
        let decoded = image::load_from_memory(&jpeg).unwrap();
        assert!(decoded.width().max(decoded.height()) <= JPEG_MAX_LONG_EDGE);
    }

    fn protected_vision_ctx() -> VisionContext {
        use gamelife_core::{ActivitySummary, CaptureContext, HintSeconds};
        VisionContext {
            slot_start: 0,
            slot_end: 900,
            quests: vec![],
            capture: CaptureContext {
                app: "1Password".into(),
                bundle_id: None,
                title: "Bank Account Password".into(),
                document_path: Some("/secret".into()),
                url: None,
                secure_input: false,
            },
            activity_summary: ActivitySummary {
                top_windows: vec![],
                hint_seconds: HintSeconds::default(),
                unobserved_seconds: 0,
            },
        }
    }

    #[test]
    fn protected_capture_does_not_require_screenshot_file() {
        let err = analyze_screenshot(
            Path::new("/no/such/screenshot.jpg"),
            "sk-test",
            protected_vision_ctx(),
            &gamelife_core::builtin_never_capture(),
        );
        assert!(err.is_err());
    }

    #[test]
    fn endpoints_from_settings_skips_missing_keys() {
        let mut settings = crate::config::default_settings();
        crate::config::normalize_vision_providers(&mut settings);
        let chain = endpoints_from_settings_with_codex(&settings, |id| match id {
            "openai" | "opencode-go" => None,
            other if other.starts_with("custom") => Some("sk-custom".into()),
            _ => None,
        }, None);
        assert!(chain.is_empty());
        settings.vision_providers.push(crate::config::VisionProviderSettings {
            id: "custom-2".into(),
            kind: crate::config::KIND_CUSTOM.into(),
            base_url: "https://example.test/v1".into(),
            model: "vision".into(),
        });
        let chain = endpoints_from_settings_with_codex(&settings, |id| match id {
            "custom-2" => Some("sk-custom".into()),
            _ => None,
        }, None);
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].api_key, "sk-custom");
        assert_eq!(chain[0].kind, EndpointKind::Compat);
    }

    #[test]
    fn endpoints_use_codex_session_without_secret_key() {
        let mut settings = crate::config::default_settings();
        crate::config::normalize_vision_providers(&mut settings);
        let session = CodexSession::ChatGpt {
            access_token: "tok".into(),
            refresh_token: "rt".into(),
            account_id: "acct".into(),
        };
        let chain = endpoints_from_settings_with_codex(&settings, |_| None, Some(session));
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].kind, EndpointKind::Codex);
        assert_eq!(chain[0].account_id, "acct");
        assert_eq!(chain[0].api_key, "tok");
    }
}
