# D Today Mainline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Quest 做成最多 3 条带显式证据的日快照（一条英雄主线），标题/路径/URL 可命中，今日页可沿用上一个工作日并显示当前窗口是否命中。

**Architecture:** 在 `gamelife-core` 增加 `quest.rs` 纯函数；`Quest` 改为 `text + evidence + hero`。`hint_sample` / `judge_slot` 用 evidence 与 `quest_list_has_evidence`。`src-tauri` 继续用 `quest_versions` 日快照。Live 命中读当天最新 `samples` 行。

**Tech Stack:** Rust workspace（`gamelife-core` + `gamelife`）、Tauri 2 commands、React/TS Today 页、vitest。

**Spec:** `docs/superpowers/specs/2026-09-11-today-mainline-design.md`

## Global Constraints

- 最多 3 条 Quest；恰好一条 `hero=true`（空列表除外）。
- `text` 不切词；匹配只用 `evidence`。token 最短 2 字符，每条最多 8 个。
- 命中字段：标题、`document_path`、URL；不匹配 `app` 名。
- 无非空 evidence → 与无 Quest 相同，credited 强制 0。
- 改 Quest 只影响尚未开始的槽。
- Live 读 DB 最近样本，不调用 `snapshot()` / ScreenCaptureKit。
- 周末或已结束：拒绝写入与沿用。
- 不按 Quest 拆 credited；不做任务库、专注模式、自动拷贝昨天。
- 新测试禁止 `std::env::set_var("HOME", …)`。不要去改无关测试里已有的 `set_var`。
- 改 `crates/`：`cargo test --offline -p gamelife-core` 必须编译并跑过。
- 改 `src-tauri/`：`cargo test --offline -p gamelife` 必须编译并跑过（`--no-run` 不够）。
- 若在 worktree：`CARGO_TARGET_DIR` 指向主仓 `target`（与 A1 相同，避免双编译）。

---

## File Structure

```
crates/gamelife-core/src/types.rs          Quest 字段
crates/gamelife-core/src/quest.rs       新建：规范化、匹配、JSON、vision label
crates/gamelife-core/src/lib.rs         mod quest; 导出
crates/gamelife-core/src/hint.rs         用 matched_quest_index + URL
crates/gamelife-core/src/judge.rs       quest_list_has_evidence；夹具
src-tauri/src/scheduler.rs             读写 JSON、沿用、vision 标签
src-tauri/src/commands.rs               set_quests / continue / get_today
src-tauri/src/lib.rs                    注册 continue_previous_workday
src/lib/api.ts                          类型与 invoke
src/lib/questLive.ts                    liveMatchLabel
src/lib/questLive.test.ts
src/pages/Today.tsx
src/styles.css                          英雄卡/chips
```

---

### Task 1: `Quest` 形状 + `quest.rs` 纯函数

**Files:**
- Create: `crates/gamelife-core/src/quest.rs`
- Modify: `crates/gamelife-core/src/types.rs`
- Modify: `crates/gamelife-core/src/lib.rs`
- Modify: `crates/gamelife-core/src/hint.rs`（`keywords` → `evidence`，匹配逻辑本 Task **仍只扫标题和 path**，URL 留给 Task 2）
- Modify: `crates/gamelife-core/src/judge.rs`（夹具字段）
- Modify: `src-tauri/src/scheduler.rs`（`Quest { evidence: q.keywords, hero: true }` 以便编译）

**Interfaces:**
- Consumes: 无
- Produces:

```rust
pub const MAX_QUESTS: usize = 3;
pub const MAX_EVIDENCE: usize = 8;
pub const MIN_EVIDENCE_CHARS: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quest {
    pub text: String,
    pub evidence: Vec<String>,
    pub hero: bool,
}

impl Quest {
    pub fn fixture(text: &str, token: &str) -> Self { /* text, evidence: vec![token], hero: true */ }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct QuestDraft {
    pub text: String,
    pub evidence: Vec<String>,
    pub hero: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuestListError { TooMany, EmptyText }

pub fn normalize_evidence(tokens: &[String]) -> Vec<String>
pub fn normalize_quest_list(drafts: Vec<QuestDraft>) -> Result<Vec<Quest>, QuestListError>
pub fn quest_list_has_evidence(quests: &[Quest]) -> bool
pub fn matched_quest_index(
    title: &str,
    document_path: Option<&str>,
    url: Option<&str>,
    quests: &[Quest],
) -> Option<usize>
pub fn parse_quest_versions_json(json: &str) -> Result<Vec<Quest>, String>
pub fn vision_quest_label(quest: &Quest) -> String
```

- [ ] **Step 1: 改 `Quest` 并让 workspace 先编不过（去掉 keywords）**

在 `types.rs` 把 `keywords` 换成 `evidence` 和 `hero`，加 `fixture`。`hint.rs` 里 `quest.keywords` 暂改为 `quest.evidence`（仍只匹配 title/path）。judge 与 hint 测试夹具同步改。scheduler `load_quests_*` 映射 `keywords` → `evidence`，`hero: true`。

- [ ] **Step 2: 写失败测试（`quest.rs` 尚未存在）**

```rust
use super::*;

fn draft(text: &str, evidence: &[&str], hero: bool) -> QuestDraft {
    QuestDraft {
        text: text.into(),
        evidence: evidence.iter().map(|s| (*s).to_string()).collect(),
        hero,
    }
}

#[test]
fn normalize_evidence_trims_drops_short_and_dedupes() {
    let raw = vec![
        "  HDP  ".into(),
        "a".into(),
        "".into(),
        "hdp".into(),
        "RL".into(),
    ];
    assert_eq!(normalize_evidence(&raw), vec!["HDP".to_string(), "RL".to_string()]);
}

#[test]
fn normalize_evidence_caps_at_eight() {
    let raw: Vec<String> = (0..12).map(|i| format!("t{i}")).collect();
    assert_eq!(normalize_evidence(&raw).len(), 8);
}

#[test]
fn normalize_quest_list_empty_ok_four_fails() {
    assert_eq!(normalize_quest_list(vec![]).unwrap(), vec![]);
    let four = vec![
        draft("a", &["ab"], false),
        draft("b", &["ab"], false),
        draft("c", &["ab"], false),
        draft("d", &["ab"], false),
    ];
    assert_eq!(normalize_quest_list(four), Err(QuestListError::TooMany));
}

#[test]
fn empty_text_fails() {
    assert_eq!(
        normalize_quest_list(vec![draft("  ", &["HDP"], true)]),
        Err(QuestListError::EmptyText)
    );
}

#[test]
fn hero_defaults_and_first_marked_wins() {
    let qs = normalize_quest_list(vec![
        draft("A", &["aa"], false),
        draft("B", &["bb"], true),
        draft("C", &["cc"], true),
    ])
    .unwrap();
    assert!(qs[0].hero == false && qs[1].hero && !qs[2].hero);

    let qs = normalize_quest_list(vec![draft("A", &["aa"], false), draft("B", &["bb"], false)])
        .unwrap();
    assert!(qs[0].hero && !qs[1].hero);
}

#[test]
fn has_evidence_false_when_only_titles() {
    let qs = vec![Quest {
        text: "Finish paper".into(),
        evidence: vec![],
        hero: true,
    }];
    assert!(!quest_list_has_evidence(&qs));
}

#[test]
fn matched_index_title_path_url_not_app() {
    let qs = vec![
        Quest::fixture("paper", "main.tex"),
        Quest::fixture("web", "overleaf.com"),
    ];
    assert_eq!(
        matched_quest_index("main.tex — HDP", None, None, &qs),
        Some(0)
    );
    assert_eq!(
        matched_quest_index("x", Some("/proj/main.tex"), None, &qs),
        Some(0)
    );
    assert_eq!(
        matched_quest_index("x", None, Some("https://overleaf.com/project/1"), &qs),
        Some(1)
    );
    assert_eq!(
        matched_quest_index("train.py", None, None, &[Quest::fixture("c", "Cursor")]),
        None
    );
}

#[test]
fn parse_old_keywords_json() {
    let qs = parse_quest_versions_json(
        r#"[{"text":"HDP","keywords":["HDP"]}]"#,
    )
    .unwrap();
    assert_eq!(qs[0].evidence, vec!["HDP".to_string()]);
    assert!(qs[0].hero);
}

#[test]
fn vision_label_prefixes_hero() {
    let mut q = Quest::fixture("HDP", "HDP");
    assert_eq!(vision_quest_label(&q), "[main] HDP");
    q.hero = false;
    assert_eq!(vision_quest_label(&q), "HDP");
}
```

在 `lib.rs` 加 `pub mod quest;` 并 `pub use quest::{...}`。

- [ ] **Step 3: 跑测试确认失败**

Run: `cargo test --offline -p gamelife-core -- quest::`

Expected: FAIL（模块不存在或 `unimplemented!`）

- [ ] **Step 4: 最小实现**

`normalize_evidence`：trim、len>=2、小写去重、截断 8。

`normalize_quest_list`：len>3 TooMany；trim text 空 EmptyText；evidence 规范化；hero 按「第一个 true，否则 index 0」。

`matched_quest_index`：三个 haystack lower，token lower，`contains`。跳过空 haystack。

`parse_quest_versions_json`：serde 结构体 `text: String`, `evidence: Option<Vec<String>>`, `keywords: Option<Vec<String>>`, `hero: Option<bool>`。evidence 缺则 keywords。再 `normalize_quest_list`（hero 缺时 Draft.hero=false，让默认规则生效）。JSON 不是数组 → Err。

`hint.rs` 的 `is_core_candidate` 暂时继续手写 title/path（用 `evidence` 字段），**本 Task 不要改成调用 matched_quest_index**（避免 URL 行为提前变化却无测试）。现有 hint 测试必须仍绿。

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test --offline -p gamelife-core`

Expected: PASS

Run: `cargo test --offline -p gamelife`

Expected: PASS（scheduler 已映射 evidence）

- [ ] **Step 6: Commit**

```bash
git add crates/gamelife-core src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
feat: store quest evidence separately from display text

Stop treating whitespace-split titles as match tokens so later
hint/URL matching can use explicit evidence only.
EOF
)"
```

---

### Task 2: hint URL 命中 + 无证据视为无 Quest

**Files:**
- Modify: `crates/gamelife-core/src/hint.rs`
- Modify: `crates/gamelife-core/src/judge.rs`

**Interfaces:**
- Consumes: `matched_quest_index`, `quest_list_has_evidence`
- Produces: `is_core_candidate` 使用 URL；`judge_slot` 用 `!quest_list_has_evidence`

- [ ] **Step 1: 写失败测试**

在 `hint.rs` tests：

```rust
#[test]
fn finish_in_title_without_hdp_evidence_is_unsure() {
    let q = [Quest {
        text: "Finish HDP tactile ablation".into(),
        evidence: vec!["HDP".into()],
        hero: true,
    }];
    let s = sample("Cursor", "Finish notes", 5);
    assert_eq!(hint_sample(&s, &hdp_policy(), &q, None), Hint::Unsure);
}

#[test]
fn trusted_chrome_url_evidence_is_core() {
    let p = Policy {
        trusted_apps: vec!["Google Chrome".into()],
        distraction_rules: vec![],
        side_project_rules: vec![],
        reading_apps: vec![],
        never_capture_apps: vec![],
    };
    let q = [Quest::fixture("overleaf", "overleaf.com")];
    let mut s = sample("Google Chrome", "Overleaf", 5);
    s.url = Some("https://overleaf.com/project/abc".into());
    assert_eq!(hint_sample(&s, &p, &q, None), Hint::CoreCandidate);
}

#[test]
fn untrusted_app_with_evidence_in_title_is_not_core() {
    let s = sample("WeChat", "HDP chat", 5);
    assert_eq!(
        hint_sample(&s, &hdp_policy(), &hdp_quest(), None),
        Hint::Unsure
    );
}
```

在 `judge.rs` 现有 `empty_quests_forces_zero_credit_no_auto_core_research` 旁新增：

```rust
#[test]
fn title_only_quests_without_evidence_force_zero_credit() {
    let samples = grid("Cursor", "main.tex", 0, 58, 15, 2);
    let out = judge_slot(JudgeInput {
        slot_start: 0,
        slot_end: 900,
        samples: &samples,
        quests: &[Quest {
            text: "paper".into(),
            evidence: vec![],
            hero: true,
        }],
        policy: &pol(),
        capture: CaptureStatus::Scheduled,
        vision: None,
        manual_core: None,
    });
    assert_eq!(out.credited_core_seconds, 0);
    assert_ne!(out.dominant, Dominant::CoreResearch);
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife-core -- hint::tests::trusted_chrome_url_evidence_is_core`

Expected: FAIL（URL 尚未进入 core candidate）

- [ ] **Step 3: 实现**

`is_core_candidate`：

```rust
fn is_core_candidate(sample: &Sample, quests: &[Quest]) -> bool {
    if is_readme_or_settings_title(&sample.window_title) {
        return false;
    }
    crate::quest::matched_quest_index(
        &sample.window_title,
        sample.document_path.as_deref(),
        sample.url.as_deref(),
        quests,
    )
    .is_some()
}
```

`judge_slot`：

```rust
let quests_empty = !crate::quest::quest_list_has_evidence(input.quests);
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife-core`

Expected: PASS。再跑：`cargo test --offline -p gamelife`

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/hint.rs crates/gamelife-core/src/judge.rs
git commit -m "$(cat <<'EOF'
feat: match quest evidence on title path and URL

Treat quests with no evidence like an empty list so credited core
cannot come from display text alone.
EOF
)"
```

---

### Task 3: `set_quests` 写新 JSON

**Files:**
- Modify: `src-tauri/src/scheduler.rs`（`QuestJson`、load 改走 `parse_quest_versions_json`、`save_quests_for_day`）
- Modify: `src-tauri/src/commands.rs`（`set_quests` 接收 `Vec<QuestDraft>`）

**Interfaces:**
- Consumes: `parse_quest_versions_json`, `normalize_quest_list`, `QuestDraft`
- Produces:

```rust
pub fn save_quests_for_day(
    conn: &Connection,
    day: &str,
    drafts: Vec<gamelife_core::QuestDraft>,
    now: i64,
) -> Result<(), DbOpError>
```

写入 JSON 必须含 `text`/`evidence`/`hero`，不得再写 `keywords`。

`set_quests`：`sampling_allowed` 假 → `Err("day ended")`。`normalize` 的 `TooMany` → `"at most 3 quests"`；`EmptyText` → `"quest text empty"`。

- [ ] **Step 1: 写失败测试**

在 `scheduler.rs` tests（用 `Connection::open_in_memory` + `migrate`，**不要** `set_var HOME`）：

```rust
#[test]
fn save_quests_writes_evidence_and_hero() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let day = "2026-09-11";
    save_quests_for_day(
        &conn,
        day,
        vec![gamelife_core::QuestDraft {
            text: "Finish HDP tactile ablation".into(),
            evidence: vec!["HDP".into(), "a".into()],
            hero: true,
        }],
        1,
    )
    .unwrap();
    let json: String = conn
        .query_row(
            "SELECT json FROM quest_versions WHERE day = ?1",
            rusqlite::params![day],
            |r| r.get(0),
        )
        .unwrap();
    assert!(json.contains("\"evidence\""));
    assert!(json.contains("HDP"));
    assert!(!json.contains("\"a\""));
    let qs = load_quests_for_day(&conn, day).unwrap();
    assert_eq!(qs[0].text, "Finish HDP tactile ablation");
    assert_eq!(qs[0].evidence, vec!["HDP".to_string()]);
    assert!(qs[0].hero);
}

#[test]
fn load_legacy_keywords_json() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let day = "2026-09-10";
    conn.execute(
        "INSERT INTO quest_versions (day, json, created_at) VALUES (?1, ?2, 1)",
        rusqlite::params![day, r#"[{"text":"robot","keywords":["robot"]}]"#],
    )
    .unwrap();
    let qs = load_quests_for_day(&conn, day).unwrap();
    assert_eq!(qs[0].evidence, vec!["robot".to_string()]);
    assert!(qs[0].hero);
}
```

把 `save_quests_for_day` 先不实现或 `unimplemented!`，确认失败。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife -- scheduler::tests::save_quests_writes_evidence_and_hero`

Expected: FAIL

- [ ] **Step 3: 实现**

`load_quests_for_day` / `load_quests_for_version`：`parse_quest_versions_json`；失败则 `eprintln` 并 `Ok(vec![])`。

`save_quests_for_day`：`normalize_quest_list` 映射到 `DbOpError::Fatal`；`serde_json::json!` 数组写入。

`commands::set_quests(quests: Vec<QuestDraft>)` 调 `save_quests_for_day`。删除旧的 whitespace split。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/scheduler.rs src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
feat: persist quest evidence and hero in day snapshots

Stop splitting quest titles on whitespace when saving so match
tokens are only what the user typed.
EOF
)"
```

---

### Task 4: 沿用上一个有 Quest 的日期

**Files:**
- Modify: `src-tauri/src/scheduler.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `parse_quest_versions_json`, `save_quests_for_day`, `sampling_allowed`
- Produces:

```rust
pub fn previous_quest_day(conn: &Connection, today: &str) -> Result<Option<String>, DbOpError>
pub fn continue_previous_workday_for_day(
    conn: &Connection,
    today: &str,
    now: i64,
) -> Result<String, DbOpError>  // Ok(copied_from_day)
```

扫描：`SELECT day, json FROM quest_versions WHERE day < ?1 ORDER BY day DESC, id DESC`。跳过 parse 失败或 `len()==0`。第一份非空列表的 `day` 即 `previousWorkday`。

沿用：把解析出的 Quest 转成 `QuestDraft`（保留 text/evidence/hero）再 `save_quests_for_day`。没有 → `Fatal("no previous quests")`。

Command：`continue_previous_workday() -> Result<String, String>`，内部 `sampling_allowed` 否则 `"day ended"`。

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn previous_quest_day_skips_empty_json() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO quest_versions (day, json, created_at) VALUES ('2026-09-10', '[]', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO quest_versions (day, json, created_at) VALUES ('2026-09-09', ?1, 1)",
        rusqlite::params![r#"[{"text":"HDP","evidence":["HDP"],"hero":true}]"#],
    )
    .unwrap();
    assert_eq!(
        previous_quest_day(&conn, "2026-09-11").unwrap().as_deref(),
        Some("2026-09-09")
    );
}

#[test]
fn continue_copies_hero_and_evidence() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO quest_versions (day, json, created_at) VALUES ('2026-09-10', ?1, 1)",
        rusqlite::params![r#"[{"text":"HDP","evidence":["HDP"],"hero":true}]"#],
    )
    .unwrap();
    let from = continue_previous_workday_for_day(&conn, "2026-09-11", 2).unwrap();
    assert_eq!(from, "2026-09-10");
    let qs = load_quests_for_day(&conn, "2026-09-11").unwrap();
    assert_eq!(qs[0].text, "HDP");
    assert_eq!(qs[0].evidence, vec!["HDP".to_string()]);
    assert!(qs[0].hero);
}

#[test]
fn continue_without_history_errors() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let err = continue_previous_workday_for_day(&conn, "2026-09-11", 1).unwrap_err();
    assert!(format!("{err:?}").contains("no previous quests"));
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife -- scheduler::tests::continue_copies_hero_and_evidence`

Expected: FAIL

- [ ] **Step 3: 实现并注册 command**

`lib.rs` 的 `generate_handler!` 与 `use commands::{...}` 加上 `continue_previous_workday`。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/scheduler.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: copy the last non-empty quest snapshot onto today

Keep new workdays empty until the user continues yesterday so
stale tokens cannot silently start crediting.
EOF
)"
```

---

### Task 5: `get_today` 结构化 Quest + live

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/scheduler.rs`（`vision_quest_label` 填入 VisionContext）

**Interfaces:**
- Consumes: `matched_quest_index`, `matches_app_identity`, `load_policy`, `previous_quest_day`, `vision_quest_label`
- Produces:

```rust
pub struct QuestView { pub text: String, pub evidence: Vec<String>, pub hero: bool }
pub struct LiveWindow {
    pub app: String,
    pub title: String,
    pub document_path: Option<String>,
    pub url: Option<String>,
    pub matched_quest_index: Option<usize>,
    pub trusted: bool,
}
// TodayView.quests: Vec<QuestView>
// TodayView.previous_workday: Option<String>
// TodayView.live: Option<LiveWindow>
```

serde `rename_all = "camelCase"`。sample 列：`app, title, document_path, url, bundle_id`，`WHERE day=? ORDER BY ts DESC LIMIT 1`。

Vision：`quests: quests.iter().map(gamelife_core::vision_quest_label).collect()`。

- [ ] **Step 1: 写失败测试**

在 `commands.rs` tests 测 `build_today`（已是 `pub(crate)` 或改成可测）。若 `build_today` 是私有，本 Task 把它留在模块内，测试同文件：

```rust
#[test]
fn today_live_matches_latest_sample() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let day = "2026-09-11";
    save_quests_for_day(
        &conn,
        day,
        vec![gamelife_core::QuestDraft {
            text: "HDP".into(),
            evidence: vec!["HDP".into()],
            hero: true,
        }],
        1,
    )
    .unwrap();
    crate::sampler::insert_sample(
        &conn, 10, day, "Cursor", "train.py — HDP", None, None, None, 1, false, false, false,
    )
    .unwrap();
    let view = build_today(&conn, day).unwrap();
    assert_eq!(view.quests[0].text, "HDP");
    assert_eq!(view.live.as_ref().unwrap().matched_quest_index, Some(0));
    assert!(view.live.as_ref().unwrap().trusted);
}

#[test]
fn today_live_null_without_samples() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let view = build_today(&conn, "2026-09-11").unwrap();
    assert!(view.live.is_none());
    assert!(view.previous_workday.is_none());
}
```

Chrome 默认 Trusted 含 Cursor。`trusted` 对 Cursor 为 true。

再在 `scheduler` 或 `vision_ctx` 测 hero 标签进入 prompt：给 `VisionContext.quests` 传入 `vision_quest_label` 的结果后，`build_vision_prompt` 含 `[main]`。可在 `vision_ctx.rs` 加：

```rust
#[test]
fn prompt_includes_main_prefix_when_provided() {
    let mut ctx = ctx_with_password_and_cursor();
    ctx.quests = vec!["[main] HDP".into(), "notes".into()];
    let prompt = build_vision_prompt(&sanitize_vision_context(ctx, &builtin_never_capture()).unwrap());
    assert!(prompt.contains("1. [main] HDP"));
    assert!(prompt.contains("2. notes"));
}
```

scheduler 接线是本 Task 必须改的生产路径。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline -p gamelife -- commands::tests::today_live_matches_latest_sample`

Expected: FAIL

- [ ] **Step 3: 实现**

`TodayView` 改字段。前端 TypeScript 下 Task 6 才改；本 Task 若 `cargo test -p gamelife` 不含 TS。**本 Task 不要改 Today.tsx**（会暂时类型不匹配，但 vitest 未覆盖 Today）。若你改了 commands 而本地 tsc 不在 cargo 里，允许前端暂时红到 Task 6。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --offline -p gamelife`

Expected: PASS

Run: `cargo test --offline -p gamelife-core`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/scheduler.rs crates/gamelife-core/src/vision_ctx.rs
git commit -m "$(cat <<'EOF'
feat: expose live quest match from the latest sample

Let Today show whether the current window hits evidence without
splitting credited time per quest.
EOF
)"
```

---

### Task 6: 今日页 UI + `liveMatchLabel`

**Files:**
- Create: `src/lib/questLive.ts`
- Create: `src/lib/questLive.test.ts`
- Modify: `src/lib/api.ts`
- Modify: `src/pages/Today.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `getToday` 新字段；`setQuests(QuestDraft[])`；`continuePreviousWorkday()`
- Produces:

```ts
export interface QuestView {
  text: string;
  evidence: string[];
  hero: boolean;
}
export interface LiveWindow {
  app: string;
  title: string;
  documentPath: string | null;
  url: string | null;
  matchedQuestIndex: number | null;
  trusted: boolean;
}
export function liveMatchLabel(
  live: LiveWindow | null,
  quests: QuestView[],
): string
```

文案必须与规格一致：

- `尚无观测`
- `当前窗口未命中 Quest`
- `命中「{text}」`
- `命中「{text}」，当前应用不在 Trusted，不会记入主线`

- [ ] **Step 1: 写失败测试**

```ts
import { describe, expect, it } from "vitest";
import { liveMatchLabel, type LiveWindow, type QuestView } from "./questLive";

const quests: QuestView[] = [
  { text: "HDP", evidence: ["HDP"], hero: true },
];

const live = (over: Partial<LiveWindow>): LiveWindow => ({
  app: "Cursor",
  title: "train.py — HDP",
  documentPath: null,
  url: null,
  matchedQuestIndex: 0,
  trusted: true,
  ...over,
});

describe("liveMatchLabel", () => {
  it("no sample", () => {
    expect(liveMatchLabel(null, quests)).toBe("尚无观测");
  });
  it("miss", () => {
    expect(liveMatchLabel(live({ matchedQuestIndex: null }), quests)).toBe(
      "当前窗口未命中 Quest",
    );
  });
  it("trusted hit", () => {
    expect(liveMatchLabel(live({}), quests)).toBe("命中「HDP」");
  });
  it("untrusted hit", () => {
    expect(liveMatchLabel(live({ trusted: false }), quests)).toBe(
      "命中「HDP」，当前应用不在 Trusted，不会记入主线",
    );
  });
});
```

- [ ] **Step 2: 跑测试确认失败**

Run: `npx vitest run src/lib/questLive.test.ts`

Expected: FAIL

- [ ] **Step 3: 实现 `questLive.ts`、api、Today、样式**

`Today.tsx`：

- 本地 state：最多 3 条 `{ text, evidenceText, hero }`，`evidenceText` 用逗号或换行编辑，保存时 `split(/[,，\n]/)`。
- 英雄卡 class `quest-hero`；次要 `quest-secondary`。
- radio/checkbox「设为今日主线」保证只有一条 hero。
- `previousWorkday` 非空时按钮「沿用 {day}」，调用 `continuePreviousWorkday` 再 refresh。
- 保存调用 `setQuests` 结构化列表。空文案的槽不提交。
- 进度/冻结/结束今天保持原样。

`api.ts`：

```ts
export function setQuests(quests: QuestView[]): Promise<void> {
  return invoke("set_quests", { quests });
}
export function continuePreviousWorkday(): Promise<string> {
  return invoke("continue_previous_workday");
}
```

`QuestView` 的 `hero` 布尔随保存发送。

- [ ] **Step 4: 跑测试确认通过**

Run: `npx vitest run --dir src`

Expected: PASS

Run: `cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/questLive.ts src/lib/questLive.test.ts src/lib/api.ts src/pages/Today.tsx src/styles.css
git commit -m "$(cat <<'EOF'
feat: show today's hero quest and live window match

Replace the three empty inputs with an evidence editor and a
continue-yesterday action that does not auto-copy on its own.
EOF
)"
```

---

## 手工验收（全部 Task 之后）

- Preview 打开含课题名的 PDF：英雄命中且 Trusted。
- Chrome Overleaf：URL 命中。
- Cursor 仅标题含 token：命中。
- 新工作日未沿用：Quest 空，credited 为 0。
- 沿用后主线与 token 回来。
- 非 Trusted 应用：命中文案含「不会记入主线」。

---

## Self-review

**Spec coverage**

| 规格 | Task |
| --- | --- |
| §2 1–3 / hero / 文案与证据 | 1, 3, 6 |
| §2 标题 path URL | 1 函数，2 接线 |
| §2 无证据 credited 0 | 2 |
| §2 沿用 | 4 |
| §2 Live + Trusted 说明 | 5, 6 |
| §3 不另 snapshot | 5 |
| §5.5 vision `[main]` | 1 label + 5 接线 |
| §6 旧 keywords JSON | 1 parse + 3 load |
| §7 测试 | 各 Task |
| §8 不做项 | 全局约束 |

**Placeholder scan:** 无 TBD。Task 5 允许 Today.tsx 短暂类型不匹配，Task 6 收口。

**Type consistency:** `QuestDraft` / `QuestView` / `LiveWindow` / `previous_quest_day` / `save_quests_for_day` / `continue_previous_workday_for_day` / `liveMatchLabel` 在后续 Task 中的名字与 Task 1–4 一致。
