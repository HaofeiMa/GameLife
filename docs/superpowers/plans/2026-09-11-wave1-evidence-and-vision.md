# Wave 1 Evidence Path and Vision Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把文档路径与截图路径拆开，采集真实 `document_path`，Vision 使用截图瞬间绑定的 context 与脱敏后的槽摘要，并带上默认 Policy、严格 JSON 校验、以及按窗口族升级的 verified_core。

**Architecture:** 判定证据只走 `Sample.document_path`。截图只在 `tick_capture` 真正到点时调用 `SampleSource::capture_context()`，立刻决定 Never Capture，立刻截图，结果写入 `slots`。Judge 与 Vision 共用 `analyze_slot_evidence`。`sanitize_vision_context` 对受保护截图 fail-closed（禁止 HTTP）；历史窗口只脱敏。SQLite `user_version = 1`，不做旧 `samples.path` backfill。

**Tech Stack:** Rust workspace（`gamelife-core` + `gamelife`）、rusqlite、chrono、serde、reqwest、osascript Accessibility。

**Spec:** `docs/superpowers/specs/2026-09-11-wave1-evidence-and-vision-design.md`

## Global Constraints

- `screenshot_path` 与旧列 `samples.path` 永远不得进入 Judge 文本 haystack。
- **禁止** `SELECT samples.path` 填进 `document_path`。旧 path 从 Task 1 起停止作为 evidence。
- Hint 只看 `app`、`title`、`url`、`document_path`。
- 不得从窗口标题伪造 `document_path`。
- `capture_context()` 只在即将截图时调用；禁止每个 15 秒都读。
- 受保护截图 → 禁止整个 Vision HTTP（含 jpg）。受保护历史窗口 → 只脱敏摘要。
- `verified_core` 按窗口族保守匹配，禁止同一 App 全部升级。
- `call_vision_api` 只能接受 `SanitizedVisionContext`。
- 损坏的 `capture_context_json` → log + `vision=None` + pending，不得 Fatal。
- Policy 用 free function `default_v01()`，并 `Serialize`/`Deserialize`。
- 测试禁止 `std::env::set_var("HOME", ...)`（并行测试会互相踩）。
- 凡修改 `src-tauri/` 的 Task：跑 `cargo test -p gamelife`（至少 `--no-run` 不够；以能编译并跑过该 crate 测试为准）。
- 不做愿望商店、设置编辑器、登录启动、通知、Pause Resume、ScreenCaptureKit、Policy SoT 拆分。

---

## File Structure

```
crates/gamelife-core/src/types.rs
crates/gamelife-core/src/document.rs       NEW
crates/gamelife-core/src/policy.rs         default_v01 + Serialize + identities
crates/gamelife-core/src/hint.rs
crates/gamelife-core/src/observe.rs       Span.sample_index
crates/gamelife-core/src/judge.rs          SlotEvidence、VisionMatchContext
crates/gamelife-core/src/vision_ctx.rs     NEW CaptureContext / sanitize / prompt / summary
crates/gamelife-core/src/lib.rs
crates/gamelife-core/Cargo.toml          serde
src-tauri/src/db.rs
src-tauri/src/sampler.rs                 SampleSource::capture_context
src-tauri/src/scheduler.rs
src-tauri/src/vision.rs
src-tauri/src/macos.rs                   一次 AppleScript 的 capture_context
src-tauri/src/config.rs
src-tauri/src/commands.rs
```

---

### Task 1: Sample 字段改为 document_path（旧 path 不再读取）

**Files:**
- Modify: `crates/gamelife-core/src/types.rs`
- Modify: `crates/gamelife-core/src/hint.rs`（测试 helper + haystack 改读 `document_path`）
- Modify: `crates/gamelife-core/src/judge.rs`（`grid()`）
- Modify: `crates/gamelife-core/src/observe.rs`
- Modify: `src-tauri/src/scheduler.rs`（`load_samples_for_slot`、`capture_app_from_samples`）
- Modify: `src-tauri/src/vision.rs`（若编译因 `Sample.path` 失败）

**Interfaces:**
- Consumes: 无
- Produces: `Sample { document_path, bundle_id, secure_input }`；删除 `Sample.path`

- [ ] **Step 1: 改类型与所有构造点**

```rust
pub struct Sample {
    pub ts: i64,
    pub app: String,
    pub window_title: String,
    pub url: Option<String>,
    pub document_path: Option<String>,
    pub bundle_id: Option<String>,
    pub idle_seconds: i64,
    pub screen_locked: bool,
    pub paused: bool,
    pub secure_input: bool,
}
```

`load_samples_for_slot` **不要** SELECT `path`：

```rust
"SELECT ts, app, title, url, idle_seconds, locked, paused
 FROM samples WHERE day = ?1 AND ts >= ?2 AND ts < ?3 ORDER BY ts"
```

```rust
document_path: None,
bundle_id: None,
secure_input: false,
```

`capture_app_from_samples` 改为 `samples.last().map(|s| s.app.clone())`。禁止任何 `s.path` / `s.document_path.is_some()` 当「有截图」启发式。

- [ ] **Step 2: 跑测试**

Run:

```
cargo test -p gamelife-core
cargo test -p gamelife
```

Expected: PASS。Tauri crate 必须能编译。

- [ ] **Step 3: Commit**

```bash
git add crates/gamelife-core/src/types.rs crates/gamelife-core/src/hint.rs crates/gamelife-core/src/judge.rs crates/gamelife-core/src/observe.rs src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
refactor: stop treating samples.path as document evidence

EOF
)"
```

---

### Task 2: `normalize_document_path`

**Files:**
- Create: `crates/gamelife-core/src/document.rs`
- Modify: `crates/gamelife-core/src/lib.rs`

**Interfaces:**
- Produces: `pub fn normalize_document_path(raw: &str) -> Option<String>`

- [ ] **Step 1: 写失败测试（禁止 set_var HOME）**

```rust
#[test]
fn tilde_expands_home_before_absolute_check() {
    let home = std::env::var("HOME").expect("HOME");
    assert_eq!(
        normalize_document_path("~/Projects/HDP"),
        Some(format!("{home}/Projects/HDP"))
    );
}

#[test]
fn file_url_percent_decodes() {
    assert_eq!(
        normalize_document_path("file:///Users/me/My%20Project/a.py"),
        Some("/Users/me/My Project/a.py".into())
    );
}

#[test]
fn file_url_strips_trailing_slash() {
    assert_eq!(
        normalize_document_path("file:///Users/me/HDP/"),
        Some("/Users/me/HDP".into())
    );
}

#[test]
fn window_title_is_not_a_path() {
    assert_eq!(normalize_document_path("train.py — HDP"), None);
}
```

实现顺序：trim → `~`/`~/` 扩 HOME → `file:` → 必须以 `/` 开头 → 去尾 `/`，不 realpath。percent-decode 手写，不新增 url crate。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p gamelife-core document:: -- --nocapture`

Expected: FAIL

- [ ] **Step 3: 实现**

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p gamelife-core document::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/document.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: normalize document paths without inventing them from titles

EOF
)"
```

---

### Task 3: `default_v01`、Policy serde、bundle ID 匹配

**Files:**
- Modify: `crates/gamelife-core/Cargo.toml`（`serde = { version = "1", features = ["derive"] }`）
- Modify: `crates/gamelife-core/src/policy.rs`
- Modify: `crates/gamelife-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub fn default_trusted_apps() -> Vec<String>`
  - `pub fn default_reading_apps() -> Vec<String>`
  - `pub fn default_v01() -> Policy`（**free function**，不要 `Policy::default_v01`）
  - `#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub struct Policy`
  - `pub fn matches_app_identity(app: &str, bundle_id: Option<&str>, names: &[String]) -> bool`

JSON 字段名：`trusted_apps`、`distraction_rules`、`side_project_rules`、`reading_apps`、`never_capture_apps`（与现有 `policy_versions` 一致，snake_case）。

Trusted / Reading / Never Capture 目录按 spec §7.1 / §7.3。

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn default_v01_includes_cursor_and_preview() {
    let p = default_v01();
    assert!(p.trusted_apps.iter().any(|a| a == "Cursor"));
    assert!(p.reading_apps.iter().any(|a| a == "Zotero"));
}

#[test]
fn default_v01_roundtrips_json() {
    let p = default_v01();
    let json = serde_json::to_string(&p).unwrap();
    let back: Policy = serde_json::from_str(&json).unwrap();
    assert_eq!(p, back);
}

#[test]
fn cursor_matches_via_bundle_id_even_if_display_name_localized() {
    let names = vec!["Cursor".into()];
    assert!(matches_app_identity(
        "光标",
        Some("com.todesktop.230313mzl4w4u92"),
        &names
    ));
}
```

- [ ] **Step 2–4: 失败 → 实现 → 通过**

Run: `cargo test -p gamelife-core policy::`

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/Cargo.toml crates/gamelife-core/src/policy.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: ship default_v01 policy with serde and bundle-id matching

EOF
)"
```

---

### Task 4: Hint haystack 与 GameLife 截图回归

**Files:**
- Modify: `crates/gamelife-core/src/hint.rs`

**Interfaces:**
- Consumes: `document_path`、`matches_app_identity`

测试（完整夹具见 spec §8.1 / §8.2）：

1. Cursor + `document_path=/Users/me/Projects/HDP/train.py` + keyword HDP + builtin GameLife → `CoreCandidate`
2. `document_path=/Users/me/Projects/GameLife/src/App.tsx` → `Side`
3. `document_path=None`、title `train.py — HDP` → 仍 `CoreCandidate`

Trusted/Reading 改 `matches_app_identity`。haystack 不含 bundle_id。helper 默认 `document_path: None`。

Run: `cargo test -p gamelife-core hint::`

Commit: `fix: keep screenshot directories out of hint matching`

---

### Task 5: 严格 `parse_vision_json` 与 `VisionMatchContext`

**Files:**
- Modify: `crates/gamelife-core/src/judge.rs`（`VisionResult`）
- Modify: `src-tauri/src/vision.rs`

**Interfaces:**
- Produces:

```rust
pub struct VisionMatchContext {
    pub app: String,
    pub title: String,
    pub document_path: Option<String>,
}

pub struct VisionResult {
    pub wants_core: bool,
    pub confidence: f64,
    pub match_context: Option<VisionMatchContext>,
    pub category: String,
}

pub enum VisionParseError { InvalidJson, InvalidCategory, InvalidConfidence, InvalidReason }

pub fn parse_vision_json(
    json: &str,
    match_context: Option<VisionMatchContext>,
) -> Result<VisionResult, VisionParseError>
```

合法 category：`core_research` `research_support` `admin` `side_project` `distraction` `break_away`。`unknown` → `InvalidCategory`。`confidence` finite 且 `[0,1]`。`reason` 缺省 `""`；`null` → `InvalidReason`。Prompt 去掉 `unknown`。

现有 judge 测试把 `context_app: Some("Isaac Sim")` 改成：

```rust
match_context: Some(VisionMatchContext {
    app: "Isaac Sim".into(),
    title: "robot".into(),
    document_path: None,
}),
```

- [ ] **Step 1–4: 失败测试（unknown / 1.1 / reason null）→ 实现 → `cargo test -p gamelife-core judge::` 与 `cargo test -p gamelife vision::`**

Commit: `fix: reject illegal vision categories and carry match context`

---

### Task 6: sanitize fail-closed

**Files:**
- Create: `crates/gamelife-core/src/vision_ctx.rs`
- Modify: `crates/gamelife-core/src/lib.rs`

**Interfaces:**
- Produces: `CaptureContext`（serde 字段与 spec JSON 一致）、`WindowShare`、`HintSeconds`、`ActivitySummary`、`VisionContext`、`SanitizedVisionContext`（不可外部构造）、`VisionPrivacyError::ProtectedCapture`、`is_protected_frontmost`、`sanitize_vision_context(...) -> Result<SanitizedVisionContext, VisionPrivacyError>`、`build_vision_prompt(&SanitizedVisionContext) -> String`、`format_span_secs`

- [ ] **Step 1: 写失败测试**

历史窗口 1Password + 截图 Cursor：`sanitize` 为 `Ok`；prompt 含 `[Protected App]` 与 `Cursor · train.py — HDP`；不得出现 `Bank Account Password` / `1Password` / `/secret`。`format_span_secs(300) == "5m00s"`。

```rust
#[test]
fn protected_capture_forbids_vision_request() {
    let mut ctx = ctx_with_password_and_cursor();
    ctx.capture.app = "1Password".into();
    ctx.capture.title = "Bank Account Password".into();
    let never = builtin_never_capture();
    assert!(matches!(
        sanitize_vision_context(ctx, &never),
        Err(VisionPrivacyError::ProtectedCapture)
    ));
}

#[test]
fn secure_input_capture_forbids_vision_request() {
    let mut ctx = ctx_with_password_and_cursor();
    ctx.capture.secure_input = true;
    assert!(matches!(
        sanitize_vision_context(ctx, &[]),
        Err(VisionPrivacyError::ProtectedCapture)
    ));
}
```

`build_vision_prompt` 只接受 `&SanitizedVisionContext`。不要实现「capture 脱敏后继续当 Sanitized」。

- [ ] **Step 2–4: 失败 → 实现 → `cargo test -p gamelife-core vision_ctx::`**

Commit: `feat: fail closed when the screenshot itself is a protected app`

---

### Task 7: `SlotEvidence` 与 `Span.sample_index`

**Files:**
- Modify: `crates/gamelife-core/src/observe.rs`
- Modify: `crates/gamelife-core/src/judge.rs`
- Modify: `crates/gamelife-core/src/vision_ctx.rs`（`activity_summary_for_vision`）
- Modify: `src-tauri/src/scheduler.rs`（`compute_slot_activity` 改为调用 `analyze_slot_evidence`）

**Interfaces:**
- Produces:

```rust
pub struct Span {
    pub start: i64,
    pub end: i64,
    pub kind: SpanKind,
    pub sample_index: Option<usize>,
}

pub struct SlotEvidence {
    pub hints: Vec<Hint>,
    pub spans: Vec<Span>,
    pub activity: ActivitySeconds,
    pub strong_core_seconds: i64,
    pub reading_bridge_seconds: i64,
    pub observed_seconds: i64,
}

pub fn analyze_slot_evidence(
    samples: &[Sample],
    policy: &Policy,
    quests: &[Quest],
    slot_start: i64,
    slot_end: i64,
) -> SlotEvidence

pub fn activity_summary_for_vision(evidence: &SlotEvidence, samples: &[Sample]) -> ActivitySummary
```

`judge_slot` 内部调用 `analyze_slot_evidence`，禁止再抄一遍 hint 循环。scheduler `compute_slot_activity` 改为：

```rust
let ev = analyze_slot_evidence(...);
(ev.activity, ev.strong_core_seconds, ev.reading_bridge_seconds)
```

Observed span 的 `sample_index`：该 hint 对应的 sample 下标。槽开头向前填充：用 **第一条 sample 的下标**，即使 `span.start < samples[0].ts`。Unobserved → `None`。

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn leading_fill_span_points_at_first_sample() {
    let samples = vec![sample_at(10, "Cursor", "t")];
    let hints = vec![Hint::Unsure];
    let spans = spans_for_slot(&samples, &hints, 0, 40);
    let lead = spans.iter().find(|s| s.start == 0).unwrap();
    assert_eq!(lead.sample_index, Some(0));
}

#[test]
fn summary_uses_evidence_not_ts_lookup() {
    // 0–10s 由 ts=10 的 Cursor 向前填充；10–40s 同一样本
    let ev = analyze_slot_evidence(&samples, &policy, &[], 0, 40);
    let sum = activity_summary_for_vision(&ev, &samples);
    assert!(sum.top_windows.iter().any(|w| w.app == "Cursor"));
}
```

`activity_summary` 用 `span.sample_index` 取 sample，**禁止** `samples.iter().filter(|s| s.ts <= span.start).last()`。

- [ ] **Step 2–4: 失败 → 实现 → `cargo test -p gamelife-core` 与 `cargo test -p gamelife`**

Commit: `feat: share slot evidence between judge and vision summary`

---

### Task 8: verified_core 按窗口族匹配

**Files:**
- Modify: `crates/gamelife-core/src/judge.rs`

**Interfaces:**
- Consumes: `VisionMatchContext`、`Span.sample_index`
- Produces: `fn span_matches_capture(sample: &Sample, ctx: &VisionMatchContext) -> bool`

规则（非人工）：

1. App 不同 → false
2. 两边 `document_path` 都是 Some → 字符串相等才 true
3. 否则两边 title 都非空 → title 相等才 true
4. 否则 true（app-only fallback）

人工 `manual_core` 仍升级全部 unsure/unsure_reading。升级只看 `span.sample_index` 指向的 sample。

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn chrome_tensorboard_screenshot_does_not_verify_personal_site() {
    let mut samples = Vec::new();
    for i in 0..20 {
        samples.push(Sample {
            ts: i * 15,
            app: "Google Chrome".into(),
            window_title: "personal site".into(),
            url: Some("https://haofei.ma/".into()),
            document_path: None,
            bundle_id: None,
            idle_seconds: 1,
            screen_locked: false,
            paused: false,
            secure_input: false,
        });
    }
    for i in 20..60 {
        samples.push(Sample {
            ts: i * 15,
            app: "Google Chrome".into(),
            window_title: "TensorBoard".into(),
            url: Some("http://localhost:6006/".into()),
            document_path: None,
            bundle_id: None,
            idle_seconds: 1,
            screen_locked: false,
            paused: false,
            secure_input: false,
        });
    }
    let out = judge_slot(JudgeInput {
        slot_start: 0,
        slot_end: 900,
        samples: &samples,
        quests: &[Quest { text: "HDP".into(), keywords: vec!["HDP".into()] }],
        policy: &Policy {
            trusted_apps: vec!["Google Chrome".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
        },
        capture: CaptureStatus::Captured,
        vision: Some(VisionResult {
            wants_core: true,
            confidence: 0.9,
            match_context: Some(VisionMatchContext {
                app: "Google Chrome".into(),
                title: "TensorBoard".into(),
                document_path: None,
            }),
            category: "core_research".into(),
        }),
        manual_core: None,
    });
    assert!(!out.pending);
    assert!(out.credited_core_seconds >= 540);
    assert!(out.credited_core_seconds <= 650);
}
```

20×15=300s personal，40×15=600s TensorBoard。只升级 TensorBoard → credited 约 600 而非 900。把 `span_context_matches` / `upgrade_verified_core` 从 app-only 改成上述规则。Isaac Sim 测试：title 与 capture 同为 `robot` 或都空 → 仍能 verified。

- [ ] **Step 2–4: `cargo test -p gamelife-core judge::`**

Commit: `fix: verify core only for the captured window family`

---

### Task 9: SQLite user_version 与新列

**Files:**
- Modify: `src-tauri/src/db.rs`

与原计划相同：`user_version=1`；`document_path` / `bundle_id` / `secure_input`；`screenshot_path` / `captured_at` / `capture_context_json`；`app_meta`。`add_column_if_missing` 用 `table_info`；只忽略明确 duplicate-column。**不**拷贝 `samples.path`。

测试：新库 version=1；legacy schema 升级；二次 migrate 不炸。

Run: `cargo test -p gamelife db::`

Commit: `feat: migrate sqlite to user_version 1 with split path columns`

---

### Task 10: 采样写入 document_path（不读 CaptureContext）

**Files:**
- Modify: `src-tauri/src/sampler.rs`
- Modify: `src-tauri/src/macos.rs`（`document_path`/`bundle_id` 先返回 None；`capture_context` stub 供编译）

**Interfaces:**
- Produces: `SampleSource::document_path`、`bundle_id`、`capture_context`；`insert_sample` 写新列，不写 `path`

`sample_once`：**不要**调用 `capture_context()`。只用 `frontmost_app` + `document_path` + `bundle_id` + `secure_input_on` 写 sample，然后 `tick_capture(conn, ..., source, ...)`。

测试：`document_path: Some("~/Projects/HDP/train.py")` 用 **进程已有 HOME** 断言展开后的绝对路径；`train.py — HDP` → DB NULL。`secure_input` 写入。禁止 `set_var("HOME")`。

Run: `cargo test -p gamelife sampler::`

Commit: `feat: persist real document_path and bundle_id on each sample`

---

### Task 11: 到点才 capture_context，立刻截图

**Files:**
- Modify: `src-tauri/src/scheduler.rs`
- Modify: `src-tauri/src/sampler.rs`（`tick_capture` 签名）
- Modify: `src-tauri/src/macos.rs`（可先用多次调用拼 `CaptureContext`；Task 15 合成一次 AppleScript）

**Interfaces:**
- Produces: 删除 `attach_screenshot_path_to_latest_sample`；`tick_capture(conn, day, slot, now, locked, paused, screen_recording, source: &dyn SampleSource, capture_fn)`

流程：

```
if now < scheduled or status != Scheduled: return
if !screen_recording or locked or paused: Skipped（不调用 capture_context）
ctx = source.capture_context()
if decide_capture(ctx.app, ctx.bundle_id, ctx.secure_input, locked, paused, never) == Skipped:
    Skipped，不写 path/json
else:
    capture_fn(path) immediately
    on success: UPDATE screenshot_path, captured_at, capture_context_json, Captured
```

`FakeSampleSource.capture_context` 可返回与周期 `app` 不同的值（WeChat vs Cursor）。

- [ ] **Step 1: 改测试**

周期 sample：Cursor + `document_path=/Users/me/HDP/train.py`。`FakeSampleSource.capture_context().app = "WeChat"`。`tick_capture` 到点。断言：

- `capture_context_json.app == "WeChat"`
- `samples.document_path` 仍是 HDP
- 不得 UPDATE `samples.path`
- 无空 sample 行
- 未到点的 `tick_capture` 不得调用 `capture_context`（FakeSource 用 `Cell<u32>` 计数）

Skipped 1Password：不写 json/path。

- [ ] **Step 2–4: `cargo test -p gamelife tick_capture`**

删除 `capture_app_from_samples`。`load_samples_for_slot` 改为 SELECT `document_path, bundle_id, secure_input`。

Commit: `fix: bind capture context at screenshot time in one shot`

---

### Task 12: finalize 用 SlotEvidence；损坏 JSON 不 Fatal；禁止上传受保护图

**Files:**
- Modify: `src-tauri/src/scheduler.rs`
- Modify: `src-tauri/src/vision.rs`

**Interfaces:**
- `maybe_vision_for_gray_zone` 接收 `VisionContext` + `never`。`analyze_screenshot(path, key, SanitizedVisionContext)` — **先** sanitize；`Err(ProtectedCapture)` → 不读 jpg、不 POST、返回 `None`。

`finalize_slot_end`：

```
let evidence = analyze_slot_evidence(...)
decidable from evidence
screenshot_path from slots.screenshot_path
capture_json = slots.capture_context_json
capture = match serde_json::from_str::<CaptureContext>(&json) {
    Ok(c) => c,
    Err(_) => { log; vision=None; /* pending via judge */ }
}
if capture missing or parse fail: vision=None
else:
  ctx = VisionContext { slot_start, slot_end, quests, capture, activity_summary: activity_summary_for_vision(&evidence, &samples) }
  match sanitize_vision_context(ctx, &never) {
    Err(ProtectedCapture) => vision=None
    Ok(s) => maybe_vision_for_gray_zone(..., s)
  }
VisionResult.match_context 来自 **未 sanitize** 的 CaptureContext（app/title/document_path）
```

- [ ] **Step 1: 测试**

1. jpg 只在 slots；json 为 Isaac Sim；无 API key → pending；samples 不含 jpg。
2. 有 jpg、json 损坏 `{not json` → pending，函数 `Ok(())`（不 Fatal）。
3. json 为 1Password + jpg 存在：即使有 key，也不得调用会 POST 的路径。可用 `analyze_screenshot` 单测：ProtectedCapture 时不要求文件存在即可返回 None/Err 且不 panic。
4. `finalize_without_capture_context_does_not_invent_app`：无 json、有 jpg、sample 全是 Cursor → `used_vision=0`。

Run: `cargo test -p gamelife finalize_`

Commit: `feat: feed vision slot evidence and refuse protected screenshots`

---

### Task 13: 截图与 capture_context 生命周期

**Files:**
- Modify: `src-tauri/src/scheduler.rs`（`finalize_slot_end`、`settle_day`、`review_pending_slot`、`purge_expired_screenshots`）

**Interfaces:**
- `fn apply_capture_retention(conn, day, slot_start, retention, slot_status) -> Result<(), DbOpError>`

pending：不动。`final`/`unknown` + none：删文件成功 → `screenshot_path=NULL` 且 `capture_context_json=NULL`。TTL purge 成功后同样两者 NULL。`capture_status` 保持 Captured。

`settle_day`：`convert_pending_to_unknown` 之后，对该日 `status='unknown'` 且仍有 `screenshot_path` 或 json 的槽调用 `apply_capture_retention`。

`review_pending_slot` 变成 final 后同样调用。

- [ ] **Step 1: 测试**

- pending + none：文件与 json 仍在
- final + none：两者 NULL，status Captured
- `settle_day`：插入 pending_review + jpg + json → settle → status unknown，两者 NULL

Run: `cargo test -p gamelife pending_slot_keeps cargo test -p gamelife settle_day`

Commit: `fix: clear capture context with screenshots on final and settle`

---

### Task 14: 一次性 Policy seed

**Files:**
- Modify: `src-tauri/src/scheduler.rs`、`sampler.rs`、`commands.rs`、`config.rs`

**Interfaces:**
- `pub fn seed_default_policy_if_needed(conn: &Connection) -> Result<(), DbOpError>`
- INSERT JSON = `serde_json::to_string(&default_v01())`
- `default_settings()` 用 `default_trusted_apps()` / `default_reading_apps()`

测试：无行 → Cursor 在 Trusted 且 `policy_seed_version=1`；seed 后再插入全空政策 → 再 seed **不**恢复。`load_policy` 无行时回落 `default_v01()`。

全程使用 `default_v01()`，不要 `Policy::default_v01()`。

Run: `cargo test -p gamelife seed_ && cargo test -p gamelife-core && cargo test -p gamelife`

Commit: `feat: seed default policy once without resetting a cleared list`

---

### Task 15: macOS 一次脚本取 capture_context

**Files:**
- Modify: `src-tauri/src/macos.rs`、`src-tauri/src/sampler.rs`

**Interfaces:**
- `pub fn document_path() -> Option<String>`
- `pub fn bundle_id() -> Option<String>`
- `pub fn capture_context() -> CaptureContext`

`capture_context` 用 **一次** osascript 返回 `app \t title \t bundle \t document`（document 为 AXDocument 或 file: AXURL）。然后 `normalize_document_path`。secure_input / URL 可紧随其后（secure_input 已是 FFI，不算 osascript）。

周期 `document_path()`/`bundle_id()` 可供 sample 使用，但 **不得**从窗口标题解析 path。非 macOS imp：空 context。

`MacSampleSource::capture_context` 调 `macos::capture_context()`。

Run: `cargo test -p gamelife-core && cargo test -p gamelife`

Commit: `feat: read capture context in one AppleScript at screenshot time`

---

## Self-review

| Spec | Task |
| --- | --- |
| 旧 path 不再当 evidence | 1 |
| haystack / GameLife 回归 | 4 |
| 无 title 伪造 path | 2, 10, 15 |
| 到点才 capture_context | 10, 11, 15 |
| SlotEvidence 共用 | 7, 12 |
| 窗口族 verified_core | 8 |
| 受保护截图禁止 HTTP | 6, 12 |
| 历史窗口脱敏 | 6 |
| 损坏 JSON 不 Fatal | 12 |
| settle + json 一起清 | 13 |
| default_v01 + serde seed | 3, 14 |
| user_version 迁移 | 9 |
| 每个 src-tauri commit 可编译 | 1 起凡改 tauri 都跑 `cargo test -p gamelife` |

**类型名：** `document_path`、`CaptureContext`、`capture_context()`、`VisionContext`、`SanitizedVisionContext`、`VisionPrivacyError`、`VisionMatchContext`、`SlotEvidence`、`analyze_slot_evidence`、`activity_summary_for_vision`、`sanitize_vision_context`、`default_v01`、`matches_app_identity`、`seed_default_policy_if_needed`。
