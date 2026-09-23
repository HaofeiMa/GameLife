//! TickTick Open API mirror: push dirty rows, pull + reconcile.
//! Real HTTP is behind `TickTickApi`; tests use `FakeApi`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{TimeZone, Utc};
use gamelife_core::task::{
    ListRole, RepeatRule, Task, PRESET_CHORE_ID, PRESET_LONGTERM_ID, PRESET_MAINLINE_ID,
    PRESET_SIDE_ID,
};
use gamelife_core::ticktick_sync::{
    kept_column_ids, map_times, parse_completed_ids, parse_open_tasks, parse_projects,
    projects_to_fetch, prune_column_roles, push_op, reconcile, sort_columns, stamp_missing_roles,
    write_target, LocalMirror, ProjectFetch, PushKind, PushOp, ReconcileAction, ReconcileInput,
    RemoteProject, RemoteTask,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::config::{load_settings, save_settings as write_settings_file, AppSettings};
use crate::db::{
    app_db_path, load_tasks, load_ticktick_projects, meta_get, meta_set, migrate, new_task_id,
    next_list_sort, open, persist_task,
};
use crate::db_error::DbOpError;
use crate::keychain::{
    delete_ticktick_secret, get_ticktick_secret, set_ticktick_secret, TICKTICK_ACCESS_TOKEN,
    TICKTICK_CLIENT_SECRET, TICKTICK_REFRESH_TOKEN,
};
use crate::scheduler::{day_str_for_ts, end_of_local_day, start_of_named_day};

const API_BASE: &str = "https://api.ticktick.com/open/v1";
const OAUTH_TOKEN_URL: &str = "https://ticktick.com/oauth/token";
const OAUTH_AUTHORIZE: &str = "https://ticktick.com/oauth/authorize";
const HOURLY_SECS: i64 = 3600;
const OAUTH_TIMEOUT_SECS: u64 = 180;
const PARTIAL_REFRESH: &str = "有清单没能刷新，这些清单仍显示上次的分组。";
const SCOPE_REFRESH: &str = "TickTick 授权不足以读取清单，请重新连接。";

#[derive(Debug, Clone)]
pub struct TickTickResponse {
    pub status: u16,
    pub body: serde_json::Value,
}

#[derive(Debug)]
pub enum TickTickError {
    Transport,
    Http(String),
}

pub trait TickTickApi {
    fn request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<TickTickResponse, TickTickError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushResult {
    Unchanged,
    Synced {
        task_id: String,
        project_id: String,
        etag: String,
    },
    Ambiguous,
    Failed(String),
    Cleared,
}

pub struct PullRound {
    pub actions: Vec<ReconcileAction>,
    pub advance_sync_at: bool,
    pub error: Option<String>,
}

fn task_to_mirror(task: &Task) -> LocalMirror {
    LocalMirror {
        local_id: task.id.clone(),
        list_id: task.list_id.clone(),
        title: task.title.clone(),
        done: task.done,
        start: task.start,
        end: task.end,
        ticktick_task_id: task.ticktick_task_id.clone(),
        ticktick_project_id: task.ticktick_project_id.clone(),
        ticktick_etag: task.ticktick_etag.clone(),
        ticktick_dirty: task.ticktick_dirty,
        ticktick_all_day: task.ticktick_all_day,
    }
}

fn format_ticktick_ts(ts: i64) -> String {
    Utc.timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).unwrap())
        .format("%Y-%m-%dT%H:%M:%S+0000")
        .to_string()
}

fn task_body(op: &PushOp) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("title".into(), json!(op.title));
    map.insert("projectId".into(), json!(op.project_id));
    map.insert("isAllDay".into(), json!(op.all_day));
    if op.all_day {
        if let Some(start) = op.start {
            map.insert("startDate".into(), json!(format_ticktick_ts(start)));
        }
    } else {
        if let Some(start) = op.start {
            map.insert("startDate".into(), json!(format_ticktick_ts(start)));
        }
        if let Some(end) = op.end {
            map.insert("dueDate".into(), json!(format_ticktick_ts(end)));
        }
    }
    Value::Object(map)
}

/// Uncomplete / reopen body: id, projectId, title, times, status: 0.
fn reopen_body(op: &PushOp) -> Value {
    let mut body = task_body(op);
    if let Some(obj) = body.as_object_mut() {
        if let Some(id) = op.task_id.as_deref() {
            obj.insert("id".into(), json!(id));
        }
        obj.insert("status".into(), json!(0));
    }
    body
}

fn chinese_push_failure(raw: &str) -> String {
    if raw == "需要重新连接" {
        return raw.to_string();
    }
    if raw == "transport" {
        return "同步超时".into();
    }
    if let Some(code) = raw
        .strip_prefix("http ")
        .and_then(|s| s.parse::<u16>().ok())
    {
        return match code {
            429 => "请求过于频繁".into(),
            400..=499 => "写回被拒绝".into(),
            _ => "写回失败".into(),
        };
    }
    "写回失败".into()
}

fn role_for_preset_list(list_id: &str) -> Option<ListRole> {
    match list_id {
        PRESET_MAINLINE_ID => Some(ListRole::Mainline),
        PRESET_SIDE_ID => Some(ListRole::Side),
        PRESET_LONGTERM_ID => Some(ListRole::Longterm),
        PRESET_CHORE_ID => Some(ListRole::Chore),
        _ => None,
    }
}

fn response_etag(body: &Value) -> String {
    body.get("etag")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn execute_push_op(api: &mut dyn TickTickApi, op: &PushOp) -> PushResult {
    let (method, path, body) = match op.kind {
        PushKind::Create => ("POST", "/task".to_string(), Some(task_body(op))),
        PushKind::Update => {
            let id = op.task_id.as_deref().unwrap_or("");
            ("POST", format!("/task/{id}"), Some(task_body(op)))
        }
        PushKind::Reopen => {
            let id = op.task_id.as_deref().unwrap_or("");
            ("POST", format!("/task/{id}"), Some(reopen_body(op)))
        }
        PushKind::Complete => {
            let tid = op.task_id.as_deref().unwrap_or("");
            (
                "POST",
                format!("/project/{}/task/{}/complete", op.project_id, tid),
                None,
            )
        }
        PushKind::Delete => {
            let tid = op.task_id.as_deref().unwrap_or("");
            (
                "DELETE",
                format!("/project/{}/task/{}", op.project_id, tid),
                None,
            )
        }
        PushKind::Move => {
            let tid = op.task_id.as_deref().unwrap_or("");
            let to = op.to_project_id.as_deref().unwrap_or("");
            let body = json!([{
                "fromProjectId": op.project_id,
                "toProjectId": to,
                "taskId": tid,
            }]);
            ("POST", "/task/move".to_string(), Some(body))
        }
    };

    match api.request(method, &path, body) {
        Err(TickTickError::Transport) if op.kind == PushKind::Create => PushResult::Ambiguous,
        Err(TickTickError::Transport) => PushResult::Failed("transport".into()),
        Err(TickTickError::Http(msg)) => PushResult::Failed(msg),
        Ok(resp) if resp.status == 404 => PushResult::Cleared,
        Ok(resp) if (200..300).contains(&resp.status) => match op.kind {
            PushKind::Create => {
                let Some(id) = resp.body.get("id").and_then(|v| v.as_str()) else {
                    return PushResult::Ambiguous;
                };
                PushResult::Synced {
                    task_id: id.to_string(),
                    project_id: op.project_id.clone(),
                    etag: response_etag(&resp.body),
                }
            }
            PushKind::Delete => PushResult::Cleared,
            PushKind::Move => PushResult::Synced {
                task_id: op.task_id.clone().unwrap_or_default(),
                project_id: op
                    .to_project_id
                    .clone()
                    .unwrap_or_else(|| op.project_id.clone()),
                etag: response_etag(&resp.body),
            },
            PushKind::Update | PushKind::Reopen | PushKind::Complete => PushResult::Synced {
                task_id: op.task_id.clone().unwrap_or_default(),
                project_id: op.project_id.clone(),
                etag: response_etag(&resp.body),
            },
        },
        // Create 5xx: server may have stored the task — treat like timeout (dirty=2).
        Ok(resp) if op.kind == PushKind::Create && resp.status >= 500 => PushResult::Ambiguous,
        Ok(resp) => PushResult::Failed(format!("http {}", resp.status)),
    }
}

pub fn push_one(
    api: &mut dyn TickTickApi,
    task: &Task,
    projects: &[RemoteProject],
    roles: &BTreeMap<String, String>,
) -> PushResult {
    let mirror = task_to_mirror(task);
    let Some(op) = push_op(&mirror, roles, projects) else {
        return PushResult::Unchanged;
    };
    execute_push_op(api, &op)
}

pub fn apply_push_result(task: &mut Task, result: &PushResult) {
    match result {
        PushResult::Unchanged => {}
        PushResult::Failed(_) => {
            // Keep pending write-back; Settings shows the Chinese reason separately.
            task.ticktick_dirty = 1;
        }
        PushResult::Synced {
            task_id,
            project_id,
            etag,
        } => {
            task.ticktick_task_id = Some(task_id.clone());
            task.ticktick_project_id = Some(project_id.clone());
            task.ticktick_etag = etag.clone();
            task.ticktick_dirty = 0;
        }
        PushResult::Ambiguous => {
            task.ticktick_dirty = 2;
        }
        PushResult::Cleared => {
            task.ticktick_task_id = None;
            task.ticktick_project_id = None;
            task.ticktick_etag.clear();
            task.ticktick_dirty = 0;
        }
    }
}

pub fn pull_round(
    api: &mut dyn TickTickApi,
    local: &[Task],
    projects: &[RemoteProject],
    roles: &BTreeMap<String, String>,
    column_roles: &BTreeMap<String, String>,
    first_sync: bool,
    now: i64,
    last_sync_at: Option<i64>,
    day_start: i64,
    next_day_start: i64,
) -> PullRound {
    let mapped: Vec<String> = projects_to_fetch(projects, roles, column_roles)
        .into_iter()
        .map(|project| project.id.clone())
        .collect();
    if mapped.is_empty() {
        return PullRound {
            actions: Vec::new(),
            advance_sync_at: false,
            error: None,
        };
    }

    let mut fetches = Vec::new();
    let mut any_fail = false;
    for pid in &mapped {
        let path = format!("/project/{pid}/data");
        match api.request("GET", &path, None) {
            Ok(resp) if (200..300).contains(&resp.status) => {
                fetches.push(ProjectFetch {
                    project_id: pid.clone(),
                    ok: true,
                    open: parse_open_tasks(pid, &resp.body),
                });
            }
            _ => {
                any_fail = true;
                fetches.push(ProjectFetch {
                    project_id: pid.clone(),
                    ok: false,
                    open: Vec::new(),
                });
            }
        }
    }

    let mut completed_ids = Vec::new();
    let completed_ok;
    if first_sync {
        completed_ok = false;
    } else {
        let start = last_sync_at.unwrap_or(now).saturating_sub(300);
        let body = json!({
            "projectIds": mapped,
            "startDate": format_ticktick_ts(start),
            "endDate": format_ticktick_ts(now),
        });
        match api.request("POST", "/task/completed", Some(body)) {
            Ok(resp) if (200..300).contains(&resp.status) => {
                completed_ids = parse_completed_ids(&resp.body);
                completed_ok = true;
            }
            _ => {
                any_fail = true;
                completed_ok = false;
            }
        }
    }

    let mirrors: Vec<LocalMirror> = local.iter().map(task_to_mirror).collect();
    let actions = reconcile(&ReconcileInput {
        local: &mirrors,
        projects,
        roles,
        column_roles,
        fetches: &fetches,
        completed_ids: &completed_ids,
        completed_ok,
        first_sync,
        day_start,
        next_day_start,
    });

    if any_fail {
        PullRound {
            actions,
            advance_sync_at: false,
            error: Some("同步失败".into()),
        }
    } else {
        PullRound {
            actions,
            advance_sync_at: true,
            error: None,
        }
    }
}

fn delete_task_row(conn: &Connection, id: &str) -> Result<(), DbOpError> {
    conn.execute("DELETE FROM tasks WHERE id = ?1", rusqlite::params![id])
        .map_err(crate::db_error::map_rusqlite)?;
    Ok(())
}

fn open_app_db() -> Result<Connection, String> {
    let path = app_db_path().ok_or_else(|| "home dir".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create db dir: {e}"))?;
    }
    let conn = open(&path).map_err(|e| format!("{e:?}"))?;
    migrate(&conn).map_err(|e| format!("{e:?}"))?;
    Ok(conn)
}

fn mirror_to_new_task(m: &LocalMirror, sort: i64) -> Task {
    Task {
        id: new_task_id(),
        list_id: m.list_id.clone(),
        title: m.title.clone(),
        done: m.done,
        start: m.start,
        end: m.end,
        range: None,
        sort,
        repeat: RepeatRule::None,
        remind_offsets: vec![],
        notes: String::new(),
        ticktick_task_id: m.ticktick_task_id.clone(),
        ticktick_project_id: m.ticktick_project_id.clone(),
        ticktick_etag: m.ticktick_etag.clone(),
        ticktick_dirty: m.ticktick_dirty,
        ticktick_all_day: m.ticktick_all_day,
    }
}

fn apply_mirror_to_task(task: &mut Task, m: &LocalMirror) {
    task.list_id = m.list_id.clone();
    task.title = m.title.clone();
    task.done = m.done;
    task.start = m.start;
    task.end = m.end;
    task.ticktick_task_id = m.ticktick_task_id.clone();
    task.ticktick_project_id = m.ticktick_project_id.clone();
    task.ticktick_etag = m.ticktick_etag.clone();
    task.ticktick_dirty = m.ticktick_dirty;
    task.ticktick_all_day = m.ticktick_all_day;
}

fn apply_reconcile_actions(conn: &Connection, actions: &[ReconcileAction]) -> Result<(), String> {
    // One SQLite transaction for all local writes — never hold it across HTTP.
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("{e:?}"))?;
    let mut tasks = load_tasks(&tx).map_err(|e| format!("{e:?}"))?;
    for action in actions {
        match action {
            ReconcileAction::Insert(m) => {
                let sort = next_list_sort(&tx, &m.list_id).map_err(|e| format!("{e:?}"))?;
                let task = mirror_to_new_task(m, sort);
                persist_task(&tx, &task).map_err(|e| format!("{e:?}"))?;
            }
            ReconcileAction::Update(m) => {
                let Some(task) = tasks.iter_mut().find(|t| t.id == m.local_id) else {
                    continue;
                };
                // Second guard: only clean rows accept Update/Delete.
                // dirty==1 is in-flight push; dirty==2 is ambiguous create — Link only.
                if task.ticktick_dirty != 0 {
                    continue;
                }
                apply_mirror_to_task(task, m);
                persist_task(&tx, task).map_err(|e| format!("{e:?}"))?;
            }
            ReconcileAction::Delete { local_id } => {
                let Some(task) = tasks.iter().find(|t| t.id == *local_id) else {
                    continue;
                };
                if task.ticktick_dirty != 0 {
                    continue;
                }
                delete_task_row(&tx, local_id).map_err(|e| format!("{e:?}"))?;
            }
            ReconcileAction::DropIgnored { local_id } => {
                if tasks.iter().all(|t| t.id != *local_id) {
                    continue;
                }
                delete_task_row(&tx, local_id).map_err(|e| format!("{e:?}"))?;
            }
            ReconcileAction::Link {
                local_id,
                ticktick_task_id,
                ticktick_project_id,
                etag,
            } => {
                let Some(task) = tasks.iter_mut().find(|t| t.id == *local_id) else {
                    continue;
                };
                if task.ticktick_dirty == 1 {
                    continue;
                }
                // dirty == 0 or 2 may Link
                task.ticktick_task_id = Some(ticktick_task_id.clone());
                task.ticktick_project_id = Some(ticktick_project_id.clone());
                task.ticktick_etag = etag.clone();
                task.ticktick_dirty = 0;
                persist_task(&tx, task).map_err(|e| format!("{e:?}"))?;
            }
        }
    }
    tx.commit().map_err(|e| format!("{e:?}"))?;
    Ok(())
}

struct LiveApi {
    client: reqwest::blocking::Client,
    access_token: String,
    refresh_token: String,
    client_id: String,
    client_secret: String,
}

impl LiveApi {
    fn from_settings() -> Result<Self, String> {
        let settings = load_settings();
        let access = get_ticktick_secret(TICKTICK_ACCESS_TOKEN)?;
        let refresh = get_ticktick_secret(TICKTICK_REFRESH_TOKEN).unwrap_or_default();
        let secret = get_ticktick_secret(TICKTICK_CLIENT_SECRET).unwrap_or_default();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            access_token: access,
            refresh_token: refresh,
            client_id: settings.ticktick_client_id,
            client_secret: secret,
        })
    }

    fn try_refresh(&mut self) -> Result<(), String> {
        if self.refresh_token.is_empty() || self.client_id.is_empty() || self.client_secret.is_empty()
        {
            return Err("需要重新连接".into());
        }
        let resp = self
            .client
            .post(OAUTH_TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("refresh_token", self.refresh_token.as_str()),
            ])
            .send()
            .map_err(|_| "需要重新连接".to_string())?;
        if !resp.status().is_success() {
            return Err("需要重新连接".into());
        }
        let body: Value = resp.json().map_err(|_| "需要重新连接".to_string())?;
        let access = body
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "需要重新连接".to_string())?;
        let refresh = body
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .unwrap_or(self.refresh_token.as_str());
        set_ticktick_secret(TICKTICK_ACCESS_TOKEN, access)?;
        set_ticktick_secret(TICKTICK_REFRESH_TOKEN, refresh)?;
        self.access_token = access.to_string();
        self.refresh_token = refresh.to_string();
        Ok(())
    }

    fn send_once(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<reqwest::blocking::Response, TickTickError> {
        let url = format!("{API_BASE}{path}");
        let mut req = match method {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "DELETE" => self.client.delete(&url),
            other => return Err(TickTickError::Http(format!("bad method {other}"))),
        };
        req = req.bearer_auth(&self.access_token);
        if let Some(b) = body {
            req = req.json(b);
        }
        req.send().map_err(|_| TickTickError::Transport)
    }
}

impl TickTickApi for LiveApi {
    fn request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<TickTickResponse, TickTickError> {
        let resp = self.send_once(method, path, body.as_ref())?;
        if resp.status().as_u16() == 401 {
            if self.try_refresh().is_err() {
                return Err(TickTickError::Http("需要重新连接".into()));
            }
            let resp = self.send_once(method, path, body.as_ref())?;
            let status = resp.status().as_u16();
            let body = resp.json().unwrap_or(Value::Null);
            return Ok(TickTickResponse { status, body });
        }
        let status = resp.status().as_u16();
        let body = resp.json().unwrap_or(Value::Null);
        Ok(TickTickResponse { status, body })
    }
}

pub(crate) fn push_and_persist(
    conn: &Connection,
    api: &mut dyn TickTickApi,
    task: &mut Task,
) -> Result<(), String> {
    let settings = load_settings();
    let projects = load_ticktick_projects(conn).map_err(|e| format!("{e:?}"))?;
    let result = push_one(api, task, &projects, &settings.ticktick_project_roles);
    apply_push_result(task, &result);
    if let PushResult::Failed(msg) = &result {
        set_last_result(conn, &chinese_push_failure(msg));
    }
    persist_task(conn, task).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

/// Background push for a local task id. No-op when the switch is off or token missing.
pub fn spawn_push_one(task_id: String) {
    std::thread::spawn(move || {
        let settings = load_settings();
        if !settings.ticktick_enabled {
            return;
        }
        if get_ticktick_secret(TICKTICK_ACCESS_TOKEN).is_err() {
            return;
        }
        let Ok(conn) = open_app_db() else {
            return;
        };
        let Ok(mut tasks) = load_tasks(&conn) else {
            return;
        };
        let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) else {
            return;
        };
        let Ok(mut api) = LiveApi::from_settings() else {
            return;
        };
        let _ = push_and_persist(&conn, &mut api, task);
    });
}

/// Background DELETE when the local row is already gone.
pub fn spawn_push_deleted(project_id: String, task_id: String) {
    std::thread::spawn(move || {
        let settings = load_settings();
        if !settings.ticktick_enabled {
            return;
        }
        if get_ticktick_secret(TICKTICK_ACCESS_TOKEN).is_err() {
            return;
        }
        let Ok(mut api) = LiveApi::from_settings() else {
            return;
        };
        let op = PushOp {
            kind: PushKind::Delete,
            local_id: String::new(),
            task_id: Some(task_id),
            project_id,
            to_project_id: None,
            title: String::new(),
            start: None,
            end: None,
            all_day: false,
        };
        let _ = execute_push_op(&mut api, &op);
    });
}

fn set_last_result(conn: &Connection, msg: &str) {
    let _ = meta_set(conn, "ticktick_last_result", msg);
}

/// Pull body after the switch/token gate. Used by `run_pull` and tests.
pub(crate) fn run_pull_body(
    now: i64,
    api: &mut dyn TickTickApi,
    conn: &Connection,
) -> Result<(), String> {
    let settings = load_settings();
    let projects = load_ticktick_projects(conn).map_err(|e| format!("{e:?}"))?;
    run_pull_body_with(
        now,
        api,
        conn,
        &projects,
        &settings.ticktick_project_roles,
        &settings.ticktick_column_roles,
    )
}

/// Testable pull body with explicit projects/roles (avoids real config.json).
pub(crate) fn run_pull_body_with(
    now: i64,
    api: &mut dyn TickTickApi,
    conn: &Connection,
    projects: &[RemoteProject],
    roles: &BTreeMap<String, String>,
    column_roles: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut push_fail_reason: Option<String> = None;
    let tasks = load_tasks(conn).map_err(|e| format!("{e:?}"))?;
    for mut task in tasks.into_iter().filter(|t| t.ticktick_dirty == 1) {
        let result = push_one(api, &task, projects, roles);
        if matches!(&result, PushResult::Failed(msg) if msg == "需要重新连接") {
            set_last_result(conn, "需要重新连接");
            return Ok(());
        }
        if let PushResult::Failed(msg) = &result {
            push_fail_reason = Some(chinese_push_failure(msg));
        }
        apply_push_result(&mut task, &result);
        if let Err(e) = persist_task(conn, &task) {
            set_last_result(conn, &format!("{e:?}"));
            return Ok(());
        }
    }

    let last_sync_at = meta_get(conn, "ticktick_last_sync_at")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<i64>().ok());
    let first_sync = last_sync_at.is_none();
    let day = day_str_for_ts(now);
    let day_start = start_of_named_day(&day).unwrap_or(0);
    let next_day_start = end_of_local_day(day_start);

    let local = load_tasks(conn).map_err(|e| format!("{e:?}"))?;
    let round = pull_round(
        api,
        &local,
        projects,
        roles,
        column_roles,
        first_sync,
        now,
        last_sync_at,
        day_start,
        next_day_start,
    );

    if let Err(e) = apply_reconcile_actions(conn, &round.actions) {
        set_last_result(conn, &e);
        return Ok(());
    }

    if round.advance_sync_at {
        let _ = meta_set(conn, "ticktick_last_sync_at", &now.to_string());
    }

    let still_ambiguous = load_tasks(conn)
        .map_err(|e| format!("{e:?}"))?
        .iter()
        .any(|t| t.ticktick_dirty == 2);

    // Push failure reason must survive a successful pull in the same round.
    let result_msg = if let Some(reason) = push_fail_reason {
        reason
    } else if still_ambiguous {
        "有任务需要手动确认".into()
    } else {
        round.error.as_deref().unwrap_or("ok").to_string()
    };
    set_last_result(conn, &result_msg);
    Ok(())
}

/// Testable gate: when `enabled` or `has_token` is false, return Ok without calling `api`.
#[cfg(test)]
pub(crate) fn run_pull_gated(
    now: i64,
    enabled: bool,
    has_token: bool,
    api: &mut dyn TickTickApi,
    conn: &Connection,
) -> Result<(), String> {
    if !enabled || !has_token {
        return Ok(());
    }
    run_pull_body(now, api, conn)
}

/// Pull cycle used by Task 7 commands and the sampler. No HTTP when off / no token.
pub fn run_pull(now: i64) -> Result<(), String> {
    let settings = load_settings();
    if !settings.ticktick_enabled {
        return Ok(());
    }
    if get_ticktick_secret(TICKTICK_ACCESS_TOKEN).is_err() {
        return Ok(());
    }

    let conn = open_app_db()?;
    let mut api = match LiveApi::from_settings() {
        Ok(api) => api,
        Err(_) => {
            set_last_result(&conn, "需要重新连接");
            return Ok(());
        }
    };
    run_pull_body(now, &mut api, &conn)
}

/// Background pull used when the settings switch flips on.
pub fn spawn_pull() {
    std::thread::spawn(|| {
        let _ = run_pull(crate::sync::now_secs());
    });
}

/// True when `last` is missing or at least an hour behind `now`.
pub(crate) fn ticktick_due(last: Option<i64>, now: i64) -> bool {
    match last {
        None => true,
        Some(t) => now.saturating_sub(t) >= HOURLY_SECS,
    }
}

/// Sampler hourly check: spawn `run_pull` when due. Does not POST dirty-2 rows.
pub fn maybe_spawn_sync(conn: &Connection, now: i64) {
    let settings = load_settings();
    if !settings.ticktick_enabled {
        return;
    }
    if get_ticktick_secret(TICKTICK_ACCESS_TOKEN).is_err() {
        return;
    }
    let last = meta_get(conn, "ticktick_last_sync_at")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<i64>().ok());
    if !ticktick_due(last, now) {
        return;
    }
    std::thread::spawn(move || {
        let _ = run_pull(now);
    });
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The redirect string TickTick must see, plus the loopback port to bind.
/// The returned string is the trimmed input, so it can match a registered URL exactly.
pub(crate) fn loopback_redirect(raw: &str) -> Result<(String, u16), String> {
    let redirect = raw.trim();
    if redirect.is_empty() {
        return Err(
            "请先填写 Redirect URL，并与 TickTick 开发者中心登记的地址完全一致。".into(),
        );
    }
    let rest = redirect.strip_prefix("http://").ok_or(
        "Redirect URL 需要是 http://127.0.0.1:端口/路径，例如 http://127.0.0.1:18789/callback",
    )?;
    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.contains('@') {
        return Err(
            "Redirect URL 需要是 http://127.0.0.1:端口/路径，例如 http://127.0.0.1:18789/callback"
                .into(),
        );
    }
    let (host, port_str) = authority.rsplit_once(':').ok_or(
        "Redirect URL 需要写明端口，例如 http://127.0.0.1:18789/callback",
    )?;
    if host != "127.0.0.1" && host != "localhost" {
        return Err("Redirect URL 只能使用 127.0.0.1 或 localhost".into());
    }
    let port: u16 = port_str
        .parse()
        .map_err(|_| "Redirect URL 的端口无效".to_string())?;
    if port == 0 {
        return Err("Redirect URL 的端口无效".into());
    }
    Ok((redirect.to_string(), port))
}

/// Build the TickTick authorize URL (PKCE S256, scope tasks:read tasks:write).
pub fn authorize_url(client_id: &str, redirect: &str, state: &str, challenge: &str) -> String {
    let scope = percent_encode("tasks:read tasks:write");
    format!(
        "{OAUTH_AUTHORIZE}?client_id={}&redirect_uri={}&response_type=code&scope={scope}&code_challenge={}&code_challenge_method=S256&state={}",
        percent_encode(client_id),
        percent_encode(redirect),
        percent_encode(challenge),
        percent_encode(state),
    )
}

fn random_base64url(nbytes: usize) -> String {
    let mut buf = vec![0u8; nbytes];
    getrandom::getrandom(&mut buf).expect("rng");
    URL_SAFE_NO_PAD.encode(&buf)
}

fn pkce_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

fn role_key(role: ListRole) -> Option<&'static str> {
    match role {
        ListRole::Mainline => Some("mainline"),
        ListRole::Side => Some("side"),
        ListRole::Longterm => Some("longterm"),
        ListRole::Chore => Some("chore"),
        ListRole::Custom => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickTickColumnView {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickTickProjectView {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
    pub role: String,
    pub columns: Vec<TickTickColumnView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickTickStatus {
    pub enabled: bool,
    pub connected: bool,
    pub client_id: String,
    pub projects: Vec<TickTickProjectView>,
    pub write_targets: BTreeMap<String, String>,
    pub last_sync_at: Option<i64>,
    pub last_result: String,
}

fn access_token_present() -> bool {
    get_ticktick_secret(TICKTICK_ACCESS_TOKEN).is_ok()
}

fn build_status(conn: &Connection) -> Result<TickTickStatus, String> {
    let settings = load_settings();
    let projects = load_ticktick_projects(conn).map_err(|e| format!("{e:?}"))?;
    let views: Vec<TickTickProjectView> = projects
        .iter()
        .map(|p| {
            let mut columns = p.columns.clone();
            sort_columns(&mut columns);
            TickTickProjectView {
                id: p.id.clone(),
                name: p.name.clone(),
                sort_order: p.sort_order,
                role: settings
                    .ticktick_project_roles
                    .get(&p.id)
                    .cloned()
                    .unwrap_or_else(|| "ignore".into()),
                columns: columns
                    .into_iter()
                    .map(|column| TickTickColumnView {
                        id: column.id,
                        name: column.name,
                        sort_order: column.sort_order,
                    })
                    .collect(),
            }
        })
        .collect();
    let mut write_targets = BTreeMap::new();
    for role in [
        ListRole::Mainline,
        ListRole::Side,
        ListRole::Longterm,
        ListRole::Chore,
    ] {
        let Some(key) = role_key(role) else {
            continue;
        };
        if let Some(p) = write_target(&projects, &settings.ticktick_project_roles, role) {
            write_targets.insert(key.to_string(), p.name.clone());
        }
    }
    let last_sync_at = meta_get(conn, "ticktick_last_sync_at")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<i64>().ok());
    let last_result = meta_get(conn, "ticktick_last_result")
        .ok()
        .flatten()
        .unwrap_or_default();
    Ok(TickTickStatus {
        enabled: settings.ticktick_enabled,
        connected: access_token_present(),
        client_id: settings.ticktick_client_id,
        projects: views,
        write_targets,
        last_sync_at,
        last_result,
    })
}

fn status_blocking() -> Result<TickTickStatus, String> {
    let conn = open_app_db()?;
    build_status(&conn)
}

/// Confirm dirty-2 rows: search first; POST only when zero unused matches.
/// Hourly sync never calls this — only 「立即同步」 after `run_pull`.
pub(crate) fn push_ambiguous_creates(
    conn: &Connection,
    api: &mut dyn TickTickApi,
    projects: &[RemoteProject],
    roles: &BTreeMap<String, String>,
    day_start: i64,
    next_day_start: i64,
) -> Result<(), String> {
    let tasks = load_tasks(conn).map_err(|e| format!("{e:?}"))?;
    let mut claimed: BTreeSet<String> = tasks
        .iter()
        .filter_map(|t| t.ticktick_task_id.clone())
        .collect();
    let mut open_cache: BTreeMap<String, Vec<RemoteTask>> = BTreeMap::new();
    let mut need_manual = false;

    for mut task in tasks.into_iter().filter(|t| t.ticktick_dirty == 2) {
        let Some(role) = role_for_preset_list(&task.list_id) else {
            need_manual = true;
            continue;
        };
        let Some(target) = write_target(projects, roles, role) else {
            need_manual = true;
            continue;
        };

        if !open_cache.contains_key(&target.id) {
            let path = format!("/project/{}/data", target.id);
            let open = match api.request("GET", &path, None) {
                Ok(resp) if (200..300).contains(&resp.status) => {
                    parse_open_tasks(&target.id, &resp.body)
                }
                _ => {
                    need_manual = true;
                    continue;
                }
            };
            open_cache.insert(target.id.clone(), open);
        }
        let open = open_cache.get(&target.id).cloned().unwrap_or_default();

        let candidates: Vec<&RemoteTask> = open
            .iter()
            .filter(|remote| {
                if claimed.contains(&remote.id) {
                    return false;
                }
                let (start, end, all_day) = map_times(
                    remote.all_day,
                    remote.start,
                    remote.end,
                    day_start,
                    next_day_start,
                );
                remote.title == task.title
                    && start == task.start
                    && end == task.end
                    && all_day == task.ticktick_all_day
            })
            .collect();

        match candidates.len() {
            1 => {
                let remote = candidates[0];
                claimed.insert(remote.id.clone());
                task.ticktick_task_id = Some(remote.id.clone());
                task.ticktick_project_id = Some(remote.project_id.clone());
                task.ticktick_etag = remote.etag.clone();
                task.ticktick_dirty = 0;
                persist_task(conn, &task).map_err(|e| format!("{e:?}"))?;
            }
            0 => {
                let mut for_push = task.clone();
                for_push.ticktick_dirty = 1;
                let result = push_one(api, &for_push, projects, roles);
                match &result {
                    PushResult::Synced { task_id, .. } => {
                        claimed.insert(task_id.clone());
                        apply_push_result(&mut task, &result);
                        persist_task(conn, &task).map_err(|e| format!("{e:?}"))?;
                    }
                    PushResult::Ambiguous => {
                        need_manual = true;
                    }
                    PushResult::Failed(msg) => {
                        apply_push_result(&mut task, &result);
                        persist_task(conn, &task).map_err(|e| format!("{e:?}"))?;
                        set_last_result(conn, &chinese_push_failure(msg));
                    }
                    PushResult::Unchanged | PushResult::Cleared => {
                        need_manual = true;
                    }
                }
            }
            _ => {
                for remote in &candidates {
                    claimed.insert(remote.id.clone());
                }
                need_manual = true;
            }
        }
    }
    if need_manual {
        set_last_result(conn, "有任务需要手动确认");
    }
    Ok(())
}

fn insufficient_scope(status: u16, body: &Value) -> bool {
    if status == 403 {
        return true;
    }
    body.to_string()
        .to_ascii_lowercase()
        .contains("insufficient scope")
}

struct PlannedRefresh {
    settings: AppSettings,
    projects_json: String,
    partial: bool,
}

fn cached_project(previous: &Value, id: &str) -> Option<Value> {
    previous.as_array().and_then(|projects| {
        projects
            .iter()
            .find(|project| project.get("id").and_then(|value| value.as_str()) == Some(id))
            .cloned()
    })
}

fn project_with_columns(list_item: &Value, columns: Value) -> Value {
    let mut object = list_item.as_object().cloned().unwrap_or_default();
    object.insert("columns".into(), columns);
    Value::Object(object)
}

fn failed_project(list_item: &Value, id: &str, previous: &Value) -> Value {
    if let Some(cached) = cached_project(previous, id) {
        cached
    } else {
        project_with_columns(list_item, json!([]))
    }
}

fn plan_refresh(
    api: &mut dyn TickTickApi,
    previous: &Value,
    settings: &AppSettings,
) -> Result<PlannedRefresh, String> {
    let resp = api.request("GET", "/project", None).map_err(|e| match e {
        TickTickError::Transport => "同步失败".to_string(),
        TickTickError::Http(msg) => msg,
    })?;
    if !(200..300).contains(&resp.status) {
        if insufficient_scope(resp.status, &resp.body) {
            return Err(SCOPE_REFRESH.into());
        }
        return Err(format!("http {}", resp.status));
    }

    let listed = resp.body.as_array().cloned().unwrap_or_default();
    let mut merged = Vec::new();
    let mut partial = false;
    for list_item in listed {
        let Some(id) = list_item
            .get("id")
            .and_then(|value| value.as_str())
            .filter(|id| !id.is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        let path = format!("/project/{id}/data");
        let data = match api.request("GET", &path, None) {
            Ok(data) => data,
            Err(_) => {
                partial = true;
                merged.push(failed_project(&list_item, &id, previous));
                continue;
            }
        };
        if !(200..300).contains(&data.status) {
            if insufficient_scope(data.status, &data.body) {
                return Err(SCOPE_REFRESH.into());
            }
            partial = true;
            merged.push(failed_project(&list_item, &id, previous));
            continue;
        }
        let columns = data
            .body
            .get("columns")
            .cloned()
            .unwrap_or_else(|| json!([]));
        merged.push(project_with_columns(&list_item, columns));
    }

    let projects_value = Value::Array(merged);
    let parsed = parse_projects(&projects_value);
    let mut next = settings.clone();
    stamp_missing_roles(&parsed, &mut next.ticktick_project_roles);
    prune_column_roles(&kept_column_ids(&parsed), &mut next.ticktick_column_roles);
    let projects_json = serde_json::to_string(&projects_value).map_err(|e| e.to_string())?;
    Ok(PlannedRefresh {
        settings: next,
        projects_json,
        partial,
    })
}

fn apply_refresh(
    api: &mut dyn TickTickApi,
    conn: &Connection,
    settings: &mut AppSettings,
) -> Result<(), String> {
    let previous = meta_get(conn, "ticktick_projects_json")
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(Value::Null);
    let planned = plan_refresh(api, &previous, settings)?;
    meta_set(conn, "ticktick_projects_json", &planned.projects_json)
        .map_err(|e| format!("{e:?}"))?;
    if planned.partial {
        meta_set(conn, "ticktick_last_result", PARTIAL_REFRESH).map_err(|e| format!("{e:?}"))?;
    } else if meta_get(conn, "ticktick_last_result")
        .ok()
        .flatten()
        .as_deref()
        == Some(PARTIAL_REFRESH)
    {
        meta_set(conn, "ticktick_last_result", "").map_err(|e| format!("{e:?}"))?;
    }
    *settings = planned.settings;
    Ok(())
}

fn refresh_projects_blocking() -> Result<TickTickStatus, String> {
    if !access_token_present() {
        return Err("尚未连接".into());
    }
    let conn = open_app_db()?;
    let mut api = LiveApi::from_settings()?;
    let mut settings = load_settings();
    apply_refresh(&mut api, &conn, &mut settings)?;
    write_settings_file(&settings)?;
    build_status(&conn)
}

fn sync_now_blocking() -> Result<TickTickStatus, String> {
    let settings = load_settings();
    if !settings.ticktick_enabled {
        return Err("同步到任务板已关闭".into());
    }
    if !access_token_present() {
        return Err("尚未连接".into());
    }
    let now = crate::sync::now_secs();
    run_pull(now)?;
    let conn = open_app_db()?;
    let mut api = LiveApi::from_settings()?;
    let projects = load_ticktick_projects(&conn).map_err(|e| format!("{e:?}"))?;
    let day = day_str_for_ts(now);
    let day_start = start_of_named_day(&day).unwrap_or(0);
    let next_day_start = end_of_local_day(day_start);
    push_ambiguous_creates(
        &conn,
        &mut api,
        &projects,
        &settings.ticktick_project_roles,
        day_start,
        next_day_start,
    )?;
    build_status(&conn)
}

fn query_param(req: &str, key: &str) -> Option<String> {
    let line = req.lines().next()?;
    let path = line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let k = parts.next()?;
        let v = parts.next().unwrap_or("");
        if k == key {
            return Some(query_percent_decode(v));
        }
    }
    None
}

fn query_percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn exchange_authorization_code(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect: &str,
    verifier: &str,
) -> Result<(String, String), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(OAUTH_TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect),
            ("code_verifier", verifier),
        ])
        .send()
        .map_err(|_| "授权失败".to_string())?;
    if !resp.status().is_success() {
        return Err("授权失败".into());
    }
    let body: Value = resp.json().map_err(|_| "授权失败".to_string())?;
    let access = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "授权失败".to_string())?;
    let refresh = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok((access.to_string(), refresh))
}

fn connect_blocking() -> Result<TickTickStatus, String> {
    let settings = load_settings();
    let client_id = settings.ticktick_client_id.trim().to_string();
    if client_id.is_empty() {
        return Err("请先填写 Client ID".into());
    }
    let client_secret = get_ticktick_secret(TICKTICK_CLIENT_SECRET).unwrap_or_default();

    let (redirect, port) = loopback_redirect(&settings.ticktick_redirect_uri)?;
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|_| {
        format!("端口 {port} 已被占用。请关掉占用它的程序，或在 TickTick 和这里改成同一个新地址。")
    })?;

    let verifier = random_base64url(64);
    let challenge = pkce_challenge(&verifier);
    let state = random_base64url(16);
    let url = authorize_url(&client_id, &redirect, &state, &challenge);
    crate::platform::open_url(&url)?;

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(listener.accept());
    });
    let (mut stream, _) = match rx.recv_timeout(Duration::from_secs(OAUTH_TIMEOUT_SECS)) {
        Ok(Ok(pair)) => pair,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => return Err("授权超时".into()),
    };

    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]);
    let got_state = query_param(&req, "state");
    let code = query_param(&req, "code");

    let body = "<html><body>可以关闭此窗口，返回 GameLife。</body></html>";
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    if got_state.as_deref() != Some(state.as_str()) {
        return Err("授权失败".into());
    }
    let Some(code) = code.filter(|c| !c.is_empty()) else {
        return Err("授权失败".into());
    };

    let (access, refresh) =
        exchange_authorization_code(&client_id, &client_secret, &code, &redirect, &verifier)?;
    set_ticktick_secret(TICKTICK_ACCESS_TOKEN, &access)?;
    set_ticktick_secret(TICKTICK_REFRESH_TOKEN, &refresh)?;

    let conn = open_app_db()?;
    build_status(&conn)
}

#[tauri::command]
pub async fn ticktick_status() -> Result<TickTickStatus, String> {
    tauri::async_runtime::spawn_blocking(status_blocking)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn ticktick_set_client_secret(secret: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || set_ticktick_secret(TICKTICK_CLIENT_SECRET, &secret))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn ticktick_disconnect() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        delete_ticktick_secret(TICKTICK_ACCESS_TOKEN)?;
        delete_ticktick_secret(TICKTICK_REFRESH_TOKEN)?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn ticktick_refresh_projects() -> Result<TickTickStatus, String> {
    tauri::async_runtime::spawn_blocking(refresh_projects_blocking)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn ticktick_sync_now() -> Result<TickTickStatus, String> {
    tauri::async_runtime::spawn_blocking(sync_now_blocking)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn ticktick_connect() -> Result<TickTickStatus, String> {
    tauri::async_runtime::spawn_blocking(connect_blocking)
        .await
        .map_err(|e| e.to_string())?
}

/// Warning copy when a preset list has no TickTick write target.
pub fn no_write_target_warning(list_id: &str) -> Option<String> {
    let label = match list_id {
        PRESET_MAINLINE_ID => "主线",
        PRESET_SIDE_ID => "支线",
        PRESET_LONGTERM_ID => "长期",
        PRESET_CHORE_ID => "杂项",
        _ => return None,
    };
    Some(format!(
        "{label}还没有 TickTick 清单，这条任务只保存在本机。"
    ))
}

/// Whether a new unlinked row on this list should be marked dirty for create.
pub fn should_mark_new_for_push(
    list_id: &str,
    projects: &[RemoteProject],
    roles: &BTreeMap<String, String>,
) -> bool {
    let settings = load_settings();
    if !settings.ticktick_enabled {
        return false;
    }
    let role = match list_id {
        PRESET_MAINLINE_ID => ListRole::Mainline,
        PRESET_SIDE_ID => ListRole::Side,
        PRESET_LONGTERM_ID => ListRole::Longterm,
        PRESET_CHORE_ID => ListRole::Chore,
        _ => return false,
    };
    write_target(projects, roles, role).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct FakeApi {
        calls: Vec<(String, String)>,
        bodies: Vec<Option<serde_json::Value>>,
        script: Vec<Result<TickTickResponse, TickTickError>>,
        timeout_match: Option<(String, String)>,
    }

    impl FakeApi {
        fn empty() -> Self {
            Self {
                calls: Vec::new(),
                bodies: Vec::new(),
                script: Vec::new(),
                timeout_match: None,
            }
        }

        fn timeout_on(method: &str, path: &str) -> Self {
            Self {
                calls: Vec::new(),
                bodies: Vec::new(),
                script: Vec::new(),
                timeout_match: Some((method.into(), path.into())),
            }
        }

        fn with_script(script: Vec<Result<TickTickResponse, TickTickError>>) -> Self {
            Self {
                calls: Vec::new(),
                bodies: Vec::new(),
                script,
                timeout_match: None,
            }
        }
    }

    impl TickTickApi for FakeApi {
        fn request(
            &mut self,
            method: &str,
            path: &str,
            body: Option<serde_json::Value>,
        ) -> Result<TickTickResponse, TickTickError> {
            self.calls.push((method.into(), path.into()));
            self.bodies.push(body);
            if let Some((m, p)) = &self.timeout_match {
                if m == method && p == path {
                    return Err(TickTickError::Transport);
                }
            }
            if self.script.is_empty() {
                return Ok(TickTickResponse {
                    status: 200,
                    body: json!({}),
                });
            }
            self.script.remove(0)
        }
    }

    fn linked_task_without_remote_id() -> Task {
        Task {
            id: "local-1".into(),
            list_id: PRESET_MAINLINE_ID.into(),
            title: "写稿".into(),
            done: false,
            start: Some(10),
            end: Some(20),
            range: None,
            sort: 0,
            repeat: Default::default(),
            remind_offsets: vec![],
            notes: String::new(),
            ticktick_task_id: None,
            ticktick_project_id: None,
            ticktick_etag: String::new(),
            ticktick_dirty: 1,
            ticktick_all_day: false,
        }
    }

    #[test]
    fn push_create_timeout_does_not_post_twice_in_pull() {
        let mut api = FakeApi::timeout_on("POST", "/task");
        let task = linked_task_without_remote_id();
        let projects = vec![RemoteProject {
            id: "p".into(),
            name: "主线".into(),
            sort_order: 1,
            columns: Vec::new(),
        }];
        let mut roles = BTreeMap::new();
        roles.insert("p".into(), "mainline".into());
        let result = push_one(&mut api, &task, &projects, &roles);
        assert!(matches!(result, PushResult::Ambiguous));
        assert_eq!(
            api.calls
                .iter()
                .filter(|(m, p)| m == "POST" && p == "/task")
                .count(),
            1
        );
    }

    #[test]
    fn pull_round_with_empty_projects_does_not_advance() {
        let mut api = FakeApi::empty();
        let round = pull_round(
            &mut api,
            &[],
            &[],
            &BTreeMap::new(),
            &BTreeMap::new(),
            true,
            1_000,
            None,
            0,
            86_400,
        );
        assert!(api.calls.is_empty());
        assert!(!round.advance_sync_at);
    }

    #[test]
    fn run_pull_switch_off_makes_no_http_calls() {
        let mut api = FakeApi::empty();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        run_pull_gated(1_000, false, true, &mut api, &conn).unwrap();
        assert!(api.calls.is_empty());
    }

    #[test]
    fn apply_reconcile_skips_update_and_delete_when_dirty_ambiguous() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        let task = Task {
            id: "ambig".into(),
            list_id: PRESET_MAINLINE_ID.into(),
            title: "原标题".into(),
            done: false,
            start: Some(10),
            end: Some(20),
            range: None,
            sort: 0,
            repeat: Default::default(),
            remind_offsets: vec![],
            notes: String::new(),
            ticktick_task_id: None,
            ticktick_project_id: None,
            ticktick_etag: String::new(),
            ticktick_dirty: 2,
            ticktick_all_day: false,
        };
        persist_task(&conn, &task).unwrap();
        let actions = vec![
            ReconcileAction::Update(LocalMirror {
                local_id: "ambig".into(),
                list_id: PRESET_MAINLINE_ID.into(),
                title: "远端标题".into(),
                done: false,
                start: Some(10),
                end: Some(20),
                ticktick_task_id: Some("tt".into()),
                ticktick_project_id: Some("p".into()),
                ticktick_etag: "e".into(),
                ticktick_dirty: 0,
                ticktick_all_day: false,
            }),
            ReconcileAction::Delete {
                local_id: "ambig".into(),
            },
        ];
        apply_reconcile_actions(&conn, &actions).unwrap();
        let row = load_tasks(&conn)
            .unwrap()
            .into_iter()
            .find(|t| t.id == "ambig")
            .unwrap();
        assert_eq!(row.title, "原标题");
        assert_eq!(row.ticktick_dirty, 2);
    }

    #[test]
    fn apply_reconcile_accepts_link_when_dirty_ambiguous() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        let task = Task {
            id: "ambig".into(),
            list_id: PRESET_MAINLINE_ID.into(),
            title: "写稿".into(),
            done: false,
            start: Some(10),
            end: Some(20),
            range: None,
            sort: 0,
            repeat: Default::default(),
            remind_offsets: vec![],
            notes: String::new(),
            ticktick_task_id: None,
            ticktick_project_id: None,
            ticktick_etag: String::new(),
            ticktick_dirty: 2,
            ticktick_all_day: false,
        };
        persist_task(&conn, &task).unwrap();
        apply_reconcile_actions(
            &conn,
            &[ReconcileAction::Link {
                local_id: "ambig".into(),
                ticktick_task_id: "tt1".into(),
                ticktick_project_id: "p1".into(),
                etag: "e1".into(),
            }],
        )
        .unwrap();
        let row = load_tasks(&conn)
            .unwrap()
            .into_iter()
            .find(|t| t.id == "ambig")
            .unwrap();
        assert_eq!(row.ticktick_task_id.as_deref(), Some("tt1"));
        assert_eq!(row.ticktick_dirty, 0);
    }

    fn ambiguous_local_task() -> Task {
        Task {
            id: "ambig-local".into(),
            list_id: PRESET_MAINLINE_ID.into(),
            title: "写稿".into(),
            done: false,
            start: Some(10),
            end: Some(20),
            range: None,
            sort: 0,
            repeat: Default::default(),
            remind_offsets: vec![],
            notes: String::new(),
            ticktick_task_id: None,
            ticktick_project_id: None,
            ticktick_etag: String::new(),
            ticktick_dirty: 2,
            ticktick_all_day: false,
        }
    }

    fn mainline_roles() -> (Vec<RemoteProject>, BTreeMap<String, String>) {
        let projects = vec![RemoteProject {
            id: "p".into(),
            name: "主线".into(),
            sort_order: 1,
            columns: Vec::new(),
        }];
        let mut roles = BTreeMap::new();
        roles.insert("p".into(), "mainline".into());
        (projects, roles)
    }

    #[test]
    fn push_ambiguous_multi_match_does_not_post() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        persist_task(&conn, &ambiguous_local_task()).unwrap();
        let (projects, roles) = mainline_roles();
        let open = json!({
            "tasks": [
                { "id": "a", "title": "写稿", "startDate": "1970-01-01T00:00:10+0000", "dueDate": "1970-01-01T00:00:20+0000", "isAllDay": false, "etag": "e1" },
                { "id": "b", "title": "写稿", "startDate": "1970-01-01T00:00:10+0000", "dueDate": "1970-01-01T00:00:20+0000", "isAllDay": false, "etag": "e2" },
            ]
        });
        let mut api = FakeApi::with_script(vec![Ok(TickTickResponse {
            status: 200,
            body: open,
        })]);
        push_ambiguous_creates(&conn, &mut api, &projects, &roles, 0, 86_400).unwrap();
        assert!(
            api.calls
                .iter()
                .all(|(m, p)| !(m == "POST" && p == "/task")),
            "must not POST when more than one unused match: {:?}",
            api.calls
        );
        let row = load_tasks(&conn)
            .unwrap()
            .into_iter()
            .find(|t| t.id == "ambig-local")
            .unwrap();
        assert_eq!(row.ticktick_dirty, 2);
        assert_eq!(
            meta_get(&conn, "ticktick_last_result").unwrap().as_deref(),
            Some("有任务需要手动确认")
        );
    }

    #[test]
    fn push_ambiguous_zero_match_posts_once() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        persist_task(&conn, &ambiguous_local_task()).unwrap();
        let (projects, roles) = mainline_roles();
        let mut api = FakeApi::with_script(vec![
            Ok(TickTickResponse {
                status: 200,
                body: json!({ "tasks": [] }),
            }),
            Ok(TickTickResponse {
                status: 200,
                body: json!({ "id": "new-tt", "etag": "e" }),
            }),
        ]);
        push_ambiguous_creates(&conn, &mut api, &projects, &roles, 0, 86_400).unwrap();
        assert_eq!(
            api.calls
                .iter()
                .filter(|(m, p)| m == "POST" && p == "/task")
                .count(),
            1
        );
        let row = load_tasks(&conn)
            .unwrap()
            .into_iter()
            .find(|t| t.id == "ambig-local")
            .unwrap();
        assert_eq!(row.ticktick_dirty, 0);
        assert_eq!(row.ticktick_task_id.as_deref(), Some("new-tt"));
    }

    #[test]
    fn reopen_body_includes_id_project_title_and_status() {
        let mut api = FakeApi::empty();
        let op = PushOp {
            kind: PushKind::Reopen,
            local_id: "L".into(),
            task_id: Some("tt1".into()),
            project_id: "p1".into(),
            to_project_id: None,
            title: "写稿".into(),
            start: Some(10),
            end: Some(20),
            all_day: false,
        };
        let _ = execute_push_op(&mut api, &op);
        let body = api.bodies[0].as_ref().expect("reopen sends a body");
        assert_eq!(body.get("id").and_then(|v| v.as_str()), Some("tt1"));
        assert_eq!(body.get("projectId").and_then(|v| v.as_str()), Some("p1"));
        assert_eq!(body.get("title").and_then(|v| v.as_str()), Some("写稿"));
        assert_eq!(body.get("status").and_then(|v| v.as_i64()), Some(0));
    }

    #[test]
    fn create_http_5xx_is_ambiguous_not_failed() {
        let mut api = FakeApi::with_script(vec![Ok(TickTickResponse {
            status: 503,
            body: json!({}),
        })]);
        let task = linked_task_without_remote_id();
        let (projects, roles) = mainline_roles();
        let result = push_one(&mut api, &task, &projects, &roles);
        assert!(matches!(result, PushResult::Ambiguous));
        let mut t = task;
        apply_push_result(&mut t, &result);
        assert_eq!(t.ticktick_dirty, 2);
    }

    #[test]
    fn create_http_4xx_stays_dirty_one() {
        let mut api = FakeApi::with_script(vec![Ok(TickTickResponse {
            status: 400,
            body: json!({}),
        })]);
        let task = linked_task_without_remote_id();
        let (projects, roles) = mainline_roles();
        let result = push_one(&mut api, &task, &projects, &roles);
        assert!(matches!(result, PushResult::Failed(_)));
        let mut t = task;
        apply_push_result(&mut t, &result);
        assert_eq!(t.ticktick_dirty, 1);
    }

    #[test]
    fn background_push_failure_keeps_dirty_and_chinese_reason() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        let mut task = linked_task_without_remote_id();
        task.ticktick_task_id = Some("tt1".into());
        task.ticktick_project_id = Some("p".into());
        persist_task(&conn, &task).unwrap();
        let result = PushResult::Failed("http 400".into());
        apply_push_result(&mut task, &result);
        set_last_result(&conn, &chinese_push_failure("http 400"));
        persist_task(&conn, &task).unwrap();
        assert_eq!(task.ticktick_dirty, 1);
        assert_eq!(
            meta_get(&conn, "ticktick_last_result").unwrap().as_deref(),
            Some("写回被拒绝")
        );
    }

    #[test]
    fn pull_keeps_push_failure_reason_over_ok() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        let mut task = linked_task_without_remote_id();
        task.ticktick_task_id = Some("tt1".into());
        task.ticktick_project_id = Some("p".into());
        persist_task(&conn, &task).unwrap();
        let (projects, roles) = mainline_roles();
        // Push (Reopen) fails 400, then first-sync pull GETs open list successfully.
        let mut api = FakeApi::with_script(vec![
            Ok(TickTickResponse {
                status: 400,
                body: json!({}),
            }),
            Ok(TickTickResponse {
                status: 200,
                body: json!({ "tasks": [] }),
            }),
        ]);
        run_pull_body_with(1_000, &mut api, &conn, &projects, &roles, &BTreeMap::new()).unwrap();
        let row = load_tasks(&conn)
            .unwrap()
            .into_iter()
            .find(|t| t.id == "local-1")
            .unwrap();
        assert_eq!(row.ticktick_dirty, 1);
        assert_eq!(
            meta_get(&conn, "ticktick_last_result").unwrap().as_deref(),
            Some("写回被拒绝")
        );
    }

    #[test]
    fn loopback_redirect_keeps_the_registered_url_and_port() {
        let (redirect, port) =
            loopback_redirect("  http://127.0.0.1:18789/callback  ").unwrap();
        assert_eq!(redirect, "http://127.0.0.1:18789/callback");
        assert_eq!(port, 18789);
        let (local, local_port) =
            loopback_redirect("http://localhost:18789/callback").unwrap();
        assert_eq!(local, "http://localhost:18789/callback");
        assert_eq!(local_port, 18789);
        assert!(loopback_redirect("").is_err());
        assert!(loopback_redirect("http://127.0.0.1/callback").is_err());
        assert!(loopback_redirect("https://127.0.0.1:18789/callback").is_err());
        assert!(loopback_redirect("http://example.com:18789/callback").is_err());
    }

    const PARTIAL_REFRESH: &str = "有清单没能刷新，这些清单仍显示上次的分组。";
    const SCOPE_REFRESH: &str = "TickTick 授权不足以读取清单，请重新连接。";

    #[test]
    fn authorize_url_asks_for_task_read_and_write() {
        let url = authorize_url("cid", "http://127.0.0.1:9/callback", "st", "ch");
        assert!(url.contains("scope=tasks%3Aread%20tasks%3Awrite"));
        assert!(!url.contains("scope=tasks%3Awrite&"));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn refresh_keeps_failed_project_columns_and_does_not_touch_tasks() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        let task = Task {
            id: "L1".into(),
            list_id: PRESET_MAINLINE_ID.into(),
            title: "写稿".into(),
            done: false,
            start: None,
            end: None,
            range: None,
            sort: 0,
            repeat: Default::default(),
            remind_offsets: vec![],
            notes: String::new(),
            ticktick_task_id: Some("tt1".into()),
            ticktick_project_id: Some("p2".into()),
            ticktick_etag: "e".into(),
            ticktick_dirty: 1,
            ticktick_all_day: false,
        };
        persist_task(&conn, &task).unwrap();
        meta_set(&conn, "ticktick_last_sync_at", "1000").unwrap();
        meta_set(
            &conn,
            "ticktick_projects_json",
            r#"[{"id":"p1","name":"甲","sortOrder":1,"columns":[{"id":"old-1","name":"旧甲","sortOrder":1,"projectId":"p1"}]},{"id":"p2","name":"乙","sortOrder":2,"viewMode":"list","columns":[{"id":"old-2","name":"旧乙","sortOrder":1,"projectId":"p2"}]}]"#,
        )
        .unwrap();
        let mut settings = crate::config::default_settings();
        settings.ticktick_project_roles.insert("p1".into(), "mainline".into());
        settings.ticktick_column_roles.insert("old-1".into(), "side".into());
        settings.ticktick_column_roles.insert("old-2".into(), "chore".into());
        let mut api = FakeApi::with_script(vec![
            Ok(TickTickResponse {
                status: 200,
                body: json!([
                    {"id": "p1", "name": "甲", "sortOrder": 1, "viewMode": "list"},
                    {"id": "p2", "name": "乙", "sortOrder": 2, "viewMode": "list"}
                ]),
            }),
            Ok(TickTickResponse {
                status: 200,
                body: json!({"columns": [{"id": "new-1", "name": "新甲", "sortOrder": 3, "projectId": "p1"}]}),
            }),
            Ok(TickTickResponse {
                status: 500,
                body: json!({"errorMessage": "boom"}),
            }),
        ]);
        apply_refresh(&mut api, &conn, &mut settings).unwrap();
        let raw = meta_get(&conn, "ticktick_projects_json").unwrap().unwrap();
        let cached: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let p1 = cached.as_array().unwrap().iter().find(|p| p["id"] == "p1").unwrap();
        let p2 = cached.as_array().unwrap().iter().find(|p| p["id"] == "p2").unwrap();
        assert_eq!(p1["columns"][0]["id"], "new-1");
        assert_eq!(p1["columns"][0]["name"], "新甲");
        assert_eq!(p1["columns"][0]["sortOrder"], 3);
        assert_eq!(p1["viewMode"], "list");
        assert_eq!(p2["columns"][0]["id"], "old-2");
        assert!(settings.ticktick_column_roles.get("old-1").is_none());
        assert_eq!(settings.ticktick_column_roles.get("old-2").map(String::as_str), Some("chore"));
        assert!(settings.ticktick_column_roles.get("new-1").is_none());
        assert_eq!(meta_get(&conn, "ticktick_last_sync_at").unwrap().as_deref(), Some("1000"));
        assert_eq!(meta_get(&conn, "ticktick_last_result").unwrap().as_deref(), Some(PARTIAL_REFRESH));
        assert_eq!(load_tasks(&conn).unwrap().len(), 1);
        assert_eq!(load_tasks(&conn).unwrap()[0].ticktick_dirty, 1);
    }

    #[test]
    fn refresh_insufficient_scope_leaves_cache_and_roles() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        meta_set(&conn, "ticktick_projects_json", r#"[{"id":"p","name":"甲","sortOrder":1}]"#).unwrap();
        meta_set(&conn, "ticktick_last_sync_at", "1000").unwrap();
        let mut settings = crate::config::default_settings();
        settings.ticktick_project_roles.insert("p".into(), "mainline".into());
        settings.ticktick_column_roles.insert("c".into(), "side".into());
        let mut api = FakeApi::with_script(vec![Ok(TickTickResponse {
            status: 500,
            body: json!({"errorCode": "client_exception", "errorMessage": "Insufficient scope for this resource"}),
        })]);
        let err = apply_refresh(&mut api, &conn, &mut settings).unwrap_err();
        assert_eq!(err, SCOPE_REFRESH);
        assert_eq!(
            meta_get(&conn, "ticktick_projects_json").unwrap().as_deref(),
            Some(r#"[{"id":"p","name":"甲","sortOrder":1}]"#)
        );
        assert_eq!(settings.ticktick_project_roles.get("p").map(String::as_str), Some("mainline"));
        assert_eq!(settings.ticktick_column_roles.get("c").map(String::as_str), Some("side"));
        assert_eq!(meta_get(&conn, "ticktick_last_sync_at").unwrap().as_deref(), Some("1000"));
    }

    #[test]
    fn refresh_drops_columns_when_the_project_disappears() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        meta_set(
            &conn,
            "ticktick_projects_json",
            r#"[{"id":"gone","name":"旧","sortOrder":1,"columns":[{"id":"c-gone","name":"组","sortOrder":1,"projectId":"gone"}]}]"#,
        )
        .unwrap();
        let mut settings = crate::config::default_settings();
        settings.ticktick_column_roles.insert("c-gone".into(), "mainline".into());
        let mut api = FakeApi::with_script(vec![
            Ok(TickTickResponse {
                status: 200,
                body: json!([{"id": "p", "name": "新", "sortOrder": 1}]),
            }),
            Ok(TickTickResponse {
                status: 200,
                body: json!({"columns": []}),
            }),
        ]);
        apply_refresh(&mut api, &conn, &mut settings).unwrap();
        assert!(settings.ticktick_column_roles.get("c-gone").is_none());
        assert_eq!(settings.ticktick_project_roles.get("p").map(String::as_str), Some("ignore"));
    }

    #[test]
    fn maybe_spawn_guard_is_due_only_after_an_hour() {
        assert!(!ticktick_due(Some(1_000), 1_000 + 3599));
        assert!(ticktick_due(Some(1_000), 1_000 + 3600));
        assert!(ticktick_due(None, 50));
    }

    #[test]
    fn pull_round_partial_project_failure_does_not_advance_or_delete() {
        let mut api = FakeApi::with_script(vec![
            Ok(TickTickResponse {
                status: 200,
                body: json!({ "tasks": [] }),
            }),
            Ok(TickTickResponse {
                status: 500,
                body: json!({ "error": "boom" }),
            }),
            Ok(TickTickResponse {
                status: 200,
                body: json!({ "tasks": [] }),
            }),
        ]);
        let local = Task {
            id: "L1".into(),
            list_id: PRESET_MAINLINE_ID.into(),
            title: "写稿".into(),
            done: false,
            start: Some(10),
            end: Some(20),
            range: None,
            sort: 0,
            repeat: Default::default(),
            remind_offsets: vec![],
            notes: String::new(),
            ticktick_task_id: Some("tt1".into()),
            ticktick_project_id: Some("p1".into()),
            ticktick_etag: "e1".into(),
            ticktick_dirty: 0,
            ticktick_all_day: false,
        };
        let projects = vec![
            RemoteProject {
                id: "p1".into(),
                name: "主线".into(),
                sort_order: 1,
                columns: Vec::new(),
            },
            RemoteProject {
                id: "p2".into(),
                name: "支线".into(),
                sort_order: 2,
                columns: Vec::new(),
            },
        ];
        let mut roles = BTreeMap::new();
        roles.insert("p1".into(), "mainline".into());
        roles.insert("p2".into(), "side".into());
        let round = pull_round(
            &mut api,
            &[local],
            &projects,
            &roles,
            &BTreeMap::new(),
            false,
            2_000,
            Some(1_000),
            0,
            86_400,
        );
        assert!(!round.advance_sync_at);
        assert_eq!(round.error.as_deref(), Some("同步失败"));
        assert!(round
            .actions
            .iter()
            .all(|a| !matches!(a, ReconcileAction::Delete { .. })));
    }
}
