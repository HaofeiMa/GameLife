use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::task::{
    ListRole, PRESET_CHORE_ID, PRESET_LIST_IDS, PRESET_LONGTERM_ID, PRESET_MAINLINE_ID,
    PRESET_SIDE_ID,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteColumn {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
    pub project_id: String,
}

pub struct RemoteProject {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
    pub columns: Vec<RemoteColumn>,
}

#[derive(Clone, Debug)]
pub struct RemoteTask {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub all_day: bool,
    pub etag: String,
    pub column_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LocalMirror {
    pub local_id: String,
    pub list_id: String,
    pub title: String,
    pub done: bool,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub ticktick_task_id: Option<String>,
    pub ticktick_project_id: Option<String>,
    pub ticktick_etag: String,
    pub ticktick_dirty: i64,
    pub ticktick_all_day: bool,
}

#[derive(Clone, Debug)]
pub struct ProjectFetch {
    pub project_id: String,
    pub ok: bool,
    pub open: Vec<RemoteTask>,
}

#[derive(Clone, Debug)]
pub enum ReconcileAction {
    Insert(LocalMirror),
    Update(LocalMirror),
    Delete { local_id: String },
    DropIgnored { local_id: String },
    Link {
        local_id: String,
        ticktick_task_id: String,
        ticktick_project_id: String,
        etag: String,
    },
}

pub struct ReconcileInput<'a> {
    pub local: &'a [LocalMirror],
    pub projects: &'a [RemoteProject],
    pub roles: &'a BTreeMap<String, String>,
    pub column_roles: &'a BTreeMap<String, String>,
    pub fetches: &'a [ProjectFetch],
    pub completed_ids: &'a [String],
    pub completed_ok: bool,
    pub first_sync: bool,
    pub day_start: i64,
    pub next_day_start: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushKind {
    Create,
    Update,
    Complete,
    Reopen,
    Delete,
    Move,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushOp {
    pub kind: PushKind,
    pub local_id: String,
    pub task_id: Option<String>,
    pub project_id: String,
    pub to_project_id: Option<String>,
    pub title: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub all_day: bool,
}

pub fn parse_role(raw: &str) -> Option<ListRole> {
    if raw.is_empty() || raw == "ignore" {
        return None;
    }
    match raw {
        "mainline" => Some(ListRole::Mainline),
        "side" => Some(ListRole::Side),
        "longterm" => Some(ListRole::Longterm),
        "chore" => Some(ListRole::Chore),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredRole {
    Ignore,
    Role(ListRole),
}

pub fn parse_stored_role(raw: &str) -> Option<StoredRole> {
    match raw {
        "ignore" => Some(StoredRole::Ignore),
        "mainline" => Some(StoredRole::Role(ListRole::Mainline)),
        "side" => Some(StoredRole::Role(ListRole::Side)),
        "longterm" => Some(StoredRole::Role(ListRole::Longterm)),
        "chore" => Some(StoredRole::Role(ListRole::Chore)),
        _ => None,
    }
}

pub fn effective_role(
    project_roles: &BTreeMap<String, String>,
    column_roles: &BTreeMap<String, String>,
    project_id: &str,
    column_id: Option<&str>,
) -> Option<ListRole> {
    if let Some(cid) = column_id.filter(|s| !s.is_empty()) {
        if let Some(stored) = column_roles.get(cid).and_then(|raw| parse_stored_role(raw)) {
            return match stored {
                StoredRole::Ignore => None,
                StoredRole::Role(role) => Some(role),
            };
        }
    }
    project_roles
        .get(project_id)
        .and_then(|raw| parse_role(raw))
}

pub fn projects_to_fetch<'a>(
    projects: &'a [RemoteProject],
    project_roles: &BTreeMap<String, String>,
    column_roles: &BTreeMap<String, String>,
) -> Vec<&'a RemoteProject> {
    projects
        .iter()
        .filter(|project| {
            let list_mapped = project_roles
                .get(&project.id)
                .and_then(|raw| parse_role(raw))
                .is_some();
            if list_mapped {
                return true;
            }
            project.columns.iter().any(|column| {
                column_roles
                    .get(&column.id)
                    .and_then(|raw| parse_role(raw))
                    .is_some()
            })
        })
        .collect()
}

pub fn sort_columns(columns: &mut [RemoteColumn]) {
    columns.sort_by(|left, right| (left.sort_order, &left.id).cmp(&(right.sort_order, &right.id)));
}

pub fn parse_columns(project_id: &str, value: &serde_json::Value) -> Vec<RemoteColumn> {
    let Some(arr) = value.get("columns").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            let id = json_string_field(obj, "id")?;
            let name = json_string_field(obj, "name").unwrap_or_default();
            let sort_order = json_i64_field(obj, "sortOrder").unwrap_or(0);
            let project_id =
                json_string_field(obj, "projectId").unwrap_or_else(|| project_id.to_string());
            Some(RemoteColumn {
                id,
                name,
                sort_order,
                project_id,
            })
        })
        .collect()
}

pub fn kept_column_ids(projects: &[RemoteProject]) -> BTreeSet<String> {
    projects
        .iter()
        .flat_map(|project| project.columns.iter().map(|column| column.id.clone()))
        .collect()
}

pub fn prune_column_roles(kept: &BTreeSet<String>, column_roles: &mut BTreeMap<String, String>) {
    column_roles.retain(|id, _| kept.contains(id));
}

pub fn list_id_for_role(role: ListRole) -> Option<&'static str> {
    match role {
        ListRole::Mainline => Some(PRESET_MAINLINE_ID),
        ListRole::Side => Some(PRESET_SIDE_ID),
        ListRole::Longterm => Some(PRESET_LONGTERM_ID),
        ListRole::Chore => Some(PRESET_CHORE_ID),
        ListRole::Custom => None,
    }
}

fn role_for_list_id(list_id: &str) -> Option<ListRole> {
    match list_id {
        PRESET_MAINLINE_ID => Some(ListRole::Mainline),
        PRESET_SIDE_ID => Some(ListRole::Side),
        PRESET_LONGTERM_ID => Some(ListRole::Longterm),
        PRESET_CHORE_ID => Some(ListRole::Chore),
        _ => None,
    }
}

fn role_for_project(roles: &BTreeMap<String, String>, project_id: &str) -> Option<ListRole> {
    roles.get(project_id).and_then(|raw| parse_role(raw))
}

fn is_preset_list(list_id: &str) -> bool {
    PRESET_LIST_IDS.contains(&list_id)
}

pub fn write_target<'a>(
    projects: &'a [RemoteProject],
    roles: &BTreeMap<String, String>,
    role: ListRole,
) -> Option<&'a RemoteProject> {
    projects
        .iter()
        .filter(|project| {
            roles
                .get(&project.id)
                .and_then(|raw| parse_role(raw))
                .is_some_and(|parsed| parsed == role)
        })
        .min_by(|left, right| (left.sort_order, &left.id).cmp(&(right.sort_order, &right.id)))
}

pub fn stamp_missing_roles(projects: &[RemoteProject], roles: &mut BTreeMap<String, String>) {
    for project in projects {
        roles
            .entry(project.id.clone())
            .or_insert_with(|| "ignore".into());
    }
}

pub fn map_times(
    all_day: bool,
    start: Option<i64>,
    end: Option<i64>,
    day_start: i64,
    next_day_start: i64,
) -> (Option<i64>, Option<i64>, bool) {
    if all_day {
        (Some(day_start), Some(next_day_start), true)
    } else {
        (start, end, false)
    }
}

fn parse_ticktick_ts(raw: &str) -> Option<i64> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(dt.timestamp());
    }
    // TickTick often emits `+0000` instead of `+00:00`.
    let fixed = if raw.len() >= 5 {
        let (head, tail) = raw.split_at(raw.len() - 5);
        if (tail.starts_with('+') || tail.starts_with('-'))
            && tail[1..].bytes().all(|b| b.is_ascii_digit())
        {
            format!("{head}{}:{}", &tail[..3], &tail[3..])
        } else {
            raw.to_string()
        }
    } else {
        raw.to_string()
    };
    chrono::DateTime::parse_from_rfc3339(&fixed)
        .ok()
        .map(|dt| dt.timestamp())
}

fn json_string_field(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    obj.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn json_i64_field(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<i64> {
    obj.get(key).and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_f64().map(|f| f as i64))
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

pub fn parse_projects(value: &serde_json::Value) -> Vec<RemoteProject> {
    let arr = value
        .as_array()
        .or_else(|| value.get("projects").and_then(|v| v.as_array()));
    let Some(arr) = arr else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            let id = json_string_field(obj, "id")?;
            let name = json_string_field(obj, "name").unwrap_or_default();
            let sort_order = json_i64_field(obj, "sortOrder").unwrap_or(0);
            let columns = parse_columns(&id, item);
            Some(RemoteProject {
                id,
                name,
                sort_order,
                columns,
            })
        })
        .collect()
}

pub fn parse_open_tasks(project_id: &str, value: &serde_json::Value) -> Vec<RemoteTask> {
    let Some(arr) = value.get("tasks").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            let status = json_i64_field(obj, "status").unwrap_or(0);
            if status == 2 {
                return None;
            }
            let id = json_string_field(obj, "id")?;
            let title = json_string_field(obj, "title").unwrap_or_default();
            let etag = json_string_field(obj, "etag").unwrap_or_default();
            let column_id = json_string_field(obj, "columnId").filter(|s| !s.is_empty());
            let all_day = obj
                .get("isAllDay")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let start = obj
                .get("startDate")
                .and_then(|v| v.as_str())
                .and_then(parse_ticktick_ts);
            let end = obj
                .get("dueDate")
                .and_then(|v| v.as_str())
                .and_then(parse_ticktick_ts);
            Some(RemoteTask {
                id,
                project_id: project_id.into(),
                title,
                start,
                end,
                all_day,
                etag,
                column_id,
            })
        })
        .collect()
}

pub fn parse_completed_ids(value: &serde_json::Value) -> Vec<String> {
    let arr = value
        .as_array()
        .or_else(|| value.get("tasks").and_then(|v| v.as_array()));
    let Some(arr) = arr else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            item.as_object()
                .and_then(|obj| json_string_field(obj, "id"))
        })
        .collect()
}

fn apply_remote_fields(
    local: &LocalMirror,
    remote: &RemoteTask,
    roles: &BTreeMap<String, String>,
    column_roles: &BTreeMap<String, String>,
    day_start: i64,
    next_day_start: i64,
    done: bool,
) -> LocalMirror {
    let (start, end, all_day) = map_times(
        remote.all_day,
        remote.start,
        remote.end,
        day_start,
        next_day_start,
    );
    let list_id = effective_role(
        roles,
        column_roles,
        &remote.project_id,
        remote.column_id.as_deref(),
    )
    .and_then(list_id_for_role)
    .map(str::to_string)
    .unwrap_or_else(|| local.list_id.clone());
    LocalMirror {
        local_id: local.local_id.clone(),
        list_id,
        title: remote.title.clone(),
        done,
        start,
        end,
        ticktick_task_id: Some(remote.id.clone()),
        ticktick_project_id: Some(remote.project_id.clone()),
        ticktick_etag: remote.etag.clone(),
        ticktick_dirty: 0,
        ticktick_all_day: all_day,
    }
}

pub fn reconcile(input: &ReconcileInput) -> Vec<ReconcileAction> {
    let mapped: BTreeSet<&str> = projects_to_fetch(input.projects, input.roles, input.column_roles)
        .into_iter()
        .map(|project| project.id.as_str())
        .collect();

    let ok_fetches: BTreeSet<&str> = input
        .fetches
        .iter()
        .filter(|f| f.ok)
        .map(|f| f.project_id.as_str())
        .collect();
    let failed: BTreeSet<&str> = mapped
        .iter()
        .copied()
        .filter(|id| !ok_fetches.contains(id))
        .collect();

    let mut open_by_id: HashMap<String, RemoteTask> = HashMap::new();
    for fetch in input.fetches.iter().filter(|f| f.ok) {
        for task in &fetch.open {
            open_by_id
                .entry(task.id.clone())
                .or_insert_with(|| task.clone());
        }
    }

    let completed: BTreeSet<&str> = input.completed_ids.iter().map(String::as_str).collect();
    let allow_delete = !input.first_sync && input.completed_ok && failed.is_empty();

    let mut claimed: BTreeSet<String> = BTreeSet::new();
    for row in input.local {
        if let Some(tid) = &row.ticktick_task_id {
            claimed.insert(tid.clone());
        }
    }

    let mut actions = Vec::new();

    // dirty == 2: exact title/mapped-times/all_day link on the write-target list
    for row in input.local.iter().filter(|r| r.ticktick_dirty == 2) {
        let Some(role) = role_for_list_id(&row.list_id) else {
            continue;
        };
        let Some(target) = write_target(input.projects, input.roles, role) else {
            continue;
        };
        let candidates: Vec<&RemoteTask> = input
            .fetches
            .iter()
            .filter(|f| f.ok && f.project_id == target.id)
            .flat_map(|f| f.open.iter())
            .filter(|t| {
                if claimed.contains(&t.id) {
                    return false;
                }
                if effective_role(
                    input.roles,
                    input.column_roles,
                    &t.project_id,
                    t.column_id.as_deref(),
                )
                .is_none()
                {
                    return false;
                }
                let (start, end, all_day) = map_times(
                    t.all_day,
                    t.start,
                    t.end,
                    input.day_start,
                    input.next_day_start,
                );
                t.title == row.title
                    && start == row.start
                    && end == row.end
                    && all_day == row.ticktick_all_day
            })
            .collect();
        if candidates.len() == 1 {
            let remote = candidates[0];
            claimed.insert(remote.id.clone());
            actions.push(ReconcileAction::Link {
                local_id: row.local_id.clone(),
                ticktick_task_id: remote.id.clone(),
                ticktick_project_id: remote.project_id.clone(),
                etag: remote.etag.clone(),
            });
        } else if candidates.len() > 1 {
            // Ambiguous: do not Link, but claim so Insert does not duplicate.
            for remote in &candidates {
                claimed.insert(remote.id.clone());
            }
        }
    }

    // Linked rows, including dirty ones, when the remote task is still open.
    for row in input.local.iter().filter(|r| r.ticktick_task_id.is_some()) {
        let Some(tid) = row.ticktick_task_id.as_deref() else {
            continue;
        };
        if let Some(remote) = open_by_id.get(tid) {
            let role = effective_role(
                input.roles,
                input.column_roles,
                &remote.project_id,
                remote.column_id.as_deref(),
            );
            if role.is_none() {
                if allow_delete {
                    actions.push(ReconcileAction::DropIgnored {
                        local_id: row.local_id.clone(),
                    });
                }
                continue;
            }
            if row.ticktick_dirty != 0 {
                continue;
            }
            let (start, end, all_day) = map_times(
                remote.all_day,
                remote.start,
                remote.end,
                input.day_start,
                input.next_day_start,
            );
            let project_changed =
                row.ticktick_project_id.as_deref() != Some(remote.project_id.as_str());
            let etag_changed = row.ticktick_etag != remote.etag;
            let title_changed = row.title != remote.title;
            let times_changed =
                row.start != start || row.end != end || row.ticktick_all_day != all_day;
            let done_changed = row.done; // remote open ⇒ should be false
            let mut updated = apply_remote_fields(
                row,
                remote,
                input.roles,
                input.column_roles,
                input.day_start,
                input.next_day_start,
                false,
            );
            if !allow_delete {
                updated.list_id = row.list_id.clone();
            }
            let list_changed = updated.list_id != row.list_id;
            if list_changed
                || project_changed
                || etag_changed
                || title_changed
                || times_changed
                || done_changed
            {
                actions.push(ReconcileAction::Update(updated));
            }
            continue;
        }
        if row.ticktick_dirty != 0 {
            continue;
        }
        if completed.contains(tid) {
            if !row.done {
                let mut updated = row.clone();
                updated.done = true;
                actions.push(ReconcileAction::Update(updated));
            }
            continue;
        }
        if allow_delete {
            actions.push(ReconcileAction::Delete {
                local_id: row.local_id.clone(),
            });
        }
    }

    // dirty == 1: no Update/Delete (already skipped above)

    // remote open ids with no local link → Insert
    for remote in open_by_id.values() {
        if claimed.contains(&remote.id) {
            continue;
        }
        let Some(role) = effective_role(
            input.roles,
            input.column_roles,
            &remote.project_id,
            remote.column_id.as_deref(),
        ) else {
            continue;
        };
        let Some(list_id) = list_id_for_role(role) else {
            continue;
        };
        let (start, end, all_day) = map_times(
            remote.all_day,
            remote.start,
            remote.end,
            input.day_start,
            input.next_day_start,
        );
        actions.push(ReconcileAction::Insert(LocalMirror {
            local_id: String::new(),
            list_id: list_id.into(),
            title: remote.title.clone(),
            done: false,
            start,
            end,
            ticktick_task_id: Some(remote.id.clone()),
            ticktick_project_id: Some(remote.project_id.clone()),
            ticktick_etag: remote.etag.clone(),
            ticktick_dirty: 0,
            ticktick_all_day: all_day,
        }));
    }

    actions
}

fn push_body(task: &LocalMirror, kind: PushKind, project_id: String, to_project_id: Option<String>) -> PushOp {
    PushOp {
        kind,
        local_id: task.local_id.clone(),
        task_id: task.ticktick_task_id.clone(),
        project_id,
        to_project_id,
        title: task.title.clone(),
        start: task.start,
        end: task.end,
        all_day: task.ticktick_all_day,
    }
}

pub fn push_op(
    task: &LocalMirror,
    roles: &BTreeMap<String, String>,
    projects: &[RemoteProject],
) -> Option<PushOp> {
    if task.ticktick_dirty != 1 {
        return None;
    }

    match task.ticktick_task_id.as_deref() {
        None => {
            let role = role_for_list_id(&task.list_id)?;
            if !is_preset_list(&task.list_id) {
                return None;
            }
            let target = write_target(projects, roles, role)?;
            Some(push_body(task, PushKind::Create, target.id.clone(), None))
        }
        Some(_) => {
            if !is_preset_list(&task.list_id) {
                let project_id = task
                    .ticktick_project_id
                    .clone()
                    .unwrap_or_default();
                return Some(push_body(task, PushKind::Delete, project_id, None));
            }
            let local_role = role_for_list_id(&task.list_id)?;
            let remote_role = task
                .ticktick_project_id
                .as_deref()
                .and_then(|pid| role_for_project(roles, pid));
            if remote_role != Some(local_role) {
                let target = write_target(projects, roles, local_role)?;
                let from = task
                    .ticktick_project_id
                    .clone()
                    .unwrap_or_else(|| target.id.clone());
                return Some(push_body(
                    task,
                    PushKind::Move,
                    from,
                    Some(target.id.clone()),
                ));
            }
            let project_id = task
                .ticktick_project_id
                .clone()
                .or_else(|| write_target(projects, roles, local_role).map(|p| p.id.clone()))?;
            if task.done {
                Some(push_body(task, PushKind::Complete, project_id, None))
            } else {
                // Dirty incomplete on matching list → Reopen (covers title/time updates too).
                Some(push_body(task, PushKind::Reopen, project_id, None))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn proj(id: &str, sort_order: i64) -> RemoteProject {
        RemoteProject {
            id: id.into(),
            name: id.into(),
            sort_order,
            columns: Vec::new(),
        }
    }

    fn column(id: &str, project: &str, sort_order: i64) -> RemoteColumn {
        RemoteColumn {
            id: id.into(),
            name: id.into(),
            sort_order,
            project_id: project.into(),
        }
    }

    fn mirror(id: &str, tt: Option<&str>, project: &str, dirty: i64) -> LocalMirror {
        LocalMirror {
            local_id: id.into(),
            list_id: PRESET_MAINLINE_ID.into(),
            title: "写稿".into(),
            done: false,
            start: Some(10),
            end: Some(20),
            ticktick_task_id: tt.map(str::to_string),
            ticktick_project_id: Some(project.into()),
            ticktick_etag: "e1".into(),
            ticktick_dirty: dirty,
            ticktick_all_day: false,
        }
    }

    fn open_column(
        id: &str,
        project: &str,
        title: &str,
        etag: &str,
        column_id: Option<&str>,
    ) -> RemoteTask {
        RemoteTask {
            id: id.into(),
            project_id: project.into(),
            title: title.into(),
            start: Some(10),
            end: Some(20),
            all_day: false,
            etag: etag.into(),
            column_id: column_id.map(str::to_string),
        }
    }

    fn open(id: &str, project: &str, title: &str, etag: &str) -> RemoteTask {
        open_column(id, project, title, etag, None)
    }

    #[test]
    fn reconcile_inserts_open_task_and_skips_subtasks_json() {
        let projects = vec![proj("p", 1)];
        let mut roles = BTreeMap::new();
        roles.insert("p".into(), "mainline".into());
        let fetches = vec![ProjectFetch {
            project_id: "p".into(),
            ok: true,
            open: vec![open("t1", "p", "写稿", "e")],
        }];
        let actions = reconcile(&ReconcileInput {
            local: &[],
            projects: &projects,
            roles: &roles,
            column_roles: &BTreeMap::new(),
            fetches: &fetches,
            completed_ids: &[],
            completed_ok: true,
            first_sync: true,
            day_start: 0,
            next_day_start: 86_400,
        });
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            ReconcileAction::Insert(row) => {
                assert_eq!(row.list_id, PRESET_MAINLINE_ID);
                assert_eq!(row.ticktick_task_id.as_deref(), Some("t1"));
                assert!(!row.done);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn dirty_local_is_not_overwritten_and_clean_etag_change_updates() {
        let projects = vec![proj("p", 1)];
        let mut roles = BTreeMap::new();
        roles.insert("p".into(), "mainline".into());
        let local = vec![mirror("L1", Some("t1"), "p", 1), mirror("L2", Some("t2"), "p", 0)];
        let fetches = vec![ProjectFetch {
            project_id: "p".into(),
            ok: true,
            open: vec![open("t1", "p", "远端标题", "e2"), open("t2", "p", "新标题", "e9")],
        }];
        let actions = reconcile(&ReconcileInput {
            local: &local,
            projects: &projects,
            roles: &roles,
            column_roles: &BTreeMap::new(),
            fetches: &fetches,
            completed_ids: &[],
            completed_ok: true,
            first_sync: false,
            day_start: 0,
            next_day_start: 86_400,
        });
        assert!(actions
            .iter()
            .all(|a| !matches!(a, ReconcileAction::Update(row) if row.local_id == "L1")));
        assert!(actions.iter().any(|a| {
            matches!(a, ReconcileAction::Update(row) if row.local_id == "L2" && row.title == "新标题")
        }));
    }

    #[test]
    fn delete_only_when_every_mapped_fetch_and_completed_succeed() {
        let projects = vec![proj("p", 1), proj("q", 2)];
        let mut roles = BTreeMap::new();
        roles.insert("p".into(), "mainline".into());
        roles.insert("q".into(), "side".into());
        let local = vec![mirror("L1", Some("gone"), "p", 0)];
        let fetches = vec![
            ProjectFetch {
                project_id: "p".into(),
                ok: true,
                open: vec![],
            },
            ProjectFetch {
                project_id: "q".into(),
                ok: false,
                open: vec![],
            },
        ];
        let blocked = reconcile(&ReconcileInput {
            local: &local,
            projects: &projects,
            roles: &roles,
            column_roles: &BTreeMap::new(),
            fetches: &fetches,
            completed_ids: &[],
            completed_ok: true,
            first_sync: false,
            day_start: 0,
            next_day_start: 86_400,
        });
        assert!(blocked
            .iter()
            .all(|a| !matches!(a, ReconcileAction::Delete { .. })));

        let fetches = vec![
            ProjectFetch {
                project_id: "p".into(),
                ok: true,
                open: vec![],
            },
            ProjectFetch {
                project_id: "q".into(),
                ok: true,
                open: vec![],
            },
        ];
        let deleted = reconcile(&ReconcileInput {
            local: &local,
            projects: &projects,
            roles: &roles,
            column_roles: &BTreeMap::new(),
            fetches: &fetches,
            completed_ids: &[],
            completed_ok: true,
            first_sync: false,
            day_start: 0,
            next_day_start: 86_400,
        });
        assert!(deleted
            .iter()
            .any(|a| matches!(a, ReconcileAction::Delete { local_id } if local_id == "L1")));
    }

    #[test]
    fn completed_list_marks_linked_task_and_does_not_insert() {
        let projects = vec![proj("p", 1)];
        let mut roles = BTreeMap::new();
        roles.insert("p".into(), "mainline".into());
        let local = vec![mirror("L1", Some("t1"), "p", 0)];
        let fetches = vec![ProjectFetch {
            project_id: "p".into(),
            ok: true,
            open: vec![],
        }];
        let actions = reconcile(&ReconcileInput {
            local: &local,
            projects: &projects,
            roles: &roles,
            column_roles: &BTreeMap::new(),
            fetches: &fetches,
            completed_ids: &["t1".into(), "historical".into()],
            completed_ok: true,
            first_sync: false,
            day_start: 0,
            next_day_start: 86_400,
        });
        assert!(actions.iter().any(
            |a| matches!(a, ReconcileAction::Update(row) if row.local_id == "L1" && row.done)
        ));
        assert!(actions
            .iter()
            .all(|a| !matches!(a, ReconcileAction::Insert(_))));
    }

    #[test]
    fn first_sync_does_not_delete_missing_links() {
        let projects = vec![proj("p", 1)];
        let mut roles = BTreeMap::new();
        roles.insert("p".into(), "mainline".into());
        let local = vec![mirror("L1", Some("gone"), "p", 0)];
        let fetches = vec![ProjectFetch {
            project_id: "p".into(),
            ok: true,
            open: vec![],
        }];
        let actions = reconcile(&ReconcileInput {
            local: &local,
            projects: &projects,
            roles: &roles,
            column_roles: &BTreeMap::new(),
            fetches: &fetches,
            completed_ids: &[],
            completed_ok: false,
            first_sync: true,
            day_start: 0,
            next_day_start: 86_400,
        });
        assert!(actions
            .iter()
            .all(|a| !matches!(a, ReconcileAction::Delete { .. })));
    }

    #[test]
    fn task_seen_on_another_mapped_list_moves_group() {
        let projects = vec![proj("p", 1), proj("q", 2)];
        let mut roles = BTreeMap::new();
        roles.insert("p".into(), "mainline".into());
        roles.insert("q".into(), "side".into());
        let local = vec![mirror("L1", Some("t1"), "p", 0)];
        let fetches = vec![
            ProjectFetch {
                project_id: "p".into(),
                ok: true,
                open: vec![],
            },
            ProjectFetch {
                project_id: "q".into(),
                ok: true,
                open: vec![open("t1", "q", "写稿", "e1")],
            },
        ];
        let actions = reconcile(&ReconcileInput {
            local: &local,
            projects: &projects,
            roles: &roles,
            column_roles: &BTreeMap::new(),
            fetches: &fetches,
            completed_ids: &[],
            completed_ok: true,
            first_sync: false,
            day_start: 0,
            next_day_start: 86_400,
        });
        assert!(actions.iter().any(|a| matches!(a, ReconcileAction::Update(row)
            if row.local_id == "L1" && row.list_id == PRESET_SIDE_ID && row.ticktick_project_id.as_deref() == Some("q"))));
        assert!(actions
            .iter()
            .all(|a| !matches!(a, ReconcileAction::Delete { .. })));
    }

    #[test]
    fn dirty_two_links_exact_title_and_times_only() {
        let projects = vec![proj("p", 1)];
        let mut roles = BTreeMap::new();
        roles.insert("p".into(), "mainline".into());
        let mut local = mirror("L1", None, "p", 2);
        local.ticktick_project_id = None;
        local.ticktick_etag.clear();
        let fetches = vec![ProjectFetch {
            project_id: "p".into(),
            ok: true,
            open: vec![open("t1", "p", "写稿", "etag")],
        }];
        let actions = reconcile(&ReconcileInput {
            local: &[local],
            projects: &projects,
            roles: &roles,
            column_roles: &BTreeMap::new(),
            fetches: &fetches,
            completed_ids: &[],
            completed_ok: true,
            first_sync: false,
            day_start: 0,
            next_day_start: 86_400,
        });
        assert!(actions.iter().any(|a| matches!(a, ReconcileAction::Link { local_id, ticktick_task_id, .. }
            if local_id == "L1" && ticktick_task_id == "t1")));
    }

    #[test]
    fn dirty_two_links_all_day_via_map_times() {
        let projects = vec![proj("p", 1)];
        let mut roles = BTreeMap::new();
        roles.insert("p".into(), "mainline".into());
        let mut local = mirror("L1", None, "p", 2);
        local.ticktick_project_id = None;
        local.ticktick_etag.clear();
        local.ticktick_all_day = true;
        local.start = Some(0);
        local.end = Some(86_400);
        let mut remote = open("t1", "p", "写稿", "etag");
        remote.all_day = true;
        remote.start = Some(100);
        remote.end = Some(200);
        let fetches = vec![ProjectFetch {
            project_id: "p".into(),
            ok: true,
            open: vec![remote],
        }];
        let actions = reconcile(&ReconcileInput {
            local: &[local],
            projects: &projects,
            roles: &roles,
            column_roles: &BTreeMap::new(),
            fetches: &fetches,
            completed_ids: &[],
            completed_ok: true,
            first_sync: false,
            day_start: 0,
            next_day_start: 86_400,
        });
        assert!(actions.iter().any(|a| matches!(a, ReconcileAction::Link { local_id, ticktick_task_id, .. }
            if local_id == "L1" && ticktick_task_id == "t1")));
        assert!(actions
            .iter()
            .all(|a| !matches!(a, ReconcileAction::Insert(_))));
    }

    #[test]
    fn push_op_create_complete_move_and_skip_clean() {
        let projects = vec![proj("p", -5), proj("s", 1)];
        let mut roles = BTreeMap::new();
        roles.insert("p".into(), "mainline".into());
        roles.insert("s".into(), "side".into());
        let mut fresh = mirror("L1", None, "p", 1);
        fresh.ticktick_project_id = None;
        fresh.list_id = PRESET_MAINLINE_ID.into();
        let created = push_op(&fresh, &roles, &projects).unwrap();
        assert_eq!(created.kind, PushKind::Create);
        assert_eq!(created.project_id, "p");

        let mut done = mirror("L2", Some("t2"), "p", 1);
        done.done = true;
        assert_eq!(
            push_op(&done, &roles, &projects).unwrap().kind,
            PushKind::Complete
        );

        let mut moved = mirror("L3", Some("t3"), "p", 1);
        moved.list_id = PRESET_SIDE_ID.into();
        let op = push_op(&moved, &roles, &projects).unwrap();
        assert_eq!(op.kind, PushKind::Move);
        assert_eq!(op.to_project_id.as_deref(), Some("s"));

        let mut custom = mirror("L4", Some("t4"), "p", 1);
        custom.list_id = "list-custom".into();
        assert_eq!(
            push_op(&custom, &roles, &projects).unwrap().kind,
            PushKind::Delete
        );

        let clean = mirror("L5", Some("t5"), "p", 0);
        assert!(push_op(&clean, &roles, &projects).is_none());
        assert!(push_op(&fresh, &roles, &[]).is_none());
    }

    #[test]
    fn parse_open_tasks_ignores_items_and_completed_ids() {
        let data = serde_json::json!({
            "tasks": [{
                "id": "t1",
                "title": "父",
                "etag": "e",
                "status": 0,
                "isAllDay": false,
                "items": [{"id": "sub", "title": "子", "status": 0}]
            }]
        });
        let tasks = parse_open_tasks("p", &data);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "t1");
        let done = serde_json::json!([{"id": "c1"}, {"id": "c2"}]);
        assert_eq!(
            parse_completed_ids(&done),
            vec!["c1".to_string(), "c2".to_string()]
        );
    }

    #[test]
    fn write_target_picks_smallest_sort_order_then_id() {
        let projects = vec![proj("b", -3), proj("a", -3), proj("c", -10)];
        let mut roles = BTreeMap::new();
        roles.insert("a".into(), "mainline".into());
        roles.insert("b".into(), "mainline".into());
        roles.insert("c".into(), "mainline".into());
        roles.insert("z".into(), "ignore".into());
        assert_eq!(
            write_target(&projects, &roles, ListRole::Mainline)
                .unwrap()
                .id,
            "c"
        );
        let only = vec![proj("b", -3), proj("a", -3)];
        assert_eq!(
            write_target(&only, &roles, ListRole::Mainline)
                .unwrap()
                .id,
            "a"
        );
        assert!(write_target(&projects, &roles, ListRole::Side).is_none());
    }

    #[test]
    fn stamp_missing_roles_defaults_ignore_and_keeps_existing() {
        let projects = vec![proj("new", 1), proj("old", 2)];
        let mut roles = BTreeMap::new();
        roles.insert("old".into(), "side".into());
        stamp_missing_roles(&projects, &mut roles);
        assert_eq!(roles.get("new").map(String::as_str), Some("ignore"));
        assert_eq!(roles.get("old").map(String::as_str), Some("side"));
    }

    #[test]
    fn map_times_all_day_uses_caller_bounds() {
        assert_eq!(
            map_times(true, Some(1), Some(2), 100, 200),
            (Some(100), Some(200), true)
        );
        assert_eq!(
            map_times(false, None, Some(5), 100, 200),
            (None, Some(5), false)
        );
    }

    #[test]
    fn list_ids_match_presets() {
        assert_eq!(
            list_id_for_role(ListRole::Mainline),
            Some(PRESET_MAINLINE_ID)
        );
        assert_eq!(list_id_for_role(ListRole::Chore), Some(PRESET_CHORE_ID));
        assert_eq!(list_id_for_role(ListRole::Custom), None);
        assert_eq!(parse_role("ignore"), None);
        assert_eq!(parse_role("longterm"), Some(ListRole::Longterm));
    }

    #[test]
    fn effective_role_column_overrides_list_including_ignore() {
        let mut lists = BTreeMap::new();
        lists.insert("p".into(), "mainline".into());
        let mut columns = BTreeMap::new();
        columns.insert("c-ignore".into(), "ignore".into());
        columns.insert("c-side".into(), "side".into());
        assert_eq!(
            effective_role(&lists, &columns, "p", Some("c-ignore")),
            None
        );
        assert_eq!(
            effective_role(&lists, &columns, "p", Some("c-side")),
            Some(ListRole::Side)
        );
        assert_eq!(
            effective_role(&lists, &columns, "p", Some("c-new")),
            Some(ListRole::Mainline)
        );
        assert_eq!(
            effective_role(&lists, &columns, "p", None),
            Some(ListRole::Mainline)
        );
        columns.insert("c-side".into(), "inherit".into());
        assert_eq!(
            effective_role(&lists, &columns, "p", Some("c-side")),
            Some(ListRole::Mainline)
        );
    }

    #[test]
    fn effective_role_ignored_list_imports_explicit_column() {
        let mut lists = BTreeMap::new();
        lists.insert("p".into(), "ignore".into());
        let columns = BTreeMap::new();
        assert_eq!(effective_role(&lists, &columns, "p", Some("c-new")), None);
        let mut columns = BTreeMap::new();
        columns.insert("c-main".into(), "mainline".into());
        assert_eq!(
            effective_role(&lists, &columns, "p", Some("c-main")),
            Some(ListRole::Mainline)
        );
    }

    #[test]
    fn projects_to_fetch_includes_ignore_list_only_for_mapped_column() {
        let mut project = proj("p", 1);
        project.columns = vec![column("c1", "p", 1), column("c2", "p", 2)];
        let projects = vec![project, proj("q", 2)];
        let mut lists = BTreeMap::new();
        lists.insert("p".into(), "ignore".into());
        lists.insert("q".into(), "side".into());
        let mut columns = BTreeMap::new();
        columns.insert("c1".into(), "ignore".into());
        assert_eq!(
            projects_to_fetch(&projects, &lists, &columns)
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            vec!["q"]
        );
        columns.insert("c2".into(), "mainline".into());
        assert_eq!(
            projects_to_fetch(&projects, &lists, &columns)
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            vec!["p", "q"]
        );
    }

    #[test]
    fn write_target_ignores_column_roles() {
        let mut later = proj("b", 5);
        later.columns = vec![column("c-main", "b", 0)];
        let projects = vec![proj("a", 1), later];
        let mut lists = BTreeMap::new();
        lists.insert("a".into(), "mainline".into());
        lists.insert("b".into(), "side".into());
        let target = write_target(&projects, &lists, ListRole::Mainline).unwrap();
        assert_eq!(target.id, "a");
    }

    #[test]
    fn parse_columns_and_task_column_id() {
        let data = serde_json::json!({
            "columns": [
                {"id": "c2", "name": "后", "sortOrder": 2, "projectId": "p"},
                {"id": "c1", "name": "先", "sortOrder": 1, "projectId": "p"}
            ],
            "tasks": [{
                "id": "t1",
                "title": "写稿",
                "status": 0,
                "columnId": "c1",
                "etag": "e"
            }]
        });
        let mut columns = parse_columns("p", &data);
        sort_columns(&mut columns);
        assert_eq!(columns[0].id, "c1");
        assert_eq!(columns[0].name, "先");
        assert_eq!(columns[0].sort_order, 1);
        assert_eq!(columns[1].id, "c2");
        let tasks = parse_open_tasks("p", &data);
        assert_eq!(tasks[0].column_id.as_deref(), Some("c1"));
    }

    #[test]
    fn prune_column_roles_drops_ids_that_are_not_kept() {
        let mut roles = BTreeMap::new();
        roles.insert("keep".into(), "side".into());
        roles.insert("gone".into(), "mainline".into());
        let mut kept = BTreeSet::new();
        kept.insert("keep".into());
        prune_column_roles(&kept, &mut roles);
        assert_eq!(roles.get("keep").map(String::as_str), Some("side"));
        assert!(roles.get("gone").is_none());
    }

    #[test]
    fn reconcile_drops_ignored_column_without_inserting_it() {
        let mut project = proj("p", 1);
        project.columns = vec![column("c-ignore", "p", 1), column("c-main", "p", 2)];
        let projects = vec![project];
        let mut lists = BTreeMap::new();
        lists.insert("p".into(), "mainline".into());
        let mut columns = BTreeMap::new();
        columns.insert("c-ignore".into(), "ignore".into());
        let local = mirror("L1", Some("t-ignore"), "p", 1);
        let fetches = vec![ProjectFetch {
            project_id: "p".into(),
            ok: true,
            open: vec![
                open_column("t-ignore", "p", "写稿", "e1", Some("c-ignore")),
                open_column("t-skip", "p", "不导入", "e3", Some("c-ignore")),
                open_column("t-new", "p", "新任务", "e2", Some("c-main")),
            ],
        }];
        let actions = reconcile(&ReconcileInput {
            local: &[local],
            projects: &projects,
            roles: &lists,
            column_roles: &columns,
            fetches: &fetches,
            completed_ids: &[],
            completed_ok: true,
            first_sync: false,
            day_start: 0,
            next_day_start: 86_400,
        });
        assert!(actions.iter().any(|a| matches!(
            a,
            ReconcileAction::DropIgnored { local_id } if local_id == "L1"
        )));
        assert!(actions.iter().all(|a| !matches!(a, ReconcileAction::Delete { .. })));
        assert!(actions.iter().all(|a| !matches!(
            a,
            ReconcileAction::Insert(row) if row.ticktick_task_id.as_deref() == Some("t-skip")
        )));
        let inserted = actions.iter().find_map(|a| match a {
            ReconcileAction::Insert(row) => Some(row),
            _ => None,
        });
        assert_eq!(inserted.unwrap().list_id, PRESET_MAINLINE_ID);
        assert_eq!(inserted.unwrap().ticktick_task_id.as_deref(), Some("t-new"));
    }

    #[test]
    fn reconcile_ignored_list_inserts_explicit_mainline_column() {
        let mut project = proj("p", 1);
        project.columns = vec![column("c-main", "p", 1)];
        let projects = vec![project];
        let mut lists = BTreeMap::new();
        lists.insert("p".into(), "ignore".into());
        let mut columns = BTreeMap::new();
        columns.insert("c-main".into(), "mainline".into());
        let fetches = vec![ProjectFetch {
            project_id: "p".into(),
            ok: true,
            open: vec![open_column("t1", "p", "写稿", "e", Some("c-main"))],
        }];
        let actions = reconcile(&ReconcileInput {
            local: &[],
            projects: &projects,
            roles: &lists,
            column_roles: &columns,
            fetches: &fetches,
            completed_ids: &[],
            completed_ok: true,
            first_sync: false,
            day_start: 0,
            next_day_start: 86_400,
        });
        match &actions[0] {
            ReconcileAction::Insert(row) => assert_eq!(row.list_id, PRESET_MAINLINE_ID),
            other => panic!("expected insert, got {other:?}"),
        }
    }

    #[test]
    fn reconcile_moves_list_only_when_the_round_fully_succeeds() {
        let mut project = proj("p", 1);
        project.columns = vec![column("c-side", "p", 1)];
        let other = proj("q", 2);
        let projects = vec![project, other];
        let mut lists = BTreeMap::new();
        lists.insert("p".into(), "mainline".into());
        lists.insert("q".into(), "side".into());
        let mut columns = BTreeMap::new();
        columns.insert("c-side".into(), "side".into());
        let local = mirror("L1", Some("t1"), "p", 0);
        let open = vec![open_column("t1", "p", "改标题", "e2", Some("c-side"))];
        let full = reconcile(&ReconcileInput {
            local: &[local.clone()],
            projects: &projects,
            roles: &lists,
            column_roles: &columns,
            fetches: &[
                ProjectFetch { project_id: "p".into(), ok: true, open: open.clone() },
                ProjectFetch { project_id: "q".into(), ok: true, open: vec![] },
            ],
            completed_ids: &[],
            completed_ok: true,
            first_sync: false,
            day_start: 0,
            next_day_start: 86_400,
        });
        match &full[0] {
            ReconcileAction::Update(row) => {
                assert_eq!(row.list_id, PRESET_SIDE_ID);
                assert_eq!(row.title, "改标题");
            }
            other => panic!("expected update, got {other:?}"),
        }
        let partial = reconcile(&ReconcileInput {
            local: &[local],
            projects: &projects,
            roles: &lists,
            column_roles: &columns,
            fetches: &[
                ProjectFetch { project_id: "p".into(), ok: true, open },
                ProjectFetch { project_id: "q".into(), ok: false, open: vec![] },
            ],
            completed_ids: &[],
            completed_ok: false,
            first_sync: false,
            day_start: 0,
            next_day_start: 86_400,
        });
        assert!(partial.iter().all(|a| !matches!(a, ReconcileAction::DropIgnored { .. })));
        match &partial[0] {
            ReconcileAction::Update(row) => {
                assert_eq!(row.list_id, PRESET_MAINLINE_ID);
                assert_eq!(row.title, "改标题");
            }
            other => panic!("expected title update, got {other:?}"),
        }
    }
}
