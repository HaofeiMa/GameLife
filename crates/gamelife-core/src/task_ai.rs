use crate::judge::{Dominant, JudgeOutput, SlotEvidence};
use crate::task::{ListRole, TaskSnapshot};

pub const TASK_MATCH_MIN: f64 = 0.7;

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
