use std::io::Cursor;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD, Engine};
use gamelife_core::judge::VisionResult;
use image::imageops::FilterType;
use image::GenericImageView;
use serde::Deserialize;

pub const VISION_TIMEOUT_SECS: u64 = 20;
pub const JPEG_MAX_LONG_EDGE: u32 = 1280;

#[derive(Debug, Deserialize)]
struct VisionApiPayload {
    category: String,
    confidence: f64,
    #[allow(dead_code)]
    reason: String,
}

/// Map OpenAI-compatible JSON `{"category","confidence","reason"}` into `VisionResult`.
pub fn parse_vision_json(json: &str, context_app: Option<String>) -> Result<VisionResult, ()> {
    let payload: VisionApiPayload = serde_json::from_str(json).map_err(|_| ())?;
    Ok(map_vision_payload(&payload.category, payload.confidence, context_app))
}

pub fn map_vision_payload(
    category: &str,
    confidence: f64,
    context_app: Option<String>,
) -> VisionResult {
    let wants_core = category == "core_research";
    VisionResult {
        wants_core,
        confidence,
        context_app,
        category: category.to_string(),
    }
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

/// POST OpenAI-compatible chat completions with a JPEG screenshot; vision failures return `Err`.
pub fn call_vision_api(
    jpeg_bytes: &[u8],
    api_key: &str,
    context_app: Option<&str>,
) -> Result<VisionResult, ()> {
    let b64 = STANDARD.encode(jpeg_bytes);
    let app_hint = context_app
        .map(|a| format!("Frontmost app at capture: {a}."))
        .unwrap_or_default();
    let prompt = format!(
        "Classify this macOS screenshot for productivity tracking. \
Return JSON only with keys category, confidence, reason. \
category must be one of: core_research, research_support, admin, side_project, distraction, break_away, unknown. \
{app_hint}"
    );
    let url = format!("{}/chat/completions", openai_base_url().trim_end_matches('/'));
    let body = serde_json::json!({
        "model": openai_model(),
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
        .build()
        .map_err(|_| ())?;
    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .map_err(|_| ())?;
    if !resp.status().is_success() {
        return Err(());
    }
    let json: serde_json::Value = resp.json().map_err(|_| ())?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or(())?;
    parse_vision_json(content, context_app.map(str::to_string))
}

pub fn analyze_screenshot(
    path: &Path,
    api_key: &str,
    context_app: Option<&str>,
) -> Result<VisionResult, ()> {
    let jpeg = read_and_prepare_jpeg(path)?;
    call_vision_api(&jpeg, api_key, context_app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_core_research_maps_to_wants_core() {
        let json = r#"{"category":"core_research","confidence":0.91,"reason":"sim viewport"}"#;
        let v = parse_vision_json(json, Some("Isaac Sim".into())).unwrap();
        assert!(v.wants_core);
        assert!((v.confidence - 0.91).abs() < 1e-6);
        assert_eq!(v.context_app.as_deref(), Some("Isaac Sim"));
    }

    #[test]
    fn parse_distraction_maps_to_not_wants_core() {
        let json = r#"{"category":"distraction","confidence":0.8,"reason":"social feed"}"#;
        let v = parse_vision_json(json, Some("Safari".into())).unwrap();
        assert!(!v.wants_core);
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
}
