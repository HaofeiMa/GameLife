# A1 In-Process Native Observation Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在单进程内用原生 API 替换 `osascript` / `screencapture`，稳定采集 S2 字段（含 Chrome / Safari / Arc 可选 URL），截图对准真正的 `CGWindowID`。

**Architecture:** 把 `macos.rs` 拆成 `macos/` 下的纯函数与 OS 绑定。`sample_once` 每拍只调一次 `observe_window()`；`MacSampleSource` 用 `ObservationState` 保存最近快照，截图只用其中的 `cg_window_id`。`gamelife-core` 不动。

**Tech Stack:** Tauri 2、Rust、`objc2` 0.6 / `objc2-app-kit` 0.3 / `objc2-screen-capture-kit` 0.3、ApplicationServices AX C API、现有 `image` crate 校验 JPEG。

**Spec:** `docs/superpowers/specs/2026-09-11-observation-engine-design.md`

## Global Constraints

- 实现前 Wave 1 schema 必须已在工作树：`samples.document_path` / `bundle_id` / `secure_input`，`slots.screenshot_path` / `capture_context_json`，`SampleSource::capture_context`。
- 只有一个 GameLife 进程；不嵌入 ActivityWatch / screenpipe。
- 采样间隔仍为 15 秒；每槽仍一张随机前台窗口截图。禁止连拍、禁止每 15 秒截图。
- `gamelife-core` 的 `CaptureContext` 不加 `CGWindowID`。
- `document_path` 只来自 `AXDocument` 或 `file:` URL，再经 `normalize_document_path`。禁止从标题伪造。
- `sample_once` 禁止调用 `capture_context()`。`capture_context()` 只在 `tick_capture` 即将截图时调用。
- 无辅助功能 → 该拍 `unobserved`，不得用 `NSWorkspace` 偷偷记 app。
- 截图最低 macOS 14；更旧系统截图 `missed`，禁止回退 `screencapture`。
- AX 超时 400ms；浏览器 URL 超时 1s；截图超时 2s。超时不堵主窗口。
- 测试禁止 `std::env::set_var("HOME", …)`。不要去改无关测试里已有的 `set_var`。
- 凡改 `src-tauri/`：`cargo test --offline -p gamelife` 必须编译并跑过（`--no-run` 不够）。
- 不做判定 / 账本 / Quest / 商店；不把浏览器自动化缺失做成权限横幅必显项。

---

## File Structure

```
src-tauri/src/macos.rs                      DELETE（改为目录）
src-tauri/src/macos/mod.rs                  公开 API、常量、非 macOS stub
src-tauri/src/macos/document.rs             ax_document_raw
src-tauri/src/macos/window_id.rs            CgWindowEntry、pick_front_window_id
src-tauri/src/macos/browser.rs               is_url_browser、url_for、fetch_browser_url
src-tauri/src/macos/state.rs                FrontmostSnapshot、ObservationState
src-tauri/src/macos/snapshot.rs             NSWorkspace + AX + CGWindowList
src-tauri/src/macos/capture.rs              ScreenCaptureKit → JPEG
src-tauri/src/macos/input.rs                idle / 锁屏 / secure / 权限（现有 FFI 搬过来）
src-tauri/src/sampler.rs                    ObservedWindow、observe_window、sample_once
src-tauri/src/scheduler.rs                  tick_capture 必须传入 capture_fn
src-tauri/Cargo.toml                        macOS 专用 objc2 依赖
src-tauri/Info.plist                        自动化用途说明
src/pages/Settings.tsx                     一句可选自动化说明
README.md                                   权限表
```

---

### Task 1: `macos/` 目录 + `ax_document_raw`

**Files:**
- Create: `src-tauri/src/macos/mod.rs`（由现有 `macos.rs` 改名）
- Create: `src-tauri/src/macos/document.rs`
- Delete: `src-tauri/src/macos.rs`

**Interfaces:**
- Consumes: `gamelife_core::normalize_document_path`
- Produces: `pub fn ax_document_raw(ax_document: Option<&str>, ax_url: Option<&str>) -> Option<String>`

- [ ] **Step 1: 把 `macos.rs` 改成目录**

```bash
mkdir -p src-tauri/src/macos
git mv src-tauri/src/macos.rs src-tauri/src/macos/mod.rs
```

`lib.rs` 的 `pub mod macos;` 不用改。

- [ ] **Step 2: 写失败测试**

在 `src-tauri/src/macos/document.rs`：

```rust
pub fn ax_document_raw(ax_document: Option<&str>, ax_url: Option<&str>) -> Option<String> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::ax_document_raw;
    use gamelife_core::normalize_document_path;

    #[test]
    fn ax_document_wins_over_url() {
        let raw = ax_document_raw(
            Some("/Users/me/HDP/train.py"),
            Some("file:///Users/me/other.py"),
        );
        assert_eq!(
            raw.as_deref().and_then(normalize_document_path).as_deref(),
            Some("/Users/me/HDP/train.py")
        );
    }

    #[test]
    fn file_url_used_when_document_missing() {
        let raw = ax_document_raw(None, Some("file:///Users/me/My%20Project/a.py"));
        assert_eq!(
            raw.as_deref().and_then(normalize_document_path).as_deref(),
            Some("/Users/me/My Project/a.py")
        );
    }

    #[test]
    fn https_url_rejected() {
        assert_eq!(
            ax_document_raw(None, Some("https://arxiv.org/abs/123")),
            None
        );
    }

    #[test]
    fn window_title_never_passed_in_becomes_none() {
        assert_eq!(ax_document_raw(None, None), None);
        assert_eq!(
            ax_document_raw(None, Some("train.py — HDP")),
            None
        );
        assert_eq!(normalize_document_path("train.py — HDP"), None);
    }

    #[test]
    fn empty_and_missing_value_rejected() {
        assert_eq!(ax_document_raw(Some(""), Some("missing value")), None);
        assert_eq!(ax_document_raw(Some("missing value"), None), None);
    }
}
```

在 `macos/mod.rs` 加 `mod document; pub use document::ax_document_raw;`。

- [ ] **Step 3: 跑测试确认失败**

Run: `cargo test --offline -p gamelife -- macos::document::tests::https_url_rejected`

Expected: FAIL（`unimplemented!` 或模块找不到）

- [ ] **Step 4: 最小实现**

```rust
fn nonempty(s: Option<&str>) -> Option<&str> {
    let t = s?.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("missing value") {
        None
    } else {
        Some(t)
    }
}

pub fn ax_document_raw(ax_document: Option<&str>, ax_url: Option<&str>) -> Option<String> {
    if let Some(doc) = nonempty(ax_document) {
        return Some(doc.to_string());
    }
    let url = nonempty(ax_url)?;
    if url.to_ascii_lowercase().starts_with("file:") {
        Some(url.to_string())
    } else {
        None
    }
}
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test --offline -p gamelife -- macos::document`

Expected: PASS。再跑：`cargo test --offline -p gamelife`（整 crate 绿）。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/macos.rs src-tauri/src/macos
git commit -m "$(cat <<'EOF'
refactor: split macos.rs into a module directory

Add ax_document_raw so file: URLs can become document_path without
treating window titles or https URLs as paths.
EOF
)"
```

---

### Task 2: 从窗口列表挑选 `CGWindowID`

**Files:**
- Create: `src-tauri/src/macos/window_id.rs`
- Modify: `src-tauri/src/macos/mod.rs`（`mod window_id; pub use window_id::{CgWindowEntry, pick_front_window_id};`）

**Interfaces:**
- Consumes: 无
- Produces:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CgWindowEntry {
    pub owner_pid: i32,
    pub layer: i64,
    pub window_id: u32,
    pub on_screen: bool,
}

pub fn pick_front_window_id(pid: i32, windows: &[CgWindowEntry]) -> Option<u32>
```

列表视为前到后（index 0 最前）。选第一扇 `on_screen && layer == 0 && owner_pid == pid`。

- [ ] **Step 1: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::{pick_front_window_id, CgWindowEntry};

    fn w(pid: i32, layer: i64, id: u32, on_screen: bool) -> CgWindowEntry {
        CgWindowEntry {
            owner_pid: pid,
            layer,
            window_id: id,
            on_screen,
        }
    }

    #[test]
    fn picks_first_layer0_for_pid() {
        let windows = [
            w(7, 0, 42, true),
            w(7, 0, 43, true),
        ];
        assert_eq!(pick_front_window_id(7, &windows), Some(42));
    }

    #[test]
    fn skips_other_pid_and_menu_layer() {
        let windows = [
            w(1, 0, 99, true),
            w(7, 25, 100, true),
            w(7, 0, 42, true),
        ];
        assert_eq!(pick_front_window_id(7, &windows), Some(42));
    }

    #[test]
    fn skips_offscreen() {
        let windows = [w(7, 0, 42, false), w(7, 0, 8, true)];
        assert_eq!(pick_front_window_id(7, &windows), Some(8));
    }

    #[test]
    fn ax_window_id_is_not_used() {
        let ax_id = 9_000_001u32;
        let windows = [w(7, 0, 42, true)];
        assert_eq!(pick_front_window_id(7, &windows), Some(42));
        assert_ne!(pick_front_window_id(7, &windows), Some(ax_id));
    }

    #[test]
    fn none_when_no_match() {
        assert_eq!(pick_front_window_id(7, &[w(1, 0, 42, true)]), None);
    }
}
```

函数体先 `unimplemented!()`。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife -- macos::window_id::tests::picks_first_layer0_for_pid`

Expected: FAIL

- [ ] **Step 3: 最小实现**

```rust
pub fn pick_front_window_id(pid: i32, windows: &[CgWindowEntry]) -> Option<u32> {
    windows
        .iter()
        .find(|w| w.on_screen && w.layer == 0 && w.owner_pid == pid)
        .map(|w| w.window_id)
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife -- macos::window_id`

Expected: PASS。再跑：`cargo test --offline -p gamelife`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/macos/window_id.rs src-tauri/src/macos/mod.rs
git commit -m "$(cat <<'EOF'
feat: pick CGWindowID from on-screen layer-0 list

Keep AX window ids out of the capture path so screenshots target
the same window the snapshot described.
EOF
)"
```

---

### Task 3: 浏览器允许列表

**Files:**
- Create: `src-tauri/src/macos/browser.rs`
- Modify: `src-tauri/src/macos/mod.rs`

**Interfaces:**
- Consumes: 无
- Produces:

```rust
pub fn is_url_browser(bundle_id: Option<&str>, app: &str) -> bool

pub fn url_for(
    bundle_id: Option<&str>,
    app: &str,
    fetch: impl FnOnce() -> Option<String>,
) -> Option<String>
```

`url_for` 仅当 `is_url_browser` 为真才调用 `fetch`。

允许列表：

| bundle id | 应用名 |
| --- | --- |
| `com.google.Chrome` | `Google Chrome` |
| `com.apple.Safari` | `Safari` |
| `company.thebrowser.Browser` | `Arc` |

名称精确匹配（不小写化）。bundle 命中即可，不依赖本地化名。

- [ ] **Step 1: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::{is_url_browser, url_for};

    #[test]
    fn allowlist_by_bundle_or_name() {
        assert!(is_url_browser(Some("com.google.Chrome"), "Chrome"));
        assert!(is_url_browser(None, "Google Chrome"));
        assert!(is_url_browser(Some("com.apple.Safari"), ""));
        assert!(is_url_browser(Some("company.thebrowser.Browser"), "Arc"));
        assert!(is_url_browser(None, "Arc"));
    }

    #[test]
    fn rejects_cursor_wechat_and_empty() {
        assert!(!is_url_browser(
            Some("com.todesktop.230313mzl4w4u92"),
            "Cursor"
        ));
        assert!(!is_url_browser(None, "WeChat"));
        assert!(!is_url_browser(None, ""));
        assert!(!is_url_browser(Some("com.microsoft.edgemac"), "Microsoft Edge"));
    }

    #[test]
    fn url_for_skips_fetch_when_not_browser() {
        let called = std::cell::Cell::new(false);
        let url = url_for(Some("com.todesktop.230313mzl4w4u92"), "Cursor", || {
            called.set(true);
            Some("https://example.com".into())
        });
        assert_eq!(url, None);
        assert!(!called.get());
    }

    #[test]
    fn url_for_calls_fetch_for_safari() {
        let called = std::cell::Cell::new(false);
        let url = url_for(Some("com.apple.Safari"), "Safari", || {
            called.set(true);
            Some("https://arxiv.org/abs/1?x=2".into())
        });
        assert_eq!(url.as_deref(), Some("https://arxiv.org/abs/1?x=2"));
        assert!(called.get());
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife -- macos::browser::tests::url_for_skips_fetch_when_not_browser`

Expected: FAIL

- [ ] **Step 3: 最小实现**

```rust
const BUNDLES: &[&str] = &[
    "com.google.Chrome",
    "com.apple.Safari",
    "company.thebrowser.Browser",
];
const NAMES: &[&str] = &["Google Chrome", "Safari", "Arc"];

pub fn is_url_browser(bundle_id: Option<&str>, app: &str) -> bool {
    if let Some(b) = bundle_id {
        if BUNDLES.contains(&b) {
            return true;
        }
    }
    NAMES.contains(&app)
}

pub fn url_for(
    bundle_id: Option<&str>,
    app: &str,
    fetch: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if is_url_browser(bundle_id, app) {
        fetch()
    } else {
        None
    }
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife -- macos::browser`

Expected: PASS。再跑：`cargo test --offline -p gamelife`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/macos/browser.rs src-tauri/src/macos/mod.rs
git commit -m "$(cat <<'EOF'
feat: allowlist Chrome Safari and Arc for optional URLs

Keep Apple Events off the sample path unless the frontmost app is
one of the three browsers.
EOF
)"
```

---

### Task 4: `ObservationState`

**Files:**
- Create: `src-tauri/src/macos/state.rs`
- Modify: `src-tauri/src/macos/mod.rs`

**Interfaces:**
- Consumes: 无
- Produces:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrontmostSnapshot {
    pub app: String,
    pub title: String,
    pub bundle_id: Option<String>,
    pub document_raw: Option<String>,
    pub pid: Option<i32>,
    pub cg_window_id: Option<u32>,
}

pub struct ObservationState { /* last: Mutex<Option<FrontmostSnapshot>> */ }

impl ObservationState {
    pub fn new() -> Self;
    pub fn store(&self, snap: FrontmostSnapshot);
    pub fn last(&self) -> Option<FrontmostSnapshot>;
    pub fn take_cg_window_id(&self) -> Option<u32>;
}
```

`take_cg_window_id`：返回当前 `cg_window_id`，并把 `last` 里该项置 `None`（其它字段保留）。`last` 为空或 id 为空 → `None`。

- [ ] **Step 1: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::{FrontmostSnapshot, ObservationState};

    #[test]
    fn take_clears_only_window_id() {
        let st = ObservationState::new();
        assert_eq!(st.take_cg_window_id(), None);
        st.store(FrontmostSnapshot {
            app: "Preview".into(),
            title: "a.pdf".into(),
            bundle_id: Some("com.apple.Preview".into()),
            document_raw: Some("/tmp/a.pdf".into()),
            pid: Some(9),
            cg_window_id: Some(42),
        });
        assert_eq!(st.take_cg_window_id(), Some(42));
        assert_eq!(st.take_cg_window_id(), None);
        let last = st.last().unwrap();
        assert_eq!(last.app, "Preview");
        assert_eq!(last.cg_window_id, None);
        assert_eq!(last.document_raw.as_deref(), Some("/tmp/a.pdf"));
    }

    #[test]
    fn last_empty_means_no_browser_snapshot() {
        let st = ObservationState::new();
        assert!(st.last().is_none());
        st.store(FrontmostSnapshot {
            app: "Safari".into(),
            ..FrontmostSnapshot::default()
        });
        assert_eq!(st.last().unwrap().app, "Safari");
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife -- macos::state::tests::take_clears_only_window_id`

Expected: FAIL

- [ ] **Step 3: 最小实现**

```rust
use std::sync::Mutex;

pub struct ObservationState {
    last: Mutex<Option<FrontmostSnapshot>>,
}

impl ObservationState {
    pub fn new() -> Self {
        Self {
            last: Mutex::new(None),
        }
    }

    pub fn store(&self, snap: FrontmostSnapshot) {
        *self.last.lock().expect("observation state") = Some(snap);
    }

    pub fn last(&self) -> Option<FrontmostSnapshot> {
        self.last.lock().expect("observation state").clone()
    }

    pub fn take_cg_window_id(&self) -> Option<u32> {
        let mut g = self.last.lock().expect("observation state");
        let snap = g.as_mut()?;
        snap.cg_window_id.take()
    }
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife -- macos::state`

Expected: PASS。再跑：`cargo test --offline -p gamelife`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/macos/state.rs src-tauri/src/macos/mod.rs
git commit -m "$(cat <<'EOF'
feat: remember last frontmost snapshot for capture

Store CGWindowID beside metadata so capture_context and the JPEG
refer to the same window without putting OS ids in core types.
EOF
)"
```

---

### Task 5: `observe_window` + `tick_capture` 注入 `capture_fn`

**Files:**
- Modify: `src-tauri/src/sampler.rs`
- Modify: `src-tauri/src/scheduler.rs`（`tick_capture` / `tick_capture_impl` 签名；所有测试调用点）

**Interfaces:**
- Consumes: 无
- Produces:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObservedWindow {
    pub app: String,
    pub title: String,
    pub bundle_id: Option<String>,
    pub document_path: Option<String>,
}

pub trait SampleSource: Send + Sync {
    fn observe_window(&self) -> Result<ObservedWindow, ()>;
    fn capture_frontmost_window(&self, path: &std::path::Path) -> Result<(), ()>;
    // 其余方法保持；frontmost_app / document_path / bundle_id 仅测试兼容
}

pub fn tick_capture(
    conn: &Connection,
    day: &str,
    slot_start_ts: i64,
    now: i64,
    locked: bool,
    paused: bool,
    screen_recording: bool,
    capture_context: impl Fn() -> CaptureContext,
    capture_fn: impl Fn(&Path) -> Result<(), ()>,
) -> Result<(), DbOpError>
```

删除 `default_capture_fn`。`sample_once` 只通过 `observe_window` 取 app/title/bundle/document，再 `optional_browser_url()`，再 `tick_capture(..., || source.capture_context(), |p| source.capture_frontmost_window(p))`。

`FakeSampleSource` 增加 `observe_window_calls: AtomicU32`、`legacy_calls: AtomicU32`。`observe_window` +1；`frontmost_app` / `document_path` / `bundle_id` +1 `legacy_calls`。`capture_frontmost_window` 返回 `Err(())`。`optional_browser_url` 仍原样返回 `self.url`（不受允许列表约束）。

- [ ] **Step 1: 写失败测试**

在 `sampler.rs` 测试模块新增：

```rust
#[test]
fn sample_once_calls_observe_window_once_not_legacy_getters() {
    let mut conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let source = FakeSampleSource {
        app: "Cursor".into(),
        title: "train.py — HDP".into(),
        url: None,
        document_path: Some("train.py — HDP".into()),
        bundle_id: Some("com.todesktop.230313mzl4w4u92".into()),
        idle: 1,
        locked: false,
        secure: false,
        paused: false,
        metadata_observation_available: true,
        capture_observation_available: true,
        ..Default::default()
    };
    let ts = 1_700_000_000i64;
    let mut state = SamplerState::default();
    sample_once(&mut conn, &source, ts, &mut state).unwrap();
    assert_eq!(source.observe_window_calls.load(Ordering::Relaxed), 1);
    assert_eq!(source.legacy_calls.load(Ordering::Relaxed), 0);
    let path: Option<String> = conn
        .query_row("SELECT document_path FROM samples WHERE ts = ?1", [ts], |r| r.get(0))
        .unwrap();
    assert_eq!(path, None);
}
```

把 `SampleSource` 加上 `observe_window` / `capture_frontmost_window` 但 **先不改** `sample_once` 内部，让上述计数失败。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife -- sampler::tests::sample_once_calls_observe_window_once_not_legacy_getters`

Expected: FAIL（`observe_window_calls == 0` 或 trait 缺方法导致编译失败——若编译失败，先在 trait 上加方法并让 Fake 实现，再确认该断言失败）

- [ ] **Step 3: 改 `sample_once` 与 `tick_capture`**

`sample_once` 中元数据段改为：

```rust
let observed = match source.observe_window() {
    Ok(v) => v,
    Err(()) => ObservedWindow::default(),
};
let url = source
    .optional_browser_url()
    .map(|u| strip_url_query_fragment(&u));
let url_ref = url.as_deref().filter(|s| !s.is_empty());
let document_path = observed
    .document_path
    .as_deref()
    .and_then(normalize_document_path);
insert_sample(
    conn,
    ts,
    &day,
    &observed.app,
    &observed.title,
    url_ref,
    document_path.as_deref(),
    observed.bundle_id.as_deref(),
    idle,
    locked,
    paused,
    secure,
)?;
```

`scheduler.rs`：`tick_capture` 增加 `capture_fn` 参数并传给 `tick_capture_impl`。删除 `default_capture_fn`。测试里所有 `tick_capture(...)` 末尾加 `|_| Err(())`（已用 `tick_capture_impl` 并自己写文件的测试保持原 `capture_fn`）。

`MacSampleSource`：

```rust
fn observe_window(&self) -> Result<ObservedWindow, ()> {
    let (app, title) = crate::macos::frontmost_app().unwrap_or_default();
    Ok(ObservedWindow {
        app,
        title,
        bundle_id: crate::macos::bundle_id(),
        document_path: crate::macos::document_path(),
    })
}

fn capture_frontmost_window(&self, path: &std::path::Path) -> Result<(), ()> {
    crate::macos::capture_frontmost_window(path)
}
```

本 Task **暂时**仍三次读 OS（下一 Task 换成一次 snapshot）。`optional_browser_url` 仍直接调 `macos::optional_browser_url()`。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife -- sampler::tests::sample_once_calls_observe_window_once_not_legacy_getters`

Expected: PASS。再跑：`cargo test --offline -p gamelife`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/sampler.rs src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
feat: sample each tick through a single observe_window call

Stop asking app, document, and bundle separately so the native
snapshot can be shared, and pass capture_fn into tick_capture.
EOF
)"
```

---

### Task 6: 原生 `snapshot()`（NSWorkspace + AX + CGWindowList）

**Files:**
- Create: `src-tauri/src/macos/snapshot.rs`
- Create: `src-tauri/src/macos/input.rs`（从 `mod.rs` 挪走 idle / lock / secure / 权限 FFI，行为不变）
- Modify: `src-tauri/src/macos/mod.rs`（删除 `FRONTMOST_META_SCRIPT`、`osascript` 读前台、AppleScript window id）
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/sampler.rs`（`MacSampleSource` 持有 `ObservationState`，`observe_window` 调 `snapshot()`）

**Interfaces:**
- Consumes: `ax_document_raw`、`pick_front_window_id`、`ObservationState`、`FrontmostSnapshot`
- Produces:

```rust
pub const AX_TIMEOUT_SECS: f32 = 0.4;

pub fn snapshot() -> FrontmostSnapshot
```

macOS：`NSWorkspace.frontmostApplication` → 名、bundle、PID；对该 PID `AXUIElementCreateApplication`，`AXUIElementSetMessagingTimeout(..., 0.4)`，读 focused/main window 的 `AXTitle`、`AXDocument`、`AXURL`；`document_raw = ax_document_raw(...)`。`CGWindowListCopyWindowInfo(OnScreenOnly | ExcludeDesktopElements, 0)` 转成 `Vec<CgWindowEntry>` 再 `pick_front_window_id`。无前台应用 → `FrontmostSnapshot::default()`。AX 超时/失败：仍返回已有 app/bundle，title 空，`document_raw = None`。

非 macOS：`FrontmostSnapshot::default()`。

`MacSampleSource`：

```rust
pub struct MacSampleSource {
    paused: Arc<AtomicBool>,
    last: crate::macos::ObservationState,
}

fn observe_window(&self) -> Result<ObservedWindow, ()> {
    let snap = crate::macos::snapshot();
    self.last.store(snap.clone());
    Ok(ObservedWindow {
        app: snap.app,
        title: snap.title,
        bundle_id: snap.bundle_id,
        document_path: snap.document_raw,
    })
}
```

`optional_browser_url` **先**改成：

```rust
fn optional_browser_url(&self) -> Option<String> {
    let last = self.last.last()?;
    crate::macos::url_for(last.bundle_id.as_deref(), &last.app, || {
        crate::macos::fetch_browser_url(&last.app) // Task 8 才做超时；本 Task 可暂用现有 osascript 函数
    })
}
```

若 `fetch_browser_url` 尚未存在，本 Task 保留 `imp::optional_browser_url` 为 `fetch` 闭包，但必须走 `url_for`（非三款浏览器零次脚本）。

删除 `mod.rs` 里依赖 AppleScript 读 AXDocument 的测试；`parse_frontmost_meta_line` 一并删除。`frontmost_app` / `document_path` / `bundle_id` / `capture_context` 改为基于 `snapshot()`，供尚未切完的调用点使用。`capture_context`：

```rust
pub fn capture_context() -> CaptureContext {
    let snap = snapshot();
    let url = url_for(snap.bundle_id.as_deref(), &snap.app, || optional_browser_url_os(&snap.app));
    CaptureContext {
        app: snap.app,
        bundle_id: snap.bundle_id,
        title: snap.title,
        document_path: snap.document_raw.and_then(|s| gamelife_core::normalize_document_path(&s)),
        url,
        secure_input: secure_input_on(),
    }
}
```

注意：自由函数 `capture_context()` 仍无 `ObservationState`。真正把 id 存进 `last` 的是 `MacSampleSource::capture_context`（本 Task 改它）：

```rust
fn capture_context(&self) -> CaptureContext {
    let snap = crate::macos::snapshot();
    self.last.store(snap.clone());
    let url = crate::macos::url_for(snap.bundle_id.as_deref(), &snap.app, || {
        crate::macos::optional_browser_url()
    });
    CaptureContext { /* 同上，document 经 normalize */ }
}
```

Cargo.toml 增加（仅 macOS）：

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-foundation = "0.3"
objc2-app-kit = { version = "0.3", features = ["NSWorkspace", "NSRunningApplication"] }
objc2-core-foundation = "0.3"
```

CGWindowList / AX 用 `extern "C"` 即可，不必强上 objc2 的 AX 绑定。`NSWorkspace` 若 feature 名与 0.3.2 不符，以编译错误为准改 feature，不要换架构。

- [ ] **Step 1: 写失败测试**

在 `snapshot.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::macos::{pick_front_window_id, CgWindowEntry};

    #[test]
    fn snapshot_maps_window_list_through_picker() {
        let pid = 11;
        let entries = [
            CgWindowEntry {
                owner_pid: pid,
                layer: 0,
                window_id: 42,
                on_screen: true,
            },
        ];
        assert_eq!(pick_front_window_id(pid, &entries), Some(42));
    }

    #[test]
    #[ignore]
    fn snapshot_does_not_panic() {
        let _ = snapshot();
    }
}
```

再加：`MacSampleSource` 在 `last` 为空时 `optional_browser_url` 为 `None`。把 `MacSampleSource` 的 `last` 做成 `pub(crate)` 或给测试用 `#[cfg(test)] pub fn last_for_test`。更简单：在 `state.rs` 已覆盖空 last；本 Task 在 `sampler.rs` 测一个可注入 snapshot 的缝。

若 `MacSampleSource` 仍调真实 `snapshot()`，单元测试不要在无权限机上断言 app 非空。改为测试 `url_for` 与空 `last`：给 `MacSampleSource` 加 `#[cfg(test)] fn with_state(paused, ObservationState)`，store 一个 Cursor snapshot，`optional_browser_url` 必须为 `None` 且不依赖 OS。

```rust
#[test]
fn mac_source_skips_browser_fetch_for_cursor_snapshot() {
    let paused = Arc::new(AtomicBool::new(false));
    let src = MacSampleSource::new(paused);
    src.last.store(crate::macos::FrontmostSnapshot {
        app: "Cursor".into(),
        bundle_id: Some("com.todesktop.230313mzl4w4u92".into()),
        ..Default::default()
    });
    assert_eq!(src.optional_browser_url(), None);
}
```

为此把 `last` 设为 `pub(crate)`。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife -- sampler::tests::mac_source_skips_browser_fetch_for_cursor_snapshot`

Expected: FAIL（字段/方法尚不存在或仍走全局 osascript）

- [ ] **Step 3: 实现 `snapshot()` 并接线**

`snapshot.rs` 要点：

1. `NSWorkspace::sharedWorkspace().frontmostApplication()`（API 名以 objc2-app-kit 0.3 为准）。
2. AX：`kAXFocusedWindowAttribute` 失败则 `kAXMainWindowAttribute`；属性名用 `CFString`。
3. `CGWindowListCopyWindowInfo` 解析 `kCGWindowOwnerPID` / `kCGWindowLayer` / `kCGWindowNumber` / `kCGWindowIsOnscreen`（缺省当 on_screen=true）。
4. 每个 CF 对象 `CFRelease`。
5. 禁止再 `Command::new("osascript")` 读前台。

把 idle/lock/secure/permissions 挪到 `input.rs`，`mod.rs` 再导出，避免 `mod.rs` 过大。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife`

Expected: PASS（`#[ignore]` 的 smoke 不跑）。若 `target/debug/build` 因路径迁移报错，按 CLAUDE.md 清掉旧 `target/debug/build/*` 后再跑。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/macos src-tauri/src/sampler.rs src-tauri/Cargo.toml Cargo.lock
git commit -m "$(cat <<'EOF'
feat: snapshot frontmost window via NSWorkspace and AX

Replace AppleScript frontmost reads so document_path and CGWindowID
come from the same native pass.
EOF
)"
```

---

### Task 7: ScreenCaptureKit 截指定窗口

**Files:**
- Create: `src-tauri/src/macos/capture.rs`
- Modify: `src-tauri/src/macos/mod.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/sampler.rs`（`MacSampleSource::capture_frontmost_window`）

**Interfaces:**
- Consumes: `ObservationState::take_cg_window_id`、`image` crate
- Produces:

```rust
pub const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);

pub fn capture_window(window_id: u32, path: &Path) -> Result<(), ()>
```

macOS 14 以下或 `SCScreenshotManager` 不可用 → `Err(())`。超时 2s → `Err(())`。写完后文件长度为 0 或 `image::open(path)` 失败 → 删除残文件并 `Err(())`。禁止调用 `/usr/sbin/screencapture`。

`MacSampleSource::capture_frontmost_window`：

```rust
fn capture_frontmost_window(&self, path: &std::path::Path) -> Result<(), ()> {
    let id = self.last.take_cg_window_id().ok_or(())?;
    crate::macos::capture_window(id, path)
}
```

自由函数 `macos::capture_frontmost_window` 若仍被引用，改为 `Err(())` 或删除，调度器不得再走它。

- [ ] **Step 1: 写失败测试**

在 `capture.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::capture_window;
    use std::path::Path;

    #[test]
    fn missing_file_after_error_is_ok_to_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.jpg");
        let _ = capture_window(0, &path);
        assert!(
            !path.exists() || std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) == 0
        );
    }
}
```

在 `sampler.rs`：

```rust
#[test]
fn capture_without_window_id_is_err() {
    let src = MacSampleSource::new(Arc::new(AtomicBool::new(false)));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.jpg");
    assert!(src.capture_frontmost_window(&path).is_err());
    assert!(!path.exists());
}
```

在 `scheduler.rs` 现有 `tick_capture_impl` 成功测试之外新增（不要 `set_var("HOME")`）：用 `tick_capture_impl`，`capture_fn` 返回 `Err(())`，断言 `capture_status = Missed` 且 `screenshot_path` 与 `capture_context_json` 均为 NULL。

```rust
#[test]
fn tick_capture_err_is_missed_without_json() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let day = "2026-09-10";
    let ss = 0i64;
    conn.execute(
        "INSERT INTO slots (day, slot_start, capture_scheduled_at, capture_status)
         VALUES (?1, ?2, 100, 'Scheduled')",
        params![day, ss],
    )
    .unwrap();
    tick_capture_impl(
        &conn,
        day,
        ss,
        100,
        false,
        false,
        true,
        || capture_ctx("Cursor"),
        |_| Err(()),
    )
    .unwrap();
    let status: String = conn
        .query_row(
            "SELECT capture_status FROM slots WHERE day = ?1 AND slot_start = ?2",
            params![day, ss],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "Missed");
    let path: Option<String> = conn
        .query_row(
            "SELECT screenshot_path FROM slots WHERE day = ?1 AND slot_start = ?2",
            params![day, ss],
            |r| r.get(0),
        )
        .unwrap();
    let json: Option<String> = conn
        .query_row(
            "SELECT capture_context_json FROM slots WHERE day = ?1 AND slot_start = ?2",
            params![day, ss],
            |r| r.get(0),
        )
        .unwrap();
    assert!(path.is_none());
    assert!(json.is_none());
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife -- sampler::tests::capture_without_window_id_is_err`

Expected: FAIL

- [ ] **Step 3: 实现 `capture_window`**

Cargo.toml 增加：

```toml
objc2-screen-capture-kit = { version = "0.3", features = ["SCScreenshotManager", "SCShareableContent", "SCContentFilter"] }
block2 = "0.6"
objc2-app-kit = { version = "0.3", features = ["NSWorkspace", "NSRunningApplication", "NSBitmapImageRep"] }
```

实现步骤（macOS 14+）：

1. `SCShareableContent` 异步取窗口列表，`recv_timeout(CAPTURE_TIMEOUT)`。
2. 找到 `windowID == window_id` 的 `SCWindow`。
3. `SCContentFilter` 只含该窗口。
4. `SCScreenshotManager.captureImageWithFilter`（名称以 0.3.2 文档为准），同样 `recv_timeout(2s)`。
5. `CGImage` → `NSBitmapImageRep` → JPEG bytes → `std::fs::write`。
6. `image::open(path).map_err(|_| ())?`；失败则 `let _ = std::fs::remove_file(path)`。

回调不得在 UI 主线程同步等待；采样线程等 channel 即可。AppKit 若必须主线程，只把「触发采集」dispatch 过去，结果仍走 channel。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/macos/capture.rs src-tauri/src/macos/mod.rs src-tauri/src/sampler.rs src-tauri/src/scheduler.rs src-tauri/Cargo.toml Cargo.lock
git commit -m "$(cat <<'EOF'
feat: capture the stored CGWindowID with ScreenCaptureKit

Drop screencapture -l so grey-zone JPEGs match capture_context and
time out to missed instead of hanging the tray window.
EOF
)"
```

---

### Task 8: 浏览器 URL 超时读取

**Files:**
- Modify: `src-tauri/src/macos/browser.rs`（`fetch_browser_url` + 超时）
- Modify: `src-tauri/src/macos/mod.rs`（删除旧的 Chrome/Safari AppleScript 长脚本，或让它只转调 `fetch_browser_url`）
- Modify: `src-tauri/src/sampler.rs`（`optional_browser_url` / `capture_context` 的 fetch 闭包）

**Interfaces:**
- Consumes: `is_url_browser`、`url_for`、`ObservationState::last`
- Produces:

```rust
pub const BROWSER_URL_TIMEOUT: Duration = Duration::from_secs(1);

pub fn fetch_browser_url(app: &str) -> Option<String>
```

仅 Safari 用 `tell application "Safari" to return URL of current tab of front window`；`Google Chrome` 与 `Arc` 用 `tell application "<name>" to return URL of active tab of front window`。`Command::new("osascript")`，stdout piped，`try_wait` 轮询，超过 1s 则 `kill`。失败 → `None`。**禁止**在非允许列表 app 上调用本函数（由 `url_for` 保证）。

`MacSampleSource::optional_browser_url`：

```rust
fn optional_browser_url(&self) -> Option<String> {
    let last = self.last.last()?;
    crate::macos::url_for(last.bundle_id.as_deref(), &last.app, || {
        crate::macos::fetch_browser_url(&last.app)
    })
}
```

`last` 为空 → `None`，不读 `NSWorkspace`，不打 AX。

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn fetch_is_not_used_when_last_is_empty() {
    let src = MacSampleSource::new(Arc::new(AtomicBool::new(false)));
    assert_eq!(src.optional_browser_url(), None);
}

#[test]
fn kill_on_timeout_returns_none() {
    // 用一个必定超时的假 Command 不现实；测 wait helper：
    // pub(crate) fn wait_output_timeout(child, Duration) -> Option<Vec<u8>>
}
```

把超时等待抽成：

```rust
pub(crate) fn wait_output_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Option<Vec<u8>>
```

测试：`Command::new("sleep").arg("2")`，timeout 50ms → `None`，且 sleep 进程已被杀（`try_wait` 随后是 `Some`）。

```rust
#[test]
fn wait_output_timeout_kills_child() {
    let mut child = std::process::Command::new("sleep")
        .arg("2")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    assert!(wait_output_timeout(&mut child, Duration::from_millis(50)).is_none());
    let still = std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .unwrap();
    assert!(!still.success());
}
```

`kill -0` 在已死后非 0。注意：在部分系统上 `kill -0` 对僵尸可能仍成功；`child.wait()` 之后再查。以 `wait_output_timeout` 返回 `None` 为必须断言。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife -- macos::browser::tests::wait_output_timeout_kills_child`

Expected: FAIL

- [ ] **Step 3: 实现超时与 `fetch_browser_url`**

`tell application` 只允许三个常量：`"Safari"`、`"Google Chrome"`、`"Arc"`。不要把 `last.app` 原样拼进脚本。bundle 在允许列表但本地化名不是这三个英文名时，按 bundle 选脚本。

```rust
pub fn fetch_browser_url_for(bundle_id: Option<&str>, app: &str) -> Option<String> {
    let target = if bundle_id == Some("com.apple.Safari") || app == "Safari" {
        "Safari"
    } else if bundle_id == Some("com.google.Chrome") || app == "Google Chrome" {
        "Google Chrome"
    } else if bundle_id == Some("company.thebrowser.Browser") || app == "Arc" {
        "Arc"
    } else {
        return None;
    };
    let script = if target == "Safari" {
        r#"tell application "Safari" to return URL of current tab of front window"#
            .to_string()
    } else {
        format!(
            r#"tell application "{target}" to return URL of active tab of front window"#
        )
    };
    let mut child = std::process::Command::new("osascript")
        .args(["-e", &script])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let bytes = wait_output_timeout(&mut child, BROWSER_URL_TIMEOUT)?;
    let url = String::from_utf8(bytes).ok()?;
    let url = url.trim();
    if url.is_empty() {
        None
    } else {
        Some(url.to_string())
    }
}
```

`url_for(bundle, app, || fetch_browser_url_for(bundle, app))`。`fetch_browser_url(app)` 若保留，只是 `fetch_browser_url_for(None, app)`。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife -- macos::browser`

Expected: PASS。再跑：`cargo test --offline -p gamelife`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/macos/browser.rs src-tauri/src/macos/mod.rs src-tauri/src/sampler.rs
git commit -m "$(cat <<'EOF'
feat: fetch browser tab URLs with a one-second timeout

Only Safari, Chrome, and Arc may receive Apple Events, and a hung
osascript is killed instead of blocking the sampler.
EOF
)"
```

---

### Task 9: 文案、权限说明、手工清单

**Files:**
- Modify: `src-tauri/Info.plist`
- Modify: `src/pages/Settings.tsx`
- Modify: `README.md`

**Interfaces:**
- Consumes: 规格 §5.2 / §7.3
- Produces: 用户可见说明；无新 API

- [ ] **Step 1: 改 `Info.plist`**

```xml
<key>NSAppleEventsUsageDescription</key>
<string>GameLife 需要自动化权限，以便在 Chrome、Safari 或 Arc 前台时读取当前标签 URL。拒绝授权时仍会采样，只是没有网址。</string>
```

不要再写「读取前台应用名称和窗口标题」。

- [ ] **Step 2: 设置页加一句（`PermissionBanner` 下面、15s 说明之后）**

```tsx
<p className="muted">
  Chrome / Safari / Arc 的当前标签 URL 需要「自动化」权限；拒绝则 URL
  为空，不影响采样与截图。不要把这项做成缺了就无法观测。
</p>
```

不要改 `PermissionBanner` 的必显逻辑（仍只显示辅助功能 / 屏幕录制）。

- [ ] **Step 3: README 权限表增加一行**

```markdown
| 自动化（可选） | 读取 Chrome / Safari / Arc 当前标签 URL |
```

并在手工清单追加：

```markdown
- [ ] Preview 打开本地 PDF：样本有真实 document_path
- [ ] Chrome / Safari / Arc：有去 query 的当前 URL；关闭自动化后 URL 空、样本仍在
- [ ] Cursor：有 app 与标题，document_path 允许空
- [ ] 到点截图：JPEG 是当时前台窗口（不是 AX window id）
```

- [ ] **Step 4: 跑测试**

Run: `cargo test --offline -p gamelife`

Expected: PASS

Run: `npx vitest run --dir src`

Expected: PASS（若无设置页测试，至少确认现有前端测试不红）

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Info.plist src/pages/Settings.tsx README.md
git commit -m "$(cat <<'EOF'
docs: describe optional automation only for browser URLs

Accessibility and screen recording remain the only blocking
permissions; missing browser automation just leaves url empty.
EOF
)"
```

---

## 手工验收（实现全部 Task 之后，不写入自动化）

在开发机按规格 §7.3 点一遍。`#[ignore]` 的 `snapshot_does_not_panic` 可在有辅助功能时跑：

```bash
cargo test --offline -p gamelife -- macos::snapshot::tests::snapshot_does_not_panic -- --ignored --nocapture
```

---

## Self-review

**Spec coverage**

| 规格 | Task |
| --- | --- |
| §2 S2 字段、Electron 路径可空 | 1, 6, 手工 |
| §3 单进程、15s、core 不加 window id | 全局 + 4, 5, 7 |
| §3 不从标题伪造路径 | 1, 5 |
| §3 capture_context 只在截图时 | 5（保留现有测试） |
| §3 无 AX → unobserved | 现有 sampler 测试，5 不得破坏 |
| §3 macOS 14+ / 禁止 screencapture 回退 | 7 |
| §4 一次快照 | 5, 6 |
| §5.1 snapshot / CGWindowID | 2, 6 |
| §5.2 三款浏览器 | 3, 8 |
| §5.3 ScreenCaptureKit 2s | 7 |
| §5.4 input FFI 不换 | 6 挪文件 |
| §5.5 横幅不反复弹、不含自动化 | 9 |
| §5.6 last / take id / Fake 不受允许列表 | 4, 5, 7, 8 |
| §6 失败对照 missed/skipped/NULL json | 7 |
| §7 测试 | 各 Task |
| §8 不做项 | 全局约束 |
| §9 B/C/D 不在本计划 | 无对应 Task |

**Placeholder scan:** 无 TBD。objc2 feature 名以 0.3.2 编译为准，不改变模块边界。

**Type consistency:** `FrontmostSnapshot` / `ObservationState` / `ObservedWindow` / `ax_document_raw` / `pick_front_window_id` / `url_for` / `fetch_browser_url_for` / `capture_window` / `tick_capture(..., capture_fn)` 在后续 Task 中的名字与 Task 1–4 一致。
