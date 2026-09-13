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

pub fn match_role_alias(tag: &str) -> Option<ListRole> {
    match tag.trim() {
        "主线" | "主线任务" => Some(ListRole::Mainline),
        "支线" | "支线任务" => Some(ListRole::Side),
        "长期" | "长期计划" => Some(ListRole::Longterm),
        "杂项" => Some(ListRole::Chore),
        _ => None,
    }
}

pub fn ticktick_snapshot_id(raw_id: &str) -> String {
    if raw_id.starts_with("tt-") {
        raw_id.to_string()
    } else {
        format!("tt-{raw_id}")
    }
}

pub fn role_from_hashtag(title: &str, fallback: ListRole) -> ListRole {
    let Some(idx) = title.rfind('#') else {
        return fallback;
    };
    let tag = title[idx + 1..].trim();
    let tag_end = tag
        .find(|c: char| c.is_whitespace() || c == ',' || c == '，')
        .unwrap_or(tag.len());
    match_role_alias(&tag[..tag_end]).unwrap_or(fallback)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimedTask {
    pub id: String,
    pub title: String,
    pub role: ListRole,
    pub start: i64,
    pub end: i64,
    pub done: bool,
}

fn overlapping_timed<'a>(
    tasks: &'a [TimedTask],
    day_start: i64,
    day_end: i64,
) -> Vec<&'a TimedTask> {
    tasks
        .iter()
        .filter(|t| !t.done && t.start < day_end && t.end > day_start)
        .collect()
}

pub fn ticktick_overlapping_count(tasks: &[TimedTask], day_start: i64, day_end: i64) -> usize {
    overlapping_timed(tasks, day_start, day_end).len()
}

pub fn ticktick_judgment_set(tasks: &[TimedTask], day_start: i64, day_end: i64) -> Vec<&TimedTask> {
    let selected = overlapping_timed(tasks, day_start, day_end);
    if selected.len() > MAX_JUDGMENT_TASKS {
        Vec::new()
    } else {
        selected
    }
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

    #[test]
    fn ticktick_id_is_prefixed_once() {
        assert_eq!(ticktick_snapshot_id("abc"), "tt-abc");
        assert_eq!(ticktick_snapshot_id("tt-abc"), "tt-abc");
    }

    #[test]
    fn hashtag_overrides_project_role() {
        assert_eq!(
            role_from_hashtag("讨论 #杂项", ListRole::Mainline),
            ListRole::Chore
        );
        assert_eq!(role_from_hashtag("讨论", ListRole::Side), ListRole::Side);
    }

    #[test]
    fn more_than_twenty_timed_tasks_yields_empty_set() {
        let tasks: Vec<TimedTask> = (0..21)
            .map(|i| TimedTask {
                id: format!("{i}"),
                title: format!("t{i}"),
                role: ListRole::Mainline,
                start: 1000,
                end: 1900,
                done: false,
            })
            .collect();
        assert!(ticktick_judgment_set(&tasks, 0, 86400).is_empty());
        assert_eq!(ticktick_overlapping_count(&tasks, 0, 86400), 21);
    }

    #[test]
    fn overlapping_count_ignores_non_overlapping_and_done() {
        let tasks = vec![
            TimedTask {
                id: "a".into(),
                title: "a".into(),
                role: ListRole::Mainline,
                start: 1000,
                end: 1900,
                done: false,
            },
            TimedTask {
                id: "b".into(),
                title: "b".into(),
                role: ListRole::Mainline,
                start: 100_000,
                end: 101_000,
                done: false,
            },
            TimedTask {
                id: "c".into(),
                title: "c".into(),
                role: ListRole::Mainline,
                start: 1000,
                end: 1900,
                done: true,
            },
        ];
        assert_eq!(ticktick_overlapping_count(&tasks, 0, 86400), 1);
        assert_eq!(ticktick_judgment_set(&tasks, 0, 86400).len(), 1);
    }
}
