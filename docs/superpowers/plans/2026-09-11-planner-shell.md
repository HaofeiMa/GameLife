# Planner Shell and Task Judgment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用可自建列表的计划本取代 Quest，槽末文本 AI 判定并按列表打折发币，四页中文外壳（今日双列日历、本周三面板、礼品卡商店、三板块设置），删除时间轴页。

**Architecture:** 列表/任务/自然语言/快照/折扣 tick/AI JSON 校验放在 `gamelife-core`。SQLite `user_version=2` 增加 `task_lists`/`tasks` 与 `slots.task_snapshot_json`。槽开始钉快照；槽结束硬规则之后打一次文本 AI，失败再走现有视觉。`resolve_slot` 为主线写 `validated_coin/xp`，为支线/杂项写独立折扣键。前端只经 `src/lib/api.ts`。

**Tech Stack:** Tauri 2、Rust、React、TypeScript、vitest、SQLite、chrono。

**Spec:** `docs/superpowers/specs/2026-09-11-planner-shell-design.md`

## Global Constraints

- 15 秒采样；缺口不外推；进程死亡是未观测，不是离开。`credited ≤ observed ≤ 实际槽长`。
- 一天最多 96 个 15 分钟槽。周末不采样。
- 永不截屏：不截图、不把该窗口标题/路径送进模型、该段不发币。内置项不可删。
- 账本 `UNIQUE reward_event_key`。已 `final` 的槽不改经济。报告误判只留记录。
- Gold Day（当天主线 credited ≥ 28800）之后不再产生硬币或能量。已有余额可继续兑换。
- 主线 tick：900s→1 硬币、90s→1 能量，每日上限 32/320。打折用更长秒数门槛，不出现小数硬币。
- 商店单事务；同一 `redemption_id` 不双扣；同时最多一段未结束的能量娱乐会话。
- 无系统通知、不申请通知权限、无音效。Habitica GPL 资源不用。
- `document_path` 不得从窗口标题伪造。
- 改 `src-tauri/`：`CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife` 必须编译并跑过。
- 改 `gamelife-core`：`CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife-core`。
- 改 `src/`：`npx vitest run --dir src`。
- 测试禁止新增 `std::env::set_var("HOME", …)`。
- 界面中文。能量在 UI 称「能量」，账本字段仍是 `xp_delta`。不出现 Quest、不出现时间轴导航。
- 不引入子任务、重复规则、云同步、协作、一条任务多个时段、全天无钟点任务（长期的周/月范围除外）。
- 升级不把旧 Quest 自动变成任务。

---

## File Structure

```
crates/gamelife-core/src/task.rs         ListRole、TaskList、Task、快照、判定集合
crates/gamelife-core/src/task_parse.rs   中文自然语言解析
crates/gamelife-core/src/task_ai.rs      parse_task_match_json
crates/gamelife-core/src/ledger.rs       tick_keys_for_discount
crates/gamelife-core/src/judge.rs        JudgeOutput 折扣秒、apply_task_match
crates/gamelife-core/src/hint.rs         新槽不再用 Quest 证据做 CoreCandidate
crates/gamelife-core/src/lib.rs          导出
src-tauri/src/db.rs                     user_version 2、表、种子愿望
src-tauri/src/commands.rs               任务 CRUD、parse_task_line、get_today/week 新字段
src-tauri/src/lib.rs                    注册命令
src-tauri/src/scheduler.rs              钉快照；停用新槽 Quest 匹配
src-tauri/src/text_ai.rs                槽末文本 AI HTTP
src-tauri/src/resolve.rs                折扣账本键
src-tauri/src/config.rs                 文本模型沿用 primary_provider
src-tauri/tauri.conf.json               窗口加宽
src/lib/api.ts                          类型与 invoke
src/lib/feel.ts                         toast 中文键
src/App.tsx                             四页导航、中文 toast
src/pages/Today.tsx                     左清单右双列
src/pages/Week.tsx                      三面板
src/pages/Shop.tsx                      方形卡两栏+记录
src/pages/Settings.tsx                  三板块
src/styles.css                          设计系统
src/pages/Timeline.tsx                  删除
```

---

### Task 1: 任务列表与判定集合纯函数

**Files:**
- Create: `crates/gamelife-core/src/task.rs`
- Modify: `crates/gamelife-core/src/lib.rs`

**Interfaces:**
- Consumes: 无
- Produces:

```rust
pub const MAX_JUDGMENT_TASKS: usize = 20;
pub const PRESET_MAINLINE_ID: &str = "list-mainline";
pub const PRESET_SIDE_ID: &str = "list-side";
pub const PRESET_LONGTERM_ID: &str = "list-longterm";
pub const PRESET_CHORE_ID: &str = "list-chore";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListRole { Mainline, Side, Longterm, Chore, Custom }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRange { Week, Month }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskList {
    pub id: String,
    pub name: String,
    pub sort: i64,
    pub role: ListRole,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub list_id: String,
    pub title: String,
    pub done: bool,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub range: Option<TaskRange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub id: String,
    pub title: String,
    pub role: ListRole,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TaskListError { NoMainline, DuplicateMainline, EmptyName, TooManyJudgment }

pub fn preset_lists() -> Vec<TaskList>
pub fn validate_lists(lists: &[TaskList]) -> Result<(), TaskListError>
pub fn align_range(start: i64, end: i64) -> (i64, i64) // 向下/上对齐 900s，最短 900
pub fn in_judgment_set(task: &Task, list: &TaskList, day_start: i64, day_end: i64) -> bool
pub fn judgment_tasks<'a>(tasks: &'a [Task], lists: &[TaskList], day_start: i64, day_end: i64) -> Result<Vec<&'a Task>, TaskListError>
pub fn snapshot_of(tasks: &[&Task], lists: &[TaskList]) -> Vec<TaskSnapshot>
pub fn parse_task_snapshot_json(json: &str) -> Result<Vec<TaskSnapshot>, String>
```

`in_judgment_set`：未完成，且 `[start,end)` 与 `[day_start,day_end)` 相交；若 `list.role == Longterm` 则还必须有 start/end。无时段的长期返回 false。

`judgment_tasks` 超过 20 条 → `TooManyJudgment`。`validate_lists`：恰好一个 `Mainline`。

- [ ] **Step 1: Write the failing test**

在 `task.rs` 底部 `#[cfg(test)]`：

```rust
fn lists() -> Vec<TaskList> { preset_lists() }

fn timed(id: &str, list_id: &str, start: i64, end: i64) -> Task {
    Task { id: id.into(), list_id: list_id.into(), title: id.into(), done: false, start: Some(start), end: Some(end), range: None }
}

#[test]
fn longterm_without_window_is_not_judged() {
    let lists = lists();
    let t = Task {
        id: "a".into(), list_id: PRESET_LONGTERM_ID.into(), title: "本月论文".into(),
        done: false, start: None, end: None, range: Some(TaskRange::Month),
    };
    let day0 = 1_778_083_200; // 2026-05-06 00:00 UTC 仅作跨度，测试用任意日界
    assert!(!in_judgment_set(&t, &lists[2], day0, day0 + 86400));
}

#[test]
fn longterm_with_window_is_judged() {
    let lists = lists();
    let t = timed("a", PRESET_LONGTERM_ID, 1000, 1900);
    assert!(in_judgment_set(&t, &lists[2], 0, 86400));
}

#[test]
fn done_or_too_many_rejected() {
    let lists = lists();
    let mut t = timed("a", PRESET_MAINLINE_ID, 1000, 1900);
    t.done = true;
    assert!(!in_judgment_set(&t, &lists[0], 0, 86400));
    let many: Vec<Task> = (0..21).map(|i| timed(&format!("{i}"), PRESET_MAINLINE_ID, 1000, 1900)).collect();
    assert_eq!(judgment_tasks(&many, &lists, 0, 86400), Err(TaskListError::TooManyJudgment));
}

#[test]
fn preset_has_single_mainline() {
    validate_lists(&preset_lists()).unwrap();
    let mut bad = preset_lists();
    bad[1].role = ListRole::Mainline;
    assert_eq!(validate_lists(&bad), Err(TaskListError::DuplicateMainline));
}

#[test]
fn align_snaps_to_900() {
    assert_eq!(align_range(100, 1000), (0, 1800));
}
```

把 `pub mod task;` 加入 `lib.rs`，并 `pub use task::{ ListRole, Task, TaskList, TaskListError, TaskRange, TaskSnapshot, MAX_JUDGMENT_TASKS, PRESET_CHORE_ID, PRESET_LONGTERM_ID, PRESET_MAINLINE_ID, PRESET_SIDE_ID, align_range, in_judgment_set, judgment_tasks, parse_task_snapshot_json, preset_lists, snapshot_of, validate_lists };`。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife-core task::`

Expected: FAIL（module 不存在或函数未定义）

- [ ] **Step 3: Write minimal implementation**

实现上述函数。`preset_lists` 名称：主线任务 / 支线任务 / 长期计划 / 杂项，sort 0..=3。`align_range`：`start = start / 900 * 900`；`end = ((end + 899) / 900) * 900`；若 `end <= start` 则 `end = start + 900`。快照 JSON 用 serde。`judgment_tasks` 先 `validate_lists`。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife-core task::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: add task list model and judgment-set rules

Replace Quest cardinality with lists/roles so later slots can snapshot today's timed tasks.
EOF
)"
```

---

### Task 2: 中文自然语言解析

**Files:**
- Create: `crates/gamelife-core/src/task_parse.rs`
- Modify: `crates/gamelife-core/src/lib.rs`

**Interfaces:**
- Consumes: `TaskList`、`PRESET_*_ID`、`align_range`
- Produces:

```rust
pub struct ParseContext<'a> {
    pub now: DateTime<FixedOffset>, // 测试固定 +08:00
    pub lists: &'a [TaskList],
    pub current_list_id: &'a str,
    pub default_list_id: &'a str,
}

pub struct ParsedTask {
    pub title: String,
    pub list_id: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub parse_ok: bool,
}

pub fn parse_task_line(input: &str, ctx: &ParseContext<'_>) -> ParsedTask
```

无 `#` 用 `current_list_id`；调用方在「今日」汇总把 `current_list_id` 设成 `default_list_id`（主线）。`#杂项` 匹配名称或 role。失败：`parse_ok=false`，`title=trim(input)`，`list_id=current_list_id`，start/end None。

- [ ] **Step 1: Write the failing test**

```rust
fn ctx<'a>(lists: &'a [TaskList]) -> ParseContext<'a> {
    let now = DateTime::parse_from_rfc3339("2026-09-11T18:00:00+08:00").unwrap();
    ParseContext { now, lists, current_list_id: PRESET_MAINLINE_ID, default_list_id: PRESET_MAINLINE_ID }
}

#[test]
fn example_chore_tomorrow_morning() {
    let lists = preset_lists();
    let p = parse_task_line("明天上午十点到十一点，RAIDS+会议讨论 #杂项", &ctx(&lists));
    assert!(p.parse_ok);
    assert_eq!(p.title, "RAIDS+会议讨论");
    assert_eq!(p.list_id, PRESET_CHORE_ID);
    let start = DateTime::parse_from_rfc3339("2026-09-12T10:00:00+08:00").unwrap().timestamp();
    let end = DateTime::parse_from_rfc3339("2026-09-12T11:00:00+08:00").unwrap().timestamp();
    assert_eq!(p.start, Some(start));
    assert_eq!(p.end, Some(end));
}

#[test]
fn no_hash_uses_current_list() {
    let lists = preset_lists();
    let p = parse_task_line("写方法节", &ctx(&lists));
    assert_eq!(p.list_id, PRESET_MAINLINE_ID);
    assert_eq!(p.title, "写方法节");
}

#[test]
fn garbage_stays_unscheduled() {
    let lists = preset_lists();
    let p = parse_task_line("asdfgh", &ctx(&lists));
    assert!(!p.parse_ok);
    assert_eq!(p.title, "asdfgh");
    assert_eq!(p.start, None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife-core task_parse::`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

解析顺序：抽 `#标签`（对 lists 的 name 或「主线」「支线」「长期」「杂项」）；抽相对日（今天/明天/后天/周X）；抽「上午|下午|晚上」+「N点|N点半」到「N点」。中文数字 十=10、十一=11。去掉已消耗片段后的剩余为标题（去前导逗号）。时段经 `align_range`。`chrono` 已在 core 依赖里。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife-core task_parse::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task_parse.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: parse Chinese natural-language task lines

Keep task creation offline: date, clock range, and #list tags without an extra model call.
EOF
)"
```

---

### Task 3: 非主线折扣账本键

**Files:**
- Modify: `crates/gamelife-core/src/ledger.rs`
- Modify: `crates/gamelife-core/src/lib.rs`（若需导出常量）

**Interfaces:**
- Consumes: 现有 `RewardEvent`、`tick_keys_for_credited`（不改其签名与主线 32/320 上限）
- Produces:

```rust
pub const SIDE_COIN_TICK_SECS: i64 = 1500;
pub const SIDE_XP_TICK_SECS: i64 = 150;
pub const CHORE_COIN_TICK_SECS: i64 = 3000;
pub const CHORE_XP_TICK_SECS: i64 = 300;

pub fn tick_keys_for_discount(day: &str, kind: ListRole, before: i64, after: i64) -> Vec<RewardEvent>
```

`ListRole::Side | Longterm | Custom` 用 SIDE 秒数，键 `validated_side_coin:{day}:{n}` / `validated_side_xp:{day}:{n}`。`Chore` 用 CHORE 秒数，键 `validated_chore_coin:{day}:{n}` / `validated_chore_xp:{day}:{n}`。`Mainline` 返回空（走 `tick_keys_for_credited`）。**无** 32/320 封顶。调用方在主线 Gold Day 后不要调用。

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn side_25_min_pays_one_coin() {
    let evs = tick_keys_for_discount("2026-09-11", ListRole::Side, 0, 1500);
    assert!(evs.iter().any(|e| e.key == "validated_side_coin:2026-09-11:1" && e.coin == 1));
}

#[test]
fn chore_50_min_pays_one_coin() {
    let evs = tick_keys_for_discount("2026-09-11", ListRole::Chore, 0, 3000);
    assert!(evs.iter().any(|e| e.key == "validated_chore_coin:2026-09-11:1" && e.coin == 1));
}

#[test]
fn mainline_discount_is_empty() {
    assert!(tick_keys_for_discount("2026-09-11", ListRole::Mainline, 0, 900).is_empty());
}

#[test]
fn existing_core_tick_unchanged() {
    let evs = tick_keys_for_credited("2026-09-11", 0, 900);
    assert!(evs.iter().any(|e| e.key == "validated_coin:2026-09-11:1"));
}
```

把 `use crate::task::ListRole;` 加进 `ledger.rs`。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife-core ledger::`

Expected: FAIL on new tests

- [ ] **Step 3: Write minimal implementation**

按 `before/tick` 到 `after/tick` 插入整数 tick，与 `tick_keys_for_credited` 同样的循环，但不 `min(MAX_*)`。Side 能量 1500s 也应产生 `validated_side_xp` 共 10 个（1500/150）。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife-core ledger::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/ledger.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: add discounted coin and energy ledger ticks

Pay side/longterm at 60% and chores at 30% without disturbing mainline Gold Day caps.
EOF
)"
```

---

### Task 4: AI JSON 校验与任务发币纯函数

**Files:**
- Create: `crates/gamelife-core/src/task_ai.rs`
- Modify: `crates/gamelife-core/src/judge.rs`（`JudgeOutput` 增加字段；`apply_task_match`）
- Modify: `crates/gamelife-core/src/lib.rs`

**Interfaces:**
- Consumes: `TaskSnapshot`、`ListRole`、`SlotEvidence`、`JudgeOutput`
- Produces:

```rust
pub struct TaskMatch {
    pub task_id: String,
    pub confidence: f64,
    pub role: ListRole,
}

pub enum TaskMatchError { InvalidJson, BadConfidence, UnknownTask }

pub fn parse_task_match_json(json: &str, snapshots: &[TaskSnapshot]) -> Result<Option<TaskMatch>, TaskMatchError>
pub fn payout_base_seconds(ev: &SlotEvidence) -> i64
pub fn apply_task_match(mut output: JudgeOutput, ev: &SlotEvidence, m: &TaskMatch) -> JudgeOutput
```

JSON：`{"task_id": string|null, "confidence": number}`。`task_id` null → `Ok(None)`。confidence 须有限且在 0..=1。id 必须在 snapshots 里。confidence < 0.7 时 `parse_task_match_json` 仍 `Ok(Some)`，由调用方决定是否视觉回退；另提供 `pub const TASK_MATCH_MIN: f64 = 0.7`。

`payout_base_seconds` = `ev.observed_seconds -` 对应 away/distraction 的 activity 秒，再 `max(0)`。

`apply_task_match`：若 `m.confidence < 0.7` 原样返回。Mainline：`credited_core_seconds = min(existing credited, payout_base)` 若 existing 为 0 则设为 payout_base，`dominant = CoreResearch`，activity.core 至少加上该秒。Side/Longterm/Custom：`credited_core_seconds = 0`，`credited_side_seconds = payout_base`，`dominant = SideProject`。Chore：`credited_chore_seconds = payout_base`，`dominant = Admin`。

`JudgeOutput` 增加 `credited_side_seconds: i64`、`credited_chore_seconds: i64`。所有现有 `JudgeOutput` 字面量逐处补 `credited_side_seconds: 0, credited_chore_seconds: 0`。不要给 `JudgeOutput` 加 Default。

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn rejects_unknown_task() {
    let snaps = [TaskSnapshot { id: "t1".into(), title: "HDP".into(), role: ListRole::Mainline }];
    assert_eq!(parse_task_match_json(r#"{"task_id":"nope","confidence":0.9}"#, &snaps), Err(TaskMatchError::UnknownTask));
}

#[test]
fn null_task_is_none() {
    let snaps = [];
    assert_eq!(parse_task_match_json(r#"{"task_id":null,"confidence":0.9}"#, &snaps), Ok(None));
}

#[test]
fn apply_mainline_sets_core() {
    let ev = SlotEvidence {
        hints: vec![],
        spans: vec![],
        activity: ActivitySeconds::default(),
        strong_core_seconds: 0,
        grounded_strong_core_seconds: 0,
        reading_bridge_seconds: 0,
        observed_seconds: 900,
    };
    let out = JudgeOutput {
        dominant: Dominant::PendingReview,
        activity: ActivitySeconds::default(),
        credited_core_seconds: 0,
        credited_side_seconds: 0,
        credited_chore_seconds: 0,
        observed_seconds: 900,
        used_vision: false,
        pending: false,
    };
    let m = TaskMatch { task_id: "t1".into(), confidence: 0.9, role: ListRole::Mainline };
    let out = apply_task_match(out, &ev, &m);
    assert_eq!(out.credited_core_seconds, 900);
    assert_eq!(out.dominant, Dominant::CoreResearch);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife-core task_ai::`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现解析与 `apply_task_match`。更新 `judge.rs` 所有 `JudgeOutput` 结构体字面量。`cargo test --offline -p gamelife-core judge::` 必须重新绿。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife-core`

Expected: PASS（整包，因 JudgeOutput 字段变更）

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task_ai.rs crates/gamelife-core/src/judge.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: validate task-match JSON and apply discounted payout

Keep model output fail-closed and map list roles onto core versus side/chore seconds.
EOF
)"
```

---

### Task 5: 空判定集合不再靠 Quest 发币

**Files:**
- Modify: `crates/gamelife-core/src/judge.rs`（`JudgeInput` 增加 `tasks: &'a [TaskSnapshot]`；`quests` 仍留着给旧 hint，直到本任务改 hint）
- Modify: `crates/gamelife-core/src/hint.rs`（`is_core_candidate` 在 `quests` 为空时不要仅因 H2 自动 Core——**改为**：无 tasks 时 `judge_slot` 开头若 `tasks.is_empty()` 则 `credited_core_seconds = 0`，即使后面 vision wants_core。保留 activity 写入。）
- Modify: 所有构造 `JudgeInput` 的测试，补 `tasks: &[]` 或带快照。

**Interfaces:**
- Consumes: `TaskSnapshot`
- Produces: `JudgeInput.tasks`；空 tasks ⇒ 主线 credited 0（规格：无当天可判定任务）

实现要点：把现有 `quests_empty` 换成 `tasks.is_empty()`。仍把 `quests` 传给 `hint_sample` 本任务可传 `&[]` 从 scheduler 侧；core 测试里需要自动 Core 的，改为先有 `TaskSnapshot` mainline 且测试走 `apply_task_match`，或暂时仍用 quests 产生 CoreCandidate——**本任务明确**：`judge_slot` 在 `input.tasks.is_empty()` 时强制 `credited_core_seconds = 0`（在现有 quests_empty 分支替换条件）。有 tasks 时不再要求 quest evidence。有 tasks 但无 vision/match 的灰区仍 pending。

- [ ] **Step 1: Write the failing test**

在 `judge.rs` 测试：

```rust
#[test]
fn empty_tasks_zero_credit_even_with_quest_evidence() {
    let quests = [Quest::fixture("HDP", "HDP")];
    let samples = grid("Cursor", "HDP train.py", 0, 60, 15, 5);
    let out = judge_slot(JudgeInput {
        slot_start: 0,
        slot_end: 900,
        samples: &samples,
        quests: &quests,
        tasks: &[],
        policy: &pol(),
        capture: CaptureStatus::Captured,
        vision: None,
        manual_core: None,
    });
    assert_eq!(out.credited_core_seconds, 0);
}
```

`grid` 与 `pol` 已在 `judge.rs` 测试模块中。所有现有 `JudgeInput` 字面量补 `tasks: &[]`；需要 credited>0 的测试改为使用

```rust
tasks: &[TaskSnapshot { id: "t1".into(), title: "HDP".into(), role: ListRole::Mainline }]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife-core judge::tests::empty_tasks_zero_credit_even_with_quest_evidence`

Expected: FAIL（仍按 quest 发币）或编译失败缺 `tasks`

- [ ] **Step 3: Write minimal implementation**

`JudgeInput` 加 `tasks`。`judge_slot` 用 `input.tasks.is_empty()` 替代 `!quest_list_has_evidence(input.quests)` 作为 credited 清零条件。补全所有 `JudgeInput` 字面量。需要自动 Core 的旧测试加上一条 mainline `TaskSnapshot`（id 任意），否则它们会变成 credited 0——这是规格要求。把那些测试改成断言：有 snapshot 时旧 grounded 路径仍可自动 Core。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife-core`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/judge.rs crates/gamelife-core/src/hint.rs
git commit -m "$(cat <<'EOF'
feat: gate core credit on task snapshots instead of quest evidence

Empty today's judgment set pays nothing, matching the planner contract.
EOF
)"
```

---

### Task 6: SQLite user_version 2 与任务表

**Files:**
- Modify: `src-tauri/src/db.rs`

**Interfaces:**
- Consumes: `preset_lists`、`WishKind`
- Produces: `TARGET_USER_VERSION = 2`；表 `task_lists`、`tasks`；`slots.task_snapshot_json`、`slots.credited_side_seconds`、`slots.credited_chore_seconds`；空 wishes 时种子 6 条示例。

```sql
CREATE TABLE IF NOT EXISTS task_lists (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, sort INTEGER NOT NULL, role TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  list_id TEXT NOT NULL,
  title TEXT NOT NULL,
  done INTEGER NOT NULL DEFAULT 0,
  start INTEGER,
  end INTEGER,
  range TEXT
);
```

种子愿望（仅 `SELECT COUNT(*) FROM wishes` 为 0 时）：

| id | name | kind | price | duration |
| --- | --- | --- | --- | --- |
| seed-coin-tea | 一杯奶茶 | coin | 32 | NULL |
| seed-coin-takeout | 一顿外卖 | coin | 64 | NULL |
| seed-coin-book | 一本新书 | coin | 48 | NULL |
| seed-xp-bilibili | B 站 45 分钟 | xp | 20 | 45 |
| seed-xp-game | 游戏 30 分钟 | xp | 18 | 30 |
| seed-xp-shorts | 短视频 15 分钟 | xp | 10 | 15 |

`seed_preset_lists_if_empty` 在 lists 为空时插入 `preset_lists()`。

- [ ] **Step 1: Write the failing test**

在 `db.rs` tests：

```rust
#[test]
fn migrate_v2_adds_task_tables_and_seed_wishes() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA user_version = 1;").unwrap();
    migrate(&conn).unwrap();
    let v: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 2);
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM task_lists", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 4);
    let w: i64 = conn.query_row("SELECT COUNT(*) FROM wishes", [], |r| r.get(0)).unwrap();
    assert_eq!(w, 6);
    migrate(&conn).unwrap();
    let w2: i64 = conn.query_row("SELECT COUNT(*) FROM wishes", [], |r| r.get(0)).unwrap();
    assert_eq!(w2, 6);
}
```

先把现有 `assert_eq!(user_version(&conn), 1)` 的测试改为 2（`migrate_new_db_sets_user_version_1` 改名并断言 2）。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife -- db::tests::migrate_v2_adds_task_tables_and_seed_wishes`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

`migrate`：`CREATE TABLE IF NOT EXISTS` 任务表；`add_column_if_missing` 三列；若 `version < 2` 则 `PRAGMA user_version=2`。每次 migrate 调种子函数（空才插）。更新所有断言 version==1 的测试。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife -- db::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db.rs
git commit -m "$(cat <<'EOF'
feat: migrate sqlite to v2 with task tables and shop seeds

Persist planner lists and gift-card examples without rewriting quest history.
EOF
)"
```

---

### Task 7: 任务命令、钉快照、折扣入账

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/scheduler.rs`（`ensure_slot` 写入 `task_snapshot_json`；finalize 读快照而非/不止 Quest）
- Modify: `src-tauri/src/resolve.rs`
- Modify: `src/lib/api.ts`

**Interfaces:**
- Consumes: `parse_task_line`、`judgment_tasks`、`snapshot_of`、`tick_keys_for_discount`
- Produces: 命令 `list_tasks`、`upsert_task`、`toggle_task_done`、`parse_task_line`、`create_list`；`TodayView` 增加 `lists`/`tasks`/`coinBalance`；槽行带 `task_snapshot_json`。

`resolve_slot` 增加读取当天已有 `SUM(credited_side_seconds)` / chore，加上本槽后再 `tick_keys_for_discount`。Gold Day：若 `credited_before >= GOLD_DAY_SECS` 则连折扣键也不写。`upsert_slot` SQL 写入新列。

`ensure_slot`：计算当天 `day_start/day_end` 本地午夜，`judgment_tasks`，serde JSON 写入 snapshot。失败 TooMany 时仍开槽但 snapshot `[]` 并打日志（命令层加任务时已拒绝 >20）。

- [ ] **Step 1: Write the failing test**

`resolve.rs`：

```rust
#[test]
fn side_seconds_insert_discount_ticks() {
    let mut conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let mut output = core_output(0);
    output.dominant = Dominant::SideProject;
    output.credited_side_seconds = 1500;
    output.observed_seconds = 1500;
    resolve_slot(&mut conn, "2026-09-11", 0, &output, 0, 0).unwrap();
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ledger WHERE reward_event_key='validated_side_coin:2026-09-11:1'",
        [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1);
}
```

`core_output` 夹具补 `credited_side_seconds: 0, credited_chore_seconds: 0`。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife -- resolve::tests::side_seconds_insert_discount_ticks`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现 resolve 折扣循环、scheduler 钉快照、commands CRUD（id 用 `Uuid` 或 `format!("task-{}", ts)`）。注册 invoke。`get_today` 返回任务与 `coinBalance`（`SUM(coin_delta)`）。前端 `api.ts` 同步类型。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/scheduler.rs src-tauri/src/resolve.rs src/lib/api.ts
git commit -m "$(cat <<'EOF'
feat: persist task snapshots and discounted ledger on resolve

New slots pin today's judgment set; side and chore seconds mint separate ticks.
EOF
)"
```

---

### Task 8: 槽末文本 AI 与永不截屏

**Files:**
- Create: `src-tauri/src/text_ai.rs`
- Modify: `src-tauri/src/scheduler.rs`（finalize 路径：硬规则后若需模型则调用）
- Modify: `src-tauri/src/lib.rs`（`mod text_ai`）
- Modify: `src-tauri/src/scheduler.rs` 中现有 `call_vision_api` 调用点（保持 vision.rs 签名不变）

**Interfaces:**
- Consumes: `parse_task_match_json`、`TASK_MATCH_MIN`、`apply_task_match`、现有 `VisionEndpoint` / `call_vision_api`
- Produces: `call_text_task_match(endpoint, snapshots, sample_summary) -> Result<String, TextAiError>` 只返回模型原文；core 负责 parse。

请求：`POST {base}/chat/completions`，`response_format: json_object`，无图片。超时 20s。User prompt：任务 id+标题+role 列表 + 本槽每条样本的 app/title/url/document_path/idle（永不截屏或 secure_input 的样本整行省略）。若省略后无任何样本 → 不调用，待复核。

调用顺序在 scheduler finalize（找到现有 `judge_slot` / vision 处）：先 `judge_slot` 得硬规则 output；若 `pending` 或 dominant 灰区且 `!tasks.is_empty()` 且未 never-capture 整槽，则文本 AI；parse 成功且 confidence≥0.7 且 Some(match) → `apply_task_match`；否则现有视觉；再失败保持 pending。

- [ ] **Step 1: Write the failing test**

`text_ai.rs` 测 prompt 构造纯函数（不打网）：

```rust
pub struct SampleLine {
    pub app: String,
    pub title: String,
    pub url: Option<String>,
    pub document_path: Option<String>,
    pub idle_seconds: i64,
    pub protected: bool,
}

#[test]
fn omits_never_capture_sample_lines() {
    let body = sample_summary_lines(&[SampleLine {
        app: "1Password".into(),
        title: "secret".into(),
        url: None,
        document_path: None,
        idle_seconds: 0,
        protected: true,
    }]);
    assert!(!body.contains("secret"));
}

#[test]
fn empty_summary_means_skip() {
    let body = sample_summary_lines(&[SampleLine {
        app: "1Password".into(),
        title: "x".into(),
        url: None,
        document_path: None,
        idle_seconds: 0,
        protected: true,
    }]);
    assert!(body.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife -- text_ai::`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现 summary、HTTP（可仿 `vision.rs` 的 reqwest 调用，不要 JPEG）。Scheduler 接上。无密钥：不调用，pending。

- [ ] **Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/text_ai.rs src-tauri/src/scheduler.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: match timed tasks with slot-end text AI

Try JSON matching before vision, and never send Never Capture window text.
EOF
)"
```

---

### Task 9: 外壳、中文、删除时间轴

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/lib/feel.ts` / `src/lib/feel.test.ts`
- Modify: `src/styles.css`
- Modify: `src-tauri/tauri.conf.json`（width 1100, height 720）
- Delete: `src/pages/Timeline.tsx`

**Interfaces:**
- Consumes: 现有 `FeelNotice`
- Produces: 导航 `today | week | shop | settings`；toast「+N 硬币」「+N 能量」「宝箱已达成」「黄金日」「早开始 +N」「连胜 N」「已兑换」

CSS 变量：

```css
:root {
  --gl-mainline: #22c55e;
  --gl-side: #3b82f6;
  --gl-longterm: #8b5cf6;
  --gl-chore: #f59e0b;
  --gl-play: #ef4444;
  --gl-muted: #9ca3af;
  --gl-rail: #1f1f23;
}
```

左侧 `.app-rail` 图标按钮。`.app { max-width: none; }`。

- [ ] **Step 1: Write the failing test**

`feel.test.ts` 或小函数 `noticeText` 抽到 `src/lib/feel.ts` 并测：

```ts
expect(noticeText({ type: "xp", n: 10 })).toBe("+10 能量");
expect(noticeText({ type: "coins", n: 1 })).toBe("+1 硬币");
```

把 `noticeText` 从 `App.tsx` 移到 `feel.ts` 以便测。

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/feel`

Expected: FAIL（仍是 XP/Coins）

- [ ] **Step 3: Write minimal implementation**

改文案、导航、删 Timeline import、加宽窗口、基础 CSS 变量与 rail。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx src/lib/feel.ts src/lib/feel.test.ts src/styles.css src-tauri/tauri.conf.json src/pages/Timeline.tsx
git commit -m "$(cat <<'EOF'
feat: switch shell to four Chinese tabs and drop timeline

Widen the window for a list/calendar split and rename XP to 能量 in toasts.
EOF
)"
```

---

### Task 10: 今日页清单 + 一天双列 + 复核

**Files:**
- Modify: `src/pages/Today.tsx`
- Modify: `src/styles.css`
- Modify: `src/lib/api.ts`（`reviewSlot` / `reportMisclassification` 已存在）
- Modify: `src-tauri/src/commands.rs`（`get_today` 的 `slots` 已有 pending/final；日历日参数 `get_day_slots(day)` 若右栏翻日期需要：加命令 `get_day_view(day: String)` 返回该日 tasks 时段 + slots，左栏仍用 `get_today`）

**Interfaces:**
- Consumes: `parse_task_line`、`upsert_task`、`toggle_task_done`、`review_slot`、`report_misclassification`
- Produces: 左栏徽章（硬币今日+总数、能量、连胜环）、输入框、按列表分组；右栏 1 天两列计划|实际，‹今天›，点实际块复核。

计划块：当天 `start/end` 任务按列表颜色。实际块：每槽 15 分钟，颜色按 dominant 中文映射（core_research→主线绿，side_project→支线蓝，admin→杂项橙，distraction→娱乐红，break_away/unobserved→灰，pending_review→黄）。当前未结束槽描边「正在识别」。

从 `Timeline.tsx` 移入分类按钮与误判表单（中文类别：主线/辅助/支线/杂项/娱乐/离开）。无 Quest 表单。沿用上一工作日按钮删除（Quest 已废）或改成「无」。规格：不迁移 Quest。

结束今天、冻结、权限条、娱乐倒计时保留。

- [ ] **Step 1: Write the failing test**

`src/lib/calendar.test.ts`：

```ts
import { slotIndex, planBlocks } from "./calendar";
test("slot index 0 is midnight", () => {
  expect(slotIndex(0)).toBe(0);
});
test("plan block uses 15m grid", () => {
  const b = planBlocks([{ start: 9 * 3600, end: 11 * 3600, role: "mainline", title: "HDP" }], 0);
  expect(b[0].rowStart).toBe(9 * 4);
  expect(b[0].rowSpan).toBe(8);
});
```

`slotIndex(ts, dayStart)` = `(ts-dayStart)/900`。

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/calendar`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

实现 `calendar.ts` 与 Today 布局（左 `.today-left` 右 `.today-cal` 两列 CSS grid 24h×2）。成品视觉：圆角块、红线 `now`、独立白面板。调用 `parse_task_line` 再 `upsert_task`。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/pages/Today.tsx src/lib/calendar.ts src/lib/calendar.test.ts src/styles.css src/lib/api.ts src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: rebuild Today as task list plus plan/actual day calendar

Move slot review onto actual blocks and drop the Quest editor.
EOF
)"
```

---

### Task 11: 本周三面板（成品级）

**Files:**
- Modify: `src-tauri/src/commands.rs`（`WeekView` 增加 `byDay: { day, core, side, chore }[]`、`byHour: { hour, core, observed }[]`、`coreLabel`）
- Modify: `src/lib/api.ts`
- Modify: `src/pages/Week.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `sum_activity`、各槽 `activity_json` 与 `credited_*`
- Produces: 类别条、按天堆叠柱、8–21 点热力（`core/observed` 越深越绿）。空状态：「本周还没有观测」。周末柱空。数字标注分钟。图例。不得只画无标签色条。

`byHour`：把本周工作日每槽按本地小时累加 `activity.core` 与 `observed_seconds`。

- [ ] **Step 1: Write the failing test**

`src/lib/weekHeat.test.ts`：

```ts
import { heatTone } from "./weekHeat";
test("no observe is empty", () => { expect(heatTone(0, 0)).toBe(0); });
test("full core is 1", () => { expect(heatTone(3600, 3600)).toBe(1); });
```

`heatTone(core, observed) = observed===0 ? 0 : core/observed` clamp 0..=1。

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/weekHeat`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

后端聚合 + 前端三张卡片（阴影、标题、图例、数值）。类别名中文。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src` 与 `CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife -- commands::`

Expected: PASS（若 commands 测试因 WeekView 字段失败则一并修夹具）

- [ ] **Step 5: Commit**

```bash
git add src/pages/Week.tsx src/lib/weekHeat.ts src/lib/weekHeat.test.ts src/lib/api.ts src/styles.css src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
feat: add weekly category, daily, and focus-hour panels

Show activity-second totals instead of a single English table.
EOF
)"
```

---

### Task 12: 商店礼品卡与设置三板块

**Files:**
- Modify: `src/pages/Shop.tsx`
- Modify: `src/pages/Settings.tsx`
- Modify: `src/styles.css`
- Modify: `src/lib/api.ts`（`RedemptionView` 补 `durationMinutes`、进行中状态；`WeekView.redemptions` 已有则扩字段）
- Modify: `src-tauri/src/commands.rs`（redemptions 查询带 name/ts/duration；会话 remaining）

**Interfaces:**
- Consumes: 现有 `create_wish`/`redeem`/`archive_wish`/`update_wish`、种子愿望
- Produces: 小方形卡网格（约 6 列）、硬币/能量两 `section`、底部记录表；设置 `基础 | API | 名单` 三个 `tab`。商店锁定时卡片 `兑换` 禁用并显示还差的主线分钟。

记录列：时间、商品、类型（硬币/能量）、扣币、能量状态。只读。

设置：基础=开机启动+截图保留+样本天数；API=现有 provider 表单中文标题；名单=信任/永不截屏/娱乐/阅读/支线。无采样间隔。

- [ ] **Step 1: Write the failing test**

`src/lib/shopSplit.test.ts`：

```ts
import { splitWishes } from "./shopSplit";
test("splits coin and energy", () => {
  const { coin, energy } = splitWishes([
    { id: "a", name: "茶", kind: "coin", price: 32, durationMinutes: null },
    { id: "b", name: "B站", kind: "xp", price: 20, durationMinutes: 45 },
  ]);
  expect(coin.map((w) => w.id)).toEqual(["a"]);
  expect(energy.map((w) => w.id)).toEqual(["b"]);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/shopSplit`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

`kind === "coin"` 进硬币栏，否则能量栏。Shop/Settings JSX 按规格重排。卡片 `aspect-ratio: 1`，`max-width: 112px`。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src`  
`CARGO_TARGET_DIR=/Volumes/MobileSSD/Program/My/GameLife/target cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/pages/Shop.tsx src/pages/Settings.tsx src/lib/shopSplit.ts src/lib/shopSplit.test.ts src/lib/api.ts src/styles.css src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
feat: restyle shop gift cards and sectioned settings

Split coin versus energy lists, show redemption history, and group settings in Chinese.
EOF
)"
```

---

## Spec coverage

| Spec 节 | Task |
| --- | --- |
| §2 导航四页、删时间轴 | 9 |
| §2 中文、能量 | 9–12 |
| §5 列表/任务/判定集合 | 1, 6, 7 |
| §5.3 自然语言 | 2, 7, 10 |
| §6 判定 AI + 折扣 | 3, 4, 5, 8 |
| §7 今日双列+复核 | 10 |
| §8 本周 | 11 |
| §9 商店卡/种子/记录 | 6, 12 |
| §10 设置 | 12 |
| §3 永不截屏不送模型 | 8 |
| §3 Gold Day | 3, 7 |
| 不迁移 Quest | 5, 7, 10 |

## 执行注意

- 每个 Task 结束才 commit；失败不要 `--no-verify`。
- `JudgeOutput` 字段变更会碰到大量字面量，Task 4 必须把 `gamelife-core` 测全绿再进入 Tauri。
- 本周与商店实现按成品视觉，不要复用 brainstorm HTML 的粗糙间距。
