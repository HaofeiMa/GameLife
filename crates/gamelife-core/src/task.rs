use chrono::{Months, TimeZone};
use serde::{Deserialize, Serialize};

pub const MAX_JUDGMENT_TASKS: usize = 20;
pub const UNSCHEDULED_DROP_SECS: i64 = 1800;
pub const PRESET_MAINLINE_ID: &str = "list-mainline";
pub const PRESET_SIDE_ID: &str = "list-side";
pub const PRESET_LONGTERM_ID: &str = "list-longterm";
pub const PRESET_CHORE_ID: &str = "list-chore";
pub const PRESET_LIST_IDS: [&str; 4] = [
    PRESET_MAINLINE_ID,
    PRESET_SIDE_ID,
    PRESET_LONGTERM_ID,
    PRESET_CHORE_ID,
];

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatRule {
    #[default]
    None,
    Daily,
    Weekly,
    Monthly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    Start,
    End,
}

pub const ALLOWED_REMIND_OFFSETS: [i64; 5] = [0, 5, 15, 30, 60];
pub const MAX_NOTES_BYTES: usize = 8192;

pub fn notes_ok(notes: &str) -> bool {
    notes.len() <= MAX_NOTES_BYTES
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
    #[serde(default)]
    pub sort: i64,
    #[serde(default)]
    pub repeat: RepeatRule,
    #[serde(default)]
    pub remind_offsets: Vec<i64>,
    #[serde(default)]
    pub notes: String,
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
    EmptyName,
    TooManyJudgment,
    PresetLocked,
    NotEmpty,
    MissingList,
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
            name: "长期规划".into(),
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
    if !lists.iter().any(|l| l.role == ListRole::Mainline) {
        return Err(TaskListError::NoMainline);
    }
    Ok(())
}

pub fn is_preset_list_id(id: &str) -> bool {
    PRESET_LIST_IDS.contains(&id)
}

pub fn parse_list_role_strict(role: &str) -> Option<ListRole> {
    match role {
        "mainline" => Some(ListRole::Mainline),
        "side" => Some(ListRole::Side),
        "longterm" => Some(ListRole::Longterm),
        "chore" => Some(ListRole::Chore),
        _ => None,
    }
}

pub fn can_delete_list(lists: &[TaskList], tasks: &[Task], id: &str) -> Result<(), TaskListError> {
    if is_preset_list_id(id) {
        return Err(TaskListError::PresetLocked);
    }
    let Some(list) = lists.iter().find(|l| l.id == id) else {
        return Err(TaskListError::MissingList);
    };
    if tasks.iter().any(|t| t.list_id == id) {
        return Err(TaskListError::NotEmpty);
    }
    if list.role == ListRole::Mainline {
        let mainline = lists
            .iter()
            .filter(|l| l.role == ListRole::Mainline)
            .count();
        if mainline <= 1 {
            return Err(TaskListError::NoMainline);
        }
    }
    Ok(())
}

pub fn align_range(start: i64, end: i64) -> (i64, i64) {
    let start = start / 900 * 900;
    let mut end = ((end + 899) / 900) * 900;
    if end <= start {
        end = start + 900;
    }
    (start, end)
}

pub fn remind_offsets_ok(offsets: &[i64]) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    for &n in offsets {
        if !ALLOWED_REMIND_OFFSETS.contains(&n) || !seen.insert(n) {
            return false;
        }
    }
    true
}

fn add_one_period(start: i64, end: i64, repeat: RepeatRule) -> (i64, i64) {
    let dur = end - start;
    let next_start = match repeat {
        RepeatRule::None => start,
        RepeatRule::Daily => start + 86_400,
        RepeatRule::Weekly => start + 7 * 86_400,
        RepeatRule::Monthly => add_calendar_month(start),
    };
    (next_start, next_start + dur)
}

fn add_calendar_month(ts: i64) -> i64 {
    let dt = chrono::Local
        .timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(|| {
            chrono::DateTime::from_timestamp(ts, 0)
                .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH)
                .with_timezone(&chrono::Local)
        });
    dt.checked_add_months(Months::new(1))
        .unwrap_or(dt)
        .timestamp()
}

pub fn next_occurrence(
    start: i64,
    end: i64,
    repeat: RepeatRule,
    today_start: i64,
) -> Option<(i64, i64)> {
    if repeat == RepeatRule::None {
        return None;
    }
    let mut s = start;
    let mut e = end;
    for _ in 0..4096 {
        let next = add_one_period(s, e, repeat);
        s = next.0;
        e = next.1;
        if s >= today_start {
            return Some((s, e));
        }
    }
    Some((s, e))
}

pub fn spawn_after_complete(
    done: &Task,
    new_id: String,
    today_start: i64,
    sort: i64,
) -> Option<Task> {
    let (start, end) = (done.start?, done.end?);
    let (start, end) = next_occurrence(start, end, done.repeat, today_start)?;
    Some(Task {
        id: new_id,
        list_id: done.list_id.clone(),
        title: done.title.clone(),
        done: false,
        start: Some(start),
        end: Some(end),
        range: done.range,
        sort,
        repeat: done.repeat,
        remind_offsets: done.remind_offsets.clone(),
        notes: done.notes.clone(),
    })
}

pub fn resize_range(start: i64, end: i64, edge: ResizeEdge, at: i64) -> (i64, i64) {
    let at = at / 900 * 900;
    match edge {
        ResizeEdge::Start => {
            let start = at.min(end - 900);
            align_range(start, end)
        }
        ResizeEdge::End => {
            let end = at.max(start + 900);
            align_range(start, end)
        }
    }
}

pub fn clear_schedule(task: &mut Task) {
    task.start = None;
    task.end = None;
    task.repeat = RepeatRule::None;
    task.remind_offsets.clear();
}

pub fn in_judgment_set(task: &Task, _list: &TaskList, _day_start: i64, _day_end: i64) -> bool {
    !task.done
}

pub fn judgment_tasks<'a>(
    tasks: &'a [Task],
    lists: &[TaskList],
    _day_start: i64,
    _day_end: i64,
) -> Result<Vec<&'a Task>, TaskListError> {
    validate_lists(lists)?;
    Ok(tasks
        .iter()
        .filter(|task| {
            lists
                .iter()
                .find(|l| l.id == task.list_id)
                .is_some_and(|list| in_judgment_set(task, list, 0, 0))
        })
        .collect())
}

pub fn schedule_from_drop(ts: i64) -> (i64, i64) {
    let start = ts / 900 * 900;
    (start, start + UNSCHEDULED_DROP_SECS)
}

pub fn move_range_to_day(
    start: i64,
    end: i64,
    old_day_start: i64,
    new_day_start: i64,
) -> (i64, i64) {
    let offset = start - old_day_start;
    let dur = end - start;
    align_range(new_day_start + offset, new_day_start + offset + dur)
}

pub fn snapshots_open(tasks: &[Task], lists: &[TaskList]) -> Vec<TaskSnapshot> {
    let selected: Vec<&Task> = tasks.iter().filter(|task| !task.done).collect();
    snapshot_of(&selected, lists)
}

pub fn snapshots_for_day(
    tasks: &[Task],
    lists: &[TaskList],
    _day_start: i64,
    _day_end: i64,
) -> Vec<TaskSnapshot> {
    snapshots_open(tasks, lists)
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
            c.is_whitespace()
                || matches!(
                    c,
                    ',' | '，' | '。' | '；' | ';' | '|' | '/' | '\\' | '.' | '_' | '-'
                )
        })
        .map(str::trim)
        .filter(|tok| tok.chars().count() >= 2 && !tok.contains('#'))
        .map(|tok| tok.to_string())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchRole {
    None,
    Mixed,
    Role(ListRole),
}

fn token_hit(title: &str, fields: &[&str]) -> bool {
    let title_l = title.to_lowercase();
    let title_toks = tokenize_title(title);
    for tok in &title_toks {
        let needle = tok.to_lowercase();
        if fields.iter().any(|f| f.to_lowercase().contains(&needle)) {
            return true;
        }
    }
    for field in fields {
        for tok in tokenize_title(field) {
            if title_l.contains(&tok.to_lowercase()) {
                return true;
            }
        }
    }
    false
}

pub fn match_task_role(
    app: &str,
    title: &str,
    url: Option<&str>,
    document_path: Option<&str>,
    snapshots: &[TaskSnapshot],
) -> MatchRole {
    let mut fields: Vec<&str> = vec![app, title];
    if let Some(u) = url {
        fields.push(u);
    }
    if let Some(p) = document_path {
        fields.push(p);
    }
    let mut roles = Vec::new();
    for snap in snapshots {
        if token_hit(&snap.title, &fields) && !roles.contains(&snap.role) {
            roles.push(snap.role);
        }
    }
    match roles.as_slice() {
        [] => MatchRole::None,
        [role] => MatchRole::Role(*role),
        _ => MatchRole::Mixed,
    }
}

pub fn select_prompt_snapshots(all: &[TaskSnapshot], haystacks: &[&str]) -> Vec<TaskSnapshot> {
    let mut out = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    let hit = |s: &TaskSnapshot| token_hit(&s.title, haystacks);
    for s in all.iter().filter(|s| hit(s)) {
        if used.insert(s.id.clone()) {
            out.push(s.clone());
        }
    }
    for s in all.iter().filter(|s| s.role == ListRole::Mainline) {
        if used.insert(s.id.clone()) {
            out.push(s.clone());
        }
    }
    for s in all {
        if used.insert(s.id.clone()) {
            out.push(s.clone());
        }
    }
    out.truncate(MAX_JUDGMENT_TASKS);
    out
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
        "长期" | "长期计划" | "长期规划" => Some(ListRole::Longterm),
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
    /// TickTick all-day / date-only tasks. Listed on 今日, never judged.
    pub all_day: bool,
}

fn overlapping_timed<'a>(
    tasks: &'a [TimedTask],
    day_start: i64,
    day_end: i64,
) -> Vec<&'a TimedTask> {
    tasks
        .iter()
        .filter(|t| !t.done && !t.all_day && t.start < day_end && t.end > day_start)
        .collect()
}

/// Start or end falls on `[day_start, day_end)`, or an all-day window overlaps it.
pub fn ticktick_listed_on_day(task: &TimedTask, day_start: i64, day_end: i64) -> bool {
    if task.done {
        return false;
    }
    if task.all_day {
        return task.start < day_end && task.end > day_start;
    }
    let start_on_day = task.start >= day_start && task.start < day_end;
    let end_on_day = task.end > day_start && task.end <= day_end;
    start_on_day || end_on_day
}

pub fn ticktick_day_list(tasks: &[TimedTask], day_start: i64, day_end: i64) -> Vec<&TimedTask> {
    let mut listed: Vec<&TimedTask> = tasks
        .iter()
        .filter(|t| ticktick_listed_on_day(t, day_start, day_end))
        .collect();
    listed.sort_by_key(|t| (t.start, t.end, t.title.as_str()));
    listed
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
    use chrono::{Datelike, TimeZone};

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
            sort: 0,
            repeat: RepeatRule::None,
            remind_offsets: vec![],
            notes: String::new(),
        }
    }

    #[test]
    fn longterm_without_window_is_judged() {
        let lists = lists();
        let t = Task {
            id: "a".into(),
            list_id: PRESET_LONGTERM_ID.into(),
            title: "本月论文".into(),
            done: false,
            start: None,
            end: None,
            range: Some(TaskRange::Month),
            sort: 0,
            repeat: RepeatRule::None,
            remind_offsets: vec![],
            notes: String::new(),
        };
        let day0 = 1_778_083_200;
        assert!(in_judgment_set(&t, &lists[2], day0, day0 + 86400));
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
        assert_eq!(judgment_tasks(&many, &lists, 0, 86400).unwrap().len(), 21);
    }

    #[test]
    fn validate_lists_allows_multiple_mainline() {
        let mut lists = preset_lists();
        lists.push(TaskList {
            id: "list-lab".into(),
            name: "实验".into(),
            sort: 4,
            role: ListRole::Mainline,
        });
        validate_lists(&lists).unwrap();
    }

    #[test]
    fn validate_lists_rejects_zero_mainline() {
        let lists: Vec<TaskList> = preset_lists()
            .into_iter()
            .filter(|l| l.role != ListRole::Mainline)
            .collect();
        assert_eq!(validate_lists(&lists), Err(TaskListError::NoMainline));
    }

    #[test]
    fn preset_longterm_is_named_planning() {
        let long = preset_lists()
            .into_iter()
            .find(|l| l.id == PRESET_LONGTERM_ID)
            .unwrap();
        assert_eq!(long.name, "长期规划");
    }

    #[test]
    fn cannot_delete_preset_or_last_mainline_or_nonempty() {
        let lists = preset_lists();
        let tasks = vec![timed("a", PRESET_MAINLINE_ID, 1000, 1900)];
        assert_eq!(
            can_delete_list(&lists, &tasks, PRESET_MAINLINE_ID),
            Err(TaskListError::PresetLocked)
        );
        assert_eq!(
            can_delete_list(&lists, &[], PRESET_SIDE_ID),
            Err(TaskListError::PresetLocked)
        );
        let mut extra = lists.clone();
        extra.push(TaskList {
            id: "list-lab".into(),
            name: "实验".into(),
            sort: 4,
            role: ListRole::Mainline,
        });
        let occupied = vec![timed("a", "list-lab", 1000, 1900)];
        assert_eq!(
            can_delete_list(&extra, &occupied, "list-lab"),
            Err(TaskListError::NotEmpty)
        );
        can_delete_list(&extra, &[], "list-lab").unwrap();
    }

    #[test]
    fn drop_unscheduled_is_thirty_minutes_aligned() {
        assert_eq!(schedule_from_drop(100), (0, 1800));
    }

    #[test]
    fn move_range_keeps_duration_on_new_day() {
        let old = 1_778_083_200;
        let start = old + 10 * 3600;
        let end = start + 3600;
        let new_day = old + 86400;
        let (s, e) = move_range_to_day(start, end, old, new_day);
        assert_eq!(e - s, 3600);
        assert_eq!(s - new_day, start - old);
    }

    #[test]
    fn snapshots_for_day_caps_at_twenty_instead_of_empty() {
        let lists = preset_lists();
        let many: Vec<Task> = (0..21)
            .map(|i| timed(&format!("{i}"), PRESET_MAINLINE_ID, 1000 + i, 1900))
            .collect();
        let snaps = snapshots_for_day(&many, &lists, 0, 86400);
        assert_eq!(snaps.len(), 21);
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
                all_day: false,
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
                all_day: false,
            },
            TimedTask {
                id: "b".into(),
                title: "b".into(),
                role: ListRole::Mainline,
                start: 100_000,
                end: 101_000,
                done: false,
                all_day: false,
            },
            TimedTask {
                id: "c".into(),
                title: "c".into(),
                role: ListRole::Mainline,
                start: 1000,
                end: 1900,
                done: true,
                all_day: false,
            },
        ];
        assert_eq!(ticktick_overlapping_count(&tasks, 0, 86400), 1);
        assert_eq!(ticktick_judgment_set(&tasks, 0, 86400).len(), 1);
    }

    fn listed(id: &str, start: i64, end: i64, all_day: bool) -> TimedTask {
        TimedTask {
            id: id.into(),
            title: id.into(),
            role: ListRole::Mainline,
            start,
            end,
            done: false,
            all_day,
        }
    }

    #[test]
    fn all_day_tasks_are_listed_but_never_judged() {
        let day_start = 0;
        let day_end = 86_400;
        let all_day = listed("all", day_start, day_end, true);
        let timed = listed("timed", 10 * 3600, 11 * 3600, false);
        let tasks = vec![all_day.clone(), timed.clone()];
        assert_eq!(ticktick_overlapping_count(&tasks, day_start, day_end), 1);
        assert_eq!(
            ticktick_judgment_set(&tasks, day_start, day_end)
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["timed"]
        );
        let listed_ids: Vec<_> = ticktick_day_list(&tasks, day_start, day_end)
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(listed_ids, vec!["all", "timed"]);
    }

    #[test]
    fn day_list_keeps_tasks_whose_start_or_end_falls_on_the_day() {
        let day_start = 86_400;
        let day_end = 172_800;
        let starts_today = listed("start", day_start + 3600, day_end + 3600, false);
        let ends_today = listed("end", day_start - 3600, day_start + 3600, false);
        let yesterday = listed("old", 0, 3600, false);
        let tasks = vec![starts_today, ends_today, yesterday];
        let ids: Vec<_> = ticktick_day_list(&tasks, day_start, day_end)
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(ids, vec!["end", "start"]);
    }

    #[test]
    fn next_daily_skips_until_today() {
        let start = 0;
        let end = 3600;
        let today = 3 * 86400;
        let (s, e) = next_occurrence(start, end, RepeatRule::Daily, today).unwrap();
        assert_eq!(s, today);
        assert_eq!(e, today + 3600);
    }

    #[test]
    fn next_monthly_clamps_jan31() {
        let jan31 = chrono::Local
            .with_ymd_and_hms(2026, 1, 31, 10, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let end = jan31 + 3600;
        let today = chrono::Local
            .with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let (s, _) = next_occurrence(jan31, end, RepeatRule::Monthly, today).unwrap();
        let dt = chrono::DateTime::from_timestamp(s, 0)
            .unwrap()
            .with_timezone(&chrono::Local);
        assert_eq!(dt.month(), 2);
        assert!(dt.day() == 28 || dt.day() == 29);
    }

    #[test]
    fn spawn_none_repeat_is_none() {
        let t = timed("a", PRESET_MAINLINE_ID, 0, 1800);
        assert!(spawn_after_complete(&t, "b".into(), 0, 1).is_none());
    }

    #[test]
    fn notes_ok_is_utf8_bytes() {
        assert!(notes_ok(""));
        assert!(notes_ok(&"a".repeat(8192)));
        assert!(!notes_ok(&"a".repeat(8193)));
    }

    #[test]
    fn new_task_notes_default_empty() {
        let t = timed("a", PRESET_MAINLINE_ID, 0, 1800);
        assert_eq!(t.notes, "");
    }

    #[test]
    fn spawn_after_complete_copies_notes() {
        let mut t = timed("a", PRESET_MAINLINE_ID, 0, 1800);
        t.repeat = RepeatRule::Daily;
        t.notes = "指标 0.91".into();
        let next = spawn_after_complete(&t, "b".into(), 86_400, 1).expect("next");
        assert_eq!(next.notes, "指标 0.91");
        assert_ne!(next.id, t.id);
    }

    #[test]
    fn snapshot_json_has_no_notes_field() {
        let mut open = timed("u", PRESET_MAINLINE_ID, 0, 1800);
        open.notes = "secret-never-judge".into();
        let snaps = snapshots_open(&[open], &lists());
        let json = serde_json::to_string(&snaps).unwrap();
        assert!(!json.contains("secret-never-judge"));
        assert!(!json.contains("notes"));
    }

    #[test]
    fn task_json_missing_notes_deserializes_empty() {
        let t: Task = serde_json::from_str(
            r#"{"id":"a","list_id":"list-mainline","title":"x","done":false,"start":null,"end":null,"range":null}"#,
        )
        .unwrap();
        assert_eq!(t.notes, "");
    }

    #[test]
    fn clear_schedule_drops_repeat() {
        let mut t = timed("a", PRESET_MAINLINE_ID, 0, 1800);
        t.repeat = RepeatRule::Weekly;
        t.remind_offsets = vec![0, 15];
        clear_schedule(&mut t);
        assert_eq!(t.start, None);
        assert_eq!(t.repeat, RepeatRule::None);
        assert!(t.remind_offsets.is_empty());
    }

    #[test]
    fn resize_start_keeps_end_min_900() {
        let (s, e) = resize_range(1800, 3600, ResizeEdge::Start, 3000);
        assert_eq!((s, e), (2700, 3600));
        let (s, e) = resize_range(0, 1800, ResizeEdge::End, 100);
        assert_eq!(e - s, 900);
    }

    #[test]
    fn remind_offsets_reject_unknown() {
        assert!(remind_offsets_ok(&[0, 15, 60]));
        assert!(!remind_offsets_ok(&[7]));
        assert!(!remind_offsets_ok(&[0, 0]));
    }

    #[test]
    fn snapshots_open_includes_unscheduled_and_other_days() {
        let lists = lists();
        let open = Task {
            id: "u".into(),
            list_id: PRESET_MAINLINE_ID.into(),
            title: "inbox".into(),
            done: false,
            start: None,
            end: None,
            range: None,
            sort: 0,
            repeat: RepeatRule::None,
            remind_offsets: vec![],
            notes: String::new(),
        };
        let snaps = snapshots_open(&[open], &lists);
        assert_eq!(snaps.len(), 1);
    }

    #[test]
    fn match_same_role_two_titles() {
        let snaps = vec![
            TaskSnapshot {
                id: "a".into(),
                title: "写论文方法节".into(),
                role: ListRole::Mainline,
            },
            TaskSnapshot {
                id: "b".into(),
                title: "写论文讨论".into(),
                role: ListRole::Mainline,
            },
        ];
        assert_eq!(
            match_task_role("Overleaf", "方法节.tex", None, None, &snaps),
            MatchRole::Role(ListRole::Mainline)
        );
    }

    #[test]
    fn match_mixed_roles_is_mixed() {
        let snaps = vec![
            TaskSnapshot {
                id: "a".into(),
                title: "报销单".into(),
                role: ListRole::Chore,
            },
            TaskSnapshot {
                id: "b".into(),
                title: "论文".into(),
                role: ListRole::Mainline,
            },
        ];
        assert_eq!(
            match_task_role("Preview", "论文 报销单", None, None, &snaps),
            MatchRole::Mixed
        );
    }

    #[test]
    fn prompt_snapshots_cap_prefers_hits() {
        let mut all = Vec::new();
        for i in 0..25 {
            all.push(TaskSnapshot {
                id: format!("{i}"),
                title: format!("任务{i}"),
                role: ListRole::Side,
            });
        }
        all[24].title = "HDP train".into();
        all[24].role = ListRole::Mainline;
        let picked = select_prompt_snapshots(&all, &["HDP train.py"]);
        assert_eq!(picked.len(), 20);
        assert!(picked.iter().any(|s| s.id == "24"));
    }
}
