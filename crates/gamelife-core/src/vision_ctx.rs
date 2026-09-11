use chrono::{Local, TimeZone};

use crate::policy::matches_app_identity;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CaptureContext {
    pub app: String,
    pub bundle_id: Option<String>,
    pub title: String,
    pub document_path: Option<String>,
    pub url: Option<String>,
    pub secure_input: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowShare {
    pub app: String,
    pub title: String,
    pub seconds: i64,
    pub bundle_id: Option<String>,
    pub document_path: Option<String>,
    pub url: Option<String>,
    pub secure_input: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HintSeconds {
    pub core_candidate: i64,
    pub core_reading: i64,
    pub unsure: i64,
    pub unsure_reading: i64,
    pub side: i64,
    pub distraction: i64,
    pub away: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivitySummary {
    pub top_windows: Vec<WindowShare>,
    pub hint_seconds: HintSeconds,
    pub unobserved_seconds: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisionContext {
    pub slot_start: i64,
    pub slot_end: i64,
    pub quests: Vec<String>,
    pub capture: CaptureContext,
    pub activity_summary: ActivitySummary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisionPrivacyError {
    ProtectedCapture,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SanitizedVisionContext {
    inner: VisionContext,
}

pub fn is_protected_frontmost(
    app: &str,
    bundle_id: Option<&str>,
    secure_input: bool,
    never_capture: &[String],
) -> bool {
    secure_input || matches_app_identity(app, bundle_id, never_capture)
}

pub fn sanitize_vision_context(
    mut ctx: VisionContext,
    never_capture: &[String],
) -> Result<SanitizedVisionContext, VisionPrivacyError> {
    if is_protected_frontmost(
        &ctx.capture.app,
        ctx.capture.bundle_id.as_deref(),
        ctx.capture.secure_input,
        never_capture,
    ) {
        return Err(VisionPrivacyError::ProtectedCapture);
    }
    for window in &mut ctx.activity_summary.top_windows {
        if is_protected_frontmost(
            &window.app,
            window.bundle_id.as_deref(),
            window.secure_input,
            never_capture,
        ) {
            window.app = "[Protected App]".into();
            window.title.clear();
            window.bundle_id = None;
            window.document_path = None;
            window.url = None;
        }
    }
    Ok(SanitizedVisionContext { inner: ctx })
}

pub fn build_vision_prompt(ctx: &SanitizedVisionContext) -> String {
    let ctx = &ctx.inner;
    let range = format!(
        "{}–{}",
        format_hhmm(ctx.slot_start),
        format_hhmm(ctx.slot_end)
    );
    let mut out = String::new();
    out.push_str("Classify this macOS screenshot for productivity tracking.\n");
    out.push_str(&format!(
        "Judge this 15-minute block {range}. Return JSON only with keys category, confidence, reason.\n"
    ));
    out.push_str(
        "category must be one of: core_research, research_support, admin, side_project, distraction, break_away.\n\n",
    );
    out.push_str("Today's main quests:\n");
    if ctx.quests.is_empty() {
        out.push_str("(none)\n");
    } else {
        for (i, quest) in ctx.quests.iter().enumerate() {
            out.push_str(&format!("{}. {quest}\n", i + 1));
        }
    }
    out.push('\n');
    out.push_str(
        "Observed activity in this block (do not treat unobserved time as work; credited time is computed separately):\n",
    );
    for window in &ctx.activity_summary.top_windows {
        let label = if window.title.is_empty() {
            window.app.clone()
        } else {
            format!("{} · {}", window.app, window.title)
        };
        out.push_str(&format!("{label}    {}\n", format_span_secs(window.seconds)));
    }
    out.push('\n');
    out.push_str("Hint seconds: ");
    out.push_str(&format_hint_seconds(&ctx.activity_summary.hint_seconds));
    out.push_str(".\n\n");
    out.push_str("Screenshot context:\n");
    out.push_str(&format!("app: {}\n", ctx.capture.app));
    out.push_str(&format!("title: {}\n", ctx.capture.title));
    out.push_str(&format!(
        "document_path: {}\n",
        optional_field(ctx.capture.document_path.as_deref())
    ));
    out.push_str(&format!(
        "url: {}\n",
        optional_field(ctx.capture.url.as_deref())
    ));
    out
}

pub fn format_span_secs(secs: i64) -> String {
    let secs = secs.max(0);
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

fn format_hhmm(ts: i64) -> String {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).unwrap())
        .format("%H:%M")
        .to_string()
}

fn optional_field(value: Option<&str>) -> &str {
    match value {
        Some(v) if !v.is_empty() => v,
        _ => "(none)",
    }
}

fn format_hint_seconds(h: &HintSeconds) -> String {
    let parts = [
        ("core_candidate", h.core_candidate),
        ("core_reading", h.core_reading),
        ("unsure", h.unsure),
        ("unsure_reading", h.unsure_reading),
        ("side", h.side),
        ("distraction", h.distraction),
        ("away", h.away),
    ];
    let formatted: Vec<String> = parts
        .iter()
        .filter(|(_, secs)| *secs > 0)
        .map(|(name, secs)| format!("{name} {}", format_span_secs(*secs)))
        .collect();
    if formatted.is_empty() {
        "(none)".into()
    } else {
        formatted.join(", ")
    }
}

pub fn activity_summary_for_vision(
    evidence: &crate::judge::SlotEvidence,
    samples: &[crate::types::Sample],
) -> ActivitySummary {
    use crate::observe::SpanKind;
    use crate::types::Hint;

    let mut hint_seconds = HintSeconds::default();
    let mut unobserved_seconds = 0;
    let mut top_windows: Vec<WindowShare> = Vec::new();

    for span in &evidence.spans {
        let secs = span.end - span.start;
        match span.kind {
            SpanKind::Unobserved => unobserved_seconds += secs,
            SpanKind::Observed(hint) => {
                match hint {
                    Hint::CoreCandidate => hint_seconds.core_candidate += secs,
                    Hint::CoreReading => hint_seconds.core_reading += secs,
                    Hint::Unsure => hint_seconds.unsure += secs,
                    Hint::UnsureReading => hint_seconds.unsure_reading += secs,
                    Hint::Side => hint_seconds.side += secs,
                    Hint::Distraction => hint_seconds.distraction += secs,
                    Hint::Away => hint_seconds.away += secs,
                }
                let Some(sample) = span.sample_index.and_then(|i| samples.get(i)) else {
                    continue;
                };
                if let Some(existing) = top_windows.iter_mut().find(|w| {
                    w.app == sample.app && w.title == sample.window_title
                }) {
                    existing.seconds += secs;
                    existing.secure_input |= sample.secure_input;
                    if existing.document_path.is_none() {
                        existing.document_path = sample.document_path.clone();
                    }
                    if existing.url.is_none() {
                        existing.url = sample.url.clone();
                    }
                    if existing.bundle_id.is_none() {
                        existing.bundle_id = sample.bundle_id.clone();
                    }
                } else {
                    top_windows.push(WindowShare {
                        app: sample.app.clone(),
                        title: sample.window_title.clone(),
                        seconds: secs,
                        bundle_id: sample.bundle_id.clone(),
                        document_path: sample.document_path.clone(),
                        url: sample.url.clone(),
                        secure_input: sample.secure_input,
                    });
                }
            }
        }
    }

    ActivitySummary {
        top_windows,
        hint_seconds,
        unobserved_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::builtin_never_capture;

    fn ctx_with_password_and_cursor() -> VisionContext {
        VisionContext {
            slot_start: 0,
            slot_end: 900,
            quests: vec!["Finish HDP tactile ablation".into()],
            capture: CaptureContext {
                app: "Cursor".into(),
                bundle_id: Some("com.todesktop.230313mzl4w4u92".into()),
                title: "train.py — HDP".into(),
                document_path: Some("/Users/me/Projects/HDP/train.py".into()),
                url: None,
                secure_input: false,
            },
            activity_summary: ActivitySummary {
                top_windows: vec![
                    WindowShare {
                        app: "Cursor".into(),
                        title: "train.py — HDP".into(),
                        seconds: 600,
                        bundle_id: None,
                        document_path: Some("/Users/me/Projects/HDP/train.py".into()),
                        url: None,
                        secure_input: false,
                    },
                    WindowShare {
                        app: "1Password".into(),
                        title: "Bank Account Password".into(),
                        seconds: 300,
                        bundle_id: None,
                        document_path: Some("/secret".into()),
                        url: None,
                        secure_input: false,
                    },
                ],
                hint_seconds: HintSeconds {
                    core_candidate: 600,
                    ..HintSeconds::default()
                },
                unobserved_seconds: 0,
            },
        }
    }

    #[test]
    fn format_span_secs_five_minutes() {
        assert_eq!(format_span_secs(300), "5m00s");
    }

    #[test]
    fn history_protected_window_is_redacted_but_cursor_capture_is_ok() {
        let ctx = ctx_with_password_and_cursor();
        let never = builtin_never_capture();
        let sanitized = sanitize_vision_context(ctx, &never).unwrap();
        let prompt = build_vision_prompt(&sanitized);
        assert!(prompt.contains("[Protected App]"));
        assert!(prompt.contains("Cursor · train.py — HDP"));
        assert!(!prompt.contains("Bank Account Password"));
        assert!(!prompt.contains("1Password"));
        assert!(!prompt.contains("/secret"));
    }

    #[test]
    fn protected_capture_forbids_vision_request() {
        let mut ctx = ctx_with_password_and_cursor();
        ctx.capture.app = "1Password".into();
        ctx.capture.title = "Bank Account Password".into();
        let never = builtin_never_capture();
        assert!(matches!(
            sanitize_vision_context(ctx, &never),
            Err(VisionPrivacyError::ProtectedCapture)
        ));
    }

    #[test]
    fn secure_input_capture_forbids_vision_request() {
        let mut ctx = ctx_with_password_and_cursor();
        ctx.capture.secure_input = true;
        assert!(matches!(
            sanitize_vision_context(ctx, &[]),
            Err(VisionPrivacyError::ProtectedCapture)
        ));
    }

    #[test]
    fn summary_uses_evidence_not_ts_lookup() {
        use crate::judge::analyze_slot_evidence;
        use crate::policy::Policy;
        use crate::types::Sample;

        let samples = vec![Sample {
            ts: 10,
            app: "Cursor".into(),
            window_title: "t".into(),
            url: None,
            document_path: None,
            bundle_id: None,
            idle_seconds: 1,
            screen_locked: false,
            paused: false,
            secure_input: false,
        }];
        let policy = Policy {
            trusted_apps: vec![],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
        };
        let ev = analyze_slot_evidence(&samples, &policy, &[], 0, 40);
        let lead = ev.spans.iter().find(|s| s.start == 0).unwrap();
        assert_eq!(lead.sample_index, Some(0));
        let sum = activity_summary_for_vision(&ev, &samples);
        assert!(sum.top_windows.iter().any(|w| w.app == "Cursor"));
    }
}
