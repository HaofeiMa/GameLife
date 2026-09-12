use crate::capture::CaptureStatus;
use crate::hint::{hint_sample, is_grounded_core_sample};
use crate::observe::{Span, SpanKind, observed_seconds, spans_for_slot};
use crate::policy::Policy;
use crate::r#const::READING_BRIDGE_SECS;
use crate::types::{ActivitySeconds, Hint, Quest, Sample};

const LOW_INPUT_IDLE_SECS: i64 = 180;
const AWAY_DOMINANT_SECS: i64 = 600;
const STRONG_CORE_AUTO_SECS: i64 = 780;
const SIDE_DISTRACTION_DOMINANT_SECS: i64 = 300;
const SIDE_DISTRACTION_MAX_FOR_AUTO_CORE: i64 = 60;
const VISION_CONFIDENCE_MIN: f64 = 0.7;
const LEGAL_VISION_CATEGORIES: &[&str] = &[
    "core_research",
    "research_support",
    "admin",
    "side_project",
    "distraction",
    "break_away",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisionMatchContext {
    pub app: String,
    pub title: String,
    pub document_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisionResult {
    pub wants_core: bool,
    pub confidence: f64,
    pub match_context: Option<VisionMatchContext>,
    pub category: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisionParseError {
    InvalidJson,
    InvalidCategory,
    InvalidConfidence,
    InvalidReason,
}

pub fn parse_vision_json(
    json: &str,
    match_context: Option<VisionMatchContext>,
) -> Result<VisionResult, VisionParseError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| VisionParseError::InvalidJson)?;
    let obj = value
        .as_object()
        .ok_or(VisionParseError::InvalidJson)?;

    let category = match obj.get("category") {
        Some(serde_json::Value::String(s)) if LEGAL_VISION_CATEGORIES.contains(&s.as_str()) => {
            s.clone()
        }
        _ => return Err(VisionParseError::InvalidCategory),
    };

    let confidence = match obj.get("confidence") {
        Some(serde_json::Value::Number(n)) => n
            .as_f64()
            .ok_or(VisionParseError::InvalidConfidence)?,
        _ => return Err(VisionParseError::InvalidConfidence),
    };
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return Err(VisionParseError::InvalidConfidence);
    }

    match obj.get("reason") {
        None | Some(serde_json::Value::String(_)) => {}
        _ => return Err(VisionParseError::InvalidReason),
    }

    Ok(VisionResult {
        wants_core: category == "core_research",
        confidence,
        match_context,
        category,
    })
}

pub struct JudgeInput<'a> {
    pub slot_start: i64,
    pub slot_end: i64,
    pub samples: &'a [Sample],
    pub quests: &'a [Quest],
    pub tasks: &'a [crate::task::TaskSnapshot],
    pub policy: &'a Policy,
    pub capture: CaptureStatus,
    pub vision: Option<VisionResult>,
    pub manual_core: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dominant {
    CoreResearch,
    ResearchSupport,
    Admin,
    SideProject,
    Distraction,
    BreakAway,
    Unobserved,
    PendingReview,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JudgeOutput {
    pub dominant: Dominant,
    pub activity: ActivitySeconds,
    pub credited_core_seconds: i64,
    pub credited_side_seconds: i64,
    pub credited_chore_seconds: i64,
    pub observed_seconds: i64,
    pub used_vision: bool,
    pub pending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotEvidence {
    pub hints: Vec<Hint>,
    pub spans: Vec<Span>,
    pub activity: ActivitySeconds,
    pub strong_core_seconds: i64,
    pub grounded_strong_core_seconds: i64,
    pub reading_bridge_seconds: i64,
    pub observed_seconds: i64,
}

pub fn analyze_slot_evidence(
    samples: &[Sample],
    policy: &Policy,
    quests: &[Quest],
    slot_start: i64,
    slot_end: i64,
) -> SlotEvidence {
    let mut last_core_interaction_ts: Option<i64> = None;
    let mut hints = Vec::with_capacity(samples.len());
    for sample in samples {
        let hint = hint_sample(sample, policy, quests, last_core_interaction_ts);
        if sample.idle_seconds < LOW_INPUT_IDLE_SECS && hint == Hint::CoreCandidate {
            last_core_interaction_ts = Some(sample.ts);
        }
        hints.push(hint);
    }

    let spans = spans_for_slot(samples, &hints, slot_start, slot_end);
    let mut activity = ActivitySeconds::default();
    let mut strong_core = 0_i64;
    let mut grounded_strong = 0_i64;
    let mut reading_bridge = 0_i64;

    for span in &spans {
        let secs = span.end - span.start;
        match span.kind {
            SpanKind::Unobserved => activity.unobserved += secs,
            SpanKind::Observed(hint) => {
                accumulate_hint_activity(&mut activity, hint, secs);
                let idle = span
                    .sample_index
                    .and_then(|i| samples.get(i))
                    .map(|s| s.idle_seconds)
                    .unwrap_or(0);
                match hint {
                    Hint::CoreCandidate if idle < LOW_INPUT_IDLE_SECS => {
                        strong_core += secs;
                        if let Some(sample) = span.sample_index.and_then(|i| samples.get(i)) {
                            if is_grounded_core_sample(sample, quests) {
                                grounded_strong += secs;
                            }
                        }
                    }
                    Hint::CoreReading => {
                        reading_bridge += secs;
                    }
                    _ => {}
                }
            }
        }
    }
    reading_bridge = reading_bridge.min(READING_BRIDGE_SECS as i64);
    let observed = observed_seconds(&spans);

    SlotEvidence {
        hints,
        spans,
        activity,
        strong_core_seconds: strong_core,
        grounded_strong_core_seconds: grounded_strong,
        reading_bridge_seconds: reading_bridge,
        observed_seconds: observed,
    }
}

/// Slot judge pipeline:
/// 1. analyze_slot_evidence (hints + spans + activity)
/// 2. branch: unobserved / break-away / strong core / side-distraction / gray+vision
/// 3. credited = min(observed, actual, strong + bridge + verified); tasks empty → 0
pub fn judge_slot(input: JudgeInput<'_>) -> JudgeOutput {
    let actual = input.slot_end - input.slot_start;
    let tasks_empty = input.tasks.is_empty();
    let ev = analyze_slot_evidence(
        input.samples,
        input.policy,
        input.quests,
        input.slot_start,
        input.slot_end,
    );
    let spans = ev.spans;
    let mut activity = ev.activity;
    let strong_core = ev.strong_core_seconds;
    let grounded_strong_core = ev.grounded_strong_core_seconds;
    let reading_bridge = ev.reading_bridge_seconds;
    let observed = ev.observed_seconds;

    // Step 5–6: branches
    let mut used_vision = false;
    let mut pending = false;
    let mut dominant = Dominant::Unknown;
    let mut credited_raw = 0_i64;

    if activity.unobserved == actual {
        dominant = Dominant::Unobserved;
    } else if activity.away >= AWAY_DOMINANT_SECS && strong_core < 300 {
        dominant = Dominant::BreakAway;
    } else if !tasks_empty
        && grounded_strong_core >= STRONG_CORE_AUTO_SECS
        && activity.side + activity.distraction <= SIDE_DISTRACTION_MAX_FOR_AUTO_CORE
    {
        dominant = Dominant::CoreResearch;
        credited_raw = grounded_strong_core + reading_bridge;
    } else if activity.side + activity.distraction >= SIDE_DISTRACTION_DOMINANT_SECS
        && activity.side + activity.distraction > strong_core + reading_bridge
    {
        dominant = if activity.side >= activity.distraction {
            Dominant::SideProject
        } else {
            Dominant::Distraction
        };
    } else {
        // Gray zone
        let vision = input.vision.as_ref();
        let vision_confident = vision.is_some_and(|v| v.confidence >= VISION_CONFIDENCE_MIN);
        let vision_wants_core =
            vision_confident && vision.is_some_and(|v| v.wants_core);
        let vision_rejects_core =
            vision_confident && vision.is_some_and(|v| !v.wants_core);
        let manual_core = input.manual_core == Some(true);

        if vision_wants_core && strong_core > 0 {
            let context_matches = input.vision.as_ref().is_some_and(|v| {
                v.match_context
                    .as_ref()
                    .is_some_and(|ctx| span_context_matches(input.samples, ctx, &spans))
            });
            if !context_matches && input.manual_core.is_none() {
                pending = true;
            }
        }

        if vision_rejects_core
            && strong_core + reading_bridge > 0
            && strong_core + reading_bridge > activity.side + activity.distraction
            && input.manual_core.is_none()
        {
            pending = true;
        }

        if !pending
            && matches!(
                input.capture,
                CaptureStatus::Missed | CaptureStatus::Skipped | CaptureStatus::Captured
            )
            && input.vision.is_none()
            && input.manual_core.is_none()
        {
            pending = true;
        }

        if input.manual_core == Some(false) {
            credited_raw = 0;
        } else if !pending && (manual_core || vision_wants_core) {
            let verified_core = upgrade_verified_core(input, &spans);
            if verified_core > 0 {
                activity.core += verified_core;
                if vision_wants_core && !manual_core {
                    used_vision = true;
                }
            }
            credited_raw = strong_core + reading_bridge + verified_core;
            if credited_raw > 0 {
                dominant = Dominant::CoreResearch;
            }
        } else if !pending && vision_confident && vision_rejects_core && input.manual_core.is_none() {
            let unsure_secs = sum_unsure_spans(&spans);
            match vision.map(|v| v.category.as_str()) {
                Some("research_support") if unsure_secs > 0 => {
                    activity.support += unsure_secs;
                    dominant = Dominant::ResearchSupport;
                }
                Some("admin") if unsure_secs > 0 => {
                    activity.admin += unsure_secs;
                    dominant = Dominant::Admin;
                }
                _ => {}
            }
        }
    }

    if pending {
        dominant = Dominant::PendingReview;
        credited_raw = 0;
    }

    // Step 7–8: final credited cap
    let mut credited = credited_raw.min(observed).min(actual).min(900);
    if tasks_empty {
        credited = 0;
        if dominant == Dominant::CoreResearch {
            dominant = dominant_from_activity(&activity, 0);
        }
    }

    if dominant == Dominant::Unknown {
        dominant = dominant_from_activity(&activity, credited);
    }

    JudgeOutput {
        dominant,
        activity,
        credited_core_seconds: credited,
        credited_side_seconds: 0,
        credited_chore_seconds: 0,
        observed_seconds: observed,
        used_vision,
        pending,
    }
}

fn accumulate_hint_activity(activity: &mut ActivitySeconds, hint: Hint, secs: i64) {
    match hint {
        Hint::Away => activity.away += secs,
        Hint::Distraction => activity.distraction += secs,
        Hint::Admin => activity.admin += secs,
        Hint::Side => activity.side += secs,
        Hint::CoreCandidate | Hint::CoreReading => activity.core += secs,
        Hint::UnsureReading | Hint::Unsure => {}
    }
}

/// Seconds from unsure / unsure_reading spans (for vision support/admin mapping).
pub fn sum_unsure_spans(spans: &[Span]) -> i64 {
    spans
        .iter()
        .filter_map(|span| {
            if let SpanKind::Observed(hint) = span.kind {
                if matches!(hint, Hint::Unsure | Hint::UnsureReading) {
                    return Some(span.end - span.start);
                }
            }
            None
        })
        .sum()
}

/// Extract up to `limit` seconds of credited-core time spans (unix timestamps).
pub fn credited_core_spans(
    samples: &[Sample],
    spans: &[Span],
    limit: i64,
    include_verified_unsure: bool,
) -> Vec<(i64, i64)> {
    if limit <= 0 {
        return vec![];
    }
    let mut out = Vec::new();
    let mut total = 0_i64;
    for span in spans {
        if total >= limit {
            break;
        }
        let secs = span.end - span.start;
        if secs <= 0 {
            continue;
        }
        let creditable = match span.kind {
            SpanKind::Unobserved => false,
            SpanKind::Observed(hint) => {
                let idle = span
                    .sample_index
                    .and_then(|i| samples.get(i))
                    .map(|s| s.idle_seconds)
                    .unwrap_or(0);
                match hint {
                    Hint::CoreCandidate if idle < LOW_INPUT_IDLE_SECS => true,
                    Hint::CoreReading => true,
                    Hint::Unsure | Hint::UnsureReading if include_verified_unsure => true,
                    _ => false,
                }
            }
        };
        if !creditable {
            continue;
        }
        let take = (limit - total).min(secs);
        out.push((span.start, span.start + take));
        total += take;
    }
    out
}

fn sample_for_span<'a>(samples: &'a [Sample], span: &Span) -> Option<&'a Sample> {
    span.sample_index.and_then(|i| samples.get(i))
}

fn span_matches_capture(sample: &Sample, ctx: &VisionMatchContext) -> bool {
    if !sample.app.eq_ignore_ascii_case(&ctx.app) {
        return false;
    }
    match (&sample.document_path, &ctx.document_path) {
        (Some(sample_path), Some(ctx_path)) => return sample_path == ctx_path,
        _ => {}
    }
    if !sample.window_title.is_empty() && !ctx.title.is_empty() {
        return sample.window_title == ctx.title;
    }
    true
}

fn span_context_matches(samples: &[Sample], ctx: &VisionMatchContext, spans: &[Span]) -> bool {
    spans.iter().any(|span| {
        matches!(span.kind, SpanKind::Observed(Hint::CoreCandidate | Hint::CoreReading))
            && sample_for_span(samples, span).is_some_and(|s| span_matches_capture(s, ctx))
    })
}

fn upgrade_verified_core(input: JudgeInput<'_>, spans: &[Span]) -> i64 {
    let manual_core = input.manual_core == Some(true);
    let vision = match &input.vision {
        Some(v) if v.wants_core && v.confidence >= VISION_CONFIDENCE_MIN => v,
        _ if manual_core => return upgrade_all_unsure(input.samples, spans),
        _ => return 0,
    };

    if !manual_core {
        match &vision.match_context {
            None => return 0,
            Some(ctx) => {
                let mut total = 0_i64;
                for span in spans {
                    let Some(sample) = sample_for_span(input.samples, span) else {
                        continue;
                    };
                    if !span_matches_capture(sample, ctx) {
                        continue;
                    }
                    if let SpanKind::Observed(hint) = span.kind {
                        if is_verified_upgrade_hint(hint) {
                            total += span.end - span.start;
                        }
                    }
                }
                total
            }
        }
    } else {
        upgrade_all_unsure(input.samples, spans)
    }
}

fn is_verified_upgrade_hint(hint: Hint) -> bool {
    matches!(hint, Hint::Unsure | Hint::UnsureReading)
        && !matches!(hint, Hint::Side | Hint::Distraction | Hint::Away)
}

fn upgrade_all_unsure(_samples: &[Sample], spans: &[Span]) -> i64 {
    let mut total = 0_i64;
    for span in spans {
        if let SpanKind::Observed(hint) = span.kind {
            if is_verified_upgrade_hint(hint) {
                total += span.end - span.start;
            }
        }
    }
    total
}

fn dominant_from_activity(activity: &ActivitySeconds, credited: i64) -> Dominant {
    if credited > 0 && activity.core >= activity_max_non_core(activity) {
        return Dominant::CoreResearch;
    }

    let entries: [(Dominant, i64); 7] = [
        (Dominant::CoreResearch, activity.core),
        (Dominant::SideProject, activity.side),
        (Dominant::Distraction, activity.distraction),
        (Dominant::BreakAway, activity.away),
        (Dominant::Unobserved, activity.unobserved),
        (Dominant::ResearchSupport, activity.support),
        (Dominant::Admin, activity.admin),
    ];

    let (dom, max_secs) = entries
        .iter()
        .max_by_key(|(_, secs)| *secs)
        .copied()
        .unwrap_or((Dominant::Unknown, 0));

    if max_secs == 0 {
        return Dominant::Unknown;
    }
    if dom == Dominant::Unobserved
        && activity.unobserved > 0
        && activity.unobserved >= activity_total_observed(activity)
    {
        return Dominant::Unobserved;
    }
    dom
}

fn activity_max_non_core(activity: &ActivitySeconds) -> i64 {
    activity
        .side
        .max(activity.distraction)
        .max(activity.away)
        .max(activity.unobserved)
        .max(activity.support)
        .max(activity.admin)
}

fn activity_total_observed(activity: &ActivitySeconds) -> i64 {
    activity.core
        + activity.side
        + activity.distraction
        + activity.away
        + activity.support
        + activity.admin
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{
        CategoryGuides, Policy, builtin_never_capture, builtin_side_project_rules,
        default_distraction_rules,
    };

    fn pol() -> Policy {
        Policy {
            trusted_apps: vec!["Cursor".into(), "Preview".into()],
            distraction_rules: vec!["bilibili".into()],
            side_project_rules: builtin_side_project_rules(),
            reading_apps: vec!["Preview".into()],
            never_capture_apps: builtin_never_capture(),
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        }
    }

    fn mainline_tasks() -> &'static [crate::task::TaskSnapshot] {
        use std::sync::OnceLock;
        static TASKS: OnceLock<Vec<crate::task::TaskSnapshot>> = OnceLock::new();
        TASKS.get_or_init(|| {
            vec![crate::task::TaskSnapshot {
                id: "t1".into(),
                title: "HDP".into(),
                role: crate::task::ListRole::Mainline,
            }]
        })
    }

    fn match_ctx(app: &str, title: &str) -> Option<VisionMatchContext> {
        Some(VisionMatchContext {
            app: app.into(),
            title: title.into(),
            document_path: None,
        })
    }

    fn grid(app: &str, title: &str, start: i64, n: usize, every: i64, idle: i64) -> Vec<Sample> {
        (0..n)
            .map(|i| Sample {
                ts: start + i as i64 * every,
                app: app.into(),
                window_title: title.into(),
                url: None,
                document_path: Some("/paper/main.tex".into()),
                bundle_id: None,
                idle_seconds: idle,
                screen_locked: false,
                paused: false,
                secure_input: false,
            })
            .collect()
    }

    #[test]
    fn isaac_unsure_plus_vision_core_gets_verified_credit() {
        let samples = grid("Isaac Sim", "robot", 0, 60, 15, 2);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "robot".into(),
                evidence: vec!["robot".into()],
                hero: true,
            }],
            tasks: mainline_tasks(),
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: Some(VisionResult {
                wants_core: true,
                confidence: 0.9,
                match_context: match_ctx("Isaac Sim", "robot"),
                category: "core_research".into(),
            }),
            manual_core: None,
        });
        assert!(!out.pending);
        assert!(
            out.credited_core_seconds > 60,
            "verified_core must credit unknown tools, got {}",
            out.credited_core_seconds
        );
        assert!(out.credited_core_seconds <= out.observed_seconds);
    }

    #[test]
    fn vision_paper_does_not_fill_unobserved_or_other_apps() {
        let mut samples = grid("Cursor", "main.tex", 0, 10, 15, 2);
        samples.extend(grid("WeChat", "chat", 400, 10, 15, 2));
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "paper".into(),
                evidence: vec!["main.tex".into()],
                hero: true,
            }],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: Some(VisionResult {
                wants_core: true,
                confidence: 0.95,
                match_context: match_ctx("Cursor", "main.tex"),
                category: "core_research".into(),
            }),
            manual_core: None,
        });
        assert!(out.credited_core_seconds < 900);
    }

    #[test]
    fn invariant_credited_le_observed_le_duration() {
        let samples = grid("Cursor", "main.tex", 0, 5, 15, 2);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 400,
            samples: &samples,
            quests: &[Quest {
                text: "paper".into(),
                evidence: vec!["main.tex".into()],
                hero: true,
            }],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Missed,
            vision: None,
            manual_core: None,
        });
        assert!(out.credited_core_seconds <= out.observed_seconds);
        assert!(out.observed_seconds <= 400);
    }

    #[test]
    fn strong_core_does_not_need_vision() {
        let samples = grid("Cursor", "main.tex", 0, 58, 15, 2);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "paper".into(),
                evidence: vec!["main.tex".into()],
                hero: true,
            }],
            tasks: mainline_tasks(),
            policy: &pol(),
            capture: CaptureStatus::Scheduled,
            vision: None,
            manual_core: None,
        });
        assert!(!out.used_vision);
        assert!(out.credited_core_seconds >= 780);
        assert_eq!(out.dominant, Dominant::CoreResearch);
    }

    #[test]
    fn thirteen_min_distraction_two_min_paper_not_near_full_credit() {
        let mut samples = grid("Safari", "bilibili", 0, 52, 15, 2);
        samples.extend(grid("Cursor", "main.tex", 780, 8, 15, 2));
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "paper".into(),
                evidence: vec!["main.tex".into()],
                hero: true,
            }],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: Some(VisionResult {
                wants_core: true,
                confidence: 0.95,
                match_context: match_ctx("Cursor", "main.tex"),
                category: "core_research".into(),
            }),
            manual_core: None,
        });
        assert!(
            out.credited_core_seconds < 600,
            "13m distraction + 2m paper must not credit ~900, got {}",
            out.credited_core_seconds
        );
    }

    #[test]
    fn eight_min_core_seven_min_side_side_activity_at_least_400() {
        let mut samples = grid("Cursor", "main.tex", 0, 32, 15, 2);
        samples.extend(grid("GameLife", "Today", 480, 28, 15, 2));
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "paper".into(),
                evidence: vec!["main.tex".into()],
                hero: true,
            }],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: None,
            manual_core: None,
        });
        assert!(
            out.activity.side >= 400,
            "7m side must appear in activity.side, got {}",
            out.activity.side
        );
    }

    #[test]
    fn vision_context_mismatch_with_strong_core_is_pending() {
        let mut samples = grid("Cursor", "main.tex", 0, 20, 15, 2);
        samples.extend(grid("WeChat", "chat", 300, 20, 15, 2));
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "paper".into(),
                evidence: vec!["main.tex".into()],
                hero: true,
            }],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: Some(VisionResult {
                wants_core: true,
                confidence: 0.9,
                match_context: match_ctx("WeChat", "chat"),
                category: "core_research".into(),
            }),
            manual_core: None,
        });
        assert!(out.pending);
        assert_eq!(out.dominant, Dominant::PendingReview);
        assert_eq!(out.credited_core_seconds, 0);
    }

    #[test]
    fn gray_zone_missed_capture_is_pending() {
        let samples = grid("Isaac Sim", "robot", 0, 30, 15, 2);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "robot".into(),
                evidence: vec!["robot".into()],
                hero: true,
            }],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Missed,
            vision: None,
            manual_core: None,
        });
        assert!(out.pending);
        assert_eq!(out.dominant, Dominant::PendingReview);
        assert_eq!(out.credited_core_seconds, 0);
    }

    #[test]
    fn gray_zone_captured_vision_failure_is_pending() {
        let samples = grid("Isaac Sim", "robot", 0, 30, 15, 2);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "robot".into(),
                evidence: vec!["robot".into()],
                hero: true,
            }],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: None,
            manual_core: None,
        });
        assert!(out.pending);
        assert_eq!(out.dominant, Dominant::PendingReview);
        assert_eq!(out.credited_core_seconds, 0);
    }

    #[test]
    fn manual_core_true_credits_unsure_without_vision() {
        let samples = grid("Isaac Sim", "robot", 0, 40, 15, 2);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "robot".into(),
                evidence: vec!["robot".into()],
                hero: true,
            }],
            tasks: mainline_tasks(),
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: None,
            manual_core: Some(true),
        });
        assert!(!out.pending);
        assert!(out.credited_core_seconds > 60);
    }

    #[test]
    fn manual_core_false_zeroes_credit_keeps_activity() {
        let mut samples = grid("Isaac Sim", "robot", 0, 20, 15, 2);
        samples.extend(grid("GameLife", "Today", 300, 20, 15, 2));
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "robot".into(),
                evidence: vec!["robot".into()],
                hero: true,
            }],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: None,
            manual_core: Some(false),
        });
        assert_eq!(out.credited_core_seconds, 0);
        assert!(out.activity.side > 0);
    }

    #[test]
    fn empty_quests_forces_zero_credit_no_auto_core_research() {
        let samples = grid("Cursor", "main.tex", 0, 58, 15, 2);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Scheduled,
            vision: None,
            manual_core: None,
        });
        assert_eq!(out.credited_core_seconds, 0);
        assert_ne!(out.dominant, Dominant::CoreResearch);
    }

    #[test]
    fn empty_tasks_zero_credit_even_with_quest_evidence() {
        let quests = [Quest::fixture("HDP", "HDP")];
        let samples = grid("Cursor", "HDP train.py", 0, 60, 15, 5);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &quests,
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: None,
            manual_core: None,
        });
        assert_eq!(out.credited_core_seconds, 0);
    }

    #[test]
    fn title_only_quests_without_evidence_force_zero_credit() {
        let samples = grid("Cursor", "main.tex", 0, 58, 15, 2);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "paper".into(),
                evidence: vec![],
                hero: true,
            }],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Scheduled,
            vision: None,
            manual_core: None,
        });
        assert_eq!(out.credited_core_seconds, 0);
        assert_ne!(out.dominant, Dominant::CoreResearch);
    }

    #[test]
    fn title_only_quests_with_manual_core_still_force_zero_credit() {
        let samples = grid("Isaac Sim", "robot", 0, 40, 15, 2);
        let base = |quests: &[Quest], tasks: &[crate::task::TaskSnapshot]| {
            judge_slot(JudgeInput {
                slot_start: 0,
                slot_end: 900,
                samples: &samples,
                quests,
                tasks,
                policy: &pol(),
                capture: CaptureStatus::Captured,
                vision: None,
                manual_core: Some(true),
            })
        };
        let title_only = base(
            &[Quest {
                text: "robot".into(),
                evidence: vec![],
                hero: true,
            }],
            &[],
        );
        assert_eq!(
            title_only.credited_core_seconds,
            0,
            "empty evidence must gate credit even when manual_core would otherwise pay"
        );
        let with_evidence = base(
            &[Quest {
                text: "robot".into(),
                evidence: vec!["robot".into()],
                hero: true,
            }],
            mainline_tasks(),
        );
        assert!(
            with_evidence.credited_core_seconds > 60,
            "same slot with evidence must credit manual_core, got {}",
            with_evidence.credited_core_seconds
        );
    }

    #[test]
    fn metadata_core_vision_wechat_is_pending() {
        let samples = grid("Cursor", "main.tex", 0, 40, 15, 2);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "paper".into(),
                evidence: vec!["main.tex".into()],
                hero: true,
            }],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: Some(VisionResult {
                wants_core: false,
                confidence: 0.9,
                match_context: match_ctx("WeChat", "chat"),
                category: "distraction".into(),
            }),
            manual_core: None,
        });
        assert!(out.pending);
        assert_eq!(out.dominant, Dominant::PendingReview);
        assert_eq!(out.credited_core_seconds, 0);
    }

    #[test]
    fn manual_core_does_not_upgrade_side_or_distraction_spans() {
        let mut samples = grid("Isaac Sim", "robot", 0, 10, 15, 2);
        samples.extend(grid("Safari", "bilibili watch", 150, 10, 15, 2));
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "robot".into(),
                evidence: vec!["robot".into()],
                hero: true,
            }],
            tasks: mainline_tasks(),
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: None,
            manual_core: Some(true),
        });
        assert!(
            out.credited_core_seconds < 200,
            "distraction spans must not be verified, got {}",
            out.credited_core_seconds
        );
        assert!(out.credited_core_seconds >= 100);
        assert!(out.activity.distraction >= 100);
    }

    #[test]
    fn parse_legal_categories() {
        for category in [
            "core_research",
            "research_support",
            "admin",
            "side_project",
            "distraction",
            "break_away",
        ] {
            let json = format!(
                r#"{{"category":"{category}","confidence":0.8,"reason":""}}"#
            );
            let v = parse_vision_json(&json, match_ctx("Cursor", "t")).unwrap();
            assert_eq!(v.category, category);
            assert_eq!(v.wants_core, category == "core_research");
            assert_eq!(v.match_context.as_ref().map(|c| c.app.as_str()), Some("Cursor"));
        }
    }

    #[test]
    fn parse_rejects_unknown_and_whatever() {
        for category in ["unknown", "whatever"] {
            let json = format!(r#"{{"category":"{category}","confidence":0.8,"reason":"x"}}"#);
            assert_eq!(
                parse_vision_json(&json, None),
                Err(VisionParseError::InvalidCategory)
            );
        }
    }

    #[test]
    fn parse_rejects_illegal_confidence() {
        for json in [
            r#"{"category":"core_research","confidence":1.1,"reason":""}"#,
            r#"{"category":"core_research","confidence":-0.1,"reason":""}"#,
            r#"{"category":"core_research","confidence":null,"reason":""}"#,
        ] {
            assert_eq!(
                parse_vision_json(json, None),
                Err(VisionParseError::InvalidConfidence)
            );
        }
    }

    #[test]
    fn parse_rejects_null_reason_allows_empty() {
        assert_eq!(
            parse_vision_json(
                r#"{"category":"core_research","confidence":0.7,"reason":null}"#,
                None
            ),
            Err(VisionParseError::InvalidReason)
        );
        let v = parse_vision_json(
            r#"{"category":"core_research","confidence":0.7,"reason":""}"#,
            None,
        )
        .unwrap();
        assert!(v.wants_core);
    }

    #[test]
    fn chrome_tensorboard_screenshot_does_not_verify_personal_site() {
        let mut samples = Vec::new();
        for i in 0..20 {
            samples.push(Sample {
                ts: i * 15,
                app: "Google Chrome".into(),
                window_title: "personal site".into(),
                url: Some("https://haofei.ma/".into()),
                document_path: None,
                bundle_id: None,
                idle_seconds: 1,
                screen_locked: false,
                paused: false,
                secure_input: false,
            });
        }
        for i in 20..60 {
            samples.push(Sample {
                ts: i * 15,
                app: "Google Chrome".into(),
                window_title: "TensorBoard".into(),
                url: Some("http://localhost:6006/".into()),
                document_path: None,
                bundle_id: None,
                idle_seconds: 1,
                screen_locked: false,
                paused: false,
                secure_input: false,
            });
        }
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest {
                text: "HDP".into(),
                evidence: vec!["HDP".into()],
                hero: true,
            }],
            tasks: mainline_tasks(),
            policy: &Policy {
                trusted_apps: vec!["Google Chrome".into()],
                distraction_rules: vec![],
                side_project_rules: vec![],
                reading_apps: vec![],
                never_capture_apps: vec![],
                admin_apps: vec![],
                category_guides: CategoryGuides::default(),
            },
            capture: CaptureStatus::Captured,
            vision: Some(VisionResult {
                wants_core: true,
                confidence: 0.9,
                match_context: Some(VisionMatchContext {
                    app: "Google Chrome".into(),
                    title: "TensorBoard".into(),
                    document_path: None,
                }),
                category: "core_research".into(),
            }),
            manual_core: None,
        });
        assert!(!out.pending);
        assert!(
            out.credited_core_seconds >= 540,
            "TensorBoard span should be verified, got {}",
            out.credited_core_seconds
        );
        assert!(
            out.credited_core_seconds <= 650,
            "personal-site span must not be verified, got {}",
            out.credited_core_seconds
        );
    }

    fn title_only_grid(
        app: &str,
        title: &str,
        start: i64,
        n: usize,
        every: i64,
        idle: i64,
    ) -> Vec<Sample> {
        (0..n)
            .map(|i| Sample {
                ts: start + i as i64 * every,
                app: app.into(),
                window_title: title.into(),
                url: None,
                document_path: None,
                bundle_id: None,
                idle_seconds: idle,
                screen_locked: false,
                paused: false,
                secure_input: false,
            })
            .collect()
    }

    fn browser_pol() -> Policy {
        Policy {
            trusted_apps: vec!["Safari".into(), "Cursor".into()],
            distraction_rules: default_distraction_rules(),
            side_project_rules: builtin_side_project_rules(),
            reading_apps: vec!["Preview".into()],
            never_capture_apps: builtin_never_capture(),
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        }
    }

    #[test]
    fn title_only_thirteen_minutes_is_pending() {
        let samples = title_only_grid("Cursor", "train.py — HDP", 0, 58, 15, 2);
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest::fixture("HDP", "HDP")],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Missed,
            vision: None,
            manual_core: None,
        });
        assert!(out.pending);
        assert_eq!(out.credited_core_seconds, 0);
    }

    #[test]
    fn overleaf_thirteen_minutes_auto_cores_without_keyword() {
        let samples: Vec<Sample> = (0..58)
            .map(|i| Sample {
                ts: i as i64 * 15,
                app: "Safari".into(),
                window_title: "Overleaf".into(),
                url: Some("https://www.overleaf.com/project/abc".into()),
                document_path: None,
                bundle_id: None,
                idle_seconds: 2,
                screen_locked: false,
                paused: false,
                secure_input: false,
            })
            .collect();
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest::fixture("HDP", "HDP")],
            tasks: mainline_tasks(),
            policy: &browser_pol(),
            capture: CaptureStatus::Scheduled,
            vision: None,
            manual_core: None,
        });
        assert!(!out.pending);
        assert!(out.credited_core_seconds >= 780);
        assert_eq!(out.dominant, Dominant::CoreResearch);
    }

    #[test]
    fn idle_overleaf_does_not_auto_core() {
        let samples: Vec<Sample> = (0..58)
            .map(|i| Sample {
                ts: i as i64 * 15,
                app: "Safari".into(),
                window_title: "Overleaf".into(),
                url: Some("https://www.overleaf.com/project/abc".into()),
                document_path: None,
                bundle_id: None,
                idle_seconds: 400,
                screen_locked: false,
                paused: false,
                secure_input: false,
            })
            .collect();
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest::fixture("HDP", "HDP")],
            tasks: &[],
            policy: &browser_pol(),
            capture: CaptureStatus::Missed,
            vision: None,
            manual_core: None,
        });
        assert!(out.pending || out.credited_core_seconds < 780);
        assert_eq!(out.credited_core_seconds, 0);
    }

    #[test]
    fn two_min_grounded_plus_ten_min_idle_not_auto_core() {
        let mut samples = grid("Cursor", "main.tex", 0, 8, 15, 2);
        samples.extend(grid("Cursor", "main.tex", 120, 40, 15, 400));
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest::fixture("paper", "main.tex")],
            tasks: &[],
            policy: &pol(),
            capture: CaptureStatus::Missed,
            vision: None,
            manual_core: None,
        });
        assert_eq!(out.credited_core_seconds, 0);
        assert!(out.pending);
    }

    #[test]
    fn youtube_dominant_is_distraction_zero_credit() {
        let samples: Vec<Sample> = (0..52)
            .map(|i| Sample {
                ts: i as i64 * 15,
                app: "Safari".into(),
                window_title: "HDP lecture".into(),
                url: Some("https://www.youtube.com/watch?v=1".into()),
                document_path: None,
                bundle_id: None,
                idle_seconds: 2,
                screen_locked: false,
                paused: false,
                secure_input: false,
            })
            .collect();
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[Quest::fixture("HDP", "HDP")],
            tasks: &[],
            policy: &browser_pol(),
            capture: CaptureStatus::Scheduled,
            vision: None,
            manual_core: None,
        });
        assert!(!out.pending);
        assert_eq!(out.dominant, Dominant::Distraction);
        assert_eq!(out.credited_core_seconds, 0);
    }

    #[test]
    fn overleaf_without_quests_credits_zero() {
        let samples: Vec<Sample> = (0..58)
            .map(|i| Sample {
                ts: i as i64 * 15,
                app: "Safari".into(),
                window_title: "Overleaf".into(),
                url: Some("https://www.overleaf.com/project/abc".into()),
                document_path: None,
                bundle_id: None,
                idle_seconds: 2,
                screen_locked: false,
                paused: false,
                secure_input: false,
            })
            .collect();
        let out = judge_slot(JudgeInput {
            slot_start: 0,
            slot_end: 900,
            samples: &samples,
            quests: &[],
            tasks: &[],
            policy: &browser_pol(),
            capture: CaptureStatus::Scheduled,
            vision: None,
            manual_core: None,
        });
        assert_eq!(out.credited_core_seconds, 0);
    }
}
