//! Cloud backup. Phase one is snapshot-only: build a `VACUUM INTO` copy of the
//! live database, trim it to the configured scope, and upload it. Nothing here
//! is read by the sampler, `judge_slot` or `resolve_slot` — the remote is a
//! copy, never a judgment input.
//!
//! Spec: `docs/superpowers/specs/2026-09-14-cloud-sync-design.md` §5.1, §6.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::{normalize_scope, SyncSettings, SCOPE_SAMPLES};

/// Long enough for a multi-megabyte snapshot on a slow link, short enough that
/// the on-exit sync cannot wedge the tray.
pub const SYNC_TIMEOUT_SECS: u64 = 30;

#[derive(Debug)]
pub enum SyncError {
    Io(String),
    Db(String),
    Transport(String),
    Auth,
    Remote(u16),
    Config(String),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Io(m) => write!(f, "io: {m}"),
            SyncError::Db(m) => write!(f, "db: {m}"),
            SyncError::Transport(m) => write!(f, "transport: {m}"),
            SyncError::Auth => write!(f, "远端认证失败"),
            SyncError::Remote(code) => write!(f, "远端返回 {code}"),
            SyncError::Config(m) => write!(f, "配置: {m}"),
        }
    }
}

impl std::error::Error for SyncError {}

#[derive(Debug, Clone)]
pub struct SnapshotReport {
    pub bytes: u64,
    pub tables: BTreeMap<String, i64>,
}

/// Tables the snapshot carries and reports on. This list drives `count_tables`
/// only — it does not by itself decide what ends up in the file. `VACUUM INTO`
/// copies everything; what is removed is `NEVER_SYNCED_TABLES` plus the scope
/// trim.
const SYNCED_TABLES: &[&str] = &[
    "slots",
    "ledger",
    "days",
    "app_day_stats",
    "host_day_stats",
    "wishes",
    "redemptions",
    "entertainment_sessions",
    "freeze_uses",
    "misclassification_reports",
    "policy_versions",
    "samples",
    // Local to a device — never merged — but it rides in the snapshot so a
    // restore does not lose rewards that were earned and not yet settled.
    "pending_rewards",
];

/// §4「永不」: rows that must not leave the machine at any scope. `VACUUM INTO`
/// copies the whole database, so these are deleted from the copy by name.
/// `app_meta` is handled separately: it is trimmed to `device_id` alone.
const NEVER_SYNCED_TABLES: &[&str] = &["heartbeat", "ticktick_cache", "task_lists", "tasks"];

/// Build a consistent snapshot of `live_db` at `dest`, trimmed to `scope`.
///
/// Order matters and cannot be inverted: the live database is copied first and
/// only the **copy** is trimmed. `aggregate` (the default, and the fallback for
/// anything unrecognised) drops `samples` entirely and nulls the capture
/// artifacts on `slots`, because `samples` stores window titles, URLs and
/// document paths verbatim — redaction happens in the AI layer, not on write.
pub fn build_snapshot(
    live_db: &Path,
    dest: &Path,
    scope: &str,
) -> Result<SnapshotReport, SyncError> {
    // `VACUUM INTO` refuses an existing destination.
    if dest.exists() {
        std::fs::remove_file(dest)
            .map_err(|e| SyncError::Io(format!("remove stale snapshot: {e}")))?;
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SyncError::Io(format!("mkdir: {e}")))?;
    }

    {
        let live = crate::db::open(live_db).map_err(|e| SyncError::Db(format!("{e:?}")))?;
        crate::db::migrate(&live).map_err(|e| SyncError::Db(format!("{e:?}")))?;
        let target = dest.to_string_lossy().to_string();
        live.execute("VACUUM INTO ?1", params![target])
            .map_err(|e| SyncError::Db(format!("vacuum into: {e}")))?;
    }

    let snap = crate::db::open(dest).map_err(|e| SyncError::Db(format!("{e:?}")))?;
    // `VACUUM INTO` copies the entire database, so the §4「永不」tables have to
    // be removed from the copy by name. They are not merely uncounted: leaving
    // them in would put TickTick task titles and this device's sync bookkeeping
    // into a file that gets uploaded.
    for table in NEVER_SYNCED_TABLES {
        let exists: i64 = snap
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table],
                |r| r.get(0),
            )
            .map_err(|e| SyncError::Db(format!("table probe: {e}")))?;
        if exists == 0 {
            continue;
        }
        snap.execute(&format!("DELETE FROM {table}"), [])
            .map_err(|e| SyncError::Db(format!("trim {table}: {e}")))?;
    }
    // `app_meta` is kept for `device_id` alone — see `NEVER_SYNCED_TABLES`.
    snap.execute(
        "DELETE FROM app_meta WHERE key <> ?1",
        params![crate::db::DEVICE_ID_KEY],
    )
    .map_err(|e| SyncError::Db(format!("trim app_meta: {e}")))?;
    if normalize_scope(scope) != SCOPE_SAMPLES {
        snap.execute_batch(
            "DELETE FROM samples;
             UPDATE slots SET screenshot_path = NULL, capture_context_json = NULL;",
        )
        .map_err(|e| SyncError::Db(format!("trim snapshot: {e}")))?;
    }
    snap.execute_batch("VACUUM;")
        .map_err(|e| SyncError::Db(format!("compact snapshot: {e}")))?;

    let tables = count_tables(&snap)?;
    let bytes = std::fs::metadata(dest)
        .map_err(|e| SyncError::Io(format!("stat snapshot: {e}")))?
        .len();
    Ok(SnapshotReport { bytes, tables })
}

/// Table names come from `SYNCED_TABLES`, so the `format!` is not injection.
/// A table missing from `sqlite_master` is skipped rather than fatal, so a
/// future schema change can never break sync.
fn count_tables(conn: &Connection) -> Result<BTreeMap<String, i64>, SyncError> {
    let mut out = BTreeMap::new();
    for table in SYNCED_TABLES {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table],
                |r| r.get(0),
            )
            .map_err(|e| SyncError::Db(format!("table probe: {e}")))?;
        if exists == 0 {
            continue;
        }
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .map_err(|e| SyncError::Db(format!("count {table}: {e}")))?;
        out.insert((*table).to_string(), n);
    }
    Ok(out)
}

/// Object-safe so the orchestration layer can hold either backend.
pub trait RemoteTarget: Send + Sync {
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), SyncError>;
    fn get(&self, path: &str) -> Result<Vec<u8>, SyncError>;
    fn list(&self, prefix: &str) -> Result<Vec<String>, SyncError>;
    fn delete(&self, path: &str) -> Result<(), SyncError>;
}

/// Percent-encode one path segment. `/` is a separator and must survive, so
/// callers encode per segment rather than over the whole path.
fn encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn webdav_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let encoded: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(encode_segment)
        .collect();
    format!("{base}/{}", encoded.join("/"))
}

pub struct WebDavTarget {
    pub base_url: String,
    pub username: String,
    pub password: String,
    client: reqwest::blocking::Client,
}

impl WebDavTarget {
    pub fn new(
        base_url: String,
        username: String,
        password: String,
    ) -> Result<Self, SyncError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(SYNC_TIMEOUT_SECS))
            .user_agent(crate::vision::USER_AGENT)
            .build()
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        Ok(Self {
            base_url,
            username,
            password,
            client,
        })
    }

    fn auth(&self, req: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
        if self.username.is_empty() {
            req
        } else {
            req.basic_auth(&self.username, Some(&self.password))
        }
    }

    fn send(
        &self,
        req: reqwest::blocking::RequestBuilder,
    ) -> Result<reqwest::blocking::Response, SyncError> {
        let resp = self
            .auth(req)
            .send()
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        check_status(resp)
    }
}

fn check_status(
    resp: reqwest::blocking::Response,
) -> Result<reqwest::blocking::Response, SyncError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    match status.as_u16() {
        401 | 403 => Err(SyncError::Auth),
        code => Err(SyncError::Remote(code)),
    }
}

impl RemoteTarget for WebDavTarget {
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), SyncError> {
        let url = webdav_url(&self.base_url, path);
        self.send(self.client.put(&url).body(bytes.to_vec()))?;
        Ok(())
    }

    fn get(&self, path: &str) -> Result<Vec<u8>, SyncError> {
        let url = webdav_url(&self.base_url, path);
        let resp = self.send(self.client.get(&url))?;
        resp.bytes()
            .map(|b| b.to_vec())
            .map_err(|e| SyncError::Transport(e.to_string()))
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, SyncError> {
        let url = webdav_url(&self.base_url, prefix);
        let method = reqwest::Method::from_bytes(b"PROPFIND")
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        let resp = self.send(self.client.request(method, &url).header("Depth", "1"))?;
        let body = resp
            .text()
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        Ok(parse_propfind_hrefs(&body, &self.base_url, prefix))
    }

    fn delete(&self, path: &str) -> Result<(), SyncError> {
        let url = webdav_url(&self.base_url, path);
        let resp = self
            .auth(self.client.delete(&url))
            .send()
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        if resp.status().as_u16() == 404 {
            return Ok(());
        }
        check_status(resp)?;
        Ok(())
    }
}

/// Pull the `href`s out of a `multistatus` body and reduce them to paths
/// relative to `base`. The collection itself and anything outside `prefix` are
/// dropped, so a server that answers with the whole tree cannot inflate the
/// result.
pub fn parse_propfind_hrefs(body: &str, base: &str, prefix: &str) -> Vec<String> {
    let base = base.trim_end_matches('/');
    let mut out = Vec::new();
    for raw in extract_tags(body, "href") {
        let decoded = percent_decode(&raw);
        let rel = decoded
            .strip_prefix(base)
            .unwrap_or(&decoded)
            .trim_start_matches('/');
        let rel = rel.trim_end_matches('/');
        if rel.is_empty() || rel == prefix.trim_end_matches('/') {
            continue;
        }
        if !rel.starts_with(prefix.trim_end_matches('/')) {
            continue;
        }
        out.push(rel.to_string());
    }
    out.sort();
    out.dedup();
    out
}

/// Flat scan for `local`-named elements. A `multistatus` body is not worth an
/// XML parser, but the namespace prefix must still be handled — a server is
/// free to answer with `<d:href>` or `<href>`, and both are legal.
fn extract_tags(body: &str, local: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let Some(lt) = rest.find('<') else { break };
        let Some(gt_rel) = rest[lt..].find('>') else { break };
        let gt = lt + gt_rel;
        let tag = &rest[lt + 1..gt];
        let text_start = gt + 1;
        let text_end = rest[text_start..]
            .find('<')
            .map(|p| text_start + p)
            .unwrap_or(rest.len());
        if local_name(tag) == local {
            let text = rest[text_start..text_end].trim();
            if !text.is_empty() {
                out.push(text.to_string());
            }
        }
        rest = &rest[text_end..];
    }
    out
}

/// `d:href` → `href`, `/d:href` → `href`, `d:response` → `response`.
fn local_name(tag: &str) -> &str {
    let t = tag.trim_start_matches('/');
    let t = t.split_whitespace().next().unwrap_or("");
    match t.rsplit_once(':') {
        Some((_, local)) => local,
        None => t,
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---- S3-compatible object storage (Cloudflare R2, B2, MinIO) ---------------
//
// Hand-rolled HMAC-SHA256 and SigV4 on top of the `sha2` dependency that is
// already in the tree. Adding `hmac` / `aws-sigv4` / an XML crate for this
// would be three dependencies for one request signer.

pub fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex_lower(&h.finalize())
}

pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;

    let mut padded = [0u8; BLOCK];
    if key.len() > BLOCK {
        padded[..32].copy_from_slice(&{
            let mut h = Sha256::new();
            h.update(key);
            h.finalize()
        });
    } else {
        padded[..key.len()].copy_from_slice(key);
    }

    let mut inner = Sha256::new();
    let mut outer = Sha256::new();
    for i in 0..BLOCK {
        inner.update([padded[i] ^ 0x36]);
        outer.update([padded[i] ^ 0x5c]);
    }
    inner.update(msg);
    let inner_digest = inner.finalize();
    outer.update(inner_digest);

    let mut out = [0u8; 32];
    out.copy_from_slice(&outer.finalize());
    out
}

/// A signed request's derived material. The caller still has to send the
/// headers — `reqwest` sets `Host` itself, which is why `host` is signed but
/// never set by hand.
pub struct SignedRequest {
    pub authorization: String,
    pub amz_date: String,
    pub payload_hash: String,
}

fn collapse_spaces(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `amz_date` is `YYYYMMDDTHHMMSSZ`; the date stamp is its first eight bytes.
#[allow(clippy::too_many_arguments)]
pub fn sigv4_sign(
    method: &str,
    canonical_uri: &str,
    canonical_query: &str,
    headers: &[(&str, &str)],
    payload: &[u8],
    access_key: &str,
    secret_key: &str,
    region: &str,
    service: &str,
    amz_date: &str,
) -> SignedRequest {
    let payload_hash = sha256_hex(payload);
    let date_stamp = amz_date.get(..8).unwrap_or(amz_date);

    let mut all: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), collapse_spaces(v)))
        .collect();
    all.push(("x-amz-content-sha256".into(), payload_hash.clone()));
    all.push(("x-amz-date".into(), amz_date.to_string()));
    all.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_headers: String = all
        .iter()
        .map(|(k, v)| format!("{k}:{v}\n"))
        .collect();
    let signed_headers = all
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let scope = format!("{date_stamp}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    let k_date = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex_lower(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    SignedRequest {
        authorization: format!(
            "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
        ),
        amz_date: amz_date.to_string(),
        payload_hash,
    }
}

pub fn host_of(endpoint: &str) -> String {
    endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
}

/// AWS canonical query: every pair encoded, sorted by encoded key, joined by
/// `&`. Sorting matters — an unsorted query is a different signature.
pub fn canonical_query(params: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| (encode_segment(k), encode_segment(v)))
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// `YYYYMMDDTHHMMSSZ`, the only timestamp format SigV4 accepts.
pub fn now_amz_date() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

pub struct S3Target {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    client: reqwest::blocking::Client,
}

impl S3Target {
    pub fn new(
        endpoint: String,
        bucket: String,
        region: String,
        access_key: String,
        secret_key: String,
    ) -> Result<Self, SyncError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(SYNC_TIMEOUT_SECS))
            .user_agent(crate::vision::USER_AGENT)
            .build()
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        Ok(Self {
            endpoint,
            bucket,
            region,
            access_key,
            secret_key,
            client,
        })
    }

    /// Path-style addressing. R2 and MinIO both accept it, and it keeps one
    /// endpoint string working for every bucket.
    fn canonical_uri(&self, path: &str) -> String {
        let encoded: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(encode_segment)
            .collect();
        if encoded.is_empty() {
            format!("/{}", self.bucket)
        } else {
            format!("/{}/{}", self.bucket, encoded.join("/"))
        }
    }

    pub fn url_for(&self, path: &str) -> String {
        format!("{}{}", self.endpoint.trim_end_matches('/'), self.canonical_uri(path))
    }

    fn sign(
        &self,
        method: &str,
        canonical_uri: &str,
        query: &str,
        payload: &[u8],
    ) -> SignedRequest {
        let host = host_of(&self.endpoint);
        sigv4_sign(
            method,
            canonical_uri,
            query,
            &[("host", &host)],
            payload,
            &self.access_key,
            &self.secret_key,
            &self.region,
            "s3",
            &now_amz_date(),
        )
    }

    fn send(
        &self,
        method: reqwest::Method,
        url: &str,
        signed: SignedRequest,
        body: Option<Vec<u8>>,
    ) -> Result<reqwest::blocking::Response, SyncError> {
        let mut req = self
            .client
            .request(method, url)
            .header("x-amz-date", signed.amz_date)
            .header("x-amz-content-sha256", signed.payload_hash)
            .header("authorization", signed.authorization);
        if let Some(b) = body {
            req = req.body(b);
        }
        let resp = req
            .send()
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        check_status(resp)
    }
}

impl RemoteTarget for S3Target {
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), SyncError> {
        let uri = self.canonical_uri(path);
        let url = format!("{}{}", self.endpoint.trim_end_matches('/'), uri);
        let signed = self.sign("PUT", &uri, "", bytes);
        self.send(reqwest::Method::PUT, &url, signed, Some(bytes.to_vec()))?;
        Ok(())
    }

    fn get(&self, path: &str) -> Result<Vec<u8>, SyncError> {
        let uri = self.canonical_uri(path);
        let url = format!("{}{}", self.endpoint.trim_end_matches('/'), uri);
        let signed = self.sign("GET", &uri, "", b"");
        let resp = self.send(reqwest::Method::GET, &url, signed, None)?;
        resp.bytes()
            .map(|b| b.to_vec())
            .map_err(|e| SyncError::Transport(e.to_string()))
    }

    /// `ListObjectsV2`, single page. The snapshot directory holds at most a few
    /// dozen objects, so pagination is not worth the cursor.
    fn list(&self, prefix: &str) -> Result<Vec<String>, SyncError> {
        let query = canonical_query(&[("list-type", "2"), ("max-keys", "1000"), ("prefix", prefix)]);
        let uri = self.canonical_uri("");
        let url = format!("{}{}?{query}", self.endpoint.trim_end_matches('/'), uri);
        let signed = self.sign("GET", &uri, &query, b"");
        let resp = self.send(reqwest::Method::GET, &url, signed, None)?;
        let body = resp
            .text()
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        Ok(extract_tags(&body, "Key")
            .into_iter()
            .map(|k| xml_unescape(&k))
            .collect())
    }

    fn delete(&self, path: &str) -> Result<(), SyncError> {
        let uri = self.canonical_uri(path);
        let url = format!("{}{}", self.endpoint.trim_end_matches('/'), uri);
        let signed = self.sign("DELETE", &uri, "", b"");
        let resp = self
            .client
            .request(reqwest::Method::DELETE, &url)
            .header("x-amz-date", signed.amz_date)
            .header("x-amz-content-sha256", signed.payload_hash)
            .header("authorization", signed.authorization)
            .send()
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        if resp.status().as_u16() == 404 {
            return Ok(());
        }
        check_status(resp)?;
        Ok(())
    }
}

// ---- Device registry and orchestration -------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEntry {
    pub device_id: String,
    pub label: String,
    pub platform: String,
    pub last_seen: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    pub at: i64,
    pub snapshot_bytes: u64,
    pub devices: Vec<DeviceEntry>,
}

/// Everything `sync_with_target` needs, injected. Nothing here reaches for
/// `HOME`, a live database handle, or the network beyond `target`, so the whole
/// upload path is testable against an in-memory target.
pub struct SyncContext<'a> {
    pub settings: &'a SyncSettings,
    pub live_db: &'a Path,
    pub scratch: &'a Path,
    pub target: &'a dyn RemoteTarget,
    pub device_id: &'a str,
    pub label: &'a str,
    pub platform: &'a str,
    pub now: i64,
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn app_meta_get(conn: &Connection, key: &str) -> Result<Option<String>, SyncError> {
    conn.query_row(
        "SELECT value FROM app_meta WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| SyncError::Db(format!("app_meta get: {e}")))
}

fn app_meta_set(conn: &Connection, key: &str, value: &str) -> Result<(), SyncError> {
    conn.execute(
        "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| SyncError::Db(format!("app_meta set: {e}")))?;
    Ok(())
}

/// Stable per-installation identifier, created on first use. It is what keeps
/// two machines' snapshots in separate remote directories and — in phase two —
/// what makes settlement ownership decidable.
///
/// The identity itself lives in `db.rs` because migration needs it to stamp
/// `device_id` columns; there is exactly one implementation so the value the
/// remote directory is named after and the value written into rows can never
/// disagree.
pub fn device_id(conn: &Connection) -> Result<String, SyncError> {
    crate::db::local_device_id(conn).map_err(|e| SyncError::Db(format!("device_id: {e:?}")))
}

pub fn device_label(settings: &SyncSettings, device_id: &str) -> String {
    let configured = settings.device_label.trim();
    if !configured.is_empty() {
        return configured.to_string();
    }
    let short = device_id.get(..6).unwrap_or(device_id);
    format!("{} · {short}", std::env::consts::OS)
}

pub fn base_dir(settings: &SyncSettings) -> String {
    let base = settings.remote_path.trim().trim_matches('/');
    if base.is_empty() {
        "gamelife".to_string()
    } else {
        base.to_string()
    }
}

pub fn remote_dir(settings: &SyncSettings, device_id: &str) -> String {
    format!("{}/{}", base_dir(settings), device_id)
}

pub fn update_registry(reg: &mut Vec<DeviceEntry>, me: &DeviceEntry) {
    match reg.iter_mut().find(|d| d.device_id == me.device_id) {
        Some(slot) => *slot = me.clone(),
        None => reg.push(me.clone()),
    }
    reg.sort_by(|a, b| a.device_id.cmp(&b.device_id));
}

pub fn utc_day_stamp(now: i64) -> String {
    chrono::DateTime::from_timestamp(now, 0)
        .map(|d| d.format("%Y%m%d").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn secrets_path() -> Result<std::path::PathBuf, SyncError> {
    crate::keychain::secrets_path().map_err(SyncError::Config)
}

/// Build the transport from settings. Credentials come from `secrets.json` and
/// never from `config.json`.
pub fn target_from_settings(s: &SyncSettings) -> Result<Box<dyn RemoteTarget>, SyncError> {
    let url = s.url.trim();
    if url.is_empty() {
        return Err(SyncError::Config("远端地址未填写".into()));
    }
    if s.target == "s3" {
        if s.bucket.trim().is_empty() {
            return Err(SyncError::Config("S3 需要填写 bucket".into()));
        }
        let secret = crate::keychain::get_in(&secrets_path()?, "sync-s3-secret")
            .map_err(|_| SyncError::Config("缺少 S3 密钥".into()))?;
        let region = if s.region.trim().is_empty() {
            "auto"
        } else {
            s.region.trim()
        };
        return Ok(Box::new(S3Target::new(
            url.to_string(),
            s.bucket.trim().to_string(),
            region.to_string(),
            s.username.trim().to_string(),
            secret,
        )?));
    }
    let password =
        crate::keychain::get_in(&secrets_path()?, "sync-webdav-password").unwrap_or_default();
    Ok(Box::new(WebDavTarget::new(
        url.to_string(),
        s.username.trim().to_string(),
        password,
    )?))
}

/// Upload one snapshot and refresh the device registry.
///
/// The scratch file is removed on every path, success or failure — a leftover
/// half-megabyte database in the temp directory would be both litter and a
/// second copy of the data outside the data root.
pub fn sync_with_target(ctx: &SyncContext) -> Result<SyncOutcome, SyncError> {
    let dir = remote_dir(ctx.settings, ctx.device_id);
    let scratch = ctx.scratch.join("gamelife-sync-snapshot.db");

    let result = (|| {
        let report = build_snapshot(ctx.live_db, &scratch, &ctx.settings.scope)?;
        let bytes = std::fs::read(&scratch)
            .map_err(|e| SyncError::Io(format!("read snapshot: {e}")))?;

        ctx.target.put(&format!("{dir}/latest.db"), &bytes)?;
        ctx.target
            .put(&format!("{dir}/snapshots/{}.db", utc_day_stamp(ctx.now)), &bytes)?;

        let reg_path = format!("{}/devices.json", base_dir(ctx.settings));
        let mut reg: Vec<DeviceEntry> = match ctx.target.get(&reg_path) {
            Ok(raw) => serde_json::from_slice(&raw).unwrap_or_default(),
            Err(SyncError::Remote(404)) => Vec::new(),
            Err(e) => return Err(e),
        };
        let me = DeviceEntry {
            device_id: ctx.device_id.to_string(),
            label: ctx.label.to_string(),
            platform: ctx.platform.to_string(),
            last_seen: ctx.now,
        };
        update_registry(&mut reg, &me);
        let reg_json = serde_json::to_vec_pretty(&reg)
            .map_err(|e| SyncError::Config(format!("registry json: {e}")))?;
        ctx.target.put(&reg_path, &reg_json)?;

        let prefix = format!("{dir}/snapshots");
        let mut names = ctx.target.list(&prefix)?;
        names.sort();
        let keep = ctx.settings.keep_snapshots.max(0) as usize;
        while names.len() > keep {
            ctx.target.delete(&names.remove(0))?;
        }

        Ok(SyncOutcome {
            at: ctx.now,
            snapshot_bytes: report.bytes,
            devices: reg,
        })
    })();

    let _ = std::fs::remove_file(&scratch);
    result
}

/// Production entry point. Records the outcome in `app_meta` so the settings
/// page can render it without a network call, and records `sync_last_at` only
/// on success — a failure should retry on the next tick, not wait out the whole
/// interval.
pub fn sync_now(settings: &SyncSettings) -> Result<SyncOutcome, SyncError> {
    let live = crate::db::app_db_path().ok_or_else(|| SyncError::Config("找不到数据目录".into()))?;
    let conn = crate::db::open(&live).map_err(|e| SyncError::Db(format!("{e:?}")))?;
    crate::db::migrate(&conn).map_err(|e| SyncError::Db(format!("{e:?}")))?;

    let id = device_id(&conn)?;
    let label = device_label(settings, &id);
    let target = target_from_settings(settings)?;
    let scratch = std::env::temp_dir();
    let now = now_secs();

    let ctx = SyncContext {
        settings,
        live_db: &live,
        scratch: &scratch,
        target: target.as_ref(),
        device_id: &id,
        label: &label,
        platform: std::env::consts::OS,
        now,
    };
    let outcome = sync_with_target(&ctx);

    match &outcome {
        Ok(o) => {
            let _ = app_meta_set(&conn, "sync_last_at", &now.to_string());
            if let Ok(json) = serde_json::to_string(o) {
                let _ = app_meta_set(&conn, "sync_last_result", &json);
            }
            let _ = app_meta_set(&conn, "sync_last_error", "");
        }
        Err(e) => {
            let _ = app_meta_set(&conn, "sync_last_error", &e.to_string());
        }
    }
    outcome
}

/// Whether the sampler tick should kick off a sync. Pure, so the interval logic
/// is testable without a clock.
pub fn is_due(last_sync_at: Option<i64>, now: i64, interval_minutes: i64) -> bool {
    if interval_minutes <= 0 {
        return false;
    }
    match last_sync_at {
        None => true,
        Some(last) => now - last >= interval_minutes * 60,
    }
}

pub fn last_sync_at(conn: &Connection) -> Option<i64> {
    app_meta_get(conn, "sync_last_at")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<i64>().ok())
}

/// The sampler loop calls this every `SYNC_CHECK_EVERY_TICKS` ticks. It never
/// returns an error and never blocks — a slow or dead remote must not stretch
/// the 15-second tick, so the upload runs on a detached thread.
///
/// Settings are re-read on every check rather than captured once at startup, so
/// toggling 云端备份 in 设置 takes effect without restarting the app.
pub fn maybe_spawn_sync(conn: &Connection, now: i64) {
    let settings = crate::config::load_settings();
    if !settings.sync.enabled {
        return;
    }
    if !is_due(last_sync_at(conn), now, settings.sync.interval_minutes) {
        return;
    }
    let sync = settings.sync;
    std::thread::spawn(move || {
        if let Err(e) = sync_now(&sync) {
            eprintln!("sync: {e}");
        }
    });
}

/// Best-effort upload on the way out. Bounded, so a dead remote cannot wedge
/// 退出 — the process leaves whether or not the upload lands.
pub fn sync_on_exit(settings: &SyncSettings) -> bool {
    if !settings.enabled {
        return false;
    }
    let sync = settings.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(sync_now(&sync).is_ok());
    });
    rx.recv_timeout(Duration::from_secs(EXIT_SYNC_TIMEOUT_SECS))
        .unwrap_or(false)
}

pub const EXIT_SYNC_TIMEOUT_SECS: u64 = 5;

const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";

/// Write `bytes` to `dest` only after proving they are a healthy SQLite
/// database.
///
/// Order is deliberate. The live-database refusal comes **first**, before the
/// content is even looked at, so there is no path on which a bad payload plus a
/// correct path can overwrite the running database. Validation happens on a
/// `.part` file, and the rename is the only step that can make the result
/// visible — a half-written database is never left where the app might open it.
pub fn restore_from_bytes(bytes: &[u8], dest: &Path) -> Result<(), SyncError> {
    if let Some(live) = crate::db::app_db_path() {
        if dest == live {
            return Err(SyncError::Config(
                "拒绝覆盖正在使用的 gamelife.db".into(),
            ));
        }
    }

    if bytes.len() < SQLITE_HEADER.len() || &bytes[..SQLITE_HEADER.len()] != SQLITE_HEADER {
        return Err(SyncError::Db("不是 SQLite 数据库".into()));
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SyncError::Io(format!("mkdir: {e}")))?;
    }
    let part = dest.with_extension("db.part");
    std::fs::write(&part, bytes).map_err(|e| SyncError::Io(format!("write restore: {e}")))?;

    let verdict = (|| -> Result<(), SyncError> {
        let conn = crate::db::open(&part).map_err(|e| SyncError::Db(format!("{e:?}")))?;
        let ok: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| SyncError::Db(format!("integrity_check: {e}")))?;
        if ok != "ok" {
            return Err(SyncError::Db(format!("integrity_check: {ok}")));
        }
        Ok(())
    })();

    if let Err(e) = verdict {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }

    std::fs::rename(&part, dest).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        SyncError::Io(format!("rename restore: {e}"))
    })
}

/// Fetch another device's latest snapshot and stage it next to the live
/// database. The result is **not** swapped in: the user quits the app and
/// renames it, because the sampler must not have its database pulled out from
/// under it.
pub fn restore_from_target(
    settings: &SyncSettings,
    device_id: &str,
    dest_dir: &Path,
) -> Result<std::path::PathBuf, SyncError> {
    let target = target_from_settings(settings)?;
    let bytes = target.get(&format!("{}/latest.db", remote_dir(settings, device_id)))?;
    let dest = dest_dir.join("gamelife.restored.db");
    restore_from_bytes(&bytes, &dest)?;
    Ok(dest)
}

// ---------------------------------------------------------------------------
// Phase two: multi-device merge
// ---------------------------------------------------------------------------

/// Which device owns the slot at one `(day, slot_start)`, given every device's
/// `(device_id, observed_seconds)` for it.
///
/// The rule has to be **computable identically on every device with no
/// coordination**, because there is no ledger host: each machine decides for
/// itself which slots it is responsible for settling, and if two machines
/// disagreed about ownership they could both try to pay the same slot. So:
/// most `observed_seconds` wins; a tie goes to the lexicographically smallest
/// `device_id`. Both halves are total orders over the same input, so every
/// device derives the same answer from the same merged rows.
///
/// A slot nobody observed (every device reports 0 seconds — the process died)
/// still gets a deterministic owner rather than none: the owner then evaluates
/// its own row, finds it `unobserved`, and pays nothing. Returning "no owner"
/// instead would make an unpaid slot indistinguishable from a slot no device
/// got round to settling yet.
///
/// Returns `""` for an empty slice; callers treat that as "no slot here".
pub fn slot_owner(rows: &[(String, i64)]) -> String {
    rows.iter()
        // Reversed id comparison, because `max_by` keeps the greatest element:
        // among equal durations this makes the *smallest* id the greatest.
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(id, _)| id.clone())
        .unwrap_or_default()
}

/// What one device's most recent readable snapshot can answer for.
///
/// Coverage is a *timestamp*, not a day, because that is the only honest
/// question: a snapshot taken in the middle of D may still be missing D's
/// evening. It answers for D only once it was taken after D ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotCoverage {
    pub device_id: String,
    pub taken_at: i64,
}

/// Default grace period before a day is settled without the stragglers, in
/// hours. Long enough that a laptop left at the office overnight still lands,
/// short enough that a machine that is gone for good stops blocking the ledger.
pub const SETTLE_GRACE_HOURS: i64 = 36;

/// May day `day` be settled yet?
///
/// The ledger is irreversible, so a day is only settled once every registered
/// device has had its say — otherwise a device that comes back tomorrow would
/// find its own slots already paid by someone else, and §3.3's ownership rule
/// would have been applied to a view that was missing half the data.
///
/// Two escapes keep that from becoming a hostage situation:
///
/// - **One device (or none) settles immediately.** That is today's behaviour
///   exactly, and it is what makes phase two invisible until a second device
///   is actually registered.
/// - **`grace_hours` after the day ends, stragglers are abandoned.** Their
///   late data still reaches the statistics view; only the ledger stops
///   waiting. This is the explicitly accepted cost recorded in §3.4.
pub fn day_is_ready(
    day: &str,
    devices: &[DeviceEntry],
    snapshots: &[SnapshotCoverage],
    now: i64,
    grace_hours: i64,
) -> bool {
    if devices.len() <= 1 {
        return true;
    }
    let Ok(date) = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d") else {
        return false;
    };
    let Some(end) = crate::scheduler::local_day_end(&chrono::Local, date) else {
        return false;
    };
    if now >= end.saturating_add(grace_hours.saturating_mul(3600)) {
        return true;
    }
    devices.iter().all(|d| {
        snapshots
            .iter()
            .any(|s| s.device_id == d.device_id && s.taken_at >= end)
    })
}

/// Where the read-only merged view lives. Beside the live database, so the
/// atomic swap is a same-filesystem rename.
pub fn merged_db_path() -> Option<std::path::PathBuf> {
    crate::platform::app_support_dir().map(|dir| dir.join("merged.db"))
}

/// The tables the merged view carries, each with the key that makes it a
/// *view* rather than a pile of rows (§5.2).
///
/// Tables keyed on `device_id` keep every machine's rows, because two machines
/// observing the same fifteen minutes is two facts. Tables with a global key
/// are shared resources — one wallet, one ledger — so duplicates collapse.
const MERGED_TABLES: &[(&str, &[&str])] = &[
    ("slots", &["device_id", "day", "slot_start"]),
    ("samples", &["device_id", "ts"]),
    ("app_day_stats", &["device_id", "day", "app", "bundle_id"]),
    ("host_day_stats", &["device_id", "day", "host"]),
    ("days", &["device_id", "day"]),
    ("policy_versions", &["device_id", "id"]),
    ("misclassification_reports", &["device_id", "id"]),
    ("ledger", &["reward_event_key"]),
    ("wishes", &["id"]),
    ("redemptions", &["redemption_id"]),
    ("entertainment_sessions", &["redemption_id"]),
    ("freeze_uses", &["protected_date"]),
];

/// SQLite's variable limit is at least 999; stay well under it so one
/// `DELETE … WHERE rowid IN (…)` per chunk always binds.
const SLOT_DELETE_CHUNK: usize = 500;

#[derive(Debug, Clone, Default)]
pub struct MergedReport {
    /// Devices whose snapshot went in.
    pub merged: Vec<String>,
    /// Devices that were registered but could not be read, with the reason.
    /// A device being offline is normal and must not abort the rebuild.
    pub skipped: Vec<(String, String)>,
    pub slots_kept: i64,
    pub slots_dropped: i64,
    pub bytes: u64,
}

/// Read the device registry. A missing one is not an error — it means nobody
/// has uploaded yet.
pub fn fetch_registry(
    target: &dyn RemoteTarget,
    settings: &SyncSettings,
) -> Result<Vec<DeviceEntry>, SyncError> {
    let path = format!("{}/devices.json", base_dir(settings));
    match target.get(&path) {
        Ok(raw) => Ok(serde_json::from_slice(&raw).unwrap_or_default()),
        Err(SyncError::Remote(404)) => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

fn db_fail(what: &str, e: rusqlite::Error) -> SyncError {
    SyncError::Db(format!("{what}: {e}"))
}

fn has_table(conn: &Connection, schema: &str, table: &str) -> Result<bool, SyncError> {
    let n: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {schema}.sqlite_master WHERE type = 'table' AND name = ?1"),
            params![table],
            |r| r.get(0),
        )
        .map_err(|e| db_fail("table probe", e))?;
    Ok(n > 0)
}

fn has_column(
    conn: &Connection,
    schema: &str,
    table: &str,
    column: &str,
) -> Result<bool, SyncError> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA {schema}.table_info({table})"))
        .map_err(|e| db_fail("table_info", e))?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| db_fail("table_info", e))?;
    for name in names {
        if name.map_err(|e| db_fail("table_info", e))? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Rebuild the read-only merged view at `dest` from every registered device's
/// `latest.db`. Nothing in the judgment pipeline reads it.
///
/// The build is deliberately schema-deriving rather than schema-declaring: the
/// merged tables are created with `CREATE TABLE … AS SELECT` from the first
/// snapshot that has them, so there is no second set of `CREATE TABLE`
/// statements to keep in step with `db.rs` — which is the whole reason §3.1
/// chose whole-database snapshots over row-level sync.
///
/// `dest` is replaced only after the new database passes `integrity_check`, and
/// a rebuild with nothing readable leaves the previous view alone rather than
/// blanking the statistics page.
pub fn rebuild_merged(
    target: &dyn RemoteTarget,
    settings: &SyncSettings,
    dest: &Path,
    scratch: &Path,
) -> Result<MergedReport, SyncError> {
    let mut devices = fetch_registry(target, settings)?;
    devices.sort_by(|a, b| a.device_id.cmp(&b.device_id));

    let part = dest.with_extension("db.part");
    let _ = std::fs::remove_file(&part);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SyncError::Io(format!("mkdir: {e}")))?;
    }

    let mut report = MergedReport::default();
    {
        let mut conn = crate::db::open(&part).map_err(|e| SyncError::Db(format!("{e:?}")))?;
        for d in &devices {
            let remote = format!("{}/latest.db", remote_dir(settings, &d.device_id));
            let bytes = match target.get(&remote) {
                Ok(b) => b,
                Err(e) => {
                    report.skipped.push((d.device_id.clone(), e.to_string()));
                    continue;
                }
            };
            let src = scratch.join(format!("merged-src-{}.db", d.device_id));
            if let Err(e) = std::fs::write(&src, &bytes) {
                report.skipped.push((d.device_id.clone(), format!("write: {e}")));
                continue;
            }
            let outcome = merge_one_snapshot(&mut conn, &src, &d.device_id);
            let _ = std::fs::remove_file(&src);
            match outcome {
                Ok(()) => report.merged.push(d.device_id.clone()),
                Err(e) => report.skipped.push((d.device_id.clone(), e.to_string())),
            }
        }

        if !report.merged.is_empty() {
            dedupe_merged(&conn)?;
            let (kept, dropped) = filter_slot_owners(&conn)?;
            report.slots_kept = kept;
            report.slots_dropped = dropped;
            index_merged(&conn)?;
        }
    }

    if report.merged.is_empty() {
        let _ = std::fs::remove_file(&part);
        return Ok(report);
    }

    {
        let check = crate::db::open(&part).map_err(|e| SyncError::Db(format!("{e:?}")))?;
        let ok: String = check
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| db_fail("integrity_check", e))?;
        if ok != "ok" {
            let _ = std::fs::remove_file(&part);
            return Err(SyncError::Db(format!("merged integrity_check: {ok}")));
        }
    }

    std::fs::rename(&part, dest).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        SyncError::Io(format!("rename merged: {e}"))
    })?;
    report.bytes = std::fs::metadata(dest)
        .map_err(|e| SyncError::Io(format!("stat merged: {e}")))?
        .len();
    Ok(report)
}

/// Attach one device's snapshot, merge every table it has, and detach again.
/// Each device is its own transaction, so a snapshot that fails halfway leaves
/// no rows behind.
fn merge_one_snapshot(
    conn: &mut Connection,
    src: &Path,
    device_id: &str,
) -> Result<(), SyncError> {
    conn.execute(
        "ATTACH DATABASE ?1 AS src",
        params![src.to_string_lossy().to_string()],
    )
    .map_err(|e| db_fail("attach snapshot", e))?;

    let outcome = (|| -> Result<(), SyncError> {
        let tx = conn.transaction().map_err(|e| db_fail("begin merge", e))?;
        merge_attached(&tx, device_id)?;
        tx.commit().map_err(|e| db_fail("commit merge", e))
    })();

    // Detach whether or not the merge worked, so the next device can attach.
    let _ = conn.execute("DETACH DATABASE src", []);
    outcome
}

fn merge_attached(conn: &Connection, device_id: &str) -> Result<(), SyncError> {
    for (table, _) in MERGED_TABLES {
        if !has_table(conn, "src", table)? {
            continue;
        }
        if !has_column(conn, "src", table, "device_id")? {
            // A snapshot from before T10. Stamp it with the id its own remote
            // directory already claims — never ours, and never via
            // `db::migrate`, which would mint a fresh identity for someone
            // else's database and silently re-attribute their history.
            conn.execute(&format!("ALTER TABLE src.{table} ADD COLUMN device_id TEXT"), [])
                .map_err(|e| db_fail("add device_id", e))?;
            conn.execute(
                &format!("UPDATE src.{table} SET device_id = ?1"),
                params![device_id],
            )
            .map_err(|e| db_fail("stamp device_id", e))?;
        }
        if !has_table(conn, "main", table)? {
            conn.execute(
                &format!("CREATE TABLE main.{table} AS SELECT * FROM src.{table}"),
                [],
            )
            .map_err(|e| db_fail("create merged table", e))?;
        } else {
            conn.execute(&format!("INSERT INTO main.{table} SELECT * FROM src.{table}"), [])
                .map_err(|e| db_fail("insert merged rows", e))?;
        }
    }
    Ok(())
}

/// Collapse each table onto its §5.2 key.
///
/// Device-keyed tables have nothing to collapse — the key contains
/// `device_id`, so the window function is a no-op pass. Globally keyed tables
/// converge on one row. `wishes` takes the newest edit; everything else takes
/// the lowest `rowid`, which keeps the result independent of the order devices
/// happened to merge in.
fn dedupe_merged(conn: &Connection) -> Result<(), SyncError> {
    for (table, key) in MERGED_TABLES {
        if !has_table(conn, "main", table)? {
            continue;
        }
        let order = if *table == "wishes" {
            "COALESCE(updated_at, 0) DESC, rowid ASC"
        } else {
            "rowid ASC"
        };
        conn.execute(
            &format!(
                "DELETE FROM main.{table} WHERE rowid NOT IN (
                   SELECT rowid FROM (
                     SELECT rowid,
                            ROW_NUMBER() OVER (PARTITION BY {} ORDER BY {order}) AS rn
                       FROM main.{table}
                   ) WHERE rn = 1
                 )",
                key.join(", ")
            ),
            [],
        )
        .map_err(|e| db_fail("dedupe merged", e))?;
    }
    Ok(())
}

/// §7.2: for each `(day, slot_start)` only the owner's row survives.
///
/// Ownership is decided by `slot_owner` — the same function the settlement path
/// uses. Writing the rule again as a SQL window function would be two
/// implementations of one rule, and they would drift.
fn filter_slot_owners(conn: &Connection) -> Result<(i64, i64), SyncError> {
    let mut stmt = conn
        .prepare(
            "SELECT rowid, day, slot_start, device_id, COALESCE(observed_seconds, 0)
               FROM main.slots ORDER BY day, slot_start",
        )
        .map_err(|e| db_fail("read merged slots", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| db_fail("read merged slots", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| db_fail("read merged slots", e))?;
    drop(stmt);

    let mut groups: BTreeMap<(String, i64), Vec<(i64, String, i64)>> = BTreeMap::new();
    for (rowid, day, slot_start, device, observed) in rows {
        groups
            .entry((day, slot_start))
            .or_default()
            .push((rowid, device, observed));
    }

    let mut doomed: Vec<i64> = Vec::new();
    let mut kept = 0i64;
    for rows in groups.values() {
        let claims: Vec<(String, i64)> = rows.iter().map(|(_, d, o)| (d.clone(), *o)).collect();
        let owner = slot_owner(&claims);
        for (rowid, device, _) in rows {
            if *device == owner {
                kept += 1;
            } else {
                doomed.push(*rowid);
            }
        }
    }

    for chunk in doomed.chunks(SLOT_DELETE_CHUNK) {
        let placeholders = std::iter::repeat("?")
            .take(chunk.len())
            .collect::<Vec<_>>()
            .join(",");
        let bindings: Vec<&dyn rusqlite::ToSql> =
            chunk.iter().map(|r| r as &dyn rusqlite::ToSql).collect();
        conn.execute(
            &format!("DELETE FROM main.slots WHERE rowid IN ({placeholders})"),
            bindings.as_slice(),
        )
        .map_err(|e| db_fail("drop non-owner slots", e))?;
    }

    Ok((kept, doomed.len() as i64))
}

fn index_merged(conn: &Connection) -> Result<(), SyncError> {
    for (table, key) in MERGED_TABLES {
        if !has_table(conn, "main", table)? {
            continue;
        }
        // The schema qualifier goes on the *index* name, not the table name —
        // `CREATE INDEX … ON main.slots` is a syntax error in SQLite.
        conn.execute(
            &format!(
                "CREATE UNIQUE INDEX IF NOT EXISTS main.merged_{table}_key ON {table}({})",
                key.join(", ")
            ),
            [],
        )
        .map_err(|e| db_fail("merged index", e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SCOPE_AGGREGATE;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::atomic::{AtomicU16, Ordering};
    use std::sync::{Arc, Mutex};

    /// A migrated database holding three titled samples and one slot that
    /// carries a screenshot path and a capture context. No `HOME`, no config.
    fn seed_live_db(path: &Path) {
        let conn = crate::db::open(path).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO samples
               (ts, day, app, title, url, document_path, bundle_id, idle_seconds, locked, paused, secure_input)
             VALUES
               (1, '2026-09-14', 'Google Chrome', 'Bank - Chrome', 'https://bank.example.com', NULL, 'com.google.Chrome', 0, 0, 0, 0),
               (2, '2026-09-14', 'Google Chrome', 'Bank - Chrome', 'https://bank.example.com', NULL, 'com.google.Chrome', 0, 0, 0, 0),
               (3, '2026-09-14', 'Google Chrome', 'Bank - Chrome', 'https://bank.example.com', NULL, 'com.google.Chrome', 0, 0, 0, 0);
             INSERT INTO slots
               (day, slot_start, category, status, screenshot_path, capture_context_json, observed_seconds, credited_core_seconds)
             VALUES
               ('2026-09-14', 1789353000, 'admin', 'final', 'screenshots/x.jpg', '{\"app\":\"Google Chrome\"}', 900, 0);",
        )
        .unwrap();
    }

    fn temp_pair() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live.db");
        let dest = dir.path().join("snap.db");
        seed_live_db(&live);
        (dir, live, dest)
    }

    #[test]
    fn aggregate_scope_drops_samples_and_capture_artifacts() {
        let (_dir, live, dest) = temp_pair();

        let report = build_snapshot(&live, &dest, SCOPE_AGGREGATE).unwrap();

        let conn = crate::db::open(&dest).unwrap();
        let samples: i64 = conn
            .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
            .unwrap();
        assert_eq!(samples, 0);
        let path: Option<String> = conn
            .query_row("SELECT screenshot_path FROM slots LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert!(path.is_none());
        let ctx: Option<String> = conn
            .query_row(
                "SELECT capture_context_json FROM slots LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(ctx.is_none());
        assert_eq!(report.tables["slots"], 1);
        assert_eq!(report.tables["samples"], 0);
        assert!(report.bytes > 0);
    }

    #[test]
    fn samples_scope_keeps_samples() {
        let (_dir, live, dest) = temp_pair();
        let report = build_snapshot(&live, &dest, SCOPE_SAMPLES).unwrap();
        assert_eq!(report.tables["samples"], 3);
    }

    #[test]
    fn an_unknown_scope_behaves_like_aggregate() {
        let (_dir, live, dest) = temp_pair();
        let report = build_snapshot(&live, &dest, "everything").unwrap();
        assert_eq!(report.tables["samples"], 0);
    }

    /// `VACUUM INTO` copies the whole database, so anything §4 lists as
    /// 「永不」 has to be trimmed out of the copy by name. `ticktick_cache`,
    /// `task_lists` and `tasks` hold user-written task titles; `app_meta` holds
    /// this device's sync bookkeeping. None of them belong in a backup, at any
    /// scope.
    #[test]
    fn snapshot_drops_per_device_tables_at_every_scope() {
        let (_dir, live, dest) = temp_pair();
        {
            let conn = crate::db::open(&live).unwrap();
            conn.execute_batch(
                "INSERT INTO ticktick_cache (id, project_id, title, role, start, end, fetched_at)
                   VALUES ('tt-1', 'p', '去买降压药', 'mainline', 1, 2, 3);
                 INSERT INTO tasks (id, list_id, title, done) VALUES ('t-1', 'l', '体检预约', 0);
                 INSERT INTO heartbeat (id, ts) VALUES (1, 1789353000);
                 INSERT INTO app_meta (key, value) VALUES ('sync_last_error', 'boom');",
            )
            .unwrap();
        }

        for scope in [SCOPE_AGGREGATE, SCOPE_SAMPLES] {
            build_snapshot(&live, &dest, scope).unwrap();
            let snap = crate::db::open(&dest).unwrap();
            for table in ["heartbeat", "ticktick_cache", "task_lists", "tasks"] {
                let n: i64 = snap
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                    .unwrap();
                assert_eq!(n, 0, "{table} leaked at scope {scope}");
            }
            let bookkeeping: i64 = snap
                .query_row(
                    "SELECT COUNT(*) FROM app_meta WHERE key <> 'device_id'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(bookkeeping, 0, "app_meta bookkeeping leaked");
            // The id itself is not a secret — it is already the remote
            // directory name — and keeping it lets a restored snapshot stamp
            // its own rows with the device they came from.
            let id: i64 = snap
                .query_row(
                    "SELECT COUNT(*) FROM app_meta WHERE key = 'device_id'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(id, 1, "the snapshot must stay self-identifying");
        }

        let bytes = std::fs::read(&dest).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("去买降压药"));
        assert!(!text.contains("体检预约"));
        assert!(!text.contains("boom"));
    }

    #[test]
    fn snapshot_never_carries_secrets_or_window_titles() {
        let (_dir, live, dest) = temp_pair();
        build_snapshot(&live, &dest, SCOPE_AGGREGATE).unwrap();

        let bytes = std::fs::read(&dest).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("Bank - Chrome"));
        assert!(!text.contains("bank.example.com"));
        assert!(!text.contains("screenshots/"));
        assert!(!text.contains("secrets"));
    }

    #[test]
    fn build_snapshot_overwrites_an_existing_destination() {
        let (_dir, live, dest) = temp_pair();
        std::fs::write(&dest, b"stale").unwrap();
        let report = build_snapshot(&live, &dest, SCOPE_AGGREGATE).unwrap();
        assert!(report.bytes > 100);
    }

    #[test]
    fn the_live_database_is_left_untouched() {
        let (_dir, live, dest) = temp_pair();
        build_snapshot(&live, &dest, SCOPE_AGGREGATE).unwrap();

        let conn = crate::db::open(&live).unwrap();
        let samples: i64 = conn
            .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
            .unwrap();
        assert_eq!(samples, 3);
        let path: Option<String> = conn
            .query_row("SELECT screenshot_path FROM slots LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(path.as_deref(), Some("screenshots/x.jpg"));
    }

    #[test]
    fn snapshot_carries_the_ledger_and_the_shop() {
        let (_dir, live, dest) = temp_pair();
        {
            let conn = crate::db::open(&live).unwrap();
            conn.execute(
                "INSERT INTO ledger (reward_event_key, day, ts, coin_delta, xp_delta)
                 VALUES ('xp_admin:2026-09-14:1789353000', '2026-09-14', 1, 5, 2)",
                [],
            )
            .unwrap();
        }
        let report = build_snapshot(&live, &dest, SCOPE_AGGREGATE).unwrap();
        assert_eq!(report.tables["ledger"], 1);
        assert_eq!(report.tables["wishes"], 6);
    }

    // ---- Task 3: WebDAV -------------------------------------------------

    #[test]
    fn webdav_url_joins_without_doubling_slashes() {
        assert_eq!(
            webdav_url(
                "https://dav.example.com/remote.php/dav/files/me/",
                "gamelife/a.db"
            ),
            "https://dav.example.com/remote.php/dav/files/me/gamelife/a.db"
        );
        assert_eq!(
            webdav_url("https://dav.example.com", "gamelife/a.db"),
            "https://dav.example.com/gamelife/a.db"
        );
    }

    #[test]
    fn webdav_url_percent_encodes_each_segment_but_not_the_separators() {
        assert_eq!(
            webdav_url("https://d.example.com/dav", "gamelife/2026 09/ab.db"),
            "https://d.example.com/dav/gamelife/2026%2009/ab.db"
        );
    }

    #[test]
    fn propfind_hrefs_are_reduced_to_paths_under_the_prefix() {
        let body = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response><d:href>/gamelife/dev1/snapshots/</d:href></d:response>
  <d:response><d:href>/gamelife/dev1/snapshots/20260914.db</d:href></d:response>
  <d:response><d:href>/gamelife/dev1/snapshots/2026%2009.db</d:href></d:response>
  <d:response><d:href>/gamelife/other/20260914.db</d:href></d:response>
</d:multistatus>"#;
        let got = parse_propfind_hrefs(body, "https://d.example.com/dav", "gamelife/dev1/snapshots");
        assert_eq!(
            got,
            vec![
                "gamelife/dev1/snapshots/2026 09.db".to_string(),
                "gamelife/dev1/snapshots/20260914.db".to_string(),
            ]
        );
    }

    #[test]
    fn put_then_get_roundtrips_through_a_local_server() {
        let server = TestDav::start();
        let target = server.target("u", "p");
        target.put("gamelife/dev1/latest.db", b"payload").unwrap();
        assert_eq!(target.get("gamelife/dev1/latest.db").unwrap(), b"payload");
        assert!(server.has("gamelife/dev1/latest.db"));
    }

    #[test]
    fn put_sends_basic_auth() {
        let server = TestDav::start();
        let target = server.target("user", "pass");
        target.put("a.db", b"x").unwrap();
        assert_eq!(server.last_authorization().unwrap(), "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn http_401_maps_to_auth_error_and_5xx_to_remote() {
        let server = TestDav::start();
        let target = server.target("u", "p");
        server.set_status(401);
        assert!(matches!(target.put("a.db", b"x"), Err(SyncError::Auth)));
        server.set_status(503);
        assert!(matches!(target.put("a.db", b"x"), Err(SyncError::Remote(503))));
    }

    #[test]
    fn get_of_a_missing_object_is_a_remote_404() {
        let server = TestDav::start();
        let target = server.target("u", "p");
        assert!(matches!(target.get("nope.db"), Err(SyncError::Remote(404))));
    }

    #[test]
    fn delete_treats_404_as_success() {
        let server = TestDav::start();
        let target = server.target("u", "p");
        assert!(target.delete("never-existed.db").is_ok());
        target.put("a.db", b"x").unwrap();
        target.delete("a.db").unwrap();
        assert!(!server.has("a.db"));
    }

    /// The exact variant depends on the environment (a sandbox proxy answers
    /// 502 rather than refusing the connection), so this asserts the property
    /// that actually matters: a dead remote fails closed with a typed, printable
    /// error and never panics.
    #[test]
    fn an_unreachable_host_reports_a_typed_error_instead_of_panicking() {
        let target = WebDavTarget::new("http://127.0.0.1:1".into(), "u".into(), "p".into()).unwrap();
        let err = target.put("a.db", b"x").unwrap_err();
        assert!(
            matches!(
                err,
                SyncError::Transport(_) | SyncError::Remote(_) | SyncError::Auth
            ),
            "got {err:?}"
        );
        assert!(!err.to_string().is_empty());
    }

    // ---- Task 4: S3 / SigV4 ---------------------------------------------

    #[test]
    fn hmac_sha256_matches_rfc4231_case_2() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex_lower(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_sha256_hashes_keys_longer_than_the_block() {
        // RFC 4231 case 6: a 131-byte key must be hashed down first.
        let key = vec![0xaau8; 131];
        let mac = hmac_sha256(
            &key,
            b"Test Using Larger Than Block-Size Key - Hash Key First",
        );
        assert_eq!(
            hex_lower(&mac),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn sha256_of_the_empty_payload_matches_the_aws_constant() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// AWS's documented "GET Object" example. The expected signature was
    /// recomputed independently with Python's `hmac`/`hashlib` from the same
    /// canonical request before this test was written, so it pins the
    /// implementation rather than restating it.
    #[test]
    fn sigv4_signature_matches_the_aws_s3_test_vector() {
        let signed = sigv4_sign(
            "GET",
            "/test.txt",
            "",
            &[
                ("host", "examplebucket.s3.amazonaws.com"),
                ("range", "bytes=0-9"),
            ],
            b"",
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "us-east-1",
            "s3",
            "20130524T000000Z",
        );
        assert_eq!(
            signed.authorization,
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
        assert_eq!(signed.amz_date, "20130524T000000Z");
        assert_eq!(signed.payload_hash, sha256_hex(b""));
    }

    #[test]
    fn sigv4_ignores_header_order_and_case() {
        let a = sigv4_sign(
            "GET",
            "/test.txt",
            "",
            &[("host", "h.example.com"), ("range", "bytes=0-9")],
            b"",
            "ak",
            "sk",
            "us-east-1",
            "s3",
            "20130524T000000Z",
        );
        let b = sigv4_sign(
            "GET",
            "/test.txt",
            "",
            &[("Range", "bytes=0-9"), ("HOST", "h.example.com")],
            b"",
            "ak",
            "sk",
            "us-east-1",
            "s3",
            "20130524T000000Z",
        );
        assert_eq!(a.authorization, b.authorization);
    }

    #[test]
    fn sigv4_changes_with_the_payload_and_the_date() {
        let base = |payload: &[u8], date: &str| {
            sigv4_sign(
                "PUT",
                "/b/k",
                "",
                &[("host", "h")],
                payload,
                "ak",
                "sk",
                "auto",
                "s3",
                date,
            )
            .authorization
        };
        assert_ne!(base(b"", "20130524T000000Z"), base(b"x", "20130524T000000Z"));
        assert_ne!(base(b"", "20130524T000000Z"), base(b"", "20130525T000000Z"));
    }

    #[test]
    fn s3_url_is_path_style_with_the_bucket_before_the_key() {
        let t = S3Target::new(
            "https://acct.r2.cloudflarestorage.com".into(),
            "gamelife".into(),
            "auto".into(),
            "k".into(),
            "s".into(),
        )
        .unwrap();
        assert_eq!(
            t.url_for("dev1/latest.db"),
            "https://acct.r2.cloudflarestorage.com/gamelife/dev1/latest.db"
        );
        assert_eq!(
            t.url_for(""),
            "https://acct.r2.cloudflarestorage.com/gamelife"
        );
    }

    #[test]
    fn s3_canonical_query_is_sorted_and_encoded() {
        assert_eq!(
            canonical_query(&[
                ("prefix", "gamelife/dev1/snapshots"),
                ("list-type", "2"),
                ("max-keys", "1000"),
            ]),
            "list-type=2&max-keys=1000&prefix=gamelife%2Fdev1%2Fsnapshots"
        );
    }

    #[test]
    fn host_is_extracted_from_the_endpoint() {
        assert_eq!(
            host_of("https://acct.r2.cloudflarestorage.com"),
            "acct.r2.cloudflarestorage.com"
        );
        assert_eq!(host_of("http://127.0.0.1:9000/"), "127.0.0.1:9000");
    }

    #[test]
    fn list_keys_are_extracted_and_unescaped() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>gamelife</Name>
  <KeyCount>2</KeyCount>
  <IsTruncated>false</IsTruncated>
  <Contents><Key>gamelife/dev1/snapshots/20260914.db</Key><Size>1</Size></Contents>
  <Contents><Key>gamelife/dev1/snapshots/a&amp;b.db</Key><Size>1</Size></Contents>
</ListBucketResult>"#;
        let keys: Vec<String> = extract_tags(body, "Key")
            .into_iter()
            .map(|k| xml_unescape(&k))
            .collect();
        assert_eq!(
            keys,
            vec![
                "gamelife/dev1/snapshots/20260914.db".to_string(),
                "gamelife/dev1/snapshots/a&b.db".to_string(),
            ]
        );
    }

    #[test]
    fn now_amz_date_has_the_sigv4_shape() {
        let d = now_amz_date();
        assert_eq!(d.len(), 16, "{d}");
        assert!(d.ends_with('Z'), "{d}");
        assert_eq!(d.chars().nth(8), Some('T'), "{d}");
        assert!(d.chars().take(8).all(|c| c.is_ascii_digit()), "{d}");
    }

    // ---- Task 5: registry and orchestration -----------------------------

    fn ctx_settings() -> SyncSettings {
        SyncSettings::default()
    }

    fn ctx<'a>(
        settings: &'a SyncSettings,
        live: &'a Path,
        scratch: &'a Path,
        target: &'a dyn RemoteTarget,
        device_id: &'a str,
        now: i64,
    ) -> SyncContext<'a> {
        SyncContext {
            settings,
            live_db: live,
            scratch,
            target,
            device_id,
            label: "Mac",
            platform: "macos",
            now,
        }
    }

    #[test]
    fn registry_upserts_this_device_without_dropping_others() {
        let mut reg = vec![
            DeviceEntry {
                device_id: "a".into(),
                label: "Mac".into(),
                platform: "macos".into(),
                last_seen: 1,
            },
            DeviceEntry {
                device_id: "b".into(),
                label: "PC".into(),
                platform: "windows".into(),
                last_seen: 2,
            },
        ];
        update_registry(
            &mut reg,
            &DeviceEntry {
                device_id: "a".into(),
                label: "MacBook".into(),
                platform: "macos".into(),
                last_seen: 9,
            },
        );
        assert_eq!(reg.len(), 2);
        let a = reg.iter().find(|d| d.device_id == "a").unwrap();
        assert_eq!(a.label, "MacBook");
        assert_eq!(a.last_seen, 9);
        assert_eq!(reg.iter().find(|d| d.device_id == "b").unwrap().label, "PC");
    }

    #[test]
    fn remote_dir_is_namespaced_per_device() {
        let s = ctx_settings();
        assert_eq!(remote_dir(&s, "dev-1"), "gamelife/dev-1");

        let mut blank = SyncSettings::default();
        blank.remote_path = "  /  ".into();
        assert_eq!(remote_dir(&blank, "dev-1"), "gamelife/dev-1");
    }

    #[test]
    fn device_label_prefers_the_configured_name() {
        let mut s = SyncSettings::default();
        let auto = device_label(&s, "abcdef1234");
        assert!(auto.ends_with("abcdef"), "{auto}");
        assert!(auto.starts_with(std::env::consts::OS), "{auto}");

        s.device_label = "  书房 Mac  ".into();
        assert_eq!(device_label(&s, "abcdef1234"), "书房 Mac");
    }

    #[test]
    fn utc_day_stamp_is_a_calendar_date() {
        assert_eq!(utc_day_stamp(0), "19700101");
        assert_eq!(utc_day_stamp(1789000000), "20260910");
    }

    #[test]
    fn is_due_respects_the_interval_and_the_disabled_state() {
        assert!(is_due(None, 1_000, 60));
        assert!(!is_due(Some(1_000), 1_000 + 60 * 60 - 1, 60));
        assert!(is_due(Some(1_000), 1_000 + 60 * 60, 60));
        assert!(!is_due(None, 1_000, 0));
        assert!(!is_due(None, 1_000, -5));
    }

    #[test]
    fn sync_uploads_the_snapshot_and_registers_this_device() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live.db");
        seed_live_db(&live);
        let target = FakeTarget::default();
        let settings = ctx_settings();

        let out = sync_with_target(&ctx(&settings, &live, dir.path(), &target, "dev-1", 1_789_000_000))
            .unwrap();

        assert!(out.snapshot_bytes > 0);
        assert!(target.has("gamelife/dev-1/latest.db"));
        assert!(target.has("gamelife/dev-1/snapshots/20260910.db"));
        assert!(target.has("gamelife/devices.json"));

        let reg: Vec<DeviceEntry> =
            serde_json::from_slice(&target.bytes("gamelife/devices.json")).unwrap();
        assert_eq!(reg.len(), 1);
        assert_eq!(reg[0].device_id, "dev-1");
        assert_eq!(reg[0].label, "Mac");
        assert_eq!(reg[0].last_seen, 1_789_000_000);

        assert!(
            !dir.path().join("gamelife-sync-snapshot.db").exists(),
            "the scratch snapshot must be removed"
        );
    }

    #[test]
    fn the_uploaded_snapshot_carries_no_window_titles() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live.db");
        seed_live_db(&live);
        let target = FakeTarget::default();
        let settings = ctx_settings();
        sync_with_target(&ctx(&settings, &live, dir.path(), &target, "dev-1", 1)).unwrap();

        let text = String::from_utf8_lossy(&target.bytes("gamelife/dev-1/latest.db")).into_owned();
        assert!(!text.contains("Bank - Chrome"));
        assert!(!text.contains("bank.example.com"));
        assert!(!text.contains("screenshots/"));
    }

    #[test]
    fn a_second_device_does_not_overwrite_the_first_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live.db");
        seed_live_db(&live);
        let target = FakeTarget::default();
        let settings = ctx_settings();

        sync_with_target(&ctx(&settings, &live, dir.path(), &target, "dev-1", 1)).unwrap();
        let first = target.bytes("gamelife/dev-1/latest.db");
        sync_with_target(&ctx(&settings, &live, dir.path(), &target, "dev-2", 2)).unwrap();

        assert_eq!(target.bytes("gamelife/dev-1/latest.db"), first);
        assert!(target.has("gamelife/dev-2/latest.db"));

        let reg: Vec<DeviceEntry> =
            serde_json::from_slice(&target.bytes("gamelife/devices.json")).unwrap();
        assert_eq!(reg.len(), 2);
        assert_eq!(reg[0].device_id, "dev-1");
        assert_eq!(reg[1].device_id, "dev-2");
    }

    #[test]
    fn snapshots_are_pruned_to_keep_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live.db");
        seed_live_db(&live);
        let target = FakeTarget::default();
        let mut settings = ctx_settings();
        settings.keep_snapshots = 2;

        for day in 1..=4 {
            sync_with_target(&ctx(
                &settings,
                &live,
                dir.path(),
                &target,
                "dev-1",
                day * 86_400,
            ))
            .unwrap();
        }

        let mut snaps = target.keys_with_prefix("gamelife/dev-1/snapshots/");
        snaps.sort();
        assert_eq!(
            snaps,
            vec![
                "gamelife/dev-1/snapshots/19700104.db".to_string(),
                "gamelife/dev-1/snapshots/19700105.db".to_string(),
            ]
        );
        assert!(target.has("gamelife/dev-1/latest.db"));
    }

    #[test]
    fn a_failing_target_reports_the_error_and_still_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live.db");
        seed_live_db(&live);
        let target = FakeTarget::failing(503);
        let settings = ctx_settings();

        let err =
            sync_with_target(&ctx(&settings, &live, dir.path(), &target, "dev-1", 1)).unwrap_err();
        assert!(matches!(err, SyncError::Remote(503)), "{err:?}");
        assert!(!dir.path().join("gamelife-sync-snapshot.db").exists());
    }

    #[test]
    fn a_missing_registry_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live.db");
        seed_live_db(&live);
        let target = FakeTarget::default();
        let settings = ctx_settings();
        let out =
            sync_with_target(&ctx(&settings, &live, dir.path(), &target, "dev-1", 1)).unwrap();
        assert_eq!(out.devices.len(), 1);
    }

    #[test]
    fn target_from_settings_refuses_an_empty_url() {
        let mut s = SyncSettings::default();
        assert!(matches!(target_from_settings(&s), Err(SyncError::Config(_))));
        s.url = "https://dav.example.com".into();
        s.target = "s3".into();
        assert!(matches!(target_from_settings(&s), Err(SyncError::Config(_))));
    }

    #[test]
    fn sync_on_exit_is_a_no_op_when_disabled() {
        let mut s = SyncSettings::default();
        assert!(!sync_on_exit(&s));
        s.enabled = true;
        s.url = String::new();
        assert!(!sync_on_exit(&s));
    }

    // ---- Task 9: restore -------------------------------------------------

    #[test]
    fn restore_rejects_a_file_that_is_not_a_sqlite_database() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("restored.db");
        let err = restore_from_bytes(b"not a database", &dest).unwrap_err();
        assert!(matches!(err, SyncError::Db(_)), "{err:?}");
        assert!(!dest.exists());
        assert!(!dir.path().join("restored.db.part").exists());
    }

    /// The path refusal fires before the payload is inspected, so even garbage
    /// cannot reach the running database.
    #[test]
    fn restore_refuses_to_overwrite_the_live_database() {
        let Some(live) = crate::db::app_db_path() else {
            return;
        };
        let err = restore_from_bytes(b"not a database", &live).unwrap_err();
        assert!(matches!(err, SyncError::Config(_)), "{err:?}");
    }

    #[test]
    fn restore_leaves_nothing_behind_when_the_payload_is_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("restored.db");
        let mut bytes = SQLITE_HEADER.to_vec();
        bytes.extend_from_slice(&[0u8; 256]);
        assert!(restore_from_bytes(&bytes, &dest).is_err());
        assert!(!dest.exists());
        assert!(!dir.path().join("restored.db.part").exists());
    }

    #[test]
    fn restore_roundtrips_a_real_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live.db");
        seed_live_db(&live);
        let snap = dir.path().join("snap.db");
        build_snapshot(&live, &snap, SCOPE_AGGREGATE).unwrap();

        let dest = dir.path().join("restored.db");
        restore_from_bytes(&std::fs::read(&snap).unwrap(), &dest).unwrap();

        let conn = crate::db::open(&dest).unwrap();
        let slots: i64 = conn
            .query_row("SELECT COUNT(*) FROM slots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(slots, 1);
        assert!(!dir.path().join("restored.db.part").exists());
    }

    #[test]
    fn restore_from_target_stages_the_snapshot_next_to_the_data_root() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("live.db");
        seed_live_db(&live);
        let target = FakeTarget::default();
        let settings = ctx_settings();
        sync_with_target(&ctx(&settings, &live, dir.path(), &target, "dev-1", 1)).unwrap();

        let staging = dir.path().join("staging");
        let restored = restore_from_bytes_staged(&target, &settings, "dev-1", &staging);
        assert_eq!(restored.file_name().unwrap(), "gamelife.restored.db");
        let conn = crate::db::open(&restored).unwrap();
        let slots: i64 = conn
            .query_row("SELECT COUNT(*) FROM slots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(slots, 1);
    }

    /// `restore_from_target` builds its transport from settings, which would
    /// reach for the real `secrets.json`; this mirrors it against a fake target
    /// so the fetch-and-stage half is still covered.
    fn restore_from_bytes_staged(
        target: &dyn RemoteTarget,
        settings: &SyncSettings,
        device_id: &str,
        dest_dir: &Path,
    ) -> std::path::PathBuf {
        let bytes = target
            .get(&format!("{}/latest.db", remote_dir(settings, device_id)))
            .unwrap();
        let dest = dest_dir.join("gamelife.restored.db");
        restore_from_bytes(&bytes, &dest).unwrap();
        dest
    }

    /// In-memory `RemoteTarget`. `fail` makes every call return `Remote(code)`,
    /// which is how the orchestration's error paths get exercised.
    struct FakeTarget {
        store: Mutex<BTreeMap<String, Vec<u8>>>,
        fail: Option<u16>,
    }

    impl Default for FakeTarget {
        fn default() -> Self {
            Self {
                store: Mutex::new(BTreeMap::new()),
                fail: None,
            }
        }
    }

    impl FakeTarget {
        fn failing(code: u16) -> Self {
            Self {
                store: Mutex::new(BTreeMap::new()),
                fail: Some(code),
            }
        }

        fn has(&self, key: &str) -> bool {
            self.store.lock().unwrap().contains_key(key)
        }

        fn bytes(&self, key: &str) -> Vec<u8> {
            self.store.lock().unwrap().get(key).cloned().unwrap_or_default()
        }

        fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
            self.store
                .lock()
                .unwrap()
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect()
        }
    }

    impl RemoteTarget for FakeTarget {
        fn put(&self, path: &str, bytes: &[u8]) -> Result<(), SyncError> {
            if let Some(c) = self.fail {
                return Err(SyncError::Remote(c));
            }
            self.store
                .lock()
                .unwrap()
                .insert(path.to_string(), bytes.to_vec());
            Ok(())
        }

        fn get(&self, path: &str) -> Result<Vec<u8>, SyncError> {
            if let Some(c) = self.fail {
                return Err(SyncError::Remote(c));
            }
            self.store
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or(SyncError::Remote(404))
        }

        fn list(&self, prefix: &str) -> Result<Vec<String>, SyncError> {
            if let Some(c) = self.fail {
                return Err(SyncError::Remote(c));
            }
            Ok(self
                .store
                .lock()
                .unwrap()
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect())
        }

        fn delete(&self, path: &str) -> Result<(), SyncError> {
            if let Some(c) = self.fail {
                return Err(SyncError::Remote(c));
            }
            self.store.lock().unwrap().remove(path);
            Ok(())
        }
    }

    /// Dependency-free in-process WebDAV stub: one accept loop on an ephemeral
    /// port, an in-memory object store, a capturable `Authorization` header and
    /// a forced status override. No server crate.
    struct TestDav {
        addr: std::net::SocketAddr,
        store: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
        auth: Arc<Mutex<Option<String>>>,
        status: Arc<AtomicU16>,
        _thread: std::thread::JoinHandle<()>,
    }

    impl TestDav {
        fn start() -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let store = Arc::new(Mutex::new(BTreeMap::new()));
            let auth = Arc::new(Mutex::new(None));
            let status = Arc::new(AtomicU16::new(200));

            let s = Arc::clone(&store);
            let a = Arc::clone(&auth);
            let st = Arc::clone(&status);
            let thread = std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { break };
                    let _ = handle_request(&mut stream, &s, &a, &st);
                }
            });

            Self {
                addr,
                store,
                auth,
                status,
                _thread: thread,
            }
        }

        fn target(&self, user: &str, pass: &str) -> WebDavTarget {
            WebDavTarget::new(self.base_url(), user.into(), pass.into()).unwrap()
        }

        fn base_url(&self) -> String {
            format!("http://{}", self.addr)
        }

        fn set_status(&self, code: u16) {
            self.status.store(code, Ordering::SeqCst);
        }

        fn last_authorization(&self) -> Option<String> {
            self.auth.lock().unwrap().clone()
        }

        fn has(&self, key: &str) -> bool {
            self.store.lock().unwrap().contains_key(&format!("/{key}"))
        }
    }

    fn handle_request(
        stream: &mut TcpStream,
        store: &Mutex<BTreeMap<String, Vec<u8>>>,
        auth: &Mutex<Option<String>>,
        status: &AtomicU16,
    ) -> std::io::Result<()> {
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 4096];
        let head_end = loop {
            if let Some(pos) = find_bytes(&buf, b"\r\n\r\n") {
                break pos + 4;
            }
            let n = stream.read(&mut chunk)?;
            if n == 0 {
                return Ok(());
            }
            buf.extend_from_slice(&chunk[..n]);
        };

        let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
        let mut lines = head.split("\r\n");
        let request_line = lines.next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let target = parts.next().unwrap_or("/").to_string();

        let mut authorization = None;
        let mut content_length = 0usize;
        for line in lines {
            let lower = line.to_ascii_lowercase();
            if let Some(v) = lower.strip_prefix("authorization:") {
                authorization = Some(line[line.len() - v.len()..].trim().to_string());
            } else if let Some(v) = lower.strip_prefix("content-length:") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
        *auth.lock().unwrap() = authorization;

        while buf.len() < head_end + content_length {
            let n = stream.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let body = buf[head_end..].to_vec();

        let forced = status.load(Ordering::SeqCst);
        if forced != 200 {
            return respond(stream, forced, "Forced", b"");
        }

        match method.as_str() {
            "PUT" => {
                store.lock().unwrap().insert(target, body);
                respond(stream, 201, "Created", b"")
            }
            "GET" => {
                let found = store.lock().unwrap().get(&target).cloned();
                match found {
                    Some(v) => respond(stream, 200, "OK", &v),
                    None => respond(stream, 404, "Not Found", b""),
                }
            }
            "DELETE" => {
                store.lock().unwrap().remove(&target);
                respond(stream, 204, "No Content", b"")
            }
            _ => respond(stream, 405, "Method Not Allowed", b""),
        }
    }

    fn respond(
        stream: &mut TcpStream,
        code: u16,
        reason: &str,
        body: &[u8],
    ) -> std::io::Result<()> {
        let head = format!(
            "HTTP/1.1 {code} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(body)?;
        stream.flush()
    }

    fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }

    fn owner(rows: &[(&str, i64)]) -> String {
        let owned: Vec<(String, i64)> = rows.iter().map(|(id, s)| ((*id).to_string(), *s)).collect();
        slot_owner(&owned)
    }

    #[test]
    fn slot_owner_picks_the_device_that_observed_most() {
        assert_eq!(owner(&[("aaa", 300), ("bbb", 900)]), "bbb");
        assert_eq!(owner(&[("bbb", 900), ("aaa", 300)]), "bbb");
        // A device that was asleep owns nothing it did not observe.
        assert_eq!(owner(&[("aaa", 900), ("bbb", 0)]), "aaa");
    }

    #[test]
    fn slot_owner_breaks_ties_by_smallest_device_id() {
        assert_eq!(owner(&[("bbb", 900), ("aaa", 900)]), "aaa");
        assert_eq!(owner(&[("aaa", 900), ("bbb", 900)]), "aaa");
        assert_eq!(owner(&[("ccc", 1), ("bbb", 1), ("aaa", 1)]), "aaa");
    }

    /// The whole point of the rule: every device must derive the same owner
    /// from the same merged rows, whatever order it happens to read them in.
    #[test]
    fn slot_owner_does_not_depend_on_row_order() {
        let a = owner(&[("d1", 60), ("d2", 900), ("d3", 60), ("d4", 0)]);
        let b = owner(&[("d4", 0), ("d3", 60), ("d2", 900), ("d1", 60)]);
        let c = owner(&[("d2", 900), ("d1", 60), ("d4", 0), ("d3", 60)]);
        assert_eq!(a, "d2");
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    /// Nobody observed it — the process died. There is still exactly one
    /// owner, and settling it pays nothing rather than paying twice.
    #[test]
    fn slot_owner_of_an_unobserved_slot_is_still_deterministic() {
        assert_eq!(owner(&[("bbb", 0), ("aaa", 0)]), "aaa");
        assert_eq!(owner(&[("aaa", 0), ("bbb", 0)]), "aaa");
    }

    #[test]
    fn slot_owner_of_an_empty_slice_is_empty() {
        assert_eq!(owner(&[]), "");
    }

    // -- T13: the day-readiness gate ----------------------------------------

    /// The boundary is whatever the machine's own zone says it is. Deriving it
    /// here rather than hardcoding an instant keeps these tests off `TZ`.
    fn day_end(day: &str) -> i64 {
        let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
        crate::scheduler::local_day_end(&chrono::Local, date).unwrap()
    }

    fn device(id: &str) -> DeviceEntry {
        DeviceEntry {
            device_id: id.to_string(),
            label: id.to_string(),
            platform: "macos".into(),
            last_seen: 0,
        }
    }

    fn covered(id: &str, taken_at: i64) -> SnapshotCoverage {
        SnapshotCoverage {
            device_id: id.to_string(),
            taken_at,
        }
    }

    const D: &str = "2026-09-13";

    /// One device is today's behaviour: settle per slot, no waiting. That is
    /// what makes phase two invisible until a second machine is registered.
    #[test]
    fn a_lone_device_is_always_ready() {
        let end = day_end(D);
        assert!(day_is_ready(D, &[], &[], end - 3600, SETTLE_GRACE_HOURS));
        assert!(day_is_ready(
            D,
            &[device("a")],
            &[],
            end - 3600,
            SETTLE_GRACE_HOURS
        ));
    }

    #[test]
    fn two_devices_settle_once_both_have_answered() {
        let end = day_end(D);
        let devices = [device("a"), device("b")];
        let now = end + 3600;

        assert!(!day_is_ready(D, &devices, &[covered("a", now)], now, SETTLE_GRACE_HOURS));
        assert!(day_is_ready(
            D,
            &devices,
            &[covered("a", now), covered("b", now)],
            now,
            SETTLE_GRACE_HOURS
        ));
    }

    /// A snapshot taken while D was still running may be missing D's evening,
    /// so it does not count as having answered for D.
    #[test]
    fn a_snapshot_taken_before_the_day_ended_does_not_cover_it() {
        let end = day_end(D);
        let devices = [device("a"), device("b")];
        let midday = end - 3600;

        assert!(!day_is_ready(
            D,
            &devices,
            &[covered("a", midday), covered("b", midday)],
            midday,
            SETTLE_GRACE_HOURS
        ));
        // ...and D is never ready while it is still running, however recently
        // everyone uploaded.
        assert!(!day_is_ready(
            D,
            &devices,
            &[covered("a", midday), covered("b", midday)],
            end - 1,
            SETTLE_GRACE_HOURS
        ));
    }

    #[test]
    fn the_grace_period_settles_without_the_straggler() {
        let end = day_end(D);
        let devices = [device("a"), device("b")];
        let only_a = [covered("a", end + 60)];

        assert!(!day_is_ready(
            D,
            &devices,
            &only_a,
            end + 35 * 3600,
            SETTLE_GRACE_HOURS
        ));
        assert!(day_is_ready(
            D,
            &devices,
            &only_a,
            end + 36 * 3600,
            SETTLE_GRACE_HOURS
        ));
        assert!(day_is_ready(
            D,
            &devices,
            &only_a,
            end + 400 * 3600,
            SETTLE_GRACE_HOURS
        ));
    }

    /// A device that registered but whose snapshot we cannot read is still a
    /// device we are waiting for — silence is not consent.
    #[test]
    fn a_registered_device_with_no_readable_snapshot_blocks_settlement() {
        let end = day_end(D);
        let devices = [device("a"), device("b"), device("c")];
        let now = end + 3600;
        assert!(!day_is_ready(
            D,
            &devices,
            &[covered("a", now), covered("c", now)],
            now,
            SETTLE_GRACE_HOURS
        ));
    }

    #[test]
    fn an_unparseable_day_is_never_ready() {
        let end = day_end(D);
        let devices = [device("a"), device("b")];
        let now = end + 3600;
        assert!(!day_is_ready(
            "yesterday",
            &devices,
            &[covered("a", now), covered("b", now)],
            now,
            SETTLE_GRACE_HOURS
        ));
    }

    /// Zero grace means "settle the moment the day ends" — no waiting at all.
    #[test]
    fn zero_grace_settles_as_soon_as_the_day_ends() {
        let end = day_end(D);
        let devices = [device("a"), device("b")];
        assert!(!day_is_ready(D, &devices, &[], end - 1, 0));
        assert!(day_is_ready(D, &devices, &[], end, 0));
    }

    // -- T11: the merged view -----------------------------------------------

    fn slot(day: &str, start: i64, observed: i64) -> String {
        format!(
            "INSERT INTO slots (day, slot_start, status, observed_seconds, credited_core_seconds)
             VALUES ('{day}', {start}, 'final', {observed}, 0);"
        )
    }

    /// A device's uploaded snapshot: the real schema, this device's id, and the
    /// rows the test needs. Built through `db::migrate`, so it has the same
    /// shape the live app uploads — including the tagging trigger.
    fn device_snapshot(dir: &Path, device: &str, rows: &[String]) -> std::path::PathBuf {
        let path = dir.join(format!("snap-{device}.db"));
        let conn = crate::db::open(&path).unwrap();
        crate::db::migrate(&conn).unwrap();
        crate::db::meta_set(&conn, crate::db::DEVICE_ID_KEY, device).unwrap();
        for sql in rows {
            conn.execute_batch(sql).unwrap();
        }
        drop(conn);
        path
    }

    struct MergeFixture {
        _dir: tempfile::TempDir,
        target: FakeTarget,
        settings: SyncSettings,
        dest: std::path::PathBuf,
        scratch: std::path::PathBuf,
    }

    impl MergeFixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let dest = dir.path().join("merged.db");
            let scratch = dir.path().join("scratch");
            std::fs::create_dir_all(&scratch).unwrap();
            Self {
                _dir: dir,
                target: FakeTarget::default(),
                settings: ctx_settings(),
                dest,
                scratch,
            }
        }

        fn register(&self, device: &str) {
            let mut reg: Vec<DeviceEntry> = match self.target.get("gamelife/devices.json") {
                Ok(raw) => serde_json::from_slice(&raw).unwrap_or_default(),
                Err(_) => Vec::new(),
            };
            update_registry(
                &mut reg,
                &DeviceEntry {
                    device_id: device.into(),
                    label: device.into(),
                    platform: "macos".into(),
                    last_seen: 1_789_000_000,
                },
            );
            self.target
                .put("gamelife/devices.json", &serde_json::to_vec(&reg).unwrap())
                .unwrap();
        }

        /// Register a device and upload its snapshot, the way `sync_now` would.
        fn upload(&self, device: &str, rows: &[String]) {
            let path = device_snapshot(&self.scratch, device, rows);
            let bytes = std::fs::read(&path).unwrap();
            let dir = remote_dir(&self.settings, device);
            self.target.put(&format!("{dir}/latest.db"), &bytes).unwrap();
            self.register(device);
        }

        fn rebuild(&self) -> MergedReport {
            rebuild_merged(&self.target, &self.settings, &self.dest, &self.scratch).unwrap()
        }

        fn open(&self) -> Connection {
            crate::db::open(&self.dest).unwrap()
        }
    }

    fn row_counts(conn: &Connection) -> Vec<(String, i64)> {
        MERGED_TABLES
            .iter()
            .map(|(t, _)| {
                let n: i64 = conn
                    .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
                    .unwrap();
                ((*t).to_string(), n)
            })
            .collect()
    }

    /// Rebuilding must be a pure function of the snapshots: the merge runs on
    /// every device that has one, so a rebuild that drifted would make the
    /// statistics page change for no reason.
    #[test]
    fn rebuilding_the_merged_view_twice_gives_the_same_view() {
        let f = MergeFixture::new();
        f.upload("aaa", &[slot("2026-09-13", 1789353000, 900)]);
        f.upload("bbb", &[slot("2026-09-13", 1789353000, 300)]);

        let first = f.rebuild();
        assert_eq!(first.merged, vec!["aaa", "bbb"]);
        assert_eq!(first.slots_kept, 1);
        assert_eq!(first.slots_dropped, 1);
        assert!(first.bytes > 0);
        let before = row_counts(&f.open());

        let second = f.rebuild();
        assert_eq!(second.slots_kept, first.slots_kept);
        assert_eq!(second.slots_dropped, first.slots_dropped);
        assert_eq!(row_counts(&f.open()), before);
    }

    /// §7.2: two machines observing the same fifteen minutes is one slot, and
    /// the one who observed more owns it.
    #[test]
    fn the_merged_view_keeps_only_the_owners_slot_row() {
        let f = MergeFixture::new();
        f.upload("aaa", &[slot("2026-09-13", 1789353000, 900)]);
        f.upload("bbb", &[slot("2026-09-13", 1789353000, 300)]);

        f.rebuild();

        let conn = f.open();
        let owners: Vec<String> = conn
            .prepare("SELECT device_id FROM slots")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(owners, vec!["aaa"]);
    }

    /// A tie goes to the smaller id — and both machines must reach the same
    /// conclusion, which is what stops them settling the same slot twice.
    #[test]
    fn a_tied_slot_is_owned_by_the_smaller_device_id() {
        let f = MergeFixture::new();
        f.upload("bbb", &[slot("2026-09-13", 1789353000, 900)]);
        f.upload("aaa", &[slot("2026-09-13", 1789353000, 900)]);

        f.rebuild();

        let conn = f.open();
        let owner: String = conn
            .query_row("SELECT device_id FROM slots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(owner, "aaa");
    }

    #[test]
    fn device_keyed_tables_keep_both_machines_rows() {
        let f = MergeFixture::new();
        let stats = "INSERT INTO app_day_stats (day, app, bundle_id, samples, idle_seconds, core, support, admin, side, distraction, away, unobserved, protected) VALUES ('2026-09-13','Cursor','',1,0,15,0,0,0,0,0,0,0);";
        f.upload("aaa", &[stats.to_string()]);
        f.upload("bbb", &[stats.to_string()]);

        f.rebuild();

        let n: i64 = f
            .open()
            .query_row("SELECT COUNT(*) FROM app_day_stats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2, "two machines observing the same app is two facts");
    }

    /// The ledger is one shared ledger, not two. A key both devices uploaded is
    /// one payment, and it must not be summed twice.
    #[test]
    fn the_shared_wallet_collapses_what_both_devices_uploaded() {
        let f = MergeFixture::new();
        let payment = "INSERT INTO ledger (reward_event_key, day, ts, coin_delta, xp_delta) VALUES ('slot:2026-09-13:1', '2026-09-13', 1, 5, 3);";
        f.upload("aaa", &[payment.to_string()]);
        f.upload("bbb", &[payment.to_string()]);

        f.rebuild();

        let conn = f.open();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM ledger", [], |r| r.get(0))
            .unwrap();
        let coins: i64 = conn
            .query_row("SELECT COALESCE(SUM(coin_delta), 0) FROM ledger", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(coins, 5, "the same payment must not be counted twice");
    }

    /// §5.2: two devices editing one wish converge on the newer edit.
    #[test]
    fn wishes_take_the_newer_edit() {
        let f = MergeFixture::new();
        f.upload("aaa", &["INSERT INTO wishes (id, name, kind, price, archived, updated_at) VALUES ('w1','旧名','coin',10,0,100);".to_string()]);
        f.upload("bbb", &["INSERT INTO wishes (id, name, kind, price, archived, updated_at) VALUES ('w1','新名','coin',20,0,200);".to_string()]);

        f.rebuild();

        let (name, price): (String, i64) = f
            .open()
            .query_row("SELECT name, price FROM wishes WHERE id = 'w1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(name, "新名");
        assert_eq!(price, 20);
    }

    /// A snapshot from before T10 has no `device_id` column. It must be
    /// attributed to the device whose directory it came from — never to
    /// whoever happened to run the merge.
    #[test]
    fn a_snapshot_without_device_id_is_attributed_to_its_own_directory() {
        let f = MergeFixture::new();
        let path = f.scratch.join("pre-t10.db");
        {
            let conn = crate::db::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE slots (
                   day TEXT NOT NULL, slot_start INTEGER NOT NULL, category TEXT,
                   status TEXT, activity_json TEXT, credited_core_seconds INTEGER,
                   credited_side_seconds INTEGER, credited_chore_seconds INTEGER,
                   observed_seconds INTEGER, used_vision INTEGER, screenshot_path TEXT,
                   captured_at INTEGER, capture_context_json TEXT,
                   quest_version_id INTEGER, policy_version_id INTEGER,
                   capture_scheduled_at INTEGER, capture_status TEXT,
                   task_snapshot_json TEXT,
                   PRIMARY KEY (day, slot_start)
                 );
                 INSERT INTO slots (day, slot_start, status, observed_seconds)
                 VALUES ('2026-09-13', 1789353000, 'final', 900);",
            )
            .unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        f.target.put("gamelife/aaa/latest.db", &bytes).unwrap();
        f.register("aaa");

        let report = f.rebuild();
        assert_eq!(report.merged, vec!["aaa"]);

        let id: String = f
            .open()
            .query_row("SELECT device_id FROM slots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(id, "aaa");
    }

    /// A registered device that is simply offline is normal. It must be
    /// recorded and skipped, not allowed to abort everyone else's view.
    #[test]
    fn an_unreadable_device_is_skipped_without_aborting_the_rebuild() {
        let f = MergeFixture::new();
        f.upload("aaa", &[slot("2026-09-13", 1789353000, 900)]);
        f.register("bbb");

        let report = f.rebuild();

        assert_eq!(report.merged, vec!["aaa"]);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, "bbb");
        assert!(!report.skipped[0].1.is_empty());
        let n: i64 = f
            .open()
            .query_row("SELECT COUNT(*) FROM slots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    /// Nothing readable: keep the previous view rather than blanking the
    /// statistics page.
    #[test]
    fn nothing_to_merge_leaves_the_previous_view_alone() {
        let f = MergeFixture::new();
        f.upload("aaa", &[slot("2026-09-13", 1789353000, 900)]);
        f.rebuild();
        let before = std::fs::read(&f.dest).unwrap();

        f.target.put("gamelife/devices.json", b"[]").unwrap();
        let report = f.rebuild();

        assert!(report.merged.is_empty());
        assert_eq!(report.bytes, 0);
        assert_eq!(std::fs::read(&f.dest).unwrap(), before);
    }

    /// The merged view is derived data. Building it must not touch the live
    /// database, and the scratch snapshots must not be left behind.
    #[test]
    fn rebuilding_leaves_no_scratch_behind() {
        let f = MergeFixture::new();
        f.upload("aaa", &[slot("2026-09-13", 1789353000, 900)]);

        f.rebuild();

        let leftovers: Vec<String> = std::fs::read_dir(&f.scratch)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("merged-src-"))
            .collect();
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    }
}
