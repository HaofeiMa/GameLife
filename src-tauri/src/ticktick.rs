use chrono::{DateTime, FixedOffset};
use gamelife_core::{align_range, role_from_hashtag, ticktick_snapshot_id, ListRole, TimedTask};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use std::sync::Mutex;
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

pub const TICKTICK_API: &str = "https://api.ticktick.com/open/v1";
pub const TICKTICK_AUTHORIZE: &str = "https://ticktick.com/oauth/authorize";
pub const TICKTICK_TOKEN: &str = "https://ticktick.com/oauth/token";
pub const TICKTICK_REDIRECT: &str = "http://127.0.0.1:18789/callback";
const CACHE_FRESH_SECS: i64 = 300;
static PKCE_VERIFIER: Mutex<Option<String>> = Mutex::new(None);

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

pub fn project_role(
    map: &std::collections::BTreeMap<String, String>,
    project_id: &str,
) -> Option<ListRole> {
    match map.get(project_id)?.as_str() {
        "mainline" => Some(ListRole::Mainline),
        "side" => Some(ListRole::Side),
        "longterm" => Some(ListRole::Longterm),
        "chore" => Some(ListRole::Chore),
        _ => None,
    }
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

#[derive(Clone, Debug, Deserialize)]
struct OpenProject {
    id: String,
    #[serde(default)]
    #[allow(dead_code)]
    name: String,
}

pub fn sync_projects(
    http: &impl TickTickHttp,
    conn: &rusqlite::Connection,
    roles: &std::collections::BTreeMap<String, String>,
    access: &str,
    now: i64,
) -> Result<usize, String> {
    let json = http.get_json(&format!("{TICKTICK_API}/project"), Some(access))?;
    if json.trim() == "429" || json.contains("\"errorCode\":429") {
        return Err("429".into());
    }
    let projects: Vec<OpenProject> =
        serde_json::from_str(&json).map_err(|e| format!("projects json: {e}"))?;
    let tz = FixedOffset::east_opt(0).unwrap();
    let mut timed = Vec::new();
    for project in projects {
        let Some(role) = project_role(roles, &project.id) else {
            continue;
        };
        let data = http.get_json(
            &format!("{TICKTICK_API}/project/{}/data", project.id),
            Some(access),
        )?;
        if data.trim_start().starts_with("429") {
            return Err("429".into());
        }
        let tasks = parse_open_project_data_tasks(&data)?;
        for task in tasks {
            if let Some(row) = open_task_to_timed(&task, role, &tz) {
                timed.push(row);
            }
        }
    }
    replace_ticktick_cache(conn, &timed, now).map_err(|e| format!("{e:?}"))?;
    Ok(timed.len())
}

pub struct ReqwestTickTick;

impl TickTickHttp for ReqwestTickTick {
    fn get_json(&self, url: &str, bearer: Option<&str>) -> Result<String, String> {
        ticktick_request("GET", url, bearer, None)
    }

    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<String, String> {
        ticktick_request("POST", url, None, Some(form))
    }
}

fn ticktick_request(
    method: &str,
    url: &str,
    bearer: Option<&str>,
    form: Option<&[(&str, &str)]>,
) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
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
    fn cache_freshness_five_minutes() {
        assert!(cache_is_fresh(Some(1000), 1299));
        assert!(!cache_is_fresh(Some(1000), 1301));
        assert!(!cache_is_fresh(None, 10));
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
        let n = sync_projects(&Fake, &conn, &roles, "tok", 1_000).unwrap();
        assert_eq!(n, 1);
        let rows = load_ticktick_cache(&conn).unwrap();
        assert_eq!(rows[0].id, "tt-1");
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
}
