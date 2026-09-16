# Tasks Interaction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 任务页与今日计划列补齐交互（对齐、右键子菜单、拖排序、改期面板、拉边、活动竖条、复制），完成后续写下一次，系统通知可选；判定改为匹配未完成任务、不看计划时段。

**Architecture:** 重复平移、拉边、匹配、快照挑选仍在 `gamelife-core`。`tasks` 加 `sort` / `repeat` / `remind_json`，`user_version = 4`。命令层负责 CRUD、完成续写、通知调度。前端自绘，不引入 FullCalendar / dnd-kit / Radix。改期面板只用现有 Dialog + token。

**Tech Stack:** Tauri 2、Rust、React、TypeScript、vitest、SQLite、chrono。macOS 通知用 `objc2-user-notifications`（仅 macos target）。Windows / Linux 通知函数空实现。

**Spec:** `docs/superpowers/specs/2026-09-15-tasks-interaction-design.md`（覆盖 `2026-09-15-local-tasks-design.md` 中排序/改期/20 条门槛/无重复提醒/按时段判定，以及落地自动 Core、默认无系统通知）。

## Global Constraints

- 15 秒采样；缺口不外推；进程死亡是未观测。`credited ≤ observed ≤ 实际槽长`。
- 观测优先：锁屏 / 离开 / 娱乐硬规则压过任务标题匹配。GameLife 自己的窗口仍是支线。
- 勾选、放弃、改期、复制、排序不发币。已 `final` 槽不改经济。已开始槽 `COALESCE` 钉快照。
- 本地匹配用钉住的全部未完成快照；AI 提示最多 20 条。不再因 20 条拒绝 `upsert`。
- 不做全天、时区、dnd-kit、Radix、FullCalendar、应用内音效、子任务、一条任务多时段。
- 界面中文。能量称「能量」。颜色只来自 `--cat-*` / `theme.ts`。
- 页面只经 `src/lib/api.ts` 调 `invoke()`。
- 改 `src-tauri/`：`CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife` 必须跑过。
- 改 `gamelife-core`：同样跑 `-p gamelife-core`。
- 改 `src/`：`npx vitest run --dir src`。
- 测试禁止新增 `std::env::set_var("HOME", …)`。
- 每个任务一条英文 conventional commit，不 `--no-verify`。
- 工作区里若有无关 rustfmt 脏文件（`hint.rs` 以外的 core/tauri 格式化），不要 `git add` 进本任务。

---

## Spec coverage（执行前对照，禁止跳过）

| Spec 条款 | 任务 |
| --- | --- |
| §5 `sort` / `repeat` / `remind_json`，`user_version` 4 | Task 1, 4 |
| §7.2 完成后续写、过期跳今天、月末钳制 | Task 1, 5 |
| §7.1 清除时段、拉边最短 15 分钟 | Task 1, 7, 10 |
| §9.1 未完成全进快照；AI 截 20 | Task 2, 3 |
| §9.2–9.4 hint 顺序、不再落地自动 Core、发币档 | Task 3 |
| §6 列表对齐、去排序、拖组、右键、复制 | Task 8, 9 |
| §7 改期面板视觉走 App | Task 7 |
| §7.3 通知总开关 | Task 6 |
| §8 日历拉边、竖条、今日同权 | Task 10, 11 |
| §10 命令 | Task 5 |
| §12 文档 | Task 12 |

---

## File Structure

```
crates/gamelife-core/src/task.rs       RepeatRule、sort、remind、next_occurrence、resize、match
crates/gamelife-core/src/hint.rs       硬规则后任务匹配；去掉路径自动 CoreCandidate
crates/gamelife-core/src/judge.rs      自动 Core 看匹配主线 strong_core，不再看 grounded_strong
crates/gamelife-core/src/lib.rs        导出新类型
src-tauri/src/db.rs                    user_version 4；load/persist 新列
src-tauri/src/commands.rs               reorder / duplicate / toggle 续写 / TaskView 新字段
src-tauri/src/lib.rs                   注册命令
src-tauri/src/scheduler.rs               snapshots_open；任务变更后同步通知
src-tauri/src/text_ai.rs               prompt 用 select_prompt_snapshots
src-tauri/src/config.rs                task_notifications
src-tauri/src/task_notify.rs            新增：登记/取消系统通知
src-tauri/src/macos/notify.rs          新增：UNUserNotificationCenter
src/lib/api.ts                         TaskView 字段；新 invoke
src/lib/taskSchedule.ts               时刻步进、默认块、偏移校验
src/lib/taskReorder.ts               组内插入 sort
src/lib/taskCalendar.ts                resizeRange
src/lib/slotRibbon.ts                 dominant → CategoryKey
src/lib/taskClipboard.ts             本进程复制 JSON
src/components/ui/context-menu.tsx    ContextMenuSub
src/components/TaskDateDialog.tsx      改期面板
src/components/TaskActionMenu.tsx    三处共用右键
src/pages/Tasks.tsx                   对齐、拖排序、拉边、竖条
src/pages/Today.tsx                   右键/拉边/复制
src/pages/Settings.tsx                 任务通知开关
src/lib/preview/*                      夹具新字段
AGENTS.md / CLAUDE.md / .cursor/rules  判定与通知
```

---

### Task 1: 重复平移、拉边、清除时段（core）

**Files:**
- Modify: `crates/gamelife-core/src/task.rs`
- Modify: `crates/gamelife-core/src/lib.rs`（导出 `RepeatRule`、`ResizeEdge`、`ALLOWED_REMIND_OFFSETS`）
- Test: 同文件 `#[cfg(test)]`

**Interfaces:**
- Consumes: 现有 `Task { id, list_id, title, done, start, end, range }`、`align_range`
- Produces:
  - `RepeatRule { None, Daily, Weekly, Monthly }` serde `snake_case`：`none` / `daily` / `weekly` / `monthly`
  - `Task.sort: i64`，`Task.repeat: RepeatRule`，`Task.remind_offsets: Vec<i64>`（serde default）
  - `pub const ALLOWED_REMIND_OFFSETS: [i64; 5] = [0, 5, 15, 30, 60];`
  - `pub fn remind_offsets_ok(offsets: &[i64]) -> bool`
  - `pub fn next_occurrence(start: i64, end: i64, repeat: RepeatRule, today_start: i64) -> Option<(i64, i64)>`
  - `pub fn spawn_after_complete(done: &Task, new_id: String, today_start: i64, sort: i64) -> Option<Task>`
  - `pub enum ResizeEdge { Start, End }`
  - `pub fn resize_range(start: i64, end: i64, edge: ResizeEdge, at: i64) -> (i64, i64)`
  - `pub fn clear_schedule(task: &mut Task)` 置空 start/end，`repeat = None`，`remind_offsets.clear()`

- [ ] **Step 1: Write the failing tests**

在 `task.rs` 的 tests 里（用现有 `timed()` helper，补上 `sort: 0, repeat: RepeatRule::None, remind_offsets: vec![]`）追加：

```rust
#[test]
fn next_daily_skips_until_today() {
    let start = 0;
    let end = 3600;
    let today = 3 * 86400;
    let (s, e) = next_occurrence(start, end, RepeatRule::Daily, today).unwrap();
    assert_eq!(s, today);
    assert_eq!(e, today + 3600);
}

#[test]
fn next_monthly_clamps_jan31() {
    let jan31 = chrono::Local
        .with_ymd_and_hms(2026, 1, 31, 10, 0, 0)
        .single()
        .unwrap()
        .timestamp();
    let end = jan31 + 3600;
    let today = chrono::Local
        .with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
        .single()
        .unwrap()
        .timestamp();
    let (s, _) = next_occurrence(jan31, end, RepeatRule::Monthly, today).unwrap();
    let dt = chrono::Local.timestamp_opt(s, 0).single().unwrap();
    assert_eq!(dt.month(), 2);
    assert!(dt.day() == 28 || dt.day() == 29);
}

#[test]
fn spawn_none_repeat_is_none() {
    let t = timed("a", PRESET_MAINLINE_ID, 0, 1800);
    assert!(spawn_after_complete(&t, "b".into(), 0, 1).is_none());
}

#[test]
fn clear_schedule_drops_repeat() {
    let mut t = timed("a", PRESET_MAINLINE_ID, 0, 1800);
    t.repeat = RepeatRule::Weekly;
    t.remind_offsets = vec![0, 15];
    clear_schedule(&mut t);
    assert_eq!(t.start, None);
    assert_eq!(t.repeat, RepeatRule::None);
    assert!(t.remind_offsets.is_empty());
}

#[test]
fn resize_start_keeps_end_min_900() {
    let (s, e) = resize_range(1800, 3600, ResizeEdge::Start, 3000);
    assert_eq!((s, e), (2700, 3600));
    let (s, e) = resize_range(0, 1800, ResizeEdge::End, 100);
    assert_eq!(e - s, 900);
}

#[test]
fn remind_offsets_reject_unknown() {
    assert!(remind_offsets_ok(&[0, 15, 60]));
    assert!(!remind_offsets_ok(&[7]));
    assert!(!remind_offsets_ok(&[0, 0]));
}
```

`timed()` / 其它 `Task { ... }` 字面量一并补三个新字段，否则先编不过。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife-core next_daily_skips_until_today -- --exact`

Expected: FAIL（函数未定义或 Task 缺字段）

- [ ] **Step 3: Write minimal implementation**

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatRule {
    #[default]
    None,
    Daily,
    Weekly,
    Monthly,
}

pub const ALLOWED_REMIND_OFFSETS: [i64; 5] = [0, 5, 15, 30, 60];

pub fn remind_offsets_ok(offsets: &[i64]) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    for &n in offsets {
        if !ALLOWED_REMIND_OFFSETS.contains(&n) || !seen.insert(n) {
            return false;
        }
    }
    true
}
```

`next_occurrence`：`None` 返回 `None`；否则用 `chrono::Local` 把 `start` 转成本地时间，Daily `+Duration::days(1)`，Weekly `+days(7)`，Monthly `checked_add_months(Months::new(1))`，失败则该月最后一天同一钟点；循环直到 `next_start >= today_start`；`end = next_start + (end-start)`。

`resize_range`：`at` 先 `/900*900`；Start 时 `start = min(at, end-900)`；End 时 `end = max(at, start+900)`；再 `align_range`。

`spawn_after_complete`：`done.done` 必须为将完成的那条（调用方已视为完成）；要求 `repeat != None` 且 start/end 齐全；新 Task `done=false`，标题/list/repeat/remind 照抄，时段用 `next_occurrence`。

- [ ] **Step 4: Run tests to verify they pass**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife-core task::`

Expected: PASS（含旧测试；`Task` 字面量已补字段）

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: add task repeat spawn and range resize

EOF
)"
```

---

### Task 2: 未完成全进快照 + 角色匹配（core）

**Files:**
- Modify: `crates/gamelife-core/src/task.rs`
- Modify: `crates/gamelife-core/src/lib.rs`
- Test: `task.rs` tests

**Interfaces:**
- Consumes: Task 1 的 `Task` 新字段、`tokenize_title`、`MAX_JUDGMENT_TASKS`
- Produces:
  - `pub enum MatchRole { None, Mixed, Role(ListRole) }`
  - `pub fn match_task_role(app: &str, title: &str, url: Option<&str>, document_path: Option<&str>, snapshots: &[TaskSnapshot]) -> MatchRole`
  - `pub fn snapshots_open(tasks: &[Task], lists: &[TaskList]) -> Vec<TaskSnapshot>`（`!done` 且能解析到列表）
  - `pub fn select_prompt_snapshots(all: &[TaskSnapshot], haystacks: &[&str]) -> Vec<TaskSnapshot>` 最多 20：先 token 命中，再 mainline，再按现有顺序
  - `snapshots_for_day` 改为调用 `snapshots_open`（**忽略** `day_start`/`day_end`），不再 `TooManyJudgment`
  - `judgment_tasks` / `in_judgment_set`：未完成即入选，不看时段；**超过 20 不再 Err**，返回全集（或改调用点只走 `snapshots_open`）
  - `snapshot_evidence_quests` 仍可留着，但匹配不再只切主线

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn snapshots_open_includes_unscheduled_and_other_days() {
    let lists = lists();
    let open = Task {
        id: "u".into(),
        list_id: PRESET_MAINLINE_ID.into(),
        title: "inbox".into(),
        done: false,
        start: None,
        end: None,
        range: None,
        sort: 0,
        repeat: RepeatRule::None,
        remind_offsets: vec![],
    };
    let snaps = snapshots_open(&[open], &lists);
    assert_eq!(snaps.len(), 1);
}

#[test]
fn match_same_role_two_titles() {
    let snaps = vec![
        TaskSnapshot { id: "a".into(), title: "写论文方法节".into(), role: ListRole::Mainline },
        TaskSnapshot { id: "b".into(), title: "写论文讨论".into(), role: ListRole::Mainline },
    ];
    assert_eq!(
        match_task_role("Overleaf", "方法节.tex", None, None, &snaps),
        MatchRole::Role(ListRole::Mainline)
    );
}

#[test]
fn match_mixed_roles_is_mixed() {
    let snaps = vec![
        TaskSnapshot { id: "a".into(), title: "报销单".into(), role: ListRole::Chore },
        TaskSnapshot { id: "b".into(), title: "论文".into(), role: ListRole::Mainline },
    ];
    assert_eq!(
        match_task_role("Preview", "论文 报销单", None, None, &snaps),
        MatchRole::Mixed
    );
}

#[test]
fn prompt_snapshots_cap_prefers_hits() {
    let mut all = Vec::new();
    for i in 0..25 {
        all.push(TaskSnapshot {
            id: format!("{i}"),
            title: format!("任务{i}"),
            role: ListRole::Side,
        });
    }
    all[24].title = "HDP train".into();
    all[24].role = ListRole::Mainline;
    let picked = select_prompt_snapshots(&all, &["HDP train.py"]);
    assert_eq!(picked.len(), 20);
    assert!(picked.iter().any(|s| s.id == "24"));
}
```

改掉现有 `more_than_twenty_timed_tasks_yields_empty_set`：超过 20 不再空，改为 `snapshots_for_day` 仍返回（长度可以 >20）。`snapshots_for_day_caps_at_twenty_instead_of_empty` 改为描述 **prompt** 截断，或删掉、改测 `select_prompt_snapshots`。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife-core snapshots_open_includes_unscheduled_and_other_days -- --exact`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

`match_task_role`：对每个 snapshot 的 `tokenize_title`，在 app/title/url/path 里大小写不敏感 `contains`；收集命中的 `role` 去重。0 个 `None`，1 种角色 `Role`，多种 `Mixed`。空 token 的标题不命中。

`select_prompt_snapshots`：先 hit 列表，再 `role==Mainline` 未入选者，再其余，去重保序，`truncate(20)`。

- [ ] **Step 4: Run tests**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife-core task::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: match unfinished tasks without a time window

EOF
)"
```

---

### Task 3: hint / judge 按任务匹配

**Files:**
- Modify: `crates/gamelife-core/src/hint.rs`
- Modify: `crates/gamelife-core/src/judge.rs`
- Modify: `src-tauri/src/text_ai.rs`（prompt 先 `select_prompt_snapshots`）
- Test: 各文件原 tests；`src-tauri` 的 `text_ai::tests`

**Interfaces:**
- Consumes: `match_task_role`、`select_prompt_snapshots`、`TaskSnapshot`
- Produces: `hint_sample(sample, policy, snapshots: &[TaskSnapshot]) -> Hint`
- `analyze_slot_evidence` 把 `input.tasks` 传给 `hint_sample`，**不要**再 `merged.extend(snapshot_evidence_quests)` 来制造路径 Core
- `judge_slot` 自动 Core 条件改为 `strong_core >= STRONG_CORE_AUTO_SECS && trio <= SIDE_DISTRACTION_MAX_FOR_AUTO_CORE`（`strong_core` 只来自匹配主线的 `CoreCandidate` 且 idle < 180）。删除对 `grounded_strong_core` 的自动 Core 分支。

`hint_sample` 顺序必须是：

1. lock/pause → Away
2. distraction_rules → Distraction
3. idle ≥ 180 → Away
4. 内置 GameLife（`matches_app_identity` 或 haystack 规则 `"GameLife"`，**不要**用使用者 `side_project_rules` / `admin_apps`）→ Side
5. `match_task_role(...)`：`Role(Mainline)` → CoreCandidate；`Side | Longterm | Custom` → Side；`Chore` → Admin；`Mixed | None` → Unsure
6. Unsure

不要调用 `is_core_candidate`。

- [ ] **Step 1: Write / rewrite failing tests**

新增：

```rust
#[test]
fn unmatched_work_path_is_unsure() {
    let mut s = sample("Overleaf", "main.tex", 5);
    s.document_path = Some("/Users/me/paper/main.tex".into());
    assert_eq!(hint_sample(&s, &default_policy(), &[]), Hint::Unsure);
}

#[test]
fn matched_mainline_title_is_core_candidate() {
    let snaps = [TaskSnapshot {
        id: "1".into(),
        title: "写方法节".into(),
        role: ListRole::Mainline,
    }];
    let h = hint_sample(&sample("Cursor", "方法节.md", 5), &default_policy(), &snaps);
    assert_eq!(h, Hint::CoreCandidate);
}

#[test]
fn youtube_beats_matching_mainline_title() {
    let snaps = [TaskSnapshot {
        id: "1".into(),
        title: "YouTube 教程".into(),
        role: ListRole::Mainline,
    }];
    let h = hint_sample(&sample("Google Chrome", "YouTube", 5), &default_policy(), &snaps);
    assert_eq!(h, Hint::Distraction);
}
```

`sample` / `default_policy` 用本文件现有 helper。把所有 `hint_sample(..., &quests)` 改成第三参 snapshots。原先「无 quest 工作路径是 CoreCandidate」的测试改为 Unsure。`admin_app_beats_*`：无匹配任务时邮箱窗口改为 Unsure，不再自动 Admin。

`judge.rs`：`empty_quests_work_path_still_auto_cores` / `empty_snapshots_still_auto_core_when_grounded` / `overleaf_thirteen_minutes_auto_cores_without_keyword` 改为 **不**自动 Core（灰区 pending 或非 CoreResearch）。新增：带主线 snapshot 且标题命中、活跃 ≥ 13 分钟 → 仍自动 Core。

`text_ai.rs` `build_text_ai_prompt`：若 `snapshots.len() > 20`，先 `select_prompt_snapshots`。加测试：25 条时 prompt 里任务行 ≤ 20。

- [ ] **Step 2: Run tests to verify they fail**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife-core unmatched_work_path_is_unsure -- --exact`

Expected: FAIL（仍是 CoreCandidate 或签名不对）

- [ ] **Step 3: Implement hint + judge + prompt cap**

所有 `hint_sample` 调用点（含 `analyze_slot_evidence`）改签名。`judge.rs` 注释里删掉 “empty snapshots still auto-core when grounded”。

- [ ] **Step 4: Run tests**

Run:

```
CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife-core
CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife text_ai::
```

Expected: PASS。若 `gamelife` 因 `hint_sample` 签名在 scheduler 编译失败，把 scheduler 里直接调用一并改（通常只走 `judge_slot`）。

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/hint.rs crates/gamelife-core/src/judge.rs src-tauri/src/text_ai.rs
git commit -m "$(cat <<'EOF'
feat: judge slots by unfinished task match

EOF
)"
```

---

### Task 4: SQLite `user_version` 4

**Files:**
- Modify: `src-tauri/src/db.rs`（`TARGET_USER_VERSION`、`SCHEMA` 的 `CREATE TABLE tasks`、`load_tasks`、`add_column_if_missing`）
- Modify: `src-tauri/src/commands.rs` 的 `persist_task` / `task_to_view` / `view_to_task` 先能读写新列（命令行为下一任务）
- Test: `db.rs` 里 `migrate_new_db_sets_user_version_3` 改为 4；新增旧库加列测试

**Interfaces:**
- Consumes: core `Task` 新字段、`RepeatRule`、`remind_offsets_ok`
- Produces: 三列 `sort INTEGER NOT NULL DEFAULT 0`、`repeat TEXT NOT NULL DEFAULT 'none'`、`remind_json TEXT NOT NULL DEFAULT '[]'`
- `load_tasks`：`SELECT id, list_id, title, done, start, end, range, sort, repeat, remind_json FROM tasks ORDER BY list_id, sort, id`
- `repeat` 非法当 `none`；`remind_json` 解析失败或 `remind_offsets_ok` 失败当 `[]`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn migrate_adds_task_sort_repeat_remind_and_sets_version_4() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE tasks (
           id TEXT PRIMARY KEY, list_id TEXT, title TEXT, done INTEGER,
           start INTEGER, end INTEGER, range TEXT
         );
         INSERT INTO tasks (id, list_id, title, done, start, end, range)
         VALUES ('a','list-mainline','x',0,NULL,NULL,NULL);
         PRAGMA user_version = 3;",
    )
    .unwrap();
    crate::db::migrate(&conn).unwrap();
    assert_eq!(user_version(&conn), 4);
    let names = column_names(&conn, "tasks");
    assert!(names.iter().any(|c| c == "sort"));
    assert!(names.iter().any(|c| c == "repeat"));
    assert!(names.iter().any(|c| c == "remind_json"));
}
```

把 `migrate_new_db_sets_user_version_3` 的断言改为 4。`user_version stays 3` 注释改掉。

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife migrate_adds_task_sort_repeat_remind_and_sets_version_4 -- --exact`

Expected: FAIL

- [ ] **Step 3: Implement migrate + persist**

`if version < 4` 里 `add_column_if_missing` 三列，然后 `TARGET_USER_VERSION = 4`。新库 `CREATE TABLE tasks` 也写上三列（`IF NOT EXISTS` 不会改建表，加列路径仍必要）。

`persist_task` INSERT/UPDATE 包含 `sort, repeat, remind_json`（`serde_json::to_string(&task.remind_offsets)`）。

`commands.rs` 里其它 `Task {` 字面量（测试夹具）补字段，否则编不过。

删掉 `reject_too_many_judgment` 的调用（函数可删）。`upsert_task` 不再因 20 条失败。

`pin_task_snapshot_json` 改 `snapshots_open`（可忽略 day 边界；保留 day 参数以免改 `ensure_slot` 签名，或去掉未用变量前加 `let _ = (day_start, day_end)`）。

- [ ] **Step 4: Run tests**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db.rs src-tauri/src/commands.rs src-tauri/src/scheduler.rs
git commit -m "$(cat <<'EOF'
feat: migrate tasks sort repeat and remind columns

EOF
)"
```

---

### Task 5: 命令 reorder / duplicate / 完成续写

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/preview/invoke.ts`、`src/lib/preview/fixtures.ts`
- Test: `commands.rs` 现有 task 测试模块（或 `db.rs` 旁的 command 测试）；前端 `src/lib/taskBoard.test.ts` 只加类型不测 IPC

**Interfaces:**
- Consumes: `spawn_after_complete`、`clear_schedule`、`remind_offsets_ok`、`next_list_sort`
- Produces:
  - `TaskView` 增加 `sort: i64`、`repeat: String`、`remindOffsets: Vec<i64>`
  - `reorder_task(id: String, list_id: String, sort: i64)`
  - `duplicate_task(id: String) -> TaskView`（新 UUID，`done=false`，`sort = max+1`）
  - `toggle_task_done`：`done=true` 且可 spawn 时同一事务 `persist_task` 下一次；`new_id = uuid`
  - `reschedule_task`：两个 `None` 则 `clear_schedule`；否则 `align_range`；未排期但 `repeat != none` 拒绝 `"repeat_needs_schedule"`
  - `upsert_task`：校验 remind；缺 sort 则接到该组末尾；默认 repeat none

Unix 时间 `now_secs` 已有。新 id：`format!("{}", uuid)` 若项目无 uuid crate，用 `format!("task-{}", now_secs)` 加 `getrandom` 16 字节 hex（`commands.rs` 其它 id 已用 `now_secs` / UUID：前端 `crypto.randomUUID()`，shell 用 `getrandom`）。

```rust
fn new_task_id() -> String {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).expect("rng");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}
```

- [ ] **Step 1: Write the failing tests**（在 `commands.rs` 的 `#[cfg(test)]` 用 tempfile DB，与现有 task 测试同样 `open`+`migrate`）

```rust
#[test]
fn completing_daily_inserts_one_future() {
    // insert scheduled daily task starting yesterday
    // toggle_task_done(id, true)
    // load_tasks: old done=true, new done=false, start >= today 0:00
}

#[test]
fn upsert_more_than_twenty_open_tasks_ok() {
    // 21 unfinished unscheduled mainline upserts succeed
}
```

若 `commands` 测试难启动，把续写测在 `db` 集成测试里直接调 `toggle_task_done`。

- [ ] **Step 2: Run to verify fail**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife completing_daily_inserts_one_future -- --exact`

Expected: FAIL

- [ ] **Step 3: Implement commands + api.ts + preview**

`src/lib/api.ts`：

```ts
export interface TaskView {
  id: string;
  listId: string;
  title: string;
  done: boolean;
  start: number | null;
  end: number | null;
  range: string | null;
  sort: number;
  repeat: string;
  remindOffsets: number[];
}

export function reorderTask(id: string, listId: string, sort: number): Promise<void> {
  return invoke("reorder_task", { id, listId, sort });
}
export function duplicateTask(id: string): Promise<TaskView> {
  return invoke("duplicate_task", { id });
}
```

preview `invoke.ts` 同步实现。夹具任务补 `sort/repeat/remindOffsets`。

`toggle_task_done` 成功后调用 `crate::task_notify::sync_from_db` 若 Task 6 尚未存在则先留 `// Task 6 hooks here` **禁止**——改为 Task 6 再接。本任务不提通知。

- [ ] **Step 4: Run tests**

```
CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife
npx vitest run --dir src
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/lib/api.ts src/lib/preview/invoke.ts src/lib/preview/fixtures.ts
git commit -m "$(cat <<'EOF'
feat: add reorder duplicate and repeat-complete commands

EOF
)"
```

---

### Task 6: 任务系统通知

**Files:**
- Create: `src-tauri/src/task_notify.rs`
- Create: `src-tauri/src/macos/notify.rs`（`cfg(target_os = "macos")`）
- Modify: `src-tauri/src/macos/mod.rs` 导出
- Modify: `src-tauri/Cargo.toml` macos 依赖加 `objc2-user-notifications`（features 按 crate 文档打开 `UNUserNotificationCenter`）
- Modify: `src-tauri/src/config.rs`、`src/lib/api.ts` `AppSettings.taskNotifications: boolean` 默认 false
- Modify: `src/pages/Settings.tsx` 开关「任务通知」
- Modify: `src-tauri/src/lib.rs` `mod task_notify`
- Modify: `src-tauri/src/commands.rs` 在 toggle/upsert/reschedule/delete/reorder 成功后 `task_notify::sync_now()`
- Modify: `src-tauri/src/scheduler.rs` 每 20 tick 顺带 `sync_now`（已有 sync 节拍则挂上去）
- Test: `task_notify.rs` 纯函数 `fires_at(start, offsets, now) -> Vec<i64>`；`config` 缺省 false；Settings 无 vitest 则测 `fires_at`

**Interfaces:**
- Consumes: `Task.start`、`remind_offsets`、`config.task_notifications`
- Produces:
  - `pub fn fires_at(start: i64, offsets: &[i64], now: i64) -> Vec<i64>` 计算 `start - off*60`，丢掉 `<= now`
  - `pub fn sync_now()`：`task_notifications==false` → 取消全部 `gamelife-task-*` 请求；否则对未完成已排期任务登记未来 fire
  - identifier：`gamelife-task-{task_id}-{offset}`
  - 标题用任务 title，正文「开始前 {n} 分钟」或「准时」
  - 关开关不写 secrets；只改 `config.json`

Windows/Linux：`sync_now` 立刻 Ok，不申请权限。

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn fires_at_drops_past_and_keeps_future() {
    let start = 10_000;
    let now = 9_500;
    assert_eq!(fires_at(start, &[0, 15], now), vec![start - 15 * 60, start]);
    assert!(fires_at(start, &[0], start + 1).is_empty());
}
```

config 反序列化缺字段 → `task_notifications == false`。

- [ ] **Step 2: Run to fail**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife fires_at_drops_past_and_keeps_future -- --exact`

Expected: FAIL

- [ ] **Step 3: Implement**

macOS `request_authorization` 仅当开关从 false→true。用 UserNotifications 安排 `UNCalendarNotificationTrigger` 或 time-interval（若 fire-now < 60s 用 interval）。失败只打 log，不让命令失败。

设置页：在现有 `SwitchRow` 附近加一条，中文「任务通知」+ 说明「到点用系统横幅提醒已排期任务。默认关闭。」`persistBasic`。

若 `objc2-user-notifications` 在当前版本 features 对不上：退回 `macos/notify.rs` 里 objc2 runtime 调 `UNUserNotificationCenter`，不要 osascript。

- [ ] **Step 4: Run tests**

```
CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife
npx vitest run --dir src
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/task_notify.rs src-tauri/src/macos/notify.rs src-tauri/src/macos/mod.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/config.rs src-tauri/src/lib.rs src-tauri/src/commands.rs src-tauri/src/scheduler.rs src/lib/api.ts src/pages/Settings.tsx src/lib/preview/fixtures.ts
git commit -m "$(cat <<'EOF'
feat: schedule optional task notification banners

EOF
)"
```

---

### Task 7: 改期面板（App 风格）

**Files:**
- Create: `src/lib/taskSchedule.ts`
- Create: `src/lib/taskSchedule.test.ts`
- Create: `src/components/TaskDateDialog.tsx`
- Modify: `src/pages/Tasks.tsx` 换掉现有 date Dialog
- Test: vitest `taskSchedule.test.ts`

**Interfaces:**
- Consumes: `rescheduleTask`、`RepeatRule` 字符串、`remindOffsets`
- Produces:
  - `export const TIME_STEPS: string[]` 00:00–23:45 每 15 分钟
  - `export function defaultRange(nowSec: number): { start: number; end: number }` 现在向下取 900，+1800
  - `export function unixAt(dayIso: string, hhmm: string): number`（现有 Tasks 里若已有则**搬过来**复用，删页面副本）
  - Dialog 不使用系统灰色；`Dialog` + `Label` + `Select` + `Button` variant outline/primary

- [ ] **Step 1: Failing tests**

```ts
import { describe, expect, it } from "vitest";
import { TIME_STEPS, defaultRange, remindToggle } from "./taskSchedule";

it("has 96 quarter-hour labels", () => {
  expect(TIME_STEPS).toHaveLength(96);
  expect(TIME_STEPS[0]).toBe("00:00");
  expect(TIME_STEPS[95]).toBe("23:45");
});

it("defaultRange is thirty minutes aligned", () => {
  const { start, end } = defaultRange(100);
  expect(start).toBe(0);
  expect(end).toBe(1800);
});

it("remindToggle adds and removes allowed offsets", () => {
  expect(remindToggle([0], 15)).toEqual([0, 15]);
  expect(remindToggle([0, 15], 0)).toEqual([15]);
});
```

- [ ] **Step 2: Run to fail**

Run: `npx vitest run --dir src lib/taskSchedule`

Expected: FAIL

- [ ] **Step 3: Implement dialog**

面板字段：开始日期、开始时刻、结束日期、结束时刻、提醒多选（准时 / 5 / 15 / 30 / 60）、重复 Select（无/每天/每周/每月）、清除、确定。无全天、无时区。

确定：`align` 后 `rescheduleTask`；再 `upsertTask` 写 repeat 与 remind（若 upsert 一次能写全字段，优先一次 upsert，不要拆成会互踩的两次）。若当前 `upsert` 会覆盖 sort，带上原 `sort`。

清除：`rescheduleTask(id, null, null)`（Task 5 已清 repeat）。

未排期打开：用 `defaultRange(now)` 填表，取消不写。

修掉旧 `confirmDate` 在时刻空 + 未排期时 no-op 的路径（直接删旧 form）。

- [ ] **Step 4: Run tests**

Run: `npx vitest run --dir src`

Expected: PASS（`npm run build` 若 Tasks 类型报错一并修）

- [ ] **Step 5: Commit**

```bash
git add src/lib/taskSchedule.ts src/lib/taskSchedule.test.ts src/components/TaskDateDialog.tsx src/pages/Tasks.tsx
git commit -m "$(cat <<'EOF'
feat: add GameLife-styled task date dialog

EOF
)"
```

---

### Task 8: 共用右键、二级「移动到」、复制

**Files:**
- Modify: `src/components/ui/context-menu.tsx`
- Create: `src/components/TaskActionMenu.tsx`
- Create: `src/lib/taskClipboard.ts`
- Create: `src/lib/taskClipboard.test.ts`
- Modify: `src/pages/Tasks.tsx`
- Modify: `src/lib/preview/invoke.ts`（duplicate 已在 Task 5）
- Test: `taskClipboard.test.ts`；context-menu 不强制组件测

**Interfaces:**
- Consumes: `duplicateTask`、`moveTask` / `reorderTask`、`deleteTask`
- Produces:
  - `ContextMenuSub({ label, children })`：悬停或点击展开右侧子菜单；不引入 Radix
  - `TaskActionMenu` props：`open, x, y, task, lists, onClose, onDate, onMove, onDuplicate, onAbandon`
  - `serializeTaskCopy(task: TaskView): string` / `parseTaskCopy(raw: string): TaskView | null` 前缀 `gamelife/task-copy+json:`

- [ ] **Step 1: Failing tests**

```ts
it("roundtrips a task copy payload", () => {
  const t = { id: "a", listId: "list-mainline", title: "x", done: false, start: 0, end: 1800, range: null, sort: 0, repeat: "none", remindOffsets: [] };
  const raw = serializeTaskCopy(t);
  const back = parseTaskCopy(raw);
  expect(back?.title).toBe("x");
  expect(parseTaskCopy("not-ours")).toBeNull();
});
```

- [ ] **Step 2: Run to fail**

Run: `npx vitest run --dir src lib/taskClipboard`

Expected: FAIL

- [ ] **Step 3: Implement**

菜单顺序：更改日期…、移动到 ▸、创建副本、放弃任务。子菜单列出 **其它** 分组。

Tasks 页：`onCopy`/`onPaste` 监听 `metaKey || ctrlKey` + c/v，焦点在任务行（`data-task-id`）时生效。粘贴 `duplicateTask` 不够（duplicate 是同 id 源）；粘贴应 `upsertTask` 新 id + 剪贴板字段，或 `duplicateTask` 后再 `reschedule`/`move`。指定：复制存 JSON；粘贴调用 `upsertTask({ ...parsed, id: crypto.randomUUID(), done: false, sort: 0 })` 再 `reorderTask` 接到当前组末尾。

创建副本按钮：`duplicateTask(task.id)`。

- [ ] **Step 4: Run**

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/context-menu.tsx src/components/TaskActionMenu.tsx src/lib/taskClipboard.ts src/lib/taskClipboard.test.ts src/pages/Tasks.tsx
git commit -m "$(cat <<'EOF'
feat: share task context menu with submenu and copy

EOF
)"
```

---

### Task 9: 列表对齐、去排序、拖分组

**Files:**
- Create: `src/lib/taskReorder.ts`
- Create: `src/lib/taskReorder.test.ts`
- Modify: `src/pages/Tasks.tsx`
- Delete usage: `src/lib/taskSort.ts` 可留文件但页面不再调用；不要删测试除非确认无引用
- Test: `taskReorder.test.ts`

**Interfaces:**
- Consumes: `reorderTask`
- Produces: `export function ranksAfterDrag(ids: string[], dragId: string, beforeId: string | null): { id: string; sort: number }[]`  
  `beforeId === null` 表示拖到该组末尾。返回每条新 `sort = index * 10`。

- [ ] **Step 1: Failing tests**

```ts
it("moves an id before another", () => {
  const next = ranksAfterDrag(["a", "b", "c"], "c", "a");
  expect(next.map((x) => x.id)).toEqual(["c", "a", "b"]);
});
```

- [ ] **Step 2: Run to fail**

Run: `npx vitest run --dir src lib/taskReorder`

Expected: FAIL

- [ ] **Step 3: Implement UI**

- 页头「排序」按钮删除；`SORT_KEY` / `useState(sort)` 删除。
- 分组行：`[chevron][dot][name][count]`；任务行 `pl` 与 name 对齐，使 checkbox 与 dot 同列（chevron 宽 `size-3.5`，任务行 `padding-left` = chevron + gap）。checkbox `style={{ accentColor: roleDot(list.role) }}`。
- 组内按 `task.sort` 排序，不要 `sortTasks(time|title)`。
- 指针拖：任务行 `onPointerDown` 区分点击 checkbox（不拖）与拖动手势（移动超过 4px 才开始）。拖到另一 `data-list-id` 分组 = `reorderTask(id, newListId, sort)`。
- 与日历拖冲突：左栏拖默认改顺序；只有拖出左栏进入日历网格才走现有 `beginCalDrag`。用 `pointer capture` + 命中测试 `gridRef`。

- [ ] **Step 4: Run**

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/taskReorder.ts src/lib/taskReorder.test.ts src/pages/Tasks.tsx
git commit -m "$(cat <<'EOF'
feat: drag-reorder task lists and align checkboxes

EOF
)"
```

---

### Task 10: 日历拉边 + 活动竖条

**Files:**
- Modify: `src/lib/taskCalendar.ts`
- Modify: `src/lib/taskCalendar.test.ts`
- Create: `src/lib/slotRibbon.ts`
- Create: `src/lib/slotRibbon.test.ts`
- Modify: `src/pages/Tasks.tsx`（`TaskCalendar`、拉边、每列右缘）
- Modify: `src/lib/api.ts` 已有 `getDayView` 则复用；否则加 `getDayView(day: string)`
- Test: 上述 vitest

**Interfaces:**
- Consumes: `resize_range` 语义、`getDayView`、`categoryOf` / `categoryColor`
- Produces:
  - `export type CalEdge = "start" | "end"`
  - `export function resizeRange(start: number, end: number, edge: CalEdge, dropTs: number): { start: number; end: number }`（与 core 相同：15 分钟、最短 900）
  - `export function dominantToCategory(dominant: string, pending: boolean): CategoryKey`
  - 竖条 96 格，`h` 与小时行对齐（`CAL_HOUR_H`）

- [ ] **Step 1: Failing tests**

```ts
it("resize start does not pass the end", () => {
  const r = resizeRange(0, 3600, "start", 4000);
  expect(r.end - r.start).toBeGreaterThanOrEqual(900);
  expect(r.end).toBe(3600);
});

it("pending maps to pending category", () => {
  expect(dominantToCategory("core_research", true)).toBe("pending");
  expect(dominantToCategory("distraction", false)).toBe("entertainment");
});
```

`categoryOf` 已有 role 映射；dominant 字符串用今日槽同一套（读 `Today.tsx` / `theme.ts` 的 `categoryOf`）。**不要**在 `slotRibbon` 写 hex。

- [ ] **Step 2: Run to fail**

Run: `npx vitest run --dir src lib/taskCalendar lib/slotRibbon`

Expected: FAIL

- [ ] **Step 3: Implement**

日历块：上/下 6px 命中 `CalEdge`，`cursor-ns-resize`；中间仍 move。`pointerup` 调 `rescheduleTask`。

每列右侧 4px 轨：`getDayView` 对 `days` 并行请求（3 或 7 次），按 `slot.start` 填 96 格。无数据透明。`pointer-events-none`。

右键日历块：复用 `TaskActionMenu`。

- [ ] **Step 4: Run**

```
npx vitest run --dir src
CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife-core resize_start
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/taskCalendar.ts src/lib/taskCalendar.test.ts src/lib/slotRibbon.ts src/lib/slotRibbon.test.ts src/pages/Tasks.tsx src/lib/api.ts
git commit -m "$(cat <<'EOF'
feat: resize calendar blocks and draw slot ribbons

EOF
)"
```

---

### Task 11: 今日计划列同权

**Files:**
- Modify: `src/pages/Today.tsx`
- Modify: `src/lib/planDrag.ts`（若拉边不适合 `moveSameDay`，在 `planDrag.ts` 加 `resizeSameDay` 或复用 `resizeRange` 再 clamp 到当天）
- Modify: `src/lib/planDrag.test.ts`
- Test: `planDrag.test.ts`

**Interfaces:**
- Consumes: `TaskActionMenu`、`TaskDateDialog`、`resizeRange`、`taskClipboard`
- Produces: 今日计划块：右键同一菜单；上下边拉时长；⌘C/V；整块拖仍只限当天（现有）

- [ ] **Step 1: Failing test**

```ts
it("resizeSameDay clamps into the local day", () => {
  const day = 1_000_000;
  const r = resizeSameDay(day, day + 1800, "end", day + 86400, day);
  expect(r.end).toBeLessThanOrEqual(day + 96 * 900);
});
```

- [ ] **Step 2: Run to fail**

Run: `npx vitest run --dir src lib/planDrag`

Expected: FAIL

- [ ] **Step 3: Implement**

计划块 `onContextMenu`；拉边命中与日历相同 6px。换日只通过改期面板（菜单「更改日期」打开 `TaskDateDialog`）。`onMovePlan` 保持 `rescheduleTask` + `refresh`。错误 `taskCommandError`。

不要在今日加自然语言输入。

- [ ] **Step 4: Run**

Run: `npx vitest run --dir src`

Expected: PASS。再 `npm run build` 保证 tsc。

- [ ] **Step 5: Commit**

```bash
git add src/pages/Today.tsx src/lib/planDrag.ts src/lib/planDrag.test.ts src/components/TaskActionMenu.tsx src/components/TaskDateDialog.tsx
git commit -m "$(cat <<'EOF'
feat: match Today plan column to the Tasks calendar

EOF
)"
```

---

### Task 12: 文档与规则

**Files:**
- Modify: `AGENTS.md`、`CLAUDE.md`
- Modify: `.cursor/rules/specs.mdc`、`core-layer.mdc`、`tauri-shell.mdc`、`frontend-ui.mdc`
- Modify: `docs/superpowers/specs/2026-09-15-tasks-interaction-design.md` 状态改为 `已确认`
- 本 plan 文件保持在 `docs/superpowers/plans/2026-09-15-tasks-interaction.md`

**Interfaces:**
- Consumes: 已实现行为
- Produces: 文档与 spec overlay 一致

- [x] **Step 1: 改文档**

写明：判定匹配未完成任务、不看时段；无匹配走 AI 不自动 Core；娱乐硬规则仍优先；重复=完成后续写；`user_version` 4；设置「任务通知」；禁止 FullCalendar/dnd-kit。`specs.mdc` 列表追加本文，并写清覆盖 2026-09-15 本地任务里的判定/排序/20 条门槛。

- [x] **Step 2: 扫一遍 CLAUDE 判定流水线段落，删掉「无任务仍自动 Core / 当天已排期 20 条拒绝」若已过时**

- [x] **Step 3: Commit**

```bash
git add AGENTS.md CLAUDE.md .cursor/rules docs/superpowers/specs/2026-09-15-tasks-interaction-design.md docs/superpowers/plans/2026-09-15-tasks-interaction.md
git commit -m "$(cat <<'EOF'
docs: describe match-based judgment and task board UX

EOF
)"
```

---

## Self-review

1. Spec coverage：§5–12 均有任务；「取消完成不收回下一次」在 Task 5 的 `toggle_task_done(false)` 只 UPDATE 该行，不要 DELETE spawn 出的行（在 Task 5 实现注释里写死）。
2. 无 TBD / “similar to Task N” 实现步骤。
3. 类型名：`RepeatRule`、`remindOffsets` camelCase 在 TS、`remind_json` 在 SQL、`remind_offsets` 在 core，Task 4 persist 负责转换。
