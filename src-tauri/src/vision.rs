use std::io::Cursor;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD, Engine};
use gamelife_core::judge::{parse_vision_json, VisionMatchContext, VisionResult};
use gamelife_core::{
    build_vision_prompt, sanitize_vision_context, SanitizedVisionContext, VisionContext,
};
use image::imageops::FilterType;
use image::GenericImageView;

pub const VISION_TIMEOUT_SECS: u64 = 20;
pub const JPEG_MAX_LONG_EDGE: u32 = 1280;
pub const USER_AGENT: &str = "GameLife/0.1";

#[derive(Clone, Debug)]
pub struct VisionEndpoint {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
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
) -> (Option<VisionEndpoint>, Option<VisionEndpoint>) {
    fn lookup(
        settings: &crate::config::AppSettings,
        id: &str,
        key_for: &impl Fn(&str) -> Option<String>,
    ) -> Option<VisionEndpoint> {
        if id.is_empty() || id == crate::config::PROVIDER_NONE {
            return None;
        }
        let spec = settings.vision_providers.iter().find(|p| p.id == id)?;
        let api_key = key_for(id)?;
        if api_key.trim().is_empty() || spec.base_url.trim().is_empty() {
            return None;
        }
        Some(VisionEndpoint {
            base_url: spec.base_url.clone(),
            model: spec.model.clone(),
            api_key,
        })
    }
    let primary = lookup(settings, &settings.primary_provider, &key_for);
    let fallback = if settings.fallback_provider == settings.primary_provider {
        None
    } else {
        lookup(settings, &settings.fallback_provider, &key_for)
    };
    (primary, fallback)
}

/// POST OpenAI-compatible chat completions with a JPEG screenshot; vision failures return `Err`.
pub fn call_vision_endpoint(
    jpeg_bytes: &[u8],
    endpoint: &VisionEndpoint,
    sanitized: &SanitizedVisionContext,
    match_context: Option<VisionMatchContext>,
) -> Result<VisionResult, VisionCallError> {
    if endpoint.api_key.trim().is_empty() || endpoint.base_url.trim().is_empty() {
        return Err(VisionCallError::Client);
    }
    let b64 = STANDARD.encode(jpeg_bytes);
    let prompt = build_vision_prompt(sanitized);
    let url = format!(
        "{}/chat/completions",
        endpoint.base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": endpoint.model,
        "response_format": { "type": "json_object" },
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": prompt },
                { "type": "image_url", "image_url": { "url": format!("data:image/jpeg;base64,{b64}") } }
            ]
        }]
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
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or(VisionCallError::Parse)?;
    parse_vision_json(content, match_context).map_err(|_| VisionCallError::Parse)
}

/// Only timeout / network / HTTP 5xx on the first endpoint try the second.
pub fn analyze_with_fallback(
    jpeg_bytes: &[u8],
    primary: Option<&VisionEndpoint>,
    fallback: Option<&VisionEndpoint>,
    sanitized: &SanitizedVisionContext,
    match_context: Option<VisionMatchContext>,
) -> Result<VisionResult, ()> {
    fn usable(ep: &VisionEndpoint) -> bool {
        !ep.api_key.trim().is_empty() && !ep.base_url.trim().is_empty()
    }
    let mut last_parse_or_client = false;
    if let Some(ep) = primary.filter(|ep| usable(ep)) {
        match call_vision_endpoint(jpeg_bytes, ep, sanitized, match_context.clone()) {
            Ok(v) => return Ok(v),
            Err(VisionCallError::Transport) => {}
            Err(VisionCallError::Client | VisionCallError::Parse) => {
                last_parse_or_client = true;
            }
        }
    }
    if last_parse_or_client {
        return Err(());
    }
    if let Some(ep) = fallback.filter(|ep| usable(ep)) {
        return call_vision_endpoint(jpeg_bytes, ep, sanitized, match_context).map_err(|_| ());
    }
    Err(())
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
        Some(&VisionEndpoint {
            base_url: openai_base_url(),
            model: openai_model(),
            api_key: api_key.to_string(),
        }),
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
        Some(&VisionEndpoint {
            base_url: openai_base_url(),
            model: openai_model(),
            api_key: api_key.to_string(),
        }),
        None,
        ctx,
        never_capture,
    )
}

pub fn analyze_screenshot_with_fallback(
    path: &Path,
    primary: Option<&VisionEndpoint>,
    fallback: Option<&VisionEndpoint>,
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
    analyze_with_fallback(&jpeg, primary, fallback, &sanitized, match_context)
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
        settings.primary_provider = "opencode-go".into();
        settings.fallback_provider = "openai".into();
        let (primary, fallback) = endpoints_from_settings(&settings, |id| match id {
            "openai" => Some("sk-openai".into()),
            _ => None,
        });
        assert!(primary.is_none());
        assert_eq!(fallback.as_ref().map(|e| e.api_key.as_str()), Some("sk-openai"));
    }
}
