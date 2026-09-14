use chrono::{DateTime, FixedOffset};
use gamelife_core::{
    align_range, role_from_hashtag, ticktick_overlapping_count, ticktick_snapshot_id, ListRole,
    TimedTask, MAX_JUDGMENT_TASKS,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use base64::Engine;

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
    #[serde(default)]
    pub column_id: Option<String>,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenColumn {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

pub fn parse_open_project_columns(json: &str) -> Result<Vec<OpenColumn>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("ticktick json: {e}"))?;
    let serde_json::Value::Object(mut obj) = value else {
        return Ok(Vec::new());
    };
    let Some(columns) = obj.remove("columns") else {
        return Ok(Vec::new());
    };
    serde_json::from_value(columns).map_err(|e| format!("ticktick columns: {e}"))
}

#[derive(Clone, Debug)]
pub struct TickTickRoleMaps {
    pub project_roles: std::collections::BTreeMap<String, String>,
    pub column_roles: std::collections::BTreeMap<String, String>,
}

impl TickTickRoleMaps {
    pub fn from_settings(settings: &crate::config::AppSettings) -> Self {
        Self {
            project_roles: settings.ticktick_project_roles.clone(),
            column_roles: settings.ticktick_column_roles.clone(),
        }
    }
}

/// Column roles are keyed `"{projectId}:{columnId}"`. Two projects can both
/// have a column called "Today", and the key also lets the backend work out
/// which projects to sync without first fetching every project's columns.
pub fn column_role_key(project_id: &str, column_id: &str) -> String {
    format!("{project_id}:{column_id}")
}

/// Projects whose tasks are needed: those with a project role, plus those
/// that own a column with a role. `listed` is what the API actually returned.
pub fn sync_target_project_ids(
    project_roles: &std::collections::BTreeMap<String, String>,
    column_roles: &std::collections::BTreeMap<String, String>,
    listed: &[String],
) -> Vec<String> {
    let mut wanted: std::collections::BTreeSet<&str> = project_roles
        .iter()
        .filter(|(_, role)| role_from_key(role).is_some())
        .map(|(id, _)| id.as_str())
        .collect();
    for key in column_roles.keys() {
        if let Some((project_id, _)) = key.split_once(':') {
            wanted.insert(project_id);
        }
    }
    listed
        .iter()
        .filter(|id| wanted.contains(id.as_str()))
        .cloned()
        .collect()
}

/// A column's role beats its project's: the user marked that grouping
/// specifically, and an unmarked project falls back to its own role.
pub fn mapped_task_role(
    project_id: &str,
    column_id: Option<&str>,
    project_roles: &std::collections::BTreeMap<String, String>,
    column_roles: &std::collections::BTreeMap<String, String>,
) -> Option<ListRole> {
    column_id
        .and_then(|c| project_role(column_roles, &column_role_key(project_id, c)))
        .or_else(|| project_role(project_roles, project_id))
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

pub const TICKTICK_API: &str = "https://api.ticktick.com/open/v1";
pub const TICKTICK_AUTHORIZE: &str = "https://ticktick.com/oauth/authorize";
pub const TICKTICK_TOKEN: &str = "https://ticktick.com/oauth/token";
pub const TICKTICK_REDIRECT: &str = "http://127.0.0.1:18789/callback";
const TICKTICK_REDIRECT_QUERY: &str = "http%3A%2F%2F127.0.0.1%3A18789%2Fcallback";
const TICKTICK_LOOPBACK_PORT: u16 = 18789;
const CACHE_FRESH_SECS: i64 = 30 * 60;
const OAUTH_LISTEN_TIMEOUT: Duration = Duration::from_secs(180);
static PKCE_VERIFIER: Mutex<Option<String>> = Mutex::new(None);
static OAUTH_LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);
static OAUTH_CANCEL: Mutex<Option<Sender<()>>> = Mutex::new(None);
static TICKTICK_SYNCING: AtomicBool = AtomicBool::new(false);

pub fn incoming_secret_to_store(incoming: Option<&str>) -> Option<String> {
    incoming
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn oauth_secret_ready(incoming: Option<&str>, stored_present: bool) -> bool {
    incoming_secret_to_store(incoming).is_some() || stored_present
}

pub fn oauth_begin_preflight(client_id: &str, secret_present: bool) -> Result<(), String> {
    if client_id.trim().is_empty() {
        return Err("missing ticktick client id".into());
    }
    if !secret_present {
        return Err("missing ticktick client secret".into());
    }
    Ok(())
}

pub fn client_secret_present() -> bool {
    crate::keychain::get_ticktick_client_secret()
        .ok()
        .is_some_and(|s| !s.trim().is_empty())
}

pub fn clear_oauth_last_error() {
    *OAUTH_LAST_ERROR.lock().expect("oauth error mutex") = None;
}

pub fn set_oauth_last_error(msg: String) {
    *OAUTH_LAST_ERROR.lock().expect("oauth error mutex") = Some(msg);
}

pub fn oauth_last_error() -> Option<String> {
    OAUTH_LAST_ERROR.lock().expect("oauth error mutex").clone()
}

pub fn public_oauth_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("missing ticktick client secret") || lower.contains("client secret") {
        return "missing ticktick client secret".into();
    }
    if lower.contains("pkce") {
        return "missing pkce verifier".into();
    }
    if lower.contains("oauth listen") || lower.contains("oauth accept") {
        return "oauth listen".into();
    }
    if lower.contains("oauth timeout") {
        return "oauth timeout".into();
    }
    "oauth token exchange failed".into()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickTickSyncResult {
    pub count: usize,
    pub truncated: bool,
}

pub fn sync_result_from_cache(
    cache: &[TimedTask],
    day_start: i64,
    day_end: i64,
) -> TickTickSyncResult {
    TickTickSyncResult {
        count: cache.len(),
        truncated: ticktick_overlapping_count(cache, day_start, day_end) > MAX_JUDGMENT_TASKS,
    }
}

pub fn store_pkce_verifier(v: String) {
    *PKCE_VERIFIER.lock().expect("pkce mutex") = Some(v);
}

pub fn take_pkce_verifier() -> Option<String> {
    PKCE_VERIFIER.lock().expect("pkce mutex").take()
}

pub fn oauth_code_from_callback(url: &str) -> Result<String, String> {
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or(url);
    for part in query.split('&') {
        if let Some(code) = part.strip_prefix("code=") {
            return Ok(code.split(['#', '&']).next().unwrap_or(code).to_string());
        }
    }
    Err("missing oauth code".into())
}

pub fn build_authorize_url(client_id: &str, challenge: &str) -> String {
    format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope=tasks:read&code_challenge={}&code_challenge_method=S256",
        TICKTICK_AUTHORIZE,
        client_id,
        TICKTICK_REDIRECT_QUERY,
        challenge
    )
}

pub fn is_ticktick_authorize_url(url: &str) -> bool {
    url.starts_with("https://ticktick.com/oauth/authorize?")
        || url.starts_with("https://dida365.com/oauth/authorize?")
}

pub fn open_in_browser(url: &str) -> Result<(), String> {
    if !is_ticktick_authorize_url(url) {
        return Err("unexpected authorize url".into());
    }
    crate::platform::open_url(url)
}

pub fn oauth_callback_kind(url: &str) -> &'static str {
    if oauth_code_from_callback(url).is_ok() {
        return "code";
    }
    let path = url
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .trim_end_matches('/');
    if path.ends_with("/callback") {
        "setting"
    } else {
        "invalid"
    }
}

pub fn complete_oauth_with_code(http: &impl TickTickHttp, code: &str) -> Result<(), String> {
    let verifier = take_pkce_verifier().ok_or("missing pkce verifier")?;
    let client_id = crate::config::load_settings().ticktick_client_id;
    let secret = crate::keychain::get_ticktick_client_secret()?;
    let body = http.post_form(
        TICKTICK_TOKEN,
        &[
            ("client_id", client_id.as_str()),
            ("client_secret", secret.as_str()),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", TICKTICK_REDIRECT),
            ("code_verifier", verifier.as_str()),
        ],
    )?;
    let (access, refresh) = parse_token_response(&body)?;
    crate::keychain::set_ticktick_access_token(&access)?;
    if let Some(refresh) = refresh {
        crate::keychain::set_ticktick_refresh_token(&refresh)?;
    }
    Ok(())
}

pub fn bind_oauth_loopback() -> Result<TcpListener, String> {
    cancel_oauth_loopback();
    for _ in 0..12 {
        match TcpListener::bind(("127.0.0.1", TICKTICK_LOOPBACK_PORT)) {
            Ok(listener) => return Ok(listener),
            Err(_) => std::thread::sleep(Duration::from_millis(40)),
        }
    }
    Err("oauth listen".to_string())
}

pub fn cancel_oauth_loopback() {
    if let Ok(mut slot) = OAUTH_CANCEL.lock() {
        if let Some(tx) = slot.take() {
            let _ = tx.send(());
        }
    }
}

pub fn spawn_oauth_loopback(listener: TcpListener) {
    let (tx, rx) = mpsc::channel();
    if let Ok(mut slot) = OAUTH_CANCEL.lock() {
        *slot = Some(tx);
    }
    std::thread::spawn(move || {
        if let Err(e) = accept_oauth_callback_once(listener, rx) {
            if e != "oauth cancelled" {
                set_oauth_last_error(public_oauth_error(&e));
            }
        }
    });
}

fn accept_oauth_callback_once(listener: TcpListener, cancel: Receiver<()>) -> Result<(), String> {
    listener
        .set_nonblocking(true)
        .map_err(|_| "oauth listen".to_string())?;
    let deadline = Instant::now() + OAUTH_LISTEN_TIMEOUT;
    loop {
        if cancel.try_recv().is_ok() {
            return Err("oauth cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("oauth timeout".into());
        }
        match listener.accept() {
            Ok((stream, addr)) => {
                if !addr.ip().is_loopback() {
                    continue;
                }
                match handle_oauth_stream(stream) {
                    Ok(()) => return Ok(()),
                    Err(e) if e == "missing oauth code" => continue,
                    Err(e) => return Err(e),
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::Interrupted =>
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return Err("oauth accept".into()),
        }
    }
}

fn write_oauth_html(stream: &mut std::net::TcpStream, connected: bool) {
    let body = if connected {
        "<!doctype html><meta charset=utf-8><p>已连接，可以关闭此页。</p>"
    } else {
        "<!doctype html><meta charset=utf-8><p>未完成授权，可以关闭此页。</p>"
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

fn handle_oauth_stream(mut stream: std::net::TcpStream) -> Result<(), String> {
    stream.set_nonblocking(false).ok();
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]);
    let path = req
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .unwrap_or("");
    let code = match oauth_code_from_callback(path) {
        Ok(code) => code,
        Err(e) => {
            write_oauth_html(&mut stream, false);
            return Err(e);
        }
    };
    match complete_oauth_with_code(&ReqwestTickTick::manual(), &code) {
        Ok(()) => {
            write_oauth_html(&mut stream, true);
            clear_oauth_last_error();
            Ok(())
        }
        Err(e) => {
            write_oauth_html(&mut stream, false);
            set_oauth_last_error(public_oauth_error(&e));
            Err(e)
        }
    }
}

pub trait TickTickHttp {
    fn get_json(&self, url: &str, bearer: Option<&str>) -> Result<String, String>;
    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<String, String>;
}

pub fn pkce_verifier() -> String {
    let mut bytes = [0u8; 32];
    let _ = getrandom::getrandom(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn pkce_challenge(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
}

pub fn parse_token_response(json: &str) -> Result<(String, Option<String>), String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("token json: {e}"))?;
    let access = value
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "token json: missing access_token".to_string())?
        .to_string();
    let refresh = value
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Ok((access, refresh))
}

pub fn cache_is_fresh(fetched_at_max: Option<i64>, now: i64) -> bool {
    fetched_at_max.is_some_and(|t| now.saturating_sub(t) <= CACHE_FRESH_SECS)
}

pub fn try_begin_ticktick_sync() -> bool {
    TICKTICK_SYNCING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

pub fn end_ticktick_sync() {
    TICKTICK_SYNCING.store(false, Ordering::SeqCst);
}

/// The four roles a TickTick project or column can carry. Anything else —
/// including `"ignore"` and an unset key — means "not judged".
pub fn role_from_key(role: &str) -> Option<ListRole> {
    match role {
        "mainline" => Some(ListRole::Mainline),
        "side" => Some(ListRole::Side),
        "longterm" => Some(ListRole::Longterm),
        "chore" => Some(ListRole::Chore),
        _ => None,
    }
}

pub fn project_role(
    map: &std::collections::BTreeMap<String, String>,
    project_id: &str,
) -> Option<ListRole> {
    role_from_key(map.get(project_id)?.as_str())
}

fn role_from_sql(s: &str) -> ListRole {
    match s {
        "mainline" => ListRole::Mainline,
        "side" => ListRole::Side,
        "longterm" => ListRole::Longterm,
        "chore" => ListRole::Chore,
        _ => ListRole::Custom,
    }
}

pub fn replace_ticktick_cache(
    conn: &rusqlite::Connection,
    rows: &[TimedTask],
    fetched_at: i64,
) -> Result<(), crate::db_error::DbOpError> {
    conn.execute("DELETE FROM ticktick_cache", [])
        .map_err(crate::db_error::map_rusqlite)?;
    for row in rows {
        conn.execute(
            "INSERT INTO ticktick_cache (id, project_id, title, role, start, end, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                row.id,
                "",
                row.title,
                crate::db::list_role_sql(row.role),
                row.start,
                row.end,
                fetched_at
            ],
        )
        .map_err(crate::db_error::map_rusqlite)?;
    }
    mark_ticktick_fetched(conn, fetched_at)?;
    Ok(())
}

fn mark_ticktick_fetched(
    conn: &rusqlite::Connection,
    fetched_at: i64,
) -> Result<(), crate::db_error::DbOpError> {
    conn.execute(
        "INSERT INTO app_meta (key, value) VALUES ('ticktick_fetched_at', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![fetched_at.to_string()],
    )
    .map_err(crate::db_error::map_rusqlite)?;
    Ok(())
}

pub fn load_ticktick_cache(
    conn: &rusqlite::Connection,
) -> Result<Vec<TimedTask>, crate::db_error::DbOpError> {
    let mut stmt = conn
        .prepare("SELECT id, title, role, start, end FROM ticktick_cache")
        .map_err(crate::db_error::map_rusqlite)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(TimedTask {
                id: r.get(0)?,
                title: r.get(1)?,
                role: role_from_sql(&r.get::<_, String>(2)?),
                start: r.get(3)?,
                end: r.get(4)?,
                done: false,
            })
        })
        .map_err(crate::db_error::map_rusqlite)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(crate::db_error::map_rusqlite)?);
    }
    Ok(out)
}

pub fn cache_fetched_at_max(
    conn: &rusqlite::Connection,
) -> Result<Option<i64>, crate::db_error::DbOpError> {
    conn.query_row("SELECT MAX(fetched_at) FROM ticktick_cache", [], |r| r.get(0))
        .map_err(crate::db_error::map_rusqlite)
}

pub fn ticktick_fetched_at(
    conn: &rusqlite::Connection,
) -> Result<Option<i64>, crate::db_error::DbOpError> {
    let cache = cache_fetched_at_max(conn)?;
    let meta: Option<String> = conn
        .query_row(
            "SELECT value FROM app_meta WHERE key = 'ticktick_fetched_at'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(crate::db_error::map_rusqlite)?;
    let meta_ts = meta.and_then(|s| s.parse().ok());
    Ok(match (cache, meta_ts) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    })
}

#[derive(Clone, Debug, Deserialize)]
struct OpenProject {
    id: String,
    #[serde(default)]
    name: String,
}

/// A project as the settings tree needs it: identity plus its columns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeColumn {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeProject {
    pub id: String,
    pub name: String,
    pub columns: Vec<TreeColumn>,
}

/// The whole TickTick structure, cached so 设置 can render it instantly
/// instead of blocking on N+1 requests every time the tab opens.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickTickTree {
    pub fetched_at: i64,
    pub projects: Vec<TreeProject>,
}

pub const TREE_META_KEY: &str = "ticktick_tree";

pub fn load_tree(conn: &rusqlite::Connection) -> Result<Option<TickTickTree>, String> {
    let raw = conn
        .query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            rusqlite::params![TREE_META_KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(raw.and_then(|r| serde_json::from_str(&r).ok()))
}

pub fn store_tree(conn: &rusqlite::Connection, tree: &TickTickTree) -> Result<(), String> {
    let json = serde_json::to_string(tree).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![TREE_META_KEY, json],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

struct FetchedProject {
    id: String,
    name: String,
    columns: Vec<TreeColumn>,
    tasks: Vec<OpenTask>,
}

/// One `/project` call plus one `/project/{id}/data` per project. The data
/// call returns columns and tasks together, so building the tree costs
/// nothing extra during a sync.
fn fetch_all_projects(
    http: &impl TickTickHttp,
    access: &str,
) -> Result<Vec<FetchedProject>, String> {
    let json = http.get_json(&format!("{TICKTICK_API}/project"), Some(access))?;
    if json.trim() == "429" || json.contains("\"errorCode\":429") {
        return Err("429".into());
    }
    let projects: Vec<OpenProject> =
        serde_json::from_str(&json).map_err(|e| format!("projects json: {e}"))?;

    let mut out = Vec::with_capacity(projects.len());
    for project in projects {
        match fetch_one_project_data(http, access, &project) {
            Ok(fetched) => out.push(fetched),
            Err(e) if e == "429" => return Err("429".into()),
            Err(_) => {
                // Keep the list so 设置 still shows every project; columns
                // and tasks can fill in on a later sync.
                out.push(FetchedProject {
                    id: project.id,
                    name: project.name,
                    columns: Vec::new(),
                    tasks: Vec::new(),
                });
            }
        }
    }
    Ok(out)
}

fn fetch_one_project_data(
    http: &impl TickTickHttp,
    access: &str,
    project: &OpenProject,
) -> Result<FetchedProject, String> {
    let data = http.get_json(
        &format!("{TICKTICK_API}/project/{}/data", project.id),
        Some(access),
    )?;
    if data.trim_start().starts_with("429") {
        return Err("429".into());
    }
    // Columns are optional: a plain list has none.
    let columns = parse_open_project_columns(&data)
        .unwrap_or_default()
        .into_iter()
        .map(|c| TreeColumn {
            id: c.id,
            name: c.name,
        })
        .collect();
    let tasks = parse_open_project_data_tasks(&data)?;
    Ok(FetchedProject {
        id: project.id.clone(),
        name: project.name.clone(),
        columns,
        tasks,
    })
}

pub fn tree_from_fetched(fetched: &[FetchedProject], now: i64) -> TickTickTree {
    TickTickTree {
        fetched_at: now,
        projects: fetched
            .iter()
            .map(|p| TreeProject {
                id: p.id.clone(),
                name: p.name.clone(),
                columns: p.columns.clone(),
            })
            .collect(),
    }
}

/// Fetches the structure without touching the task cache.
pub fn fetch_tree(
    http: &impl TickTickHttp,
    access: &str,
    now: i64,
) -> Result<TickTickTree, String> {
    let fetched = fetch_all_projects(http, access)?;
    Ok(tree_from_fetched(&fetched, now))
}

pub fn sync_projects(
    http: &impl TickTickHttp,
    conn: &rusqlite::Connection,
    maps: &TickTickRoleMaps,
    access: &str,
    now: i64,
) -> Result<usize, String> {
    let fetched = fetch_all_projects(http, access)?;
    let listed: Vec<String> = fetched.iter().map(|p| p.id.clone()).collect();
    let targets = sync_target_project_ids(&maps.project_roles, &maps.column_roles, &listed);
    let tz = FixedOffset::east_opt(0).unwrap();
    let mut timed = Vec::new();
    for project in &fetched {
        if !targets.iter().any(|id| id == &project.id) {
            continue;
        }
        for task in &project.tasks {
            let project_id = task.project_id.as_deref().unwrap_or(project.id.as_str());
            let Some(role) = mapped_task_role(
                project_id,
                task.column_id.as_deref(),
                &maps.project_roles,
                &maps.column_roles,
            ) else {
                continue;
            };
            if let Some(row) = open_task_to_timed(task, role, &tz) {
                timed.push(row);
            }
        }
    }
    replace_ticktick_cache(conn, &timed, now).map_err(|e| format!("{e:?}"))?;
    store_tree(conn, &tree_from_fetched(&fetched, now))?;
    Ok(timed.len())
}

/// Slot-start refresh must fail closed fast so sampling is not blocked.
pub const SLOT_START_HTTP_TIMEOUT_SECS: u64 = 2;
/// Manual 同步任务 / OAuth: TickTick's `/project/{id}/data` is often slower.
pub const MANUAL_HTTP_TIMEOUT_SECS: u64 = 10;

pub struct ReqwestTickTick {
    timeout_secs: u64,
}

impl ReqwestTickTick {
    pub fn slot_start() -> Self {
        Self {
            timeout_secs: SLOT_START_HTTP_TIMEOUT_SECS,
        }
    }

    pub fn manual() -> Self {
        Self {
            timeout_secs: MANUAL_HTTP_TIMEOUT_SECS,
        }
    }
}

impl TickTickHttp for ReqwestTickTick {
    fn get_json(&self, url: &str, bearer: Option<&str>) -> Result<String, String> {
        ticktick_request("GET", url, bearer, None, self.timeout_secs)
    }

    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<String, String> {
        ticktick_request("POST", url, None, Some(form), self.timeout_secs)
    }
}

fn ticktick_request(
    method: &str,
    url: &str,
    bearer: Option<&str>,
    form: Option<&[(&str, &str)]>,
    timeout_secs: u64,
) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = match method {
        "POST" => client.post(url),
        _ => client.get(url),
    };
    if let Some(token) = bearer {
        req = req.bearer_auth(token);
    }
    if let Some(form) = form {
        req = req.form(form);
    }
    let resp = req.send().map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().map_err(|e| e.to_string())?;
    if status.as_u16() == 429 {
        return Err("429".into());
    }
    if !status.is_success() {
        return Err(format!("http {status}"));
    }
    Ok(text)
}

pub fn ticktick_backoff_until(conn: &Connection) -> Result<Option<i64>, crate::db_error::DbOpError> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM app_meta WHERE key = 'ticktick_backoff_until'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(crate::db_error::map_rusqlite)?;
    Ok(value.and_then(|s| s.parse().ok()))
}

pub fn set_ticktick_backoff(
    conn: &Connection,
    until: i64,
) -> Result<(), crate::db_error::DbOpError> {
    conn.execute(
        "INSERT INTO app_meta (key, value) VALUES ('ticktick_backoff_until', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![until.to_string()],
    )
    .map_err(crate::db_error::map_rusqlite)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use gamelife_core::ListRole;
    use rusqlite::Connection;
    use std::collections::BTreeMap;

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
            column_id: None,
        };
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        let got = open_task_to_timed(&t, ListRole::Mainline, &tz).unwrap();
        assert_eq!(got.role, ListRole::Chore);
    }

    #[test]
    fn pkce_challenge_is_s256_unpadded() {
        let chal = pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(chal, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn cache_freshness_thirty_minutes() {
        assert!(cache_is_fresh(Some(1000), 1000 + 30 * 60));
        assert!(cache_is_fresh(Some(1000), 1000 + 30 * 60 - 1));
        assert!(!cache_is_fresh(Some(1000), 1000 + 30 * 60 + 1));
        assert!(!cache_is_fresh(None, 10));
    }

    #[test]
    fn empty_sync_still_marks_cache_fresh() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        replace_ticktick_cache(&conn, &[], 1_000).unwrap();
        let fetched = ticktick_fetched_at(&conn).unwrap();
        assert_eq!(fetched, Some(1_000));
        assert!(cache_is_fresh(fetched, 1_000 + 30 * 60 - 1));
        assert!(!cache_is_fresh(fetched, 1_000 + 30 * 60 + 1));
    }

    #[test]
    fn ignore_role_excludes_project() {
        let mut map = BTreeMap::new();
        map.insert("p1".into(), "ignore".into());
        map.insert("p2".into(), "mainline".into());
        assert!(project_role(&map, "p1").is_none());
        assert_eq!(project_role(&map, "p2"), Some(ListRole::Mainline));
    }

    #[test]
    fn parse_project_data_reads_columns_and_task_column_id() {
        let json = r#"{
            "tasks":[{"id":"1","title":"HDP","status":0,"columnId":"col-main","startDate":"2026-09-13T02:00:00+0000","dueDate":"2026-09-13T03:00:00+0000"}],
            "columns":[{"id":"col-main","name":"主线组","sortOrder":1}]
        }"#;
        let tasks = parse_open_project_data_tasks(json).unwrap();
        assert_eq!(tasks[0].column_id.as_deref(), Some("col-main"));
        let columns = parse_open_project_columns(json).unwrap();
        assert_eq!(columns.len(), 1);
        assert_eq!(columns[0].id, "col-main");
        assert_eq!(columns[0].name, "主线组");
    }

    #[test]
    fn sync_targets_cover_roled_projects_and_column_owners() {
        let mut project_roles = BTreeMap::new();
        project_roles.insert("p1".into(), "mainline".into());
        project_roles.insert("p2".into(), "ignore".into());
        let mut column_roles = BTreeMap::new();
        // p3 has no project role, but one of its columns does.
        column_roles.insert(column_role_key("p3", "col-a"), "side".into());
        let listed = vec!["p1".into(), "p2".into(), "p3".into(), "p4".into()];

        assert_eq!(
            sync_target_project_ids(&project_roles, &column_roles, &listed),
            vec!["p1".to_string(), "p3".to_string()]
        );
        // A project the API no longer returns is dropped, not invented.
        assert!(sync_target_project_ids(&BTreeMap::new(), &BTreeMap::new(), &listed).is_empty());
    }

    #[test]
    fn column_role_beats_project_role() {
        let mut project_roles = BTreeMap::new();
        project_roles.insert("p1".into(), "side".into());
        let mut column_roles = BTreeMap::new();
        column_roles.insert(column_role_key("p1", "col-main"), "mainline".into());

        // The marked column wins…
        assert_eq!(
            mapped_task_role("p1", Some("col-main"), &project_roles, &column_roles),
            Some(ListRole::Mainline)
        );
        // …an unmarked column falls back to the project…
        assert_eq!(
            mapped_task_role("p1", Some("col-other"), &project_roles, &column_roles),
            Some(ListRole::Side)
        );
        // …and a task with no column uses the project too.
        assert_eq!(
            mapped_task_role("p1", None, &project_roles, &column_roles),
            Some(ListRole::Side)
        );
    }

    #[test]
    fn column_role_key_does_not_collide_across_projects() {
        let project_roles = BTreeMap::new();
        let mut column_roles = BTreeMap::new();
        column_roles.insert(column_role_key("p1", "today"), "mainline".into());

        assert_eq!(
            mapped_task_role("p1", Some("today"), &project_roles, &column_roles),
            Some(ListRole::Mainline)
        );
        assert_eq!(
            mapped_task_role("p2", Some("today"), &project_roles, &column_roles),
            None
        );
    }

    #[test]
    fn sync_with_fake_http_writes_cache() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        struct Fake;
        impl TickTickHttp for Fake {
            fn get_json(&self, url: &str, _b: Option<&str>) -> Result<String, String> {
                if url.ends_with("/open/v1/project") {
                    return Ok(r#"[{"id":"p2","name":"科研"}]"#.into());
                }
                Ok(
                    r#"{"tasks":[{"id":"1","title":"HDP","status":0,"startDate":"2026-09-13T02:00:00+0000","dueDate":"2026-09-13T03:00:00+0000"}]}"#
                        .into(),
                )
            }
            fn post_form(&self, _u: &str, _f: &[(&str, &str)]) -> Result<String, String> {
                Err("no write".into())
            }
        }
        let mut roles = BTreeMap::new();
        roles.insert("p2".into(), "mainline".into());
        let maps = TickTickRoleMaps {
            project_roles: roles,
            column_roles: BTreeMap::new(),
        };
        let n = sync_projects(&Fake, &conn, &maps, "tok", 1_000).unwrap();
        assert_eq!(n, 1);
        let rows = load_ticktick_cache(&conn).unwrap();
        assert_eq!(rows[0].id, "tt-1");

        // The same fetch also refreshes the cached tree the settings page reads.
        let tree = load_tree(&conn).unwrap().expect("tree cached");
        assert_eq!(tree.fetched_at, 1_000);
        assert_eq!(tree.projects.len(), 1);
        assert_eq!(tree.projects[0].name, "科研");
    }

    #[test]
    fn sync_keeps_other_projects_when_one_data_call_fails() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        struct Fake;
        impl TickTickHttp for Fake {
            fn get_json(&self, url: &str, _b: Option<&str>) -> Result<String, String> {
                if url.ends_with("/open/v1/project") {
                    return Ok(
                        r#"[{"id":"p1","name":"慢"},{"id":"p2","name":"快"}]"#.into(),
                    );
                }
                if url.contains("/p1/data") {
                    return Err("operation timed out".into());
                }
                Ok(r#"{"tasks":[{"id":"9","title":"HDP","status":0,"startDate":"2026-09-13T02:00:00+0000","dueDate":"2026-09-13T03:00:00+0000"}],"columns":[{"id":"c","name":"组"}]}"#.into())
            }
            fn post_form(&self, _u: &str, _f: &[(&str, &str)]) -> Result<String, String> {
                Err("no write".into())
            }
        }
        let mut roles = BTreeMap::new();
        roles.insert("p2".into(), "mainline".into());
        let maps = TickTickRoleMaps {
            project_roles: roles,
            column_roles: BTreeMap::new(),
        };
        let n = sync_projects(&Fake, &conn, &maps, "tok", 2_000).unwrap();
        assert_eq!(n, 1);
        let tree = load_tree(&conn).unwrap().expect("tree cached");
        assert_eq!(
            tree.projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["慢", "快"]
        );
        assert!(tree.projects[0].columns.is_empty());
        assert_eq!(tree.projects[1].columns.len(), 1);
    }

    #[test]
    fn sync_aborts_on_429_without_writing_a_partial_tree() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        struct Fake;
        impl TickTickHttp for Fake {
            fn get_json(&self, url: &str, _b: Option<&str>) -> Result<String, String> {
                if url.ends_with("/open/v1/project") {
                    return Ok(
                        r#"[{"id":"p1","name":"A"},{"id":"p2","name":"B"}]"#.into(),
                    );
                }
                if url.contains("/p2/data") {
                    return Err("429".into());
                }
                Ok(r#"{"tasks":[]}"#.into())
            }
            fn post_form(&self, _u: &str, _f: &[(&str, &str)]) -> Result<String, String> {
                Err("no write".into())
            }
        }
        let maps = TickTickRoleMaps {
            project_roles: BTreeMap::new(),
            column_roles: BTreeMap::new(),
        };
        let err = sync_projects(&Fake, &conn, &maps, "tok", 3_000).unwrap_err();
        assert_eq!(err, "429");
        assert!(load_tree(&conn).unwrap().is_none());
    }

    #[test]
    fn parse_token_response_reads_access_and_refresh() {
        let (access, refresh) = parse_token_response(
            r#"{"access_token":"a","refresh_token":"r","token_type":"bearer","expires_in":3600}"#,
        )
        .unwrap();
        assert_eq!(access, "a");
        assert_eq!(refresh.as_deref(), Some("r"));
    }

    fn timed(i: i64, start: i64, end: i64) -> TimedTask {
        TimedTask {
            id: format!("tt-{i}"),
            title: format!("t{i}"),
            role: ListRole::Mainline,
            start,
            end,
            done: false,
        }
    }

    #[test]
    fn sync_result_truncated_when_today_overlapping_exceeds_twenty() {
        let twenty: Vec<_> = (0..20).map(|i| timed(i, 0, 3600)).collect();
        let under = sync_result_from_cache(&twenty, 0, 86400);
        assert_eq!(under.count, 20);
        assert!(!under.truncated);

        let twenty_one: Vec<_> = (0..21).map(|i| timed(i, 0, 3600)).collect();
        let over = sync_result_from_cache(&twenty_one, 0, 86400);
        assert_eq!(over.count, 21);
        assert!(over.truncated);

        let none_today: Vec<_> = (0..21).map(|i| timed(i, 100_000, 101_000)).collect();
        let none = sync_result_from_cache(&none_today, 0, 86400);
        assert_eq!(none.count, 21);
        assert!(
            !none.truncated,
            "cache≥21 with 0 overlapping timed tasks must not be truncated"
        );
    }

    #[test]
    fn oauth_begin_preflight_rejects_missing_secret_without_touching_pkce() {
        store_pkce_verifier("keep-me".into());
        let err = oauth_begin_preflight("client-id", false).unwrap_err();
        assert_eq!(err, "missing ticktick client secret");
        assert_eq!(take_pkce_verifier().as_deref(), Some("keep-me"));
        assert!(oauth_begin_preflight("client-id", true).is_ok());
        assert_eq!(
            oauth_begin_preflight("  ", true).unwrap_err(),
            "missing ticktick client id"
        );
    }

    #[test]
    fn incoming_secret_to_store_trims_and_skips_blank() {
        assert_eq!(incoming_secret_to_store(None), None);
        assert_eq!(incoming_secret_to_store(Some("  ")), None);
        assert_eq!(
            incoming_secret_to_store(Some("  abc  ")).as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn oauth_secret_ready_accepts_incoming_or_stored() {
        assert!(!oauth_secret_ready(None, false));
        assert!(oauth_secret_ready(Some("x"), false));
        assert!(oauth_secret_ready(None, true));
    }

    #[test]
    fn oauth_callback_kind_separates_code_from_redirect_setting() {
        assert_eq!(
            oauth_callback_kind("http://127.0.0.1:18789/callback?code=abc"),
            "code"
        );
        assert_eq!(
            oauth_callback_kind("http://localhost:3000/callback"),
            "setting"
        );
        assert_eq!(oauth_callback_kind("https://ticktick.com/"), "invalid");
    }

    #[test]
    fn authorize_url_is_ticktick_oauth_not_the_loopback_redirect() {
        let url = build_authorize_url("client-id", "challenge-xyz");
        assert!(url.starts_with("https://ticktick.com/oauth/authorize?"));
        assert!(url.contains("client_id=client-id"));
        assert!(url.contains("code_challenge=challenge-xyz"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A18789%2Fcallback"));
        assert!(is_ticktick_authorize_url(&url));
        assert!(!is_ticktick_authorize_url(TICKTICK_REDIRECT));
        assert!(!is_ticktick_authorize_url("http://localhost:3000/callback"));
    }

    #[test]
    fn open_in_browser_rejects_the_loopback_redirect() {
        let err = open_in_browser("http://127.0.0.1:18789/callback").unwrap_err();
        assert!(err.contains("unexpected"));
    }
}
