//! TickTick Open API mirror: push dirty rows, pull + reconcile.
//! Real HTTP is behind `TickTickApi`; tests use `FakeApi`.

use std::collections::BTreeMap;
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
    parse_completed_ids, parse_open_tasks, parse_projects, parse_role, push_op, reconcile,
    stamp_missing_roles, write_target, LocalMirror, ProjectFetch, PushKind, PushOp,
    ReconcileAction, ReconcileInput, RemoteProject,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::config::{load_settings, save_settings as write_settings_file};
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
            let mut body = task_body(op);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("status".into(), json!(0));
            }
            ("POST", format!("/task/{id}"), Some(body))
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
        PushResult::Unchanged | PushResult::Failed(_) => {}
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

fn mapped_project_ids(projects: &[RemoteProject], roles: &BTreeMap<String, String>) -> Vec<String> {
    projects
        .iter()
        .filter(|p| roles.get(&p.id).and_then(|r| parse_role(r)).is_some())
        .map(|p| p.id.clone())
        .collect()
}

pub fn pull_round(
    api: &mut dyn TickTickApi,
    local: &[Task],
    projects: &[RemoteProject],
    roles: &BTreeMap<String, String>,
    first_sync: bool,
    now: i64,
    last_sync_at: Option<i64>,
    day_start: i64,
    next_day_start: i64,
) -> PullRound {
    let mapped = mapped_project_ids(projects, roles);
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
    let mut tasks = load_tasks(conn).map_err(|e| format!("{e:?}"))?;
    for action in actions {
        match action {
            ReconcileAction::Insert(m) => {
                let sort = next_list_sort(conn, &m.list_id).map_err(|e| format!("{e:?}"))?;
                let task = mirror_to_new_task(m, sort);
                persist_task(conn, &task).map_err(|e| format!("{e:?}"))?;
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
                persist_task(conn, task).map_err(|e| format!("{e:?}"))?;
            }
            ReconcileAction::Delete { local_id } => {
                let Some(task) = tasks.iter().find(|t| t.id == *local_id) else {
                    continue;
                };
                if task.ticktick_dirty != 0 {
                    continue;
                }
                delete_task_row(conn, local_id).map_err(|e| format!("{e:?}"))?;
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
                persist_task(conn, task).map_err(|e| format!("{e:?}"))?;
            }
        }
    }
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

fn push_and_persist(
    conn: &Connection,
    api: &mut dyn TickTickApi,
    task: &mut Task,
) -> Result<(), String> {
    let settings = load_settings();
    let projects = load_ticktick_projects(conn).map_err(|e| format!("{e:?}"))?;
    let result = push_one(api, task, &projects, &settings.ticktick_project_roles);
    apply_push_result(task, &result);
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
    let roles = settings.ticktick_project_roles.clone();

    let tasks = load_tasks(conn).map_err(|e| format!("{e:?}"))?;
    for mut task in tasks.into_iter().filter(|t| t.ticktick_dirty == 1) {
        let result = push_one(api, &task, &projects, &roles);
        if matches!(&result, PushResult::Failed(msg) if msg == "需要重新连接") {
            set_last_result(conn, "需要重新连接");
            return Ok(());
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
        &projects,
        &roles,
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
    let result_msg = round.error.as_deref().unwrap_or("ok");
    set_last_result(conn, result_msg);
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

/// Build the TickTick authorize URL (PKCE S256, scope tasks:write).
pub fn authorize_url(client_id: &str, redirect: &str, state: &str, challenge: &str) -> String {
    format!(
        "{OAUTH_AUTHORIZE}?client_id={}&redirect_uri={}&response_type=code&scope=tasks%3Awrite&code_challenge={}&code_challenge_method=S256&state={}",
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
pub struct TickTickProjectView {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
    pub role: String,
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
        .map(|p| TickTickProjectView {
            id: p.id.clone(),
            name: p.name.clone(),
            sort_order: p.sort_order,
            role: settings
                .ticktick_project_roles
                .get(&p.id)
                .cloned()
                .unwrap_or_else(|| "ignore".into()),
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

/// Confirm dirty-2 rows with one Create each. Hourly sync never calls this.
fn push_ambiguous_creates(conn: &Connection, api: &mut dyn TickTickApi) -> Result<(), String> {
    let settings = load_settings();
    let projects = load_ticktick_projects(conn).map_err(|e| format!("{e:?}"))?;
    let roles = &settings.ticktick_project_roles;
    let tasks = load_tasks(conn).map_err(|e| format!("{e:?}"))?;
    let mut need_manual = false;
    for mut task in tasks.into_iter().filter(|t| t.ticktick_dirty == 2) {
        let mut for_push = task.clone();
        for_push.ticktick_dirty = 1;
        let result = push_one(api, &for_push, &projects, roles);
        match &result {
            PushResult::Synced { .. } => {
                apply_push_result(&mut task, &result);
                persist_task(conn, &task).map_err(|e| format!("{e:?}"))?;
            }
            PushResult::Ambiguous => {
                need_manual = true;
            }
            PushResult::Failed(_) | PushResult::Unchanged | PushResult::Cleared => {
                if task.ticktick_task_id.is_none() {
                    need_manual = true;
                }
            }
        }
    }
    if need_manual {
        set_last_result(conn, "有任务需要手动确认");
    }
    Ok(())
}

fn refresh_projects_blocking() -> Result<TickTickStatus, String> {
    if !access_token_present() {
        return Err("尚未连接".into());
    }
    let conn = open_app_db()?;
    let mut api = LiveApi::from_settings()?;
    let resp = api
        .request("GET", "/project", None)
        .map_err(|e| match e {
            TickTickError::Transport => "同步失败".to_string(),
            TickTickError::Http(msg) => msg,
        })?;
    if !(200..300).contains(&resp.status) {
        return Err(format!("http {}", resp.status));
    }
    let projects = parse_projects(&resp.body);
    let mut settings = load_settings();
    stamp_missing_roles(&projects, &mut settings.ticktick_project_roles);
    write_settings_file(&settings)?;
    let raw = serde_json::to_string(&resp.body).map_err(|e| e.to_string())?;
    meta_set(&conn, "ticktick_projects_json", &raw).map_err(|e| format!("{e:?}"))?;
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
    push_ambiguous_creates(&conn, &mut api)?;
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

    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}/callback");

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
        script: Vec<Result<TickTickResponse, TickTickError>>,
        timeout_match: Option<(String, String)>,
    }

    impl FakeApi {
        fn empty() -> Self {
            Self {
                calls: Vec::new(),
                script: Vec::new(),
                timeout_match: None,
            }
        }

        fn timeout_on(method: &str, path: &str) -> Self {
            Self {
                calls: Vec::new(),
                script: Vec::new(),
                timeout_match: Some((method.into(), path.into())),
            }
        }

        fn with_script(script: Vec<Result<TickTickResponse, TickTickError>>) -> Self {
            Self {
                calls: Vec::new(),
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
            _body: Option<serde_json::Value>,
        ) -> Result<TickTickResponse, TickTickError> {
            self.calls.push((method.into(), path.into()));
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

    #[test]
    fn authorize_url_asks_for_task_write_and_s256() {
        let url = authorize_url("cid", "http://127.0.0.1:9/callback", "st", "ch");
        assert!(url.starts_with("https://ticktick.com/oauth/authorize?"));
        assert!(url.contains("client_id=cid"));
        assert!(url.contains("scope=tasks%3Awrite") || url.contains("scope=tasks:write"));
        assert!(url.contains("code_challenge=ch"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=st"));
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
            },
            RemoteProject {
                id: "p2".into(),
                name: "支线".into(),
                sort_order: 2,
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
