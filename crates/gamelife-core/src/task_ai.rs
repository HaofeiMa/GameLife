use crate::judge::{Dominant, JudgeOutput, SlotEvidence};
use crate::policy::{nonempty_guides, truncate_guide, CategoryGuides, Policy};
use crate::task::{select_prompt_snapshots, ListRole, TaskSnapshot, MAX_JUDGMENT_TASKS};

pub const TASK_MATCH_MIN: f64 = 0.7;

const TASK_MATCH_INTRO: &str = "Match the observed windows to at most one unfinished task. Category guides and app lists are evidence for which task fits; they are not categories to return. If none fits, task_id is null. Reply JSON {\"task_id\": string|null, \"confidence\": number}.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskMatchPrompt {
    pub text: String,
    pub shown: Vec<TaskSnapshot>,
}

fn task_match_role_name(role: ListRole) -> &'static str {
    match role {
        ListRole::Mainline => "mainline",
        ListRole::Side => "side",
        ListRole::Longterm => "longterm",
        ListRole::Chore => "chore",
        ListRole::Custom => "custom",
    }
}

fn join_capped(names: &[String]) -> String {
    names
        .iter()
        .take(20)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ")
}

fn policy_list_lines(policy: &Policy) -> String {
    let mut lines = Vec::new();
    if !policy.trusted_apps.is_empty() {
        lines.push(format!("主线应用: {}", join_capped(&policy.trusted_apps)));
    }
    if !policy.side_project_rules.is_empty() {
        lines.push(format!("支线: {}", join_capped(&policy.side_project_rules)));
    }
    if !policy.admin_apps.is_empty() {
        lines.push(format!("杂项: {}", join_capped(&policy.admin_apps)));
    }
    if !policy.distraction_rules.is_empty() {
        lines.push(format!("娱乐: {}", join_capped(&policy.distraction_rules)));
    }
    lines.join("\n")
}

pub fn build_task_match_prompt(
    snapshots: &[TaskSnapshot],
    guides: &CategoryGuides,
    policy: &Policy,
    windows: &str,
) -> TaskMatchPrompt {
    let shown = if snapshots.len() <= MAX_JUDGMENT_TASKS {
        snapshots.to_vec()
    } else {
        select_prompt_snapshots(snapshots, &[windows])
    };

    let tasks = shown
        .iter()
        .map(|s| {
            format!(
                "id={} title={} role={}",
                s.id,
                s.title,
                task_match_role_name(s.role)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let guide_lines = nonempty_guides(guides)
        .into_iter()
        .map(|(key, text)| format!("{key}: {}", truncate_guide(&text)))
        .collect::<Vec<_>>()
        .join("\n");

    let list_lines = policy_list_lines(policy);

    let mut body = format!("{TASK_MATCH_INTRO}\nTasks:\n{tasks}");
    if !guide_lines.is_empty() {
        body.push('\n');
        body.push_str(&guide_lines);
    }
    if !list_lines.is_empty() {
        body.push('\n');
        body.push_str(&list_lines);
    }
    body.push_str("\nWindows:\n");
    body.push_str(windows);

    TaskMatchPrompt {
        text: body,
        shown,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskMatch {
    pub task_id: String,
    pub confidence: f64,
    pub role: ListRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskMatchError {
    InvalidJson,
    BadConfidence,
    UnknownTask,
}

pub fn parse_task_match_json(
    json: &str,
    snapshots: &[TaskSnapshot],
) -> Result<Option<TaskMatch>, TaskMatchError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| TaskMatchError::InvalidJson)?;
    let obj = value.as_object().ok_or(TaskMatchError::InvalidJson)?;

    let confidence = match obj.get("confidence") {
        Some(serde_json::Value::Number(n)) => n.as_f64().ok_or(TaskMatchError::BadConfidence)?,
        _ => return Err(TaskMatchError::BadConfidence),
    };
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return Err(TaskMatchError::BadConfidence);
    }

    match obj.get("task_id") {
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(id)) => {
            let snap = snapshots
                .iter()
                .find(|s| s.id == *id)
                .ok_or(TaskMatchError::UnknownTask)?;
            Ok(Some(TaskMatch {
                task_id: id.clone(),
                confidence,
                role: snap.role,
            }))
        }
        _ => Err(TaskMatchError::InvalidJson),
    }
}

pub fn payout_base_seconds(ev: &SlotEvidence) -> i64 {
    (ev.observed_seconds - ev.activity.away - ev.activity.distraction).max(0)
}

pub fn apply_task_match(mut output: JudgeOutput, ev: &SlotEvidence, m: &TaskMatch) -> JudgeOutput {
    if m.confidence < TASK_MATCH_MIN {
        return output;
    }
    let base = payout_base_seconds(ev);
    match m.role {
        ListRole::Mainline => {
            output.credited_core_seconds = if output.credited_core_seconds == 0 {
                base
            } else {
                output.credited_core_seconds.min(base)
            };
            output.activity.core = output.activity.core.max(output.credited_core_seconds);
            output.dominant = Dominant::CoreResearch;
            output.pending = false;
        }
        ListRole::Side | ListRole::Longterm | ListRole::Custom => {
            output.credited_core_seconds = 0;
            output.credited_side_seconds = base;
            output.credited_chore_seconds = 0;
            output.activity.side = output.activity.side.max(base);
            output.dominant = Dominant::SideProject;
            output.pending = false;
        }
        ListRole::Chore => {
            output.credited_core_seconds = 0;
            output.credited_side_seconds = 0;
            output.credited_chore_seconds = base;
            output.activity.admin = output.activity.admin.max(base);
            output.dominant = Dominant::Admin;
            output.pending = false;
        }
    }
    output
}

/// Apply a text-AI reply only when it actually settles the slot (confidence ≥ 0.7 and not pending).
pub fn settle_from_text_ai(
    output: JudgeOutput,
    ev: &SlotEvidence,
    tasks: &[TaskSnapshot],
    raw: &str,
) -> Option<JudgeOutput> {
    let matched = parse_task_match_json(raw, tasks).ok()??;
    let next = apply_task_match(output, ev, &matched);
    if next.pending {
        None
    } else {
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ActivitySeconds;

    #[test]
    fn rejects_unknown_task() {
        let snaps = [TaskSnapshot {
            id: "t1".into(),
            title: "HDP".into(),
            role: ListRole::Mainline,
        }];
        assert_eq!(
            parse_task_match_json(r#"{"task_id":"nope","confidence":0.9}"#, &snaps),
            Err(TaskMatchError::UnknownTask)
        );
    }

    #[test]
    fn null_task_is_none() {
        let snaps = [];
        assert_eq!(
            parse_task_match_json(r#"{"task_id":null,"confidence":0.9}"#, &snaps),
            Ok(None)
        );
    }

    #[test]
    fn apply_mainline_sets_core() {
        let ev = SlotEvidence {
            hints: vec![],
            spans: vec![],
            activity: ActivitySeconds::default(),
            strong_core_seconds: 0,
            grounded_strong_core_seconds: 0,
            reading_bridge_seconds: 0,
            observed_seconds: 900,
        };
        let out = JudgeOutput {
            dominant: Dominant::PendingReview,
            activity: ActivitySeconds::default(),
            credited_core_seconds: 0,
            credited_side_seconds: 0,
            credited_chore_seconds: 0,
            observed_seconds: 900,
            used_vision: false,
            pending: false,
        };
        let m = TaskMatch {
            task_id: "t1".into(),
            confidence: 0.9,
            role: ListRole::Mainline,
        };
        let out = apply_task_match(out, &ev, &m);
        assert_eq!(out.credited_core_seconds, 900);
        assert_eq!(out.dominant, Dominant::CoreResearch);
    }

    fn empty_output(observed: i64) -> JudgeOutput {
        JudgeOutput {
            dominant: Dominant::Unknown,
            activity: ActivitySeconds::default(),
            credited_core_seconds: 0,
            credited_side_seconds: 0,
            credited_chore_seconds: 0,
            observed_seconds: observed,
            used_vision: false,
            pending: true,
        }
    }

    fn empty_evidence(observed: i64) -> SlotEvidence {
        SlotEvidence {
            hints: vec![],
            spans: vec![],
            activity: ActivitySeconds::default(),
            strong_core_seconds: 0,
            grounded_strong_core_seconds: 0,
            reading_bridge_seconds: 0,
            observed_seconds: observed,
        }
    }

    #[test]
    fn apply_side_and_chore_fill_activity_buckets() {
        let ev = empty_evidence(900);
        let side = apply_task_match(
            empty_output(900),
            &ev,
            &TaskMatch {
                task_id: "s".into(),
                confidence: 0.9,
                role: ListRole::Side,
            },
        );
        assert_eq!(side.dominant, Dominant::SideProject);
        assert_eq!(side.credited_side_seconds, 900);
        assert_eq!(side.activity.side, 900);
        assert!(!side.pending);
        for role in [ListRole::Longterm, ListRole::Custom] {
            let out = apply_task_match(
                empty_output(900),
                &ev,
                &TaskMatch {
                    task_id: "x".into(),
                    confidence: 0.9,
                    role,
                },
            );
            assert_eq!(out.dominant, Dominant::SideProject);
            assert_eq!(out.activity.side, 900);
            assert_eq!(out.credited_side_seconds, 900);
        }

        let chore = apply_task_match(
            empty_output(900),
            &ev,
            &TaskMatch {
                task_id: "c".into(),
                confidence: 0.9,
                role: ListRole::Chore,
            },
        );
        assert_eq!(chore.dominant, Dominant::Admin);
        assert_eq!(chore.credited_chore_seconds, 900);
        assert_eq!(chore.activity.admin, 900);
        assert!(!chore.pending);
    }

    #[test]
    fn apply_task_match_with_zero_payable_seconds_still_closes() {
        let mut ev = empty_evidence(900);
        ev.activity.away = 900;
        let out = apply_task_match(
            empty_output(900),
            &ev,
            &TaskMatch {
                task_id: "c".into(),
                confidence: 0.9,
                role: ListRole::Chore,
            },
        );
        assert!(!out.pending);
        assert_eq!(out.dominant, Dominant::Admin);
        assert_eq!(out.credited_chore_seconds, 0);
        assert_eq!(out.activity.admin, 0);
    }

    #[test]
    fn category_json_does_not_settle_even_when_no_tasks() {
        let ev = empty_evidence(900);
        assert!(settle_from_text_ai(
            empty_output(900),
            &ev,
            &[],
            r#"{"category":"admin","confidence":0.9}"#,
        )
        .is_none());
    }

    #[test]
    fn text_ai_null_task_does_not_settle_even_at_high_confidence() {
        let snaps = [TaskSnapshot {
            id: "t1".into(),
            title: "HDP".into(),
            role: ListRole::Mainline,
        }];
        let ev = empty_evidence(900);
        assert!(settle_from_text_ai(
            empty_output(900),
            &ev,
            &snaps,
            r#"{"task_id":null,"confidence":0.9}"#,
        )
        .is_none());
    }

    #[test]
    fn prompt_includes_guides_and_skips_empty_lists() {
        use crate::policy::{default_v01, CategoryGuides};

        let snaps = [TaskSnapshot {
            id: "t1".into(),
            title: "微信".into(),
            role: ListRole::Chore,
        }];
        let guides = CategoryGuides {
            admin: "聊天工具".into(),
            ..CategoryGuides::default()
        };
        let mut policy = default_v01();
        policy.admin_apps = vec!["微信".into()];
        policy.side_project_rules.clear();
        policy.distraction_rules.clear();
        policy.trusted_apps.clear();
        let built = build_task_match_prompt(
            &snaps,
            &guides,
            &policy,
            "app=微信 title=微信 url= document_path= idle=0",
        );
        assert!(built.text.contains("task_id"));
        assert!(built.text.contains("id=t1 title=微信 role=chore"));
        assert!(built.text.contains("admin: 聊天工具"));
        assert!(built.text.contains("杂项: 微信"));
        assert!(!built.text.contains("支线:"));
        assert!(!built.text.contains("娱乐:"));
        assert!(!built.text.contains("主线应用:"));
        assert!(!built.text.contains("{\"category\""));
        assert_eq!(built.shown.len(), 1);
    }

    #[test]
    fn prompt_shown_slice_is_capped_at_20() {
        use crate::policy::{default_v01, CategoryGuides};

        let snaps: Vec<TaskSnapshot> = (0..25)
            .map(|i| TaskSnapshot {
                id: format!("t{i}"),
                title: format!("Task{i:02}"),
                role: ListRole::Mainline,
            })
            .collect();
        let built = build_task_match_prompt(
            &snaps,
            &CategoryGuides::default(),
            &default_v01(),
            "app=Cursor title=x url= document_path= idle=1",
        );
        assert_eq!(built.shown.len(), 20);
        assert!(!built.text.contains("id=t20 "));
    }

    #[test]
    fn text_ai_mainline_task_match_settles() {
        let snaps = [TaskSnapshot {
            id: "t1".into(),
            title: "HDP".into(),
            role: ListRole::Mainline,
        }];
        let ev = empty_evidence(900);
        let out = settle_from_text_ai(
            empty_output(900),
            &ev,
            &snaps,
            r#"{"task_id":"t1","confidence":0.9}"#,
        )
        .expect("should settle");
        assert!(!out.pending);
        assert_eq!(out.dominant, Dominant::CoreResearch);
    }
}
