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
    let obj = value
        .as_object()
        .ok_or(TaskMatchError::InvalidJson)?;

    let confidence = match obj.get("confidence") {
        Some(serde_json::Value::Number(n)) => n
            .as_f64()
            .ok_or(TaskMatchError::BadConfidence)?,
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
            output.dominant = Dominant::SideProject;
            output.pending = false;
        }
        ListRole::Chore => {
            output.credited_core_seconds = 0;
            output.credited_side_seconds = 0;
            output.credited_chore_seconds = base;
            output.dominant = Dominant::Admin;
            output.pending = false;
        }
    }
    output
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
}
