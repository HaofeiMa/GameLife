use chrono::{DateTime, FixedOffset};
use gamelife_core::{align_range, role_from_hashtag, ticktick_snapshot_id, ListRole, TimedTask};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenTask {
    pub id: String,
    pub project_id: Option<String>,
    pub title: String,
    pub status: Option<i64>,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub time_zone: Option<String>,
    pub is_all_day: Option<bool>,
}

pub fn parse_open_project_data_tasks(json: &str) -> Result<Vec<OpenTask>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("ticktick json: {e}"))?;
    match value {
        serde_json::Value::Array(_) => {
            serde_json::from_value(value).map_err(|e| format!("ticktick tasks: {e}"))
        }
        serde_json::Value::Object(mut obj) => {
            let tasks = obj
                .remove("tasks")
                .ok_or_else(|| "ticktick json: missing tasks".to_string())?;
            serde_json::from_value(tasks).map_err(|e| format!("ticktick tasks: {e}"))
        }
        _ => Err("ticktick json: expected object or array".into()),
    }
}

fn parse_ticktick_datetime(raw: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%z").ok()
}

pub fn open_task_to_timed(
    task: &OpenTask,
    fallback_role: ListRole,
    _default_tz: &FixedOffset,
) -> Option<TimedTask> {
    if task.status == Some(2) || task.is_all_day == Some(true) {
        return None;
    }
    let start_raw = task.start_date.as_deref()?;
    let due_raw = task.due_date.as_deref()?;
    let start = parse_ticktick_datetime(start_raw)?;
    let end = parse_ticktick_datetime(due_raw)?;
    let (start, end) = align_range(start.timestamp(), end.timestamp());
    Some(TimedTask {
        id: ticktick_snapshot_id(&task.id),
        title: task.title.clone(),
        role: role_from_hashtag(&task.title, fallback_role),
        start,
        end,
        done: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gamelife_core::ListRole;

    #[test]
    fn skips_all_day_and_completed() {
        let json = r#"{"tasks":[
      {"id":"1","title":"A","status":0,"startDate":"2026-09-13T02:00:00+0000","dueDate":"2026-09-13T03:00:00+0000","timeZone":"UTC"},
      {"id":"2","title":"B","status":2,"startDate":"2026-09-13T02:00:00+0000","dueDate":"2026-09-13T03:00:00+0000"},
      {"id":"3","title":"C","status":0,"isAllDay":true,"startDate":"2026-09-13T00:00:00+0000","dueDate":"2026-09-13T00:00:00+0000"}
    ]}"#;
        let tasks = parse_open_project_data_tasks(json).unwrap();
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        let timed: Vec<_> = tasks
            .iter()
            .filter_map(|t| open_task_to_timed(t, ListRole::Mainline, &tz))
            .collect();
        assert_eq!(timed.len(), 1);
        assert_eq!(timed[0].id, "tt-1");
        assert!(timed[0].end > timed[0].start);
    }

    #[test]
    fn hashtag_chore_from_title() {
        let t = OpenTask {
            id: "9".into(),
            project_id: None,
            title: "报销 #杂项".into(),
            status: Some(0),
            start_date: Some("2026-09-13T01:00:00+0000".into()),
            due_date: Some("2026-09-13T02:00:00+0000".into()),
            time_zone: Some("UTC".into()),
            is_all_day: Some(false),
        };
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        let got = open_task_to_timed(&t, ListRole::Mainline, &tz).unwrap();
        assert_eq!(got.role, ListRole::Chore);
    }
}
