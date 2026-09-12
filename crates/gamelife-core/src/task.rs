use serde::{Deserialize, Serialize};

pub const MAX_JUDGMENT_TASKS: usize = 20;
pub const PRESET_MAINLINE_ID: &str = "list-mainline";
pub const PRESET_SIDE_ID: &str = "list-side";
pub const PRESET_LONGTERM_ID: &str = "list-longterm";
pub const PRESET_CHORE_ID: &str = "list-chore";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListRole {
    Mainline,
    Side,
    Longterm,
    Chore,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRange {
    Week,
    Month,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskList {
    pub id: String,
    pub name: String,
    pub sort: i64,
    pub role: ListRole,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub list_id: String,
    pub title: String,
    pub done: bool,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub range: Option<TaskRange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub id: String,
    pub title: String,
    pub role: ListRole,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TaskListError {
    NoMainline,
    DuplicateMainline,
    EmptyName,
    TooManyJudgment,
}

pub fn preset_lists() -> Vec<TaskList> {
    vec![
        TaskList {
            id: PRESET_MAINLINE_ID.into(),
            name: "主线任务".into(),
            sort: 0,
            role: ListRole::Mainline,
        },
        TaskList {
            id: PRESET_SIDE_ID.into(),
            name: "支线任务".into(),
            sort: 1,
            role: ListRole::Side,
        },
        TaskList {
            id: PRESET_LONGTERM_ID.into(),
            name: "长期计划".into(),
            sort: 2,
            role: ListRole::Longterm,
        },
        TaskList {
            id: PRESET_CHORE_ID.into(),
            name: "杂项".into(),
            sort: 3,
            role: ListRole::Chore,
        },
    ]
}

pub fn validate_lists(lists: &[TaskList]) -> Result<(), TaskListError> {
    if lists.iter().any(|l| l.name.trim().is_empty()) {
        return Err(TaskListError::EmptyName);
    }
    let mainline = lists
        .iter()
        .filter(|l| l.role == ListRole::Mainline)
        .count();
    match mainline {
        0 => Err(TaskListError::NoMainline),
        1 => Ok(()),
        _ => Err(TaskListError::DuplicateMainline),
    }
}

pub fn align_range(start: i64, end: i64) -> (i64, i64) {
    let start = start / 900 * 900;
    let mut end = ((end + 899) / 900) * 900;
    if end <= start {
        end = start + 900;
    }
    (start, end)
}

pub fn in_judgment_set(task: &Task, _list: &TaskList, day_start: i64, day_end: i64) -> bool {
    if task.done {
        return false;
    }
    let (Some(start), Some(end)) = (task.start, task.end) else {
        return false;
    };
    start < day_end && end > day_start
}

pub fn judgment_tasks<'a>(
    tasks: &'a [Task],
    lists: &[TaskList],
    day_start: i64,
    day_end: i64,
) -> Result<Vec<&'a Task>, TaskListError> {
    validate_lists(lists)?;
    let selected: Vec<&Task> = tasks
        .iter()
        .filter(|task| {
            lists
                .iter()
                .find(|l| l.id == task.list_id)
                .is_some_and(|list| in_judgment_set(task, list, day_start, day_end))
        })
        .collect();
    if selected.len() > MAX_JUDGMENT_TASKS {
        return Err(TaskListError::TooManyJudgment);
    }
    Ok(selected)
}

pub fn snapshot_of(tasks: &[&Task], lists: &[TaskList]) -> Vec<TaskSnapshot> {
    tasks
        .iter()
        .filter_map(|task| {
            let list = lists.iter().find(|l| l.id == task.list_id)?;
            Some(TaskSnapshot {
                id: task.id.clone(),
                title: task.title.clone(),
                role: list.role,
            })
        })
        .collect()
}

pub fn parse_task_snapshot_json(json: &str) -> Result<Vec<TaskSnapshot>, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

pub fn tokenize_title(title: &str) -> Vec<String> {
    title
        .split(|c: char| {
            c.is_whitespace() || matches!(c, ',' | '，' | '。' | '；' | ';' | '|' | '/' | '\\')
        })
        .map(str::trim)
        .filter(|tok| tok.chars().count() >= 2 && !tok.contains('#'))
        .map(|tok| tok.to_string())
        .collect()
}

pub fn snapshot_evidence_quests(snapshots: &[TaskSnapshot]) -> Vec<crate::types::Quest> {
    snapshots
        .iter()
        .filter(|s| s.role == ListRole::Mainline)
        .map(|s| crate::types::Quest {
            text: s.title.clone(),
            evidence: tokenize_title(&s.title),
            hero: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lists() -> Vec<TaskList> {
        preset_lists()
    }

    fn timed(id: &str, list_id: &str, start: i64, end: i64) -> Task {
        Task {
            id: id.into(),
            list_id: list_id.into(),
            title: id.into(),
            done: false,
            start: Some(start),
            end: Some(end),
            range: None,
        }
    }

    #[test]
    fn longterm_without_window_is_not_judged() {
        let lists = lists();
        let t = Task {
            id: "a".into(),
            list_id: PRESET_LONGTERM_ID.into(),
            title: "本月论文".into(),
            done: false,
            start: None,
            end: None,
            range: Some(TaskRange::Month),
        };
        let day0 = 1_778_083_200;
        assert!(!in_judgment_set(&t, &lists[2], day0, day0 + 86400));
    }

    #[test]
    fn longterm_with_window_is_judged() {
        let lists = lists();
        let t = timed("a", PRESET_LONGTERM_ID, 1000, 1900);
        assert!(in_judgment_set(&t, &lists[2], 0, 86400));
    }

    #[test]
    fn done_or_too_many_rejected() {
        let lists = lists();
        let mut t = timed("a", PRESET_MAINLINE_ID, 1000, 1900);
        t.done = true;
        assert!(!in_judgment_set(&t, &lists[0], 0, 86400));
        let many: Vec<Task> = (0..21)
            .map(|i| timed(&format!("{i}"), PRESET_MAINLINE_ID, 1000, 1900))
            .collect();
        assert_eq!(
            judgment_tasks(&many, &lists, 0, 86400),
            Err(TaskListError::TooManyJudgment)
        );
    }

    #[test]
    fn preset_has_single_mainline() {
        validate_lists(&preset_lists()).unwrap();
        let mut bad = preset_lists();
        bad[1].role = ListRole::Mainline;
        assert_eq!(validate_lists(&bad), Err(TaskListError::DuplicateMainline));
    }

    #[test]
    fn align_snaps_to_900() {
        assert_eq!(align_range(100, 1000), (0, 1800));
    }

    #[test]
    fn tokenize_title_drops_hash_and_short_tokens() {
        let t = tokenize_title("RAIDS+ 讨论 #杂项");
        assert!(t.iter().any(|x| x == "RAIDS+"));
        assert!(t.iter().any(|x| x == "讨论"));
        assert!(!t.iter().any(|x| x.contains('#')));
    }

    #[test]
    fn snapshot_evidence_only_mainline() {
        let snaps = [
            TaskSnapshot {
                id: "tt-1".into(),
                title: "HDP train".into(),
                role: ListRole::Mainline,
            },
            TaskSnapshot {
                id: "tt-2".into(),
                title: "报销".into(),
                role: ListRole::Chore,
            },
        ];
        let q = snapshot_evidence_quests(&snaps);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].text, "HDP train");
        assert!(q[0].evidence.iter().any(|e| e == "HDP"));
    }
}
