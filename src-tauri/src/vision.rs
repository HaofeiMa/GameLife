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
    sanitized: &SanitizedVisionContext,
    match_context: Option<VisionMatchContext>,
) -> Result<VisionResult, ()> {
    let b64 = STANDARD.encode(jpeg_bytes);
    let prompt = build_vision_prompt(sanitized);
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
    parse_vision_json(content, match_context).map_err(|_| ())
}

pub fn analyze_screenshot(
    path: &Path,
    api_key: &str,
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
    call_vision_api(&jpeg, api_key, &sanitized, match_context)
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
}
