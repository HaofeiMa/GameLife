# Cloud backup & multi-device ledger Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Back the local database up to a remote WebDAV or S3-compatible store, and (phase two) let several machines' observations merge into one ledger without a privileged host machine.

**Architecture:** Phase one is purely additive — a new `src-tauri/src/sync.rs` builds a `VACUUM INTO` snapshot of the live database, trims it to the configured scope, and PUTs it to `<remote>/gamelife/<device_id>/latest.db`. Nothing in the judgment pipeline, the schema semantics, or the live database's primary keys changes. Phase two adds a `device_id` tag to the rows and rebuilds a **separate, read-only** `merged.db` with composite keys, so the merge risk never reaches `resolve.rs` / `scheduler.rs`.

**Tech Stack:** `reqwest` blocking client (already a dependency, same pattern as `vision.rs`), `sha2` for SigV4 (already a dependency; HMAC-SHA256 hand-rolled on top of it, ~20 lines, no new crate), `rusqlite` for `VACUUM INTO`, `getrandom` for the device UUID. No new crates.

**Spec:** `docs/superpowers/specs/2026-09-14-cloud-sync-design.md` — §3.0 (phasing), §5.1 / §6 / §8 / §9 (phase one), §3.3 / §3.4 / §5.2 / §7 (phase two).

## Global Constraints

- `secrets.json` and `screenshots/` never enter any upload payload, at any scope. One test must assert the payload contains no `secrets` table and no `screenshots` path.
- Default scope is `aggregate`: the uploaded snapshot contains **no `samples` rows** and no window title / URL / document path string.
- The snapshot is a read-only copy. The live database stays the runtime's single source of truth; the remote is never a judgment input.
- `ledger.reward_event_key` stays globally unique and is never rewritten.
- Network failure must not block sampling, judgment, or settlement. A failed sync records status and returns; it never propagates into the sampler loop.
- `sync.enabled = false` ⇒ no outbound request from the sync module.
- Commands that touch the network must be `async` (`tauri::async_runtime::spawn_blocking` + `reqwest::blocking`, mirroring `ticktick_sync`).
- Credentials go through `keychain.rs` (`secrets.json`, mode 0600). Never into `config.json`.
- UI strings are Chinese. Energy is **能量**, never XP / 经验.
- No hex colours in components; colours come from `--cat-*` / `src/lib/theme.ts`.
- `invoke()` only from `src/lib/api.ts`.
- Tests must not call `std::env::set_var("HOME", …)`.
- Gates: `src-tauri/` → `cargo test --offline -p gamelife` must **run**; `src/` → `npx vitest run --dir src`.
- Do not commit the leftover UI / hint working tree.

## Progress

| Task | State |
| --- | --- |
| 1 `SyncSettings` | **done** — `config.rs`, 4 new tests |
| 2 Snapshot build + trim | **done** — `sync.rs`, 7 new tests |
| 3 `RemoteTarget` + WebDAV | **done** — `sync.rs`, 9 new tests |
| 4 S3 / SigV4 | **done** — `sync.rs`, 11 new tests |
| 5 Registry + orchestration | **done** — `sync.rs`, 12 new tests |
| 6 Timer / on-exit triggers | **done** — `sampler.rs` + `lib.rs` |
| 7 Tauri commands | **done** — `commands.rs`, `lib.rs`, `api.ts` |
| 8 设置 card | **done** — `Settings.tsx`, `cloudSync.ts` + 9 vitest |
| 9 Restore | **done** — `sync.rs`, 5 new tests |
| T10 `device_id` tagging | **done** — `db.rs`, 1 new test; also fixed a phase-one scope leak (amendment 7) |
| T11 `merged.db` rebuild | **done** — `sync.rs` + `db.rs`, 10 new tests; spec §5.3 added |
| T12 Owner rule | **done** — `sync.rs`, 5 new tests |
| T13 Readiness gate | **done** — `sync.rs`, 7 new tests; `scheduler::local_day_end` → `pub(crate)` |
| T14 Settlement | **T14a done**; T14b (settler) and T14c (mode plumbing) not started |
| T15 统计 / 商店 merged view | not started |
| T16 Settings card | not started |

Phase one is complete.

```
cargo test --offline -p gamelife-core   165 passed
cargo test --offline -p gamelife        295 passed / 1 ignored   (was 217 before Task 1)
npx vitest run --dir src                 94 passed               (was 85)
npm run build                            ok
```

### Amendments found while implementing

1. **`SyncSettings` gained `bucket` and `region`.** The plan's field list had no place to put an S3 bucket. `url` is the endpoint, `username` the access key id, `bucket` the path-style bucket, `region` defaults to `auto` (correct for Cloudflare R2).
2. **`sigv4_sign` derives the date stamp** from the first eight bytes of `amz_date` instead of taking it as a separate argument — one fewer thing to get wrong at every call site. The plan's trailing `&[]` parameter was dropped.
3. **`sync_with_target` takes a `SyncContext` struct** rather than seven positional arguments, and the context carries `scratch` so tests inject a tempdir instead of reaching for `TMPDIR`.
4. **`an_unreachable_host_is_a_transport_error_not_a_panic` is not portable.** In a sandboxed environment an outbound request to a dead port is answered by an HTTP proxy (502) rather than refused, so the variant cannot be asserted. The shipped test asserts the property that matters: a dead remote yields a typed, printable error and never panics.
5. **The AWS SigV4 expected signature was recomputed independently** with Python's `hmac`/`hashlib` from the same canonical request before the Rust test was written, so the test pins the implementation rather than restating it. RFC 4231 cases 2 and 6 pin the hand-rolled HMAC the same way.
6. **The sampler checks for a due sync every 20 ticks (5 minutes)**, not every 15-second tick, and re-reads settings each time so toggling 云端备份 takes effect without a restart.
7. **§4's「永不」tables were not actually being trimmed** (found while implementing T10). `VACUUM INTO` copies the whole database; the phase-one trim only deleted `samples`, so `heartbeat`, `ticktick_cache`, `task_lists`, `tasks` and the whole of `app_meta` were riding along in every uploaded snapshot — including TickTick task titles, which contradicts the `aggregate` scope's promise that user text stays local. Fixed by deleting `NEVER_SYNCED_TABLES` from the copy and trimming `app_meta` to `device_id` alone (the id is already public as the remote directory name, and keeping it lets a restored snapshot stamp its own rows). `SYNCED_TABLES` never controlled the file contents, only the reported counts — its doc comment now says so.

## Discovered, not yet fixed

**`cargo test -p gamelife` is intermittently flaky, and the cause is a documented-rule violation.** `purge_expired_screenshots_removes_old_files` failed once in four full runs with `assert!(path.is_none())` — the slot's `screenshot_path` had not been cleared.

Root cause: three tests call `std::env::set_var("HOME", …)` (`scheduler.rs` ~2556, ~2684, ~3408), which `AGENTS.md` forbids precisely because parallel tests clobber each other. `platform::screenshots_dir()` is derived from `HOME`, and `purge_expired_screenshots` resolves it *internally*. So a test that creates its file at directory `T1` can call purge which resolves directory `T2`, not find the file, skip the `UPDATE … WHERE screenshot_path = ?1`, and leave the row uncleared. It is not a race in the code under test — it is unsynchronised process-global state.

Recommended fix (its own change, not part of T14): inject the directory the way `SyncContext.scratch` already is — add `purge_expired_screenshots_in(conn, dir, retention, now)` and `screenshot_path_for_in(dir, …)`, have the existing functions delegate with `screenshots_dir()`, thread an optional override through `finalize_slot_end`, and delete the three `set_var("HOME")` calls. Serialising the three tests against each other would *not* fix it: any other test reaching `finalize_slot_end` reads the same ambient directory concurrently.

---

## Phase one — single-machine snapshot backup

### Task 1: `SyncSettings` in `config.json`

**Files:**
- Modify: `src-tauri/src/config.rs`
- Test: same file, `mod tests`

**Interfaces:**
- Produces: `pub struct SyncSettings { enabled, target, url, remote_path, username, interval_minutes, scope, keep_snapshots, device_label }` with `Default`; `AppSettings.sync: SyncSettings` (`#[serde(default)]`); `pub const SCOPE_AGGREGATE: &str = "aggregate"`, `SCOPE_SAMPLES: &str = "samples"`; `pub fn normalize_scope(&str) -> &'static str`.
- Consumes: nothing new.

- [ ] **Step 1: Write the failing tests** (production change that would fail them: dropping `#[serde(default)]` from `sync`)

```rust
#[test]
fn old_config_json_without_sync_gets_disabled_defaults() {
    let parsed: AppSettings = serde_json::from_str(
        r#"{"screenshotRetention":"none","sampleKeepDays":7,"loginAtStartup":true,"trustedApps":[],"distractionRules":[],"sideProjectRules":[],"readingApps":[],"neverCaptureApps":[]}"#,
    )
    .unwrap();
    assert!(!parsed.sync.enabled);
    assert_eq!(parsed.sync.target, "webdav");
    assert_eq!(parsed.sync.scope, SCOPE_AGGREGATE);
    assert_eq!(parsed.sync.interval_minutes, 60);
    assert_eq!(parsed.sync.keep_snapshots, 7);
}

#[test]
fn unknown_scope_falls_back_to_aggregate() {
    assert_eq!(normalize_scope("samples"), SCOPE_SAMPLES);
    assert_eq!(normalize_scope("everything"), SCOPE_AGGREGATE);
    assert_eq!(normalize_scope(""), SCOPE_AGGREGATE);
}

#[test]
fn sync_settings_are_not_part_of_the_policy_snapshot() {
    let a = default_settings();
    let mut b = default_settings();
    b.sync.enabled = true;
    b.sync.url = "https://dav.example.com".into();
    assert_eq!(policy_snapshot_json(&a), policy_snapshot_json(&b));
}
```

- [ ] **Step 2: Run the tests, confirm they fail**

```bash
cargo test --offline -p gamelife config::tests
```

- [ ] **Step 3: Implement**

Add to `config.rs`, next to `VisionProviderSettings`:

```rust
pub const SCOPE_AGGREGATE: &str = "aggregate";
pub const SCOPE_SAMPLES: &str = "samples";

pub fn normalize_scope(s: &str) -> &'static str {
    match s {
        SCOPE_SAMPLES => SCOPE_SAMPLES,
        _ => SCOPE_AGGREGATE,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    pub enabled: bool,
    pub target: String,
    pub url: String,
    pub remote_path: String,
    pub username: String,
    pub interval_minutes: i64,
    pub scope: String,
    pub keep_snapshots: i64,
    pub device_label: String,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            target: "webdav".into(),
            url: String::new(),
            remote_path: "gamelife".into(),
            username: String::new(),
            interval_minutes: 60,
            scope: SCOPE_AGGREGATE.into(),
            keep_snapshots: 7,
            device_label: String::new(),
        }
    }
}
```

Add to `AppSettings` (after `theme`):

```rust
    #[serde(default)]
    pub sync: SyncSettings,
```

and `sync: SyncSettings::default(),` to `default_settings()`. `policy_snapshot_json` is untouched — it builds an explicit `json!` of named fields, so `sync` is excluded for free; the third test locks that in.

- [ ] **Step 4: Run the tests, confirm they pass**

```bash
cargo test --offline -p gamelife config::tests
```

- [ ] **Step 5: Commit** — `feat: add cloud sync settings to config`

---

### Task 2: Snapshot generation and scope trimming

**Files:**
- Create: `src-tauri/src/sync.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod sync;`)
- Test: `src-tauri/src/sync.rs`, `mod tests`

**Interfaces:**
- Consumes: `crate::config::{normalize_scope, SCOPE_SAMPLES}`, `crate::db::migrate`.
- Produces: `pub fn build_snapshot(live_db: &Path, dest: &Path, scope: &str) -> Result<SnapshotReport, SyncError>`; `pub struct SnapshotReport { pub bytes: u64, pub tables: BTreeMap<String, i64> }`; `pub enum SyncError { Io(String), Db(String), Transport(String), Auth, Remote(u16), Config(String) }` (with `Display`).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn aggregate_scope_drops_samples_and_capture_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    seed_live_db(&live); // helper below: 3 samples with titles, 1 slot with a screenshot_path
    let dest = dir.path().join("snap.db");

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
        .query_row("SELECT capture_context_json FROM slots LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert!(ctx.is_none());
    assert!(report.tables["slots"] >= 1);
    assert!(report.bytes > 0);
}

#[test]
fn samples_scope_keeps_samples() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    seed_live_db(&live);
    let dest = dir.path().join("snap.db");
    build_snapshot(&live, &dest, SCOPE_SAMPLES).unwrap();
    let conn = crate::db::open(&dest).unwrap();
    let samples: i64 = conn
        .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
        .unwrap();
    assert_eq!(samples, 3);
}

#[test]
fn snapshot_never_carries_secrets_or_screenshot_paths() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    seed_live_db(&live);
    let dest = dir.path().join("snap.db");
    build_snapshot(&live, &dest, SCOPE_AGGREGATE).unwrap();

    let bytes = std::fs::read(&dest).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("sk-secret"));
    assert!(!text.contains("secrets"));
    assert!(!text.contains("screenshots/"));
    assert!(!text.contains("Bank - Chrome"));
}

#[test]
fn build_snapshot_overwrites_an_existing_destination() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    seed_live_db(&live);
    let dest = dir.path().join("snap.db");
    std::fs::write(&dest, b"stale").unwrap();
    build_snapshot(&live, &dest, SCOPE_AGGREGATE).unwrap();
    assert!(std::fs::metadata(&dest).unwrap().len() > 100);
}
```

`seed_live_db` is a test helper that creates a migrated database and inserts three `samples` rows whose `title` is `"Bank - Chrome"` plus one `slots` row with `screenshot_path = "screenshots/x.jpg"` and a `capture_context_json`. It must not touch `HOME`.

- [ ] **Step 2: Run the tests, confirm they fail**

```bash
cargo test --offline -p gamelife sync::tests
```

- [ ] **Step 3: Implement**

The order matters and cannot be inverted — trim the **copy**, never the live database:

```rust
pub fn build_snapshot(live_db: &Path, dest: &Path, scope: &str) -> Result<SnapshotReport, SyncError> {
    // `VACUUM INTO` refuses an existing destination.
    if dest.exists() {
        std::fs::remove_file(dest).map_err(|e| SyncError::Io(format!("remove stale snapshot: {e}")))?;
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SyncError::Io(format!("mkdir: {e}")))?;
    }

    {
        let live = crate::db::open(live_db).map_err(|e| SyncError::Db(format!("{e:?}")))?;
        crate::db::migrate(&live).map_err(|e| SyncError::Db(format!("{e:?}")))?;
        let target = dest.to_string_lossy().to_string();
        live.execute("VACUUM INTO ?1", rusqlite::params![target])
            .map_err(|e| SyncError::Db(format!("vacuum into: {e}")))?;
    }

    let snap = crate::db::open(dest).map_err(|e| SyncError::Db(format!("{e:?}")))?;
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
```

`count_tables` runs `SELECT COUNT(*)` over a fixed list of the syncable table names (skip any that `sqlite_master` does not report, so a schema change never panics the sync).

- [ ] **Step 4: Run the tests, confirm they pass**

```bash
cargo test --offline -p gamelife sync::tests
```

- [ ] **Step 5: Commit** — `feat: build a scope-trimmed sqlite snapshot for sync`

---

### Task 3: `RemoteTarget` trait and the WebDAV implementation

**Files:**
- Modify: `src-tauri/src/sync.rs`
- Test: same file, `mod tests`

**Interfaces:**
- Produces:

```rust
pub trait RemoteTarget {
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), SyncError>;
    fn get(&self, path: &str) -> Result<Vec<u8>, SyncError>;
    fn list(&self, prefix: &str) -> Result<Vec<String>, SyncError>;
    fn delete(&self, path: &str) -> Result<(), SyncError>;
}

pub struct WebDavTarget { pub base_url: String, pub username: String, pub password: String, pub client: reqwest::blocking::Client }

pub fn webdav_url(base: &str, path: &str) -> String;
pub fn join_remote(base: &str, path: &str) -> String;
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn webdav_url_joins_without_doubling_slashes() {
    assert_eq!(webdav_url("https://dav.example.com/remote.php/dav/files/me/", "gamelife/a.db"),
               "https://dav.example.com/remote.php/dav/files/me/gamelife/a.db");
    assert_eq!(webdav_url("https://dav.example.com", "gamelife/a.db"),
               "https://dav.example.com/gamelife/a.db");
}

#[test]
fn webdav_url_percent_encodes_each_segment_but_not_the_separators() {
    assert_eq!(webdav_url("https://d.example.com/dav", "gamelife/2026 09/ab.db"),
               "https://d.example.com/dav/gamelife/2026%2009/ab.db");
}

#[test]
fn put_then_get_roundtrips_through_a_local_server() {
    let server = TestDav::start(); // std::net::TcpListener on 127.0.0.1:0, one thread
    let target = WebDavTarget::for_test(server.base_url(), "u", "p");
    target.put("gamelife/dev1/latest.db", b"payload").unwrap();
    assert_eq!(target.get("gamelife/dev1/latest.db").unwrap(), b"payload");
}

#[test]
fn put_sends_basic_auth() {
    let server = TestDav::start();
    let target = WebDavTarget::for_test(server.base_url(), "user", "pass");
    target.put("a.db", b"x").unwrap();
    assert_eq!(server.last_authorization().unwrap(), "Basic dXNlcjpwYXNz");
}

#[test]
fn http_401_maps_to_auth_error_and_5xx_to_remote() {
    let server = TestDav::start();
    server.set_status(401);
    let target = WebDavTarget::for_test(server.base_url(), "u", "p");
    assert!(matches!(target.put("a.db", b"x"), Err(SyncError::Auth)));
    server.set_status(503);
    assert!(matches!(target.put("a.db", b"x"), Err(SyncError::Remote(503))));
}
```

`TestDav` is a dependency-free in-process HTTP server (`std::net::TcpListener` bound to `127.0.0.1:0`, one accept loop thread, a `Mutex<HashMap<String, Vec<u8>>>` store, an `AtomicU16` status override, and a captured `Authorization` header). It lives in `mod tests`. Do not add a server crate.

- [ ] **Step 2: Run the tests, confirm they fail**

```bash
cargo test --offline -p gamelife sync::tests
```

- [ ] **Step 3: Implement**

`webdav_url` splits `path` on `/`, percent-encodes each segment (`%` → `%25` first, then space and the RFC 3986 unsafe set), and rejoins — separators stay literal.

`WebDavTarget::put` is a plain `client.put(url).basic_auth(user, Some(pass)).body(bytes).send()`, mapping `401`/`403` → `SyncError::Auth`, `5xx` → `SyncError::Remote(status)`, other non-success → `SyncError::Remote(status)`, and `send()` errors → `SyncError::Transport`. `get` mirrors it. `list` issues `PROPFIND` with `Depth: 1` and extracts `<d:href>` values, returning the paths relative to the base. `delete` issues `DELETE` and treats `404` as success.

Follow `vision.rs` for the client: `reqwest::blocking::Client::builder().timeout(Duration::from_secs(SYNC_TIMEOUT_SECS)).user_agent(USER_AGENT).build()`. Reuse `crate::vision::USER_AGENT` rather than inventing a second one.

`RemoteTarget` must be object-safe (`Box<dyn RemoteTarget>`) so the orchestration layer in Task 5 can hold either implementation.

- [ ] **Step 4: Run the tests, confirm they pass**

```bash
cargo test --offline -p gamelife sync::tests
```

- [ ] **Step 5: Commit** — `feat: add webdav remote target for sync`

---

### Task 4: S3-compatible target (Cloudflare R2, B2, MinIO)

**Files:**
- Modify: `src-tauri/src/sync.rs`
- Test: same file, `mod tests`

**Interfaces:**
- Produces: `pub struct S3Target { pub endpoint: String, pub bucket: String, pub region: String, pub access_key: String, pub secret_key: String, pub client: reqwest::blocking::Client }`; `pub fn sigv4_sign(...)`; `pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32]`; `pub fn hex_lower(bytes: &[u8]) -> String`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn hmac_sha256_matches_rfc4231_case_2() {
    let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
    assert_eq!(hex_lower(&mac), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
}

#[test]
fn sigv4_signature_matches_the_aws_s3_test_vector() {
    // AWS "Signature Version 4 Test Suite" — get-vanilla / s3.
    let signed = sigv4_sign(
        "GET",
        "/test.txt",
        "",
        &[("host", "examplebucket.s3.amazonaws.com"), ("range", "bytes=0-9")],
        b"",
        "AKIAIOSFODNN7EXAMPLE",
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        "us-east-1",
        "s3",
        "20130524T000000Z",
        "20130524",
        &[],
    );
    assert_eq!(
        signed.authorization,
        "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, \
         SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, \
         Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
    );
}

#[test]
fn s3_url_puts_the_bucket_in_the_host_when_the_endpoint_has_no_bucket() {
    let t = S3Target::for_test("https://accountid.r2.cloudflarestorage.com", "gamelife", "auto");
    assert_eq!(t.url_for("dev1/latest.db"),
               "https://accountid.r2.cloudflarestorage.com/gamelife/dev1/latest.db");
}
```

- [ ] **Step 2: Run the tests, confirm they fail**

```bash
cargo test --offline -p gamelife sync::tests
```

- [ ] **Step 3: Implement**

`hmac_sha256` is written on top of the existing `sha2` dependency (ipad 0x36 / opad 0x5c, block size 64, key hashed first when longer than 64 bytes). It must match RFC 4231 before anything else is built on it — that is what the first test is for.

`sigv4_sign` builds the canonical request (`METHOD\ncanonical_uri\ncanonical_query\ncanonical_headers\nsigned_headers\npayload_hash`), the string to sign, the derived signing key, and the final HMAC chain. `canonical_uri` uses the same segment encoder as `webdav_url` (extract it into one shared private helper). `x-amz-content-sha256` and `x-amz-date` are always signed, matching the test vector.

`S3Target` implements `RemoteTarget` with `PUT` / `GET` / `DELETE`; `list` uses `GET /?list-type=2&prefix=…` and extracts `<Key>` values from the XML with a small string scan (no XML crate).

Do **not** add `hmac`, `aws-sigv4`, or an XML crate.

- [ ] **Step 4: Run the tests, confirm they pass**

```bash
cargo test --offline -p gamelife sync::tests
```

- [ ] **Step 5: Commit** — `feat: add s3-compatible remote target for sync`

---

### Task 5: Device registry and the sync orchestration

**Files:**
- Modify: `src-tauri/src/sync.rs`
- Test: same file, `mod tests`

**Interfaces:**
- Consumes: `build_snapshot` (Task 2), `RemoteTarget` (Tasks 3–4), `crate::config::{load_settings, SyncSettings}`.
- Produces:

```rust
pub fn device_id() -> Result<String, SyncError>;          // app_meta['device_id'], created once
pub fn device_label() -> String;                          // settings.sync.device_label, else hostname
pub fn remote_dir(settings: &SyncSettings) -> String;     // "{remote_path}/{device_id}"
pub fn target_from_settings(s: &SyncSettings) -> Result<Box<dyn RemoteTarget>, SyncError>;
pub fn sync_now(settings: &SyncSettings) -> Result<SyncOutcome, SyncError>;
pub struct SyncOutcome { pub uploaded: u64, pub snapshot_bytes: u64, pub devices: Vec<DeviceEntry>, pub at: i64 }
#[derive(Serialize, Deserialize, Clone)] pub struct DeviceEntry { pub device_id: String, pub label: String, pub platform: String, pub last_seen: i64 }
pub fn update_registry(reg: &mut Vec<DeviceEntry>, me: &DeviceEntry);
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn registry_upserts_this_device_without_dropping_others() {
    let mut reg = vec![
        DeviceEntry { device_id: "a".into(), label: "Mac".into(), platform: "macos".into(), last_seen: 1 },
        DeviceEntry { device_id: "b".into(), label: "PC".into(), platform: "windows".into(), last_seen: 2 },
    ];
    update_registry(&mut reg, &DeviceEntry { device_id: "a".into(), label: "MacBook".into(), platform: "macos".into(), last_seen: 9 });
    assert_eq!(reg.len(), 2);
    assert_eq!(reg.iter().find(|d| d.device_id == "a").unwrap().label, "MacBook");
    assert_eq!(reg.iter().find(|d| d.device_id == "a").unwrap().last_seen, 9);
}

#[test]
fn remote_dir_is_namespaced_per_device() {
    let mut s = SyncSettings::default();
    s.remote_path = "gamelife".into();
    let dir = remote_dir_for(&s, "dev-1");
    assert_eq!(dir, "gamelife/dev-1");
}

#[test]
fn sync_now_uploads_snapshot_and_registry_to_the_fake_target() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    seed_live_db(&live);
    let target = FakeTarget::default(); // in-memory RemoteTarget
    let settings = sync_settings_for(&dir, "webdav");

    let out = sync_with_target(&settings, &live, &target, "dev-1", "Mac", "macos", 1_789_000_000).unwrap();

    assert!(out.snapshot_bytes > 0);
    assert!(target.has("gamelife/dev-1/latest.db"));
    assert!(target.has("gamelife/devices.json"));
    let reg: Vec<DeviceEntry> = serde_json::from_slice(&target.get_bytes("gamelife/devices.json")).unwrap();
    assert_eq!(reg.len(), 1);
    assert_eq!(reg[0].device_id, "dev-1");
}

#[test]
fn a_second_device_does_not_overwrite_the_first_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    seed_live_db(&live);
    let target = FakeTarget::default();
    let settings = sync_settings_for(&dir, "webdav");
    sync_with_target(&settings, &live, &target, "dev-1", "Mac", "macos", 1).unwrap();
    let first = target.get_bytes("gamelife/dev-1/latest.db");
    sync_with_target(&settings, &live, &target, "dev-2", "PC", "windows", 2).unwrap();
    assert_eq!(target.get_bytes("gamelife/dev-1/latest.db"), first);
    assert!(target.has("gamelife/dev-2/latest.db"));
    let reg: Vec<DeviceEntry> = serde_json::from_slice(&target.get_bytes("gamelife/devices.json")).unwrap();
    assert_eq!(reg.len(), 2);
}

#[test]
fn a_failing_target_reports_and_does_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    seed_live_db(&live);
    let target = FakeTarget::failing(503);
    let settings = sync_settings_for(&dir, "webdav");
    let err = sync_with_target(&settings, &live, &target, "dev-1", "Mac", "macos", 1).unwrap_err();
    assert!(matches!(err, SyncError::Remote(503)));
}

#[test]
fn snapshots_are_pruned_to_keep_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    seed_live_db(&live);
    let target = FakeTarget::default();
    let mut settings = sync_settings_for(&dir, "webdav");
    settings.keep_snapshots = 2;
    for day in 1..=4 {
        sync_with_target(&settings, &live, &target, "dev-1", "Mac", "macos", day * 86_400).unwrap();
    }
    let snaps = target.keys_with_prefix("gamelife/dev-1/snapshots/");
    assert_eq!(snaps.len(), 2);
}
```

`FakeTarget` is an in-memory `RemoteTarget` in `mod tests` (a `Mutex<BTreeMap<String, Vec<u8>>>` plus an optional forced error). `sync_settings_for` writes a `SyncSettings` pointing at a tempdir; `seed_live_db` is the Task 2 helper.

- [ ] **Step 2: Run the tests, confirm they fail**

```bash
cargo test --offline -p gamelife sync::tests
```

- [ ] **Step 3: Implement**

`device_id()` reads `app_meta['device_id']`; when absent it generates 16 random bytes via `getrandom` and stores the lowercase hex. It takes a `&Connection` internally so tests can point it at a tempdir database — do not let it reach for `HOME`.

`sync_with_target` is the pure, injectable core (tests drive it directly):

1. `build_snapshot(live, tmp, scope)` into a temp dir.
2. `PUT {remote}/{device_id}/latest.db`.
3. `PUT {remote}/{device_id}/snapshots/{yyyymmdd}.db` using the **UTC** day of `now`.
4. `GET {remote}/devices.json` (404 → empty), `update_registry`, `PUT` it back.
5. Prune `snapshots/` to `keep_snapshots` by name order (the names are date-ordered), deleting the oldest.
6. Delete the temp file.

`sync_now(settings)` is the thin wrapper that resolves `device_id` / label / platform, builds the target from settings, calls `sync_with_target` with `now_secs()`, and writes the outcome into `app_meta` under `sync_last_result` (a JSON blob of `SyncOutcome`, or the error string) so the settings page can show it without a network call.

`target_from_settings` reads the password from `crate::keychain::get_in(path, "sync-webdav-password")` for `webdav` and `"sync-s3-secret"` for `s3`, and errors with `SyncError::Config` when the URL or credentials are missing. It never reads credentials from `config.json`.

- [ ] **Step 4: Run the tests, confirm they pass**

```bash
cargo test --offline -p gamelife sync::tests
```

- [ ] **Step 5: Commit** — `feat: orchestrate snapshot upload with a device registry`

---

### Task 6: Periodic and on-exit triggers

**Files:**
- Modify: `src-tauri/src/scheduler.rs` (the tick that already runs the sampler), `src-tauri/src/lib.rs` (the tray 退出 path)
- Test: `src-tauri/src/scheduler.rs`, `mod tests`

**Interfaces:**
- Consumes: `crate::sync::{sync_now, is_due}`.
- Produces: `pub fn is_due(last_sync_at: Option<i64>, now: i64, interval_minutes: i64) -> bool` (pure).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn is_due_respects_the_interval_and_the_disabled_state() {
    assert!(is_due(None, 1_000, 60));
    assert!(!is_due(Some(1_000), 1_000 + 60 * 60 - 1, 60));
    assert!(is_due(Some(1_000), 1_000 + 60 * 60, 60));
}

#[test]
fn is_due_is_never_true_for_a_non_positive_interval() {
    assert!(!is_due(None, 1_000, 0));
    assert!(!is_due(None, 1_000, -5));
}
```

- [ ] **Step 2: Run the tests, confirm they fail**

```bash
cargo test --offline -p gamelife scheduler::tests::is_due
```

- [ ] **Step 3: Implement**

In the sampler tick, after the existing slot work: when `settings.sync.enabled` and `is_due(app_meta['sync_last_at'], now, interval)`, spawn the sync on a **detached thread** so a slow remote cannot stretch the 15-second tick. Record `sync_last_at` only on success — a failure should retry on the next tick, not wait out the whole interval. Never propagate a sync error into the sampler's `Result`.

On tray → 退出, call `sync_now` with a short timeout (`SYNC_EXIT_TIMEOUT_SECS`, 5) before `ALLOW_EXIT` lets the process go. A failure or timeout must not block exit.

- [ ] **Step 4: Run the tests, confirm they pass**

```bash
cargo test --offline -p gamelife scheduler::tests
```

- [ ] **Step 5: Commit** — `feat: trigger sync on a timer and before exit`

---

### Task 7: Tauri commands

**Files:**
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` (register the commands)
- Modify: `src/lib/api.ts`

**Interfaces:**
- Produces (all `async` where they touch the network):

```rust
pub fn sync_status() -> Result<SyncStatusView, String>;              // cached, no network
pub async fn sync_now_cmd() -> Result<SyncStatusView, String>;       // spawn_blocking
pub async fn sync_test_connection() -> Result<SyncStatusView, String>;
pub fn sync_set_credentials(password: String) -> Result<(), String>;
pub async fn sync_list_devices() -> Result<Vec<DeviceView>, String>;
```

`SyncStatusView { enabled, target, url, scope, interval_minutes, last_at, last_ok, last_error, snapshot_bytes, devices }` — `#[serde(rename_all = "camelCase")]`.

- [ ] **Step 1: Implement**

`sync_status` reads `app_meta['sync_last_result']` and `app_meta['sync_last_at']` only — opening 设置 must not hit the network (the same rule that made 设置 → TickTick hang before `ticktick_tree` was split). `sync_now_cmd` and `sync_test_connection` follow `ticktick_sync`: `tauri::async_runtime::spawn_blocking(...)`. `sync_set_credentials` writes `secrets.json` via `keychain::set_in` and never returns the value back to the frontend.

Register all of them in the `invoke_handler` list in `lib.rs`, and add the typed wrappers to `src/lib/api.ts` — no `invoke()` anywhere else.

- [ ] **Step 2: Run the gate**

```bash
cargo test --offline -p gamelife
```

- [ ] **Step 3: Commit** — `feat: expose sync commands`

---

### Task 8: 设置 → 云端备份 card

**Files:**
- Modify: `src/pages/Settings.tsx`, `src/lib/api.ts`
- Test: `src/lib/sync.test.ts` (new, for any view-model helper)

**Interfaces:**
- Consumes: the Task 7 commands.
- Produces: a `云端备份` card with 开关 / 目标类型 / 地址 / 账号 / 凭据（只写不读回）/ 同步范围 / 间隔 / 保留快照数 / 设备名 / 立即同步 / 测试连接 / 设备列表 / 最近一次结果.

- [ ] **Step 1: Implement**

Follow the existing TickTick card's shape in `Settings.tsx`: a `Card` with `rounded-xl border bg-card`, `Switch` for 开关, `Input` / `Select` from `src/components/ui/*`, `Button` for 立即同步 / 测试连接, and a `Toaster` for the result. Colours come from tokens only — a success state uses the green token, a failure the red one, never a hex.

The credentials field is `type="password"` and is never prefilled from a status call. Scope is a `Select` with two options: `仅判定与汇总（不含窗口标题）` and `含原始采样（含窗口标题与路径）` — the second shows an inline amber warning that it uploads window titles.

Opening 设置 must render from the cached status alone; only 立即同步 / 测试连接 / 同步任务 hit the network.

- [ ] **Step 2: Run the gate**

```bash
npx vitest run --dir src
npm run build
```

- [ ] **Step 3: Commit** — `feat: add the cloud backup settings card`

---

### Task 9: Restore

**Files:**
- Modify: `src-tauri/src/sync.rs`, `src-tauri/src/commands.rs`
- Test: `src-tauri/src/sync.rs`, `mod tests`

**Interfaces:**
- Produces: `pub fn restore_from_bytes(bytes: &[u8], dest: &Path) -> Result<(), SyncError>`; `pub async fn sync_restore(device_id: String) -> Result<(), String>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn restore_rejects_a_file_that_is_not_a_sqlite_database() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("restored.db");
    assert!(matches!(restore_from_bytes(b"not a database", &dest), Err(SyncError::Db(_))));
    assert!(!dest.exists());
}

#[test]
fn restore_writes_a_valid_database_and_refuses_to_clobber_the_live_one() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    seed_live_db(&live);
    let snap = dir.path().join("snap.db");
    build_snapshot(&live, &snap, SCOPE_AGGREGATE).unwrap();
    let bytes = std::fs::read(&snap).unwrap();

    let dest = dir.path().join("restored.db");
    restore_from_bytes(&bytes, &dest).unwrap();
    let conn = crate::db::open(&dest).unwrap();
    let slots: i64 = conn.query_row("SELECT COUNT(*) FROM slots", [], |r| r.get(0)).unwrap();
    assert!(slots >= 1);

    assert!(matches!(restore_from_bytes(&bytes, &live), Err(SyncError::Config(_))));
}
```

- [ ] **Step 2: Run the tests, confirm they fail**

```bash
cargo test --offline -p gamelife sync::tests
```

- [ ] **Step 3: Implement**

`restore_from_bytes` writes to a temp file, then verifies the SQLite header (`"SQLite format 3\0"`) **and** runs `PRAGMA integrity_check` on it, and only then renames it into place — never leave a half-written database where the app might open it. It refuses to overwrite the live `gamelife.db`, returning `SyncError::Config`. A 401/404 from the remote surfaces as `Auth` / `Remote`, not as a corrupt file.

`sync_restore` fetches `{remote}/{device_id}/latest.db`, verifies, and writes `gamelife.restored.db` next to the live database, returning the path. The user renames it manually after quitting — the app must not swap the live database out from under a running sampler.

- [ ] **Step 4: Run the gate**

```bash
cargo test --offline -p gamelife
```

- [ ] **Step 5: Commit** — `feat: restore a database from a remote snapshot`

---

## Phase two — multi-device ledger

Design in the spec. The parts that are additive and behaviour-neutral are being built first, so that nothing about settlement timing is committed before the user signs off on it.

- **Built and green:** T10 (tagging), T11 (`merged.db`), T12 (owner rule), T13 (readiness gate). T10 and T11 touch only the migration and the sync module; T12/T13 are pure. No judgment path, no ledger write, no behaviour change for a single-device install.
- **Unblocked and next:** T14 (settlement) and T15 (今日 showing 待结算预览). §13.1 was decided on 2026-09-14 — the user accepted that with two devices the 今日 page's 能量 becomes a 待结算预览. Still not started.

Outline, in dependency order:

- [x] **T10 `device_id` tagging** — **done**. `DEVICE_TAGGED_TABLES` (8 tables) each gain `device_id TEXT NOT NULL DEFAULT ''`; `db::local_device_id` mints the id once into `app_meta` and is now the *only* implementation (`sync::device_id` delegates to it, so the remote directory name and the value written into rows cannot disagree). Existing rows are stamped by an `UPDATE … WHERE device_id = ''`; **new** rows are stamped by an `AFTER INSERT` trigger per table, because there are a dozen production insert sites across `sampler.rs` / `scheduler.rs` / `resolve.rs` / `commands.rs` and a missed one would silently produce an unattributable row. The trigger reads the id from `app_meta` rather than baking it in, so a restored snapshot re-stamps with the id it carries; verified that `VACUUM INTO` preserves triggers. `user_version` stays 3.
  Test: `migrate_tags_synced_tables_with_this_device_id` — all eight columns exist, every pre-existing row carries the id, a row inserted after migration is tagged, an explicitly tagged row is not overwritten, and re-migrating does not mint a second identity.
- [x] **T11 `merged.db` schema and rebuild** — **done**, plus `wishes.updated_at`.
  Design recorded as spec **§5.3**. The merged tables are created with `CREATE TABLE … AS SELECT` from the first snapshot that has them, so there is no second set of `CREATE TABLE` statements to keep in step with `db.rs` — that is the whole reason §3.1 chose whole-database snapshots. §5.2's primary keys become `UNIQUE INDEX`es, since `CREATE TABLE AS SELECT` does not carry constraints.
  - Each device attaches, merges inside its own transaction, and detaches, so a snapshot that fails halfway leaves no rows. A device that is merely offline is recorded in `MergedReport::skipped` and does not abort the rebuild.
  - A pre-T10 snapshot lacking `device_id` gets `ALTER TABLE src.<t> ADD COLUMN device_id TEXT` + `UPDATE … SET device_id = ?1` from its **own directory name**. Deliberately *not* `db::migrate`: that would mint a fresh identity for someone else's database and re-attribute their history.
  - §7.2's owner filter calls the Rust `slot_owner` rather than restating the rule as a SQL window function; two implementations of one rule drift.
  - `dedupe_merged` collapses each table onto its key. Device-keyed tables have nothing to collapse; `wishes` takes the newest `updated_at`, everything else the lowest `rowid`, so the result never depends on merge order.
  - A rebuild with nothing readable leaves the previous view alone instead of blanking the statistics page.
  - `wishes.updated_at` added to the live schema and stamped by `insert_wish` / `update_wish` / `archive_wish`; `db::now_unix()` factored out of the two inline `SystemTime` computations.
  - 10 tests.
  - **Not done:** uploading `merged.db` to `<remote>/gamelife/merged/latest.db` (§6 lists it as optional). Local only for now.
- [x] **T12 Owner rule** — **done**. `sync::slot_owner(&[(device_id, observed_seconds)])`: max `observed_seconds`, tie → smallest `device_id`, reversed-id comparison inside `max_by` so it stays a total order. `""` for an empty slice. 5 tests, including order-independence (the property that actually matters: every device must derive the same owner from the same merged rows) and the all-zero `unobserved` case — a slot nobody observed still gets a deterministic owner, who then finds it `unobserved` and pays nothing, rather than "no owner", which would be indistinguishable from "not settled yet".
- [x] **T13 Readiness gate** — **done**. `sync::day_is_ready(day, devices, snapshots, now, grace_hours)` with `SnapshotCoverage { device_id, taken_at }` and `SETTLE_GRACE_HOURS = 36`. Coverage is a **timestamp**, not a day: a snapshot taken mid-day may still be missing that evening, so it only answers for D once taken after D ended. `≤1` registered device short-circuits to `true`, which is what keeps phase two invisible until a second device exists. `scheduler::local_day_end` became `pub(crate)` so there is one definition of when a local day ends. 7 tests.
- [ ] **T14 Settlement** — split into three, because the risky part (touching the ledger) is much smaller than it first looked. Design recorded as spec **§7.5**.
  - [x] **T14a Deferred destination** — **done**. `SettlementMode { Immediate, Deferred }` stored in `app_meta['settle_mode']`; a new `pending_rewards` table with the *same* primary key as `ledger`; `resolve.rs` routes all six reward writes through one `record_reward` helper. The reward computation is untouched — deferral is a pure change of destination, which is exactly the property the tests state. `admin_xp_slots_today` counts whichever destination the mode writes to, so the four-a-day cap still behaves per device. Unrecognised or absent mode reads as `Immediate`: the failure direction is always "pay as usual". 5 tests, including a direct assertion that the two modes produce identical event sets.
  - [ ] **T14b Settler** — for each day with pending rewards, if `day_is_ready`, take the owner map for that day from `merged.db`, pay **only this device's owned slots** in `slot_start` order, then delete those pending rows. Apply the day-level four-slot cap on `xp_admin` here. §7.3's "both insert, the unique key arbitrates" is the safety net; ownership filtering is the mechanism.
  - [ ] **T14c Mode plumbing** — the sync flow writes `settle_mode` from the registry it already reads (≥2 devices → `deferred`, ≤1 → `immediate`), and `days.settled_at` / `outcome` follow §7.4.
- [ ] **T15 统计 / 商店 read the merged view** — and 今日 shows 待结算预览 when more than one device is registered.
- [ ] **T16 Settings card** — device list, 结算宽限期, per-device last-seen.

## Verification

```bash
cargo test --offline -p gamelife-core
cargo test --offline -p gamelife
npx vitest run --dir src
npm run build
```

Manual, phase one:

1. `sync.enabled = false` → no outbound request; behaviour byte-identical to today.
2. Configure a WebDAV target (坚果云 / Nextcloud), 立即同步 → `gamelife/<device_id>/latest.db` and `gamelife/devices.json` appear; the settings card shows the byte count and time.
3. Inspect the uploaded file: `PRAGMA integrity_check` is `ok`, `SELECT COUNT(*) FROM samples` is 0, and `strings` finds no window title and no `secrets`.
4. Delete the live database, `sync_restore`, rename the result into place, relaunch → 统计 shows the same week and the wallet the same balance.
5. Point the target at an unreachable host → sampling and judgment continue, the card shows the failure, nothing hangs.
