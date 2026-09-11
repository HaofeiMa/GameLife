use crate::capture::CaptureStatus;
use crate::hint::hint_sample;
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

#[derive(Clone, Debug, PartialEq)]
pub struct VisionResult {
    pub wants_core: bool,
    pub confidence: f64,
    pub context_app: Option<String>,
    pub category: String,
}

pub struct JudgeInput<'a> {
    pub slot_start: i64,
    pub slot_end: i64,
    pub samples: &'a [Sample],
    pub quests: &'a [Quest],
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
    pub observed_seconds: i64,
    pub used_vision: bool,
    pub pending: bool,
}

/// Slot judge pipeline:
/// 1. hint_sample per sample (maintain last_core_interaction_ts)
/// 2. spans_for_slot → activity + strong_core / reading_bridge
/// 3. observed_seconds, actual duration
/// 4. branch: unobserved / break-away / strong core / side-distraction / gray+vision
/// 5. credited = min(observed, actual, strong + bridge + verified); quests empty → 0
pub fn judge_slot(input: JudgeInput<'_>) -> JudgeOutput {
    let actual = input.slot_end - input.slot_start;
    let quests_empty = input.quests.is_empty();

    // Step 1: hints with last_core_interaction_ts maintenance
    let mut last_core_interaction_ts: Option<i64> = None;
    let mut hints = Vec::with_capacity(input.samples.len());
    for sample in input.samples {
        let hint = hint_sample(sample, input.policy, input.quests, last_core_interaction_ts);
        if sample.idle_seconds < LOW_INPUT_IDLE_SECS && hint == Hint::CoreCandidate {
            last_core_interaction_ts = Some(sample.ts);
        }
        hints.push(hint);
    }

    // Step 2: spans → activity, strong_core, reading_bridge
    let spans = spans_for_slot(
        input.samples,
        &hints,
        input.slot_start,
        input.slot_end,
    );
    let mut activity = ActivitySeconds::default();
    let mut strong_core = 0_i64;
    let mut reading_bridge = 0_i64;

    for span in &spans {
        let secs = span.end - span.start;
        match span.kind {
            SpanKind::Unobserved => activity.unobserved += secs,
            SpanKind::Observed(hint) => {
                accumulate_hint_activity(&mut activity, hint, secs);
                let idle = sample_idle_at(input.samples, span.start);
                match hint {
                    Hint::CoreCandidate if idle < LOW_INPUT_IDLE_SECS => {
                        strong_core += secs;
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

    // Step 3
    let observed = observed_seconds(&spans);

    // Step 5–6: branches
    let mut used_vision = false;
    let mut pending = false;
    let mut dominant = Dominant::Unknown;
    let mut credited_raw = 0_i64;

    if activity.unobserved == actual {
        dominant = Dominant::Unobserved;
    } else if activity.away >= AWAY_DOMINANT_SECS && strong_core < 300 {
        dominant = Dominant::BreakAway;
    } else if !quests_empty
        && strong_core >= STRONG_CORE_AUTO_SECS
        && activity.side + activity.distraction <= SIDE_DISTRACTION_MAX_FOR_AUTO_CORE
    {
        dominant = Dominant::CoreResearch;
        credited_raw = strong_core + reading_bridge;
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
                v.context_app
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
    if quests_empty {
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
        observed_seconds: observed,
        used_vision,
        pending,
    }
}

fn accumulate_hint_activity(activity: &mut ActivitySeconds, hint: Hint, secs: i64) {
    match hint {
        Hint::Away => activity.away += secs,
        Hint::Distraction => activity.distraction += secs,
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
                let idle = sample_idle_at(samples, span.start);
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

fn sample_idle_at(samples: &[Sample], ts: i64) -> i64 {
    samples
        .iter()
        .filter(|s| s.ts <= ts)
        .max_by_key(|s| s.ts)
        .map(|s| s.idle_seconds)
        .unwrap_or(0)
}

fn sample_at(samples: &[Sample], ts: i64) -> Option<&Sample> {
    samples.iter().filter(|s| s.ts <= ts).max_by_key(|s| s.ts)
}

fn app_matches_context(sample_app: &str, context_app: &str) -> bool {
    sample_app.eq_ignore_ascii_case(context_app)
}

fn span_context_matches(samples: &[Sample], context_app: &str, spans: &[Span]) -> bool {
    spans.iter().any(|span| {
        matches!(span.kind, SpanKind::Observed(Hint::CoreCandidate | Hint::CoreReading))
            && sample_at(samples, span.start)
                .is_some_and(|s| app_matches_context(&s.app, context_app))
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
        match &vision.context_app {
            None => return 0,
            Some(context_app) => {
                let mut total = 0_i64;
                for span in spans {
                    let Some(sample) = sample_at(input.samples, span.start) else {
                        continue;
                    };
                    if !app_matches_context(&sample.app, context_app) {
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
    use crate::policy::{Policy, builtin_never_capture, builtin_side_project_rules};

    fn pol() -> Policy {
        Policy {
            trusted_apps: vec!["Cursor".into(), "Preview".into()],
            distraction_rules: vec!["bilibili".into()],
            side_project_rules: builtin_side_project_rules(),
            reading_apps: vec!["Preview".into()],
            never_capture_apps: builtin_never_capture(),
        }
    }

    fn grid(app: &str, title: &str, start: i64, n: usize, every: i64, idle: i64) -> Vec<Sample> {
        (0..n)
            .map(|i| Sample {
                ts: start + i as i64 * every,
                app: app.into(),
                window_title: title.into(),
                url: None,
                path: Some("/paper/main.tex".into()),
                idle_seconds: idle,
                screen_locked: false,
                paused: false,
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
                keywords: vec!["robot".into()],
            }],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: Some(VisionResult {
                wants_core: true,
                confidence: 0.9,
                context_app: Some("Isaac Sim".into()),
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
                keywords: vec!["main.tex".into()],
            }],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: Some(VisionResult {
                wants_core: true,
                confidence: 0.95,
                context_app: Some("Cursor".into()),
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
                keywords: vec!["main.tex".into()],
            }],
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
                keywords: vec!["main.tex".into()],
            }],
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
                keywords: vec!["main.tex".into()],
            }],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: Some(VisionResult {
                wants_core: true,
                confidence: 0.95,
                context_app: Some("Cursor".into()),
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
                keywords: vec!["main.tex".into()],
            }],
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
                keywords: vec!["main.tex".into()],
            }],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: Some(VisionResult {
                wants_core: true,
                confidence: 0.9,
                context_app: Some("WeChat".into()),
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
                keywords: vec!["robot".into()],
            }],
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
                keywords: vec!["robot".into()],
            }],
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
                keywords: vec!["robot".into()],
            }],
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
                keywords: vec!["robot".into()],
            }],
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
            policy: &pol(),
            capture: CaptureStatus::Scheduled,
            vision: None,
            manual_core: None,
        });
        assert_eq!(out.credited_core_seconds, 0);
        assert_ne!(out.dominant, Dominant::CoreResearch);
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
                keywords: vec!["main.tex".into()],
            }],
            policy: &pol(),
            capture: CaptureStatus::Captured,
            vision: Some(VisionResult {
                wants_core: false,
                confidence: 0.9,
                context_app: Some("WeChat".into()),
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
                keywords: vec!["robot".into()],
            }],
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
}
