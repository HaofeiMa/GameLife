# Local Tasks Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用本机「任务」页（分组、自然语言、自绘日历）完全取代 TickTick，作为计划来源和槽快照。

**Architecture:** 列表校验、解析、改期算术、判定集合仍在 `gamelife-core`。SQLite 继续用 `task_lists` / `tasks`（`user_version` 仍为 3）。槽开始钉本地任务快照，不再读 `ticktick_cache`。CRUD 与 `get_day_view` 在 `src-tauri`。前端第五轨 `Tasks.tsx`；今日只读 `dayTasks`。云备份副本保留任务表。删除 TickTick 命令与设置页。

**Tech Stack:** Tauri 2、Rust、React、TypeScript、vitest、SQLite、chrono。不引入 FullCalendar、dnd-kit、Radix。

**Spec:** `docs/superpowers/specs/2026-09-15-local-tasks-design.md`

## Global Constraints

- 15 秒采样；缺口不外推；进程死亡是未观测，不是离开。`credited ≤ observed ≤ 实际槽长`。
- 一天最多 96 个 15 分钟槽。周末不采样。周末可以排期。
- 观测优先于计划。勾选、放弃、改期都不发币。
- 已 `final` 的槽不改经济。`task_snapshot_json` 用 `COALESCE` 钉死。
- 当天判定集合最多 20 条。写入拒绝超限；钉快照若已脏则按 `start` 取前 20，不钉 `[]`。
- 无系统通知、无音效。不引入 FullCalendar、dnd-kit、Radix、子任务、重复规则、全天条。
- 界面中文。能量称「能量」。颜色只来自 `--cat-*` / `theme.ts`。
- 页面只经 `src/lib/api.ts` 调 `invoke()`。
- `user_version` 保持 3。不删 `ticktick_cache` 表。
- 改 `src-tauri/`：`CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife` 必须跑过。
- 改 `gamelife-core`：同样 `CARGO_TARGET_DIR` 跑 `-p gamelife-core`。
- 改 `src/`：`npx vitest run --dir src`。
- 测试禁止新增 `std::env::set_var("HOME", …)`。
- 每个任务一条英文 conventional commit，不 `--no-verify`。

---

## Spec coverage（执行前对照，禁止跳过）

| Spec 条款 | 任务 |
| --- | --- |
| §5.1 多主线、至少一条、预置不可删、长期规划改名 | Task 1, 3 |
| §5.2 未排期 / done / 放弃=删除 / 一条时段 | Task 1, 3 |
| §6 `parse_task_line`、`#长期规划`、无时段 inbox | Task 2 |
| §9 钉本地快照、停 TickTick 刷新、超 20 取前 20 | Task 4 |
| §12 命令、`dayTasks` | Task 3, 5 |
| §10 备份含 tasks | Task 6 |
| §11 删除 TickTick | Task 7 |
| §7 任务页外壳、页头、左栏 | Task 8, 9 |
| §7.3 日历 3/7 天与拖动 | Task 10 |
| §8 今日 `dayTasks`、去掉全天条、拖钟点 | Task 5, 11 |
| §13 不做清单 | 各任务不得引入 |
| 文档 AGENTS / CLAUDE / rules | Task 12 |

---

## File Structure

```
crates/gamelife-core/src/task.rs          多主线、删除校验、改期算术、preset 名
crates/gamelife-core/src/task_parse.rs    #长期规划
crates/gamelife-core/src/lib.rs           导出
src-tauri/src/db.rs                     长期规划改名；create_list 角色
src-tauri/src/scheduler.rs                钉本地快照；删 maybe_refresh_ticktick_cache
src-tauri/src/commands.rs               CRUD；DayView.day_tasks
src-tauri/src/lib.rs                    注册/注销命令
src-tauri/src/sync.rs                   NEVER_SYNCED 去掉 tasks
src-tauri/src/config.rs                 ticktick 字段 skip_serializing
src-tauri/src/ticktick.rs               整文件删除
src-tauri/src/keychain.rs               删除 TickTick 读写
src/lib/api.ts                          dayTasks、listTaskBoard、CRUD
src/lib/taskBoard.ts                    角色/时间文案（从 ticktickBoard 迁）
src/lib/taskCalendar.ts                3/7 天日期、拖入 30 分钟
src/lib/taskSort.ts                     组内排序
src/hooks/useTaskSplit.ts               左右栏宽度
src/pages/Tasks.tsx                     新页
src/pages/Today.tsx                    dayTasks
src/pages/Settings.tsx                  去掉 TickTick 标签；备份说明
src/lib/rail.ts                         五页
src/App.tsx                             渲染 Tasks
src/lib/preview/*                       夹具
docs + AGENTS.md / CLAUDE.md / rules
```

---

### Task 1: 列表规则——多主线、删除校验、改期算术

**Files:**
- Modify: `crates/gamelife-core/src/task.rs`
- Modify: `crates/gamelife-core/src/lib.rs`

**Interfaces:**
- Consumes: 现有 `TaskList` / `Task` / `TaskListError` / `preset_lists` / `align_range`
- Produces:

```rust
pub const UNSCHEDULED_DROP_SECS: i64 = 1800; // 30 minutes
pub const PRESET_LIST_IDS: [&str; 4] =
    [PRESET_MAINLINE_ID, PRESET_SIDE_ID, PRESET_LONGTERM_ID, PRESET_CHORE_ID];

impl TaskListError {
    // 删除 DuplicateMainline
    // 新增: PresetLocked, NotEmpty, MissingList
}

pub fn is_preset_list_id(id: &str) -> bool;
pub fn parse_list_role_strict(role: &str) -> Option<ListRole>; // 不含 custom
pub fn can_delete_list(lists: &[TaskList], tasks: &[Task], id: &str) -> Result<(), TaskListError>;
pub fn schedule_from_drop(ts: i64) -> (i64, i64); // align_range(ts, ts + 1800)
pub fn move_range_to_day(start: i64, end: i64, old_day_start: i64, new_day_start: i64) -> (i64, i64);
pub fn snapshots_for_day(tasks: &[Task], lists: &[TaskList], day_start: i64, day_end: i64) -> Vec<TaskSnapshot>;
```

`preset_lists()` 里长期组 `name` 改为 `"长期规划"`。`validate_lists`：0 条主线 → `NoMainline`；≥1 条 → `Ok`。`snapshots_for_day`：先 `judgment_tasks`；若 `TooManyJudgment`，把 `in_judgment_set` 为真的任务按 `start` 升序取 20 条再 `snapshot_of`。其它 `validate_lists` 错误返回空 Vec（调用方不应在非法列表上钉快照）。

- [ ] **Step 1: 把失败测试写进 `task.rs` 的 `tests` 模块**

把 `preset_has_single_mainline` 改成下面两组，并追加其余测试：

```rust
#[test]
fn validate_lists_allows_multiple_mainline() {
    let mut lists = preset_lists();
    lists.push(TaskList {
        id: "list-lab".into(),
        name: "实验".into(),
        sort: 4,
        role: ListRole::Mainline,
    });
    validate_lists(&lists).unwrap();
}

#[test]
fn validate_lists_rejects_zero_mainline() {
    let lists: Vec<TaskList> = preset_lists()
        .into_iter()
        .filter(|l| l.role != ListRole::Mainline)
        .collect();
    assert_eq!(validate_lists(&lists), Err(TaskListError::NoMainline));
}

#[test]
fn preset_longterm_is_named_planning() {
    let long = preset_lists()
        .into_iter()
        .find(|l| l.id == PRESET_LONGTERM_ID)
        .unwrap();
    assert_eq!(long.name, "长期规划");
}

#[test]
fn cannot_delete_preset_or_last_mainline_or_nonempty() {
    let lists = preset_lists();
    let tasks = vec![timed("a", PRESET_MAINLINE_ID, 1000, 1900)];
    assert_eq!(
        can_delete_list(&lists, &tasks, PRESET_MAINLINE_ID),
        Err(TaskListError::PresetLocked)
    );
    assert_eq!(
        can_delete_list(&lists, &[], PRESET_SIDE_ID),
        Err(TaskListError::PresetLocked)
    );
    let mut extra = lists.clone();
    extra.push(TaskList {
        id: "list-lab".into(),
        name: "实验".into(),
        sort: 4,
        role: ListRole::Mainline,
    });
    let occupied = vec![timed("a", "list-lab", 1000, 1900)];
    assert_eq!(
        can_delete_list(&extra, &occupied, "list-lab"),
        Err(TaskListError::NotEmpty)
    );
    can_delete_list(&extra, &[], "list-lab").unwrap();
}

#[test]
fn drop_unscheduled_is_thirty_minutes_aligned() {
    assert_eq!(schedule_from_drop(100), (0, 1800));
}

#[test]
fn move_range_keeps_duration_on_new_day() {
    let old = 1_778_083_200; // some local day start used in other tests
    let start = old + 10 * 3600;
    let end = start + 3600;
    let new_day = old + 86400;
    let (s, e) = move_range_to_day(start, end, old, new_day);
    assert_eq!(e - s, 3600);
    assert_eq!(s - new_day, start - old);
}

#[test]
fn snapshots_for_day_caps_at_twenty_instead_of_empty() {
    let lists = preset_lists();
    let many: Vec<Task> = (0..21)
        .map(|i| timed(&format!("{i}"), PRESET_MAINLINE_ID, 1000 + i, 1900))
        .collect();
    let snaps = snapshots_for_day(&many, &lists, 0, 86400);
    assert_eq!(snaps.len(), 20);
}
```

`timed` 辅助函数已在同模块。`move_range_to_day` 的 `old` 若现有测试日戳更方便，用 `longterm_without_window_is_not_judged` 里的 `1_778_083_200`。

- [ ] **Step 2: 跑测试，确认失败**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife-core validate_lists_allows_multiple_mainline -- --nocapture`

Expected: FAIL（`DuplicateMainline` 或函数不存在）

- [ ] **Step 3: 最小实现**

```rust
pub fn validate_lists(lists: &[TaskList]) -> Result<(), TaskListError> {
    if lists.iter().any(|l| l.name.trim().is_empty()) {
        return Err(TaskListError::EmptyName);
    }
    if !lists.iter().any(|l| l.role == ListRole::Mainline) {
        return Err(TaskListError::NoMainline);
    }
    Ok(())
}

pub fn is_preset_list_id(id: &str) -> bool {
    PRESET_LIST_IDS.contains(&id)
}

pub fn parse_list_role_strict(role: &str) -> Option<ListRole> {
    match role {
        "mainline" => Some(ListRole::Mainline),
        "side" => Some(ListRole::Side),
        "longterm" => Some(ListRole::Longterm),
        "chore" => Some(ListRole::Chore),
        _ => None,
    }
}

pub fn can_delete_list(
    lists: &[TaskList],
    tasks: &[Task],
    id: &str,
) -> Result<(), TaskListError> {
    if is_preset_list_id(id) {
        return Err(TaskListError::PresetLocked);
    }
    let Some(list) = lists.iter().find(|l| l.id == id) else {
        return Err(TaskListError::MissingList);
    };
    if tasks.iter().any(|t| t.list_id == id) {
        return Err(TaskListError::NotEmpty);
    }
    if list.role == ListRole::Mainline {
        let mainline = lists.iter().filter(|l| l.role == ListRole::Mainline).count();
        if mainline <= 1 {
            return Err(TaskListError::NoMainline);
        }
    }
    Ok(())
}

pub fn schedule_from_drop(ts: i64) -> (i64, i64) {
    align_range(ts, ts + UNSCHEDULED_DROP_SECS)
}

pub fn move_range_to_day(
    start: i64,
    end: i64,
    old_day_start: i64,
    new_day_start: i64,
) -> (i64, i64) {
    let offset = start - old_day_start;
    let dur = end - start;
    align_range(new_day_start + offset, new_day_start + offset + dur)
}

pub fn snapshots_for_day(
    tasks: &[Task],
    lists: &[TaskList],
    day_start: i64,
    day_end: i64,
) -> Vec<TaskSnapshot> {
    match judgment_tasks(tasks, lists, day_start, day_end) {
        Ok(selected) => snapshot_of(&selected, lists),
        Err(TaskListError::TooManyJudgment) => {
            let mut selected: Vec<&Task> = tasks
                .iter()
                .filter(|task| {
                    lists
                        .iter()
                        .find(|l| l.id == task.list_id)
                        .is_some_and(|list| in_judgment_set(task, list, day_start, day_end))
                })
                .collect();
            selected.sort_by_key(|t| (t.start, t.id.as_str()));
            selected.truncate(MAX_JUDGMENT_TASKS);
            snapshot_of(&selected, lists)
        }
        Err(_) => Vec::new(),
    }
}
```

`TaskListError` 增加 `PresetLocked` / `NotEmpty` / `MissingList`，删除 `DuplicateMainline`。全仓库 `DuplicateMainline` 匹配一并删掉（目前只有 `task.rs` 测试）。

`match_role_alias` 增加 `"长期规划"`。

- [ ] **Step 4: 跑 core 测试**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife-core`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: allow multiple mainline lists and cap pinned snapshots

EOF
)"
```

---

### Task 2: 自然语言 `#长期规划`

**Files:**
- Modify: `crates/gamelife-core/src/task_parse.rs`

**Interfaces:**
- Consumes: `match_role_alias`（Task 1 已含「长期规划」）
- Produces: `#长期规划` 与 `#长期计划` 都进长期组

- [ ] **Step 1: 写失败测试**

在 `task_parse.rs` 现有 `tests` 里追加：

```rust
#[test]
fn hashtag_longterm_planning_alias() {
    let lists = crate::task::preset_lists();
    let p = parse_task_line(
        "写开题 #长期规划",
        &ctx(&lists),
    );
    assert_eq!(p.list_id, crate::task::PRESET_LONGTERM_ID);
    assert!(!p.parse_ok);
    assert_eq!(p.title, "写开题");
}
```

`ctx` 已在该文件。若 `strip_title` 会留下别的空白，按现有 `写方法节` 测试同样断言。

- [ ] **Step 2: 跑测试，确认失败**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife-core hashtag_longterm_planning_alias -- --nocapture`

Expected: FAIL（list_id 不是长期组，直到 alias 接通；若 Task 1 已加 alias 且 `match_list` 走 role，此测试可能已过——若已过，仍提交本任务只加测试，保证回归。）

- [ ] **Step 3: 若失败，只改 `match_role_alias`**

```rust
"长期" | "长期计划" | "长期规划" => Some(ListRole::Longterm),
```

- [ ] **Step 4: 跑 core 测试**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife-core task_parse::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task.rs crates/gamelife-core/src/task_parse.rs
git commit -m "$(cat <<'EOF'
feat: parse #长期规划 as a longterm list alias

EOF
)"
```

---

### Task 3: 任务命令——建组带角色、删除、移动、改期

**Files:**
- Modify: `src-tauri/src/db.rs`（长期规划改名；可选）
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 的 `can_delete_list` / `parse_list_role_strict` / `schedule_from_drop` / `move_range_to_day` / `is_preset_list_id`
- Produces:

```rust
#[tauri::command]
pub fn list_task_board() -> Result<TaskBoardView, String>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskBoardView {
    pub lists: Vec<TaskListView>,
    pub tasks: Vec<TaskView>,
}

#[tauri::command]
pub fn create_list(name: String, role: String) -> Result<TaskListView, String>;

#[tauri::command]
pub fn rename_list(id: String, name: String) -> Result<(), String>;

#[tauri::command]
pub fn delete_list(id: String) -> Result<(), String>;

#[tauri::command]
pub fn delete_task(id: String) -> Result<(), String>;

#[tauri::command]
pub fn move_task(id: String, list_id: String) -> Result<(), String>;

#[tauri::command]
pub fn reschedule_task(
    id: String,
    start: Option<i64>,
    end: Option<i64>,
) -> Result<(), String>;
```

删除命令 `list_tasks`（它返回 `TodayView`）。`create_list` 的旧签名 `(name)` 改为必须 `role`。`upsert_task` 继续拒绝空标题和当天 `TooManyJudgment`。`reschedule_task`：两个 `None` 表示清成未排期；只给一个 → `Rejected("need_start_and_end")`；两个都有 → `align_range` 后写入，再跑当天 `judgment_tasks`。

`db.rs` `migrate` 末尾（`seed_preset_lists_if_empty` 之后）加：

```rust
conn.execute(
    "UPDATE task_lists SET name = '长期规划'
     WHERE id = 'list-longterm' AND name = '长期计划'",
    [],
)?;
```

- [ ] **Step 1: 在 `commands.rs` 测试模块（文件底部现有 `#[cfg(test)]`）写失败测试**

`migrate_renames_legacy_longterm_list` 放进 `src-tauri/src/db.rs` 现有 `#[cfg(test)]`。`commands.rs` 已有多处 `#[cfg(test)]`，不要再开一个根 `mod tests`。`delete_task` 用 in-memory `migrate` + 插入行后调用命令所用的同一 `DELETE`。

```rust
#[test]
fn create_list_requires_strict_role() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    assert!(parse_list_role_strict("custom").is_none());
    assert_eq!(parse_list_role_strict("mainline"), Some(ListRole::Mainline));
}

#[test]
fn migrate_renames_legacy_longterm_list() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    conn.execute(
        "UPDATE task_lists SET name = '长期计划' WHERE id = 'list-longterm'",
        [],
    )
    .unwrap();
    migrate(&conn).unwrap();
    let name: String = conn
        .query_row(
            "SELECT name FROM task_lists WHERE id = 'list-longterm'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "长期规划");
}
```

第二条要求 `migrate` 幂等可再跑。`parse_list_role_strict` 从 core 引用。

再写一个 `delete_task_removes_row`：insert task、`DELETE FROM tasks WHERE id=?`、count 0。若不想测 command 包装，测 `persist` 旁抽出的 `fn delete_task_row(conn, id) -> Result<(), DbOpError>`。

- [ ] **Step 2: 跑测试，确认失败**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife migrate_renames_legacy_longterm_list -- --nocapture`

Expected: FAIL（名字仍是种子或 UPDATE 不存在）

- [ ] **Step 3: 实现命令与 migrate 改名**

`create_list`：

```rust
let role = parse_list_role_strict(&role)
    .ok_or_else(|| DbOpError::Rejected("invalid_role".into()))?;
```

`delete_list`：`load` → `can_delete_list` → `DELETE FROM task_lists WHERE id=?`。错误映射：`PresetLocked`/`NotEmpty`/`NoMainline`/`MissingList` → `Rejected`。

`delete_task`：`DELETE FROM tasks WHERE id=?`，`n==0` → `Fatal("task missing")`。

`move_task`：目标 `list_id` 必须存在；`UPDATE tasks SET list_id=?`。

`reschedule_task`：如上。清时段时 `start=NULL, end=NULL`。

`list_task_board`：返回全部 lists + tasks，不做「仅今日」过滤。

`lib.rs`：从 handler 去掉 `list_tasks`，加上新命令。

- [ ] **Step 4: 跑 gamelife 测试**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife`

Expected: PASS（此时前端还在调 `list_tasks` / `create_list({name})`，本任务只改 Rust；预览夹具要到 Task 8 才接。若有 Rust 测试调用 `list_tasks`，一并改。）

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: add local task board commands with typed list roles

EOF
)"
```

---

### Task 4: 槽快照改钉本地任务

**Files:**
- Modify: `src-tauri/src/scheduler.rs`

**Interfaces:**
- Consumes: `snapshots_for_day`、`load_task_lists`、`load_tasks`、`start_of_named_day`、`end_of_local_day`
- Produces: `pin_task_snapshot_json` 不再读 `ticktick_cache`；删除 `maybe_refresh_ticktick_cache` 及其在 `ensure_slot` 的调用

- [ ] **Step 1: 改写现有失败语义**

把 `pin_snapshot_uses_ticktick_cache_not_local_tasks` 换成：

```rust
#[test]
fn pin_snapshot_uses_local_tasks_not_ticktick_cache() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let day = "2026-09-13";
    let day_start = start_of_named_day(day).expect("named day");
    let start = day_start + 10 * 3600;
    let end = start + 3600;
    conn.execute(
        "INSERT INTO tasks (id, list_id, title, done, start, end)
         VALUES ('task-local','list-mainline','LOCAL',0, ?1, ?2)",
        params![start, end],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ticktick_cache (id, project_id, title, role, start, end, fetched_at)
         VALUES ('tt-9','p','RAIDS+', 'mainline', ?1, ?2, 50)",
        params![start, end],
    )
    .unwrap();
    let json = pin_task_snapshot_json(&conn, day);
    assert!(json.contains("LOCAL"), "{json}");
    assert!(!json.contains("tt-9"), "{json}");
}
```

`migrate` 已种子 `list-mainline`，不要再插一次同 id。

再加：

```rust
#[test]
fn pin_snapshot_skips_done_and_unscheduled() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    let day = "2026-09-13";
    let day_start = start_of_named_day(day).expect("named day");
    let start = day_start + 10 * 3600;
    let end = start + 3600;
    conn.execute(
        "INSERT INTO tasks (id, list_id, title, done, start, end)
         VALUES ('a','list-mainline','DONE',1, ?1, ?2)",
        params![start, end],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tasks (id, list_id, title, done, start, end)
         VALUES ('b','list-mainline','INBOX',0, NULL, NULL)",
        [],
    )
    .unwrap();
    let json = pin_task_snapshot_json(&conn, day);
    assert_eq!(json, "[]");
}
```

- [ ] **Step 2: 跑测试，确认失败**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife pin_snapshot_uses_local_tasks_not_ticktick_cache -- --nocapture`

Expected: FAIL（json 仍含 `tt-9`）

- [ ] **Step 3: 实现**

```rust
fn pin_task_snapshot_json(conn: &Connection, day: &str) -> String {
    let Some(day_start) = start_of_named_day(day) else {
        return "[]".into();
    };
    let day_end = end_of_local_day(day_start);
    let lists = match crate::db::load_task_lists(conn) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("load_task_lists failed while pinning snapshot: {e:?}");
            return "[]".into();
        }
    };
    let tasks = match crate::db::load_tasks(conn) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("load_tasks failed while pinning snapshot: {e:?}");
            return "[]".into();
        }
    };
    let snaps = gamelife_core::snapshots_for_day(&tasks, &lists, day_start, day_end);
    serde_json::to_string(&snaps).unwrap_or_else(|_| "[]".into())
}
```

删除 `maybe_refresh_ticktick_cache` 整函数和 `ensure_slot` 里对它的调用。删掉因此变成 unused 的 `ticktick` import。`lib.rs` 仍 `pub mod ticktick` 直到 Task 7。

导出 `snapshots_for_day`：Task 1 已在 `lib.rs` `pub use`。

- [ ] **Step 4: 跑 gamelife 测试**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/scheduler.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: pin slot snapshots from local tasks

EOF
)"
```

---

### Task 5: `DayView.dayTasks` 取代 TickTick 当日列表

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/preview/fixtures.ts`
- Modify: `src/lib/timelinePlan.ts`
- Modify: `src/lib/timelinePlan.test.ts`
- Modify: `src/lib/taskCompare.ts`
- Modify: `src/lib/taskCompare.test.ts`
- Modify: `src/pages/Today.tsx`

**Interfaces:**
- Consumes: `load_tasks` / `load_task_lists` / `in_judgment_set`（未完成且有时段且与日相交；长期已排期也进）
- Produces:

```rust
pub struct DayView {
    // 删除 ticktick_tasks
    pub day_tasks: Vec<DayTaskView>,
    ...
}

pub struct DayTaskView {
    pub id: String,
    pub title: String,
    pub role: String,
    pub start: i64,
    pub end: i64,
}
```

前端：

```ts
export interface DayTask {
  id: string;
  title: string;
  role: string;
  start: number;
  end: number;
}
```

`DayView.dayTasks`。删除 `TickTickTask.allDay`。`splitTicktickPlan` 改名为 `splitTimedPlan(tasks: DayTask[])`，不再分 all-day：无全天。`taskCompare` 的参数改为 `DayTask[]`。

`load_plan_marks` 改为遍历本地未完成已排期任务，不再 `load_ticktick_cache`。

今日底部卡片标题「当天任务」；空态：「当天没有已排期任务。可在任务页添加。」删除全天顶条（`allDay` 分支）。文案里「TickTick」改成「计划」/「任务页」。计划列仍用 `planBlocks(dayTasks)`。本任务 **还不做** 计划列拖动（Task 11）。

- [ ] **Step 1: 先改前端纯函数测试**

`timelinePlan.test.ts`：去掉 `allDay: true` 用例；断言 `splitTimedPlan` 只保留 `end > start`。

`taskCompare.test.ts`：类型改为 `DayTask`，去掉 `allDay` 字段。

- [ ] **Step 2: 跑 vitest，确认失败**

Run: `npx vitest run --dir src lib/timelinePlan lib/taskCompare`

Expected: FAIL（符号改名）

- [ ] **Step 3: 改 Rust `DayView` + 前端类型 + Today 编译通过**

`load_ticktick_day_tasks` 换成：

```rust
fn load_day_tasks(
    conn: &Connection,
    day_start: i64,
    day_end: i64,
) -> Result<Vec<DayTaskView>, DbOpError> {
    let lists = load_task_lists(conn)?;
    let mut out = Vec::new();
    for task in load_tasks(conn)? {
        if task.done {
            continue;
        }
        let (Some(start), Some(end)) = (task.start, task.end) else {
            continue;
        };
        if end <= start || start >= day_end || end <= day_start {
            continue;
        }
        let Some(list) = lists.iter().find(|l| l.id == task.list_id) else {
            continue;
        };
        out.push(DayTaskView {
            id: task.id,
            title: task.title,
            role: list_role_sql(list.role).into(),
            start,
            end,
        });
    }
    out.sort_by_key(|t| (t.start, t.end, t.title.clone()));
    Ok(out)
}
```

`get_day_view` 填 `day_tasks`。预览 `previewDayView` 同步改字段名。`Today.tsx` 全部 `ticktickTasks` → `dayTasks`。

- [ ] **Step 4: 跑测试**

Run:

```
CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife
npx vitest run --dir src
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src/lib/api.ts src/lib/preview src/lib/timelinePlan.ts src/lib/timelinePlan.test.ts src/lib/taskCompare.ts src/lib/taskCompare.test.ts src/pages/Today.tsx
git commit -m "$(cat <<'EOF'
feat: serve local day tasks instead of TickTick on Today

EOF
)"
```

---

### Task 6: 云备份带上任务表

**Files:**
- Modify: `src-tauri/src/sync.rs`
- Modify: `src/lib/cloudSync.ts`（若有测试）
- Modify: `src/pages/Settings.tsx` 备份范围说明一句

**Interfaces:**
- Consumes: `NEVER_SYNCED_TABLES`
- Produces: `["heartbeat", "ticktick_cache"]`。副本里 `task_lists`/`tasks` 保留。

- [ ] **Step 1: 改现有测试**

`snapshot_drops_per_device_tables_at_every_scope`：

- `for table in ["heartbeat", "ticktick_cache"]` 仍要求 count 0。
- `task_lists` / `tasks` 的 count **大于 0**（种子列表 + 插入的 `t-1`）。
- `assert!(text.contains("体检预约"));` 取代「不得含体检预约」。
- `assert!(!text.contains("去买降压药"));` 仍成立（ticktick_cache 删除）。
- 注释改成：任务表进入备份；TickTick 缓存仍永不上传。

- [ ] **Step 2: 跑测试，确认失败**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife snapshot_drops_per_device_tables_at_every_scope -- --nocapture`

Expected: FAIL（tasks 仍被删，找不到「体检预约」）

- [ ] **Step 3: 改常量和注释**

```rust
const NEVER_SYNCED_TABLES: &[&str] = &["heartbeat", "ticktick_cache"];
```

设置页 `option value="aggregate"` 旁或下方加一句中文：「计划本（任务与分组）会进入备份。」不要把窗口标题说成会上传。

- [ ] **Step 4: 跑 gamelife 测试**

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/sync.rs src/pages/Settings.tsx src/lib/cloudSync.ts
git commit -m "$(cat <<'EOF'
feat: include local tasks in cloud backup snapshots

EOF
)"
```

---

### Task 7: 拆除 TickTick

**Files:**
- Delete: `src-tauri/src/ticktick.rs`
- Modify: `src-tauri/src/lib.rs`（去掉 `mod ticktick` 与全部 `ticktick_*` 命令）
- Modify: `src-tauri/src/commands.rs`（删除 TickTick 命令函数和 `TickTick*` 视图；`get_week` 若带 `today_tasks: Vec<TickTickTaskView>` 则改 `DayTaskView` 或删除该字段——先 grep `today_tasks`）
- Modify: `src-tauri/src/config.rs`（`ticktick_*` 字段 `#[serde(default, skip_serializing)]`）
- Modify: `src-tauri/src/keychain.rs`（删除 TickTick 读写函数及其测试）
- Modify: `src-tauri/src/scheduler.rs`（确认无 ticktick 引用）
- Modify: `src/pages/Settings.tsx`（去掉 tab `ticktick` 及整段 UI；关于页里「排期留在 TickTick」改成「排期在任务页」）
- Modify: `src/lib/api.ts`（删除 `ticktick*` 函数与类型）
- Modify: `src/lib/preview/invoke.ts` / `fixtures.ts`
- Modify: `src/lib/guides.ts` 主线样稿去掉 TickTick
- Delete or slim: `src/lib/ticktickBoard.ts` 与 `ticktickBoard.test.ts`（OAuth/sync 文案）。角色中文标签迁到 `src/lib/taskBoard.ts`：

```ts
export function listRoleLabel(role: string): string {
  switch (role) {
    case "mainline": return "主线";
    case "side": return "支线";
    case "longterm": return "长期";
    case "chore": return "杂项";
    default: return "支线";
  }
}

export function taskTimeLabel(start: number, end: number): string {
  // 与现 ticktickTaskTimeLabel 相同的 zh-CN HH:mm–HH:mm
}
```

- Modify: `src/lib/secretField.ts` / `secretField.test.ts`：删除 TickTick 专用错误串；通用 OAuth 测试若只为 TickTick 存在则删。

**Interfaces:**
- Consumes: Task 5 之后今日已不读 TickTick
- Produces: 进程内不再有 TickTick HTTP / OAuth

- [ ] **Step 1: 用编译当测试**

先删 `lib.rs` 里 `pub mod ticktick` 和 handler 中的 `ticktick_*`，跑：

Run: `CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife --no-run`

Expected: FAIL 一堆 unresolved ticktick

（`--no-run` 仅用于本步定位引用；本任务结束前必须 **跑** 完整 `cargo test --offline -p gamelife`。）

- [ ] **Step 2: 按编译错误清引用，删除 `ticktick.rs`**

Grep `ticktick` 于 `src-tauri/` 与 `src/`，除 `ticktick_cache` 表名和本 spec/plan 文档外全部去掉。`config` 测试 `policy_snapshot_ignores_ticktick_client_id` 仍可保留：改成「字段 skip_serializing 后 snapshot 不含 client id」。

- [ ] **Step 3: 前端 Settings / api / preview / guides**

`TABS` 去掉 TickTick。删除 `ttStatus` / `tree` / OAuth handlers。关于页与 `guides.ts` 的 TickTick 句子改成任务页。

- [ ] **Step 4: 跑两侧测试**

```
CARGO_TARGET_DIR=/Volumes/MobileSSD/MyProjects/GameLife/target /opt/homebrew/bin/cargo test --offline -p gamelife
npx vitest run --dir src
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A src-tauri src
git commit -m "$(cat <<'EOF'
feat: remove TickTick OAuth, sync, and settings

EOF
)"
```

不要 `git add -A` 仓库里无关脏文件。只 add 本任务改过的路径。若 `ticktick.rs` 是删除，`git add -u src-tauri/src/ticktick.rs`。

---

### Task 8: 轨道第五页 + 任务页外壳

**Files:**
- Modify: `src/lib/rail.ts`
- Modify: `src/App.tsx`
- Create: `src/pages/Tasks.tsx`
- Create: `src/hooks/useTaskSplit.ts`
- Create: `src/lib/taskSplit.ts` + `src/lib/taskSplit.test.ts`
- Modify: `src/lib/api.ts`（接 `list_task_board` 等；若 Task 3 已加命令）
- Modify: `src/lib/preview/invoke.ts` / `fixtures.ts`

**Interfaces:**
- Consumes: Task 3 命令；PageHeader / Card / Segmented
- Produces: `RailTabId` 含 `"tasks"`；预览 `?tab=tasks`

```ts
// src/lib/taskSplit.ts
export const TASK_SPLIT_KEY = "gl-task-list-width";
export const LIST_MIN = 240;
export const LIST_DEFAULT = 320;
export const LIST_MAX = 520;
export function clampListWidth(value: number): number { ... }
```

`useTaskSplit` 抄 `useSidebarWidth` / `useTimelineSplit`：指针捕获、方向键、`localStorage`。左栏百分比不要；存 **像素宽度**。

`Tasks.tsx` 第一版即可交互的骨架：

- `PageHeader` title `任务`；`center`：排序按钮（先只是 `<Button variant="outline" size="sm">排序</Button>`，点一下在 `按时间`/`按标题` 间切，写入 `localStorage` key `gl-task-sort`）；`···` 按钮先打开现有 `Dialog` 占位（标题「更多」，里面两个按钮：添加分组、显示已完成）。`actions`：`Segmented` 选项 `3 天` / `7 天`，key `gl-task-cal-days`，默认 `"7"`。
- 内容：`flex-1 min-h-0 px-[22px] pb-4` 里横排两张 `Card` + 中间 handle。左 Card：顶上 `Input` placeholder `明天上午十点到十二点，写方法节 #主线`。其下按列表渲染分组标题（色点 `categoryColor`：mainline/side/admin/longterm）和任务行（先只读）。右 Card：先空，写「日历」占位一行即可（Task 10 再画格）。
- `listTaskBoard()` 加载。预览夹具返回四组预置 + 两条示例任务。

`rail.ts`：`{ id: "tasks", label: "任务", icon: ListTodo }`（lucide）。插在「今日」和「统计」之间。

`App.tsx` `initialTab` 与 `tab === "tasks"`。

- [ ] **Step 1: 写 `taskSplit.test.ts` 与 `rail` 断言**

若没有 `rail.test.ts` 就建 `src/lib/rail.test.ts`：

```ts
import { railTabs } from "./rail";
it("includes 任务 between 今日 and 统计", () => {
  expect(railTabs().map((t) => t.id)).toEqual([
    "today",
    "tasks",
    "week",
    "shop",
    "settings",
  ]);
});
```

`taskSplit.test.ts`：clamp 边界与非法值回默认，照 `timelinePlan.test.ts` 的 width 测试。

- [ ] **Step 2: 跑 vitest，确认失败**

Run: `npx vitest run --dir src lib/rail lib/taskSplit`

Expected: FAIL

- [ ] **Step 3: 实现 rail、hook、Tasks 骨架、preview 命令**

`api.ts`：

```ts
export interface TaskBoardView {
  lists: TaskListView[];
  tasks: TaskView[];
}
export function listTaskBoard(): Promise<TaskBoardView> {
  return invoke("list_task_board");
}
export function createList(name: string, role: string): Promise<TaskListView> {
  return invoke("create_list", { name, role });
}
export function renameList(id: string, name: string): Promise<void> {
  return invoke("rename_list", { id, name });
}
export function deleteList(id: string): Promise<void> {
  return invoke("delete_list", { id });
}
export function deleteTask(id: string): Promise<void> {
  return invoke("delete_task", { id });
}
export function moveTask(id: string, listId: string): Promise<void> {
  return invoke("move_task", { id, listId });
}
export function rescheduleTask(
  id: string,
  start: number | null,
  end: number | null,
): Promise<void> {
  return invoke("reschedule_task", { id, start, end });
}
```

删除旧 `listTasks` / 旧 `createList(name)`。preview `invoke` 为这些命令返回夹具或 no-op。

- [ ] **Step 4: 跑 vitest**

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/rail.ts src/lib/rail.test.ts src/lib/taskSplit.ts src/lib/taskSplit.test.ts src/hooks/useTaskSplit.ts src/pages/Tasks.tsx src/App.tsx src/lib/api.ts src/lib/preview
git commit -m "$(cat <<'EOF'
feat: add Tasks rail page with split cards

EOF
)"
```

---

### Task 9: 左栏——解析输入、折叠分组、右键、对话框

**Files:**
- Create: `src/lib/taskBoard.ts` + `src/lib/taskBoard.test.ts`（若 Task 7 已建，本任务只补排序/折叠）
- Create: `src/lib/taskSort.ts` + `src/lib/taskSort.test.ts`
- Create: `src/components/ui/context-menu.tsx`（手写：fixed 定位、点外面关闭、Esc；不要 Radix）
- Modify: `src/pages/Tasks.tsx`

**Interfaces:**
- Consumes: `parseTaskLine` / `upsertTask` / `toggleTaskDone` / `deleteTask` / `moveTask` / `rescheduleTask` / `createList` / `renameList` / `deleteList`
- Produces: 规格 §6–§7.2 的左栏行为

```ts
// taskSort.ts
export type TaskSort = "time" | "title";
export function sortTasks(tasks: TaskView[], sort: TaskSort): TaskView[] {
  const copy = tasks.slice();
  if (sort === "title") {
    copy.sort((a, b) => a.title.localeCompare(b.title, "zh"));
    return copy;
  }
  copy.sort((a, b) => {
    const as = a.start;
    const bs = b.start;
    if (as == null && bs == null) return a.title.localeCompare(b.title, "zh");
    if (as == null) return 1;
    if (bs == null) return -1;
    return as - bs || (a.end ?? 0) - (b.end ?? 0);
  });
  return copy;
}
```

折叠 key：`gl-task-collapsed` = JSON string array of list ids。默认全部展开。

已完成：state `showDone`，默认 false；`···` 切换。组计数 = 未完成数。

输入：`onChange` 调 `parseTaskLine(line, focusedListId)`，框下三个碎片（分组名、时段或「未排期」、标题）。回车：`upsertTask` 新 id 用 `crypto.randomUUID()` 或留空让后端生成（后端已有空 id → `task-{now}`）。`currentListId`：若某分组标题或其中任务行处于 focus，用该 `list_id`，否则第一个 `role==="mainline"` 的组。

右键菜单项：更改日期（Dialog：`<input type="date">` + 两个 `type="time"`；已排期改日期不填时间则用 `move_range` 的前端等价：`start/end += (newDayStart - oldDayStart)`）；移动到其它分组；放弃任务（第二个 Dialog 确认后 `deleteTask`）。

分组标题右键：重命名 Dialog；自建空组显示删除。

添加分组 Dialog：名称 + `Select` 角色四项。

色点：`style={{ background: categoryColor(role === "chore" ? "admin" : role === "longterm" ? "longterm" : categoryOfRole) }}`。不要写 hex。`categoryColor("mainline" | "side" | "admin" | "longterm")`。

- [ ] **Step 1: `taskSort.test.ts`**

```ts
it("puts unscheduled last when sorting by time", () => {
  const tasks = [
    { id: "b", listId: "l", title: "乙", done: false, start: null, end: null, range: null },
    { id: "a", listId: "l", title: "甲", done: false, start: 100, end: 200, range: null },
  ];
  expect(sortTasks(tasks, "time").map((t) => t.id)).toEqual(["a", "b"]);
});
```

- [ ] **Step 2: 跑测试，确认失败**

Run: `npx vitest run --dir src lib/taskSort`

Expected: FAIL

- [ ] **Step 3: 实现排序 + Tasks 左栏完整交互**

Context menu 组件 API：

```tsx
export function ContextMenu({
  x, y, open, onClose, children,
}: {
  x: number; y: number; open: boolean; onClose: () => void; children: React.ReactNode;
}): React.ReactNode
```

`open && createPortal(...)`。不要引入新依赖。

超 20 条：`upsert`/`reschedule` 抛错时 toast「当天已排期任务超过 20，请先完成、改期或放弃。」把 `too_many_judgment_tasks` 映射到这句。

- [ ] **Step 4: 跑 vitest**

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/taskSort.ts src/lib/taskSort.test.ts src/lib/taskBoard.ts src/lib/taskBoard.test.ts src/components/ui/context-menu.tsx src/pages/Tasks.tsx src/lib/api.ts
git commit -m "$(cat <<'EOF'
feat: add task list input, grouping, and context menus

EOF
)"
```

---

### Task 10: 右栏 3/7 天日历与拖动改期

**Files:**
- Create: `src/lib/taskCalendar.ts` + `src/lib/taskCalendar.test.ts`
- Modify: `src/pages/Tasks.tsx`

**Interfaces:**
- Consumes: `DayTask` 形状的已排期未完成任务；`rescheduleTask`；`schedule_from_drop` 的前端等价
- Produces:

```ts
export type CalSpan = 3 | 7;

export function calendarDays(today: string, span: CalSpan): string[] {
  // 3: today, tomorrow, day after（本地日历日，用与 calendar.ts addDays 相同的实现）
  // 7: Monday of the week containing today, then +6
}

export function dropRange(dropTs: number): { start: number; end: number } {
  const start = Math.floor(dropTs / 900) * 900;
  let end = start + 1800;
  if (end <= start) end = start + 900;
  return { start, end };
}
```

不要自己发明时区函数：复用 `src/lib/calendar.ts` 的 `addDays`。若没有「本周一」，写：

```ts
export function mondayOf(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const dt = new Date(y, m - 1, d);
  const wd = (dt.getDay() + 6) % 7; // 0 = Monday
  return addDays(day, -wd);
}
```

日历 UI：右 Card `overflow-auto`。列 = `calendarDays`。行 = 24 小时，行高可小于今日（例如每小时 36px）以免 7 列炸高。块样式抄今日计划列：`color-mix` + `borderLeft: 3px solid categoryColor(...)`。

拖动：

- 已排期块 `onPointerDown` 记录 `grabOffset`，`pointermove` 时按指针所在 **日列 + 分钟** 算出新 `start`，`end = start + duration`，松手 `rescheduleTask`。
- 左栏未排期行 `draggable` 不要用 HTML5 dnd；用同一套指针捕获：在日历格子 `pointerup` 若正在拖一条未排期，则 `dropRange` 后 `rescheduleTask`。
- 对齐 15 分钟。禁止 dnd-kit。

今日列用 `--cat-mainline` 的淡底标「今天」，不要画硬框线封死整页。

- [ ] **Step 1: 日历日期测试**

```ts
it("three days are today and the next two", () => {
  expect(calendarDays("2026-09-15", 3)).toEqual([
    "2026-09-15",
    "2026-09-16",
    "2026-09-17",
  ]);
});

it("seven days are Monday through Sunday", () => {
  expect(calendarDays("2026-09-15", 7)[0]).toBe("2026-09-14"); // 2026-09-15 is Tuesday
  expect(calendarDays("2026-09-15", 7)).toHaveLength(7);
});

it("drop range is thirty minutes snapped to a slot", () => {
  expect(dropRange(100)).toEqual({ start: 0, end: 1800 });
});
```

（先确认 2026-09-15 是周二：用户信息是 Tuesday Sep 15, 2026，周一是 14 日。）

- [ ] **Step 2: 跑测试，确认失败**

Run: `npx vitest run --dir src lib/taskCalendar`

Expected: FAIL

- [ ] **Step 3: 实现纯函数 + 日历网格 + 拖动**

重叠任务：同一天同一时段用 `assignPlanLanes`（已在 `timelinePlan.ts`）分列，避免叠死。

- [ ] **Step 4: 跑 vitest**

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/taskCalendar.ts src/lib/taskCalendar.test.ts src/pages/Tasks.tsx
git commit -m "$(cat <<'EOF'
feat: add 3/7-day task calendar with drag reschedule

EOF
)"
```

---

### Task 11: 今日计划列拖钟点

**Files:**
- Modify: `src/pages/Today.tsx`
- Create: `src/lib/planDrag.ts` + `src/lib/planDrag.test.ts`

**Interfaces:**
- Consumes: `dayTasks`；`rescheduleTask`；`align` 15 分钟
- Produces: 计划块在 **当天** 上下拖改 `start/end`，时长不变；不能拖到别的日期（没有日列）。

```ts
export function moveSameDay(
  start: number,
  end: number,
  dayStart: number,
  grabDeltaSlots: number,
): { start: number; end: number } {
  const dur = end - start;
  let next = start + grabDeltaSlots * 900;
  const min = dayStart;
  const max = dayStart + 96 * 900 - dur;
  if (next < min) next = min;
  if (next > max) next = max;
  next = Math.round(next / 900) * 900;
  return { start: next, end: next + dur };
}
```

`Today` 的 `Timeline` 计划块加 pointer 捕获。松手调用 `rescheduleTask`。只读任务卡片已是 Task 5 的「当天任务」。

- [ ] **Step 1: `planDrag.test.ts`**

```ts
it("clamps a dragged block inside the day", () => {
  const day = 1_000_000;
  const moved = moveSameDay(day + 3600, day + 7200, day, -10);
  expect(moved.start).toBe(day);
  expect(moved.end - moved.start).toBe(3600);
});
```

- [ ] **Step 2: 跑测试，确认失败**

Run: `npx vitest run --dir src lib/planDrag`

Expected: FAIL

- [ ] **Step 3: 实现拖动并接到 Timeline 计划块**

不要让拖动误开 `SlotReview`（实际列才 `onPick`）。

- [ ] **Step 4: 跑 vitest**

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/planDrag.ts src/lib/planDrag.test.ts src/pages/Today.tsx
git commit -m "$(cat <<'EOF'
feat: drag Today plan blocks to change clock time

EOF
)"
```

---

### Task 12: 文档与规则跟上规格

**Files:**
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`
- Modify: `.cursor/rules/specs.mdc`
- Modify: `.cursor/rules/frontend-ui.mdc`
- Modify: `.cursor/rules/cloud-sync.mdc`（永不表不再含 tasks）

**Interfaces:**
- Consumes: 已落地的行为
- Produces: 文档不再写「计划在 TickTick / 禁止本地任务 CRUD」

把「计划在 TickTick，只读」改成「计划在本机任务页；判定钉本地已排期任务；观测优先」。`NEVER_SYNCED` 文档改为 `heartbeat` / `ticktick_cache`。rail 增加「任务」。前端规则：Pages 含 `Tasks.tsx`。

不要改经济、观测、视觉章节。

- [ ] **Step 1: 全文搜索过时句子**

Run: `rg -n "禁止本地任务|计划在 TickTick|无自建列表|list_tasks" AGENTS.md CLAUDE.md .cursor/rules docs/superpowers/specs/2026-09-15-local-tasks-design.md`

（规格本身保留「覆盖并取代」历史表述。只改 AGENTS / CLAUDE / rules。）

- [ ] **Step 2: 按规格 §11 最后一段改这三处**

- [ ] **Step 3: 再搜 `src/` 用户可见中文是否还剩「TickTick」**

Run: `rg -n "TickTick" src --glob '!*.test.ts'`

设置关于页、guides、空态应已在 Task 5/7 清掉。若还有，本任务一并改。

- [ ] **Step 4: 不跑业务测试**（纯文档）。若改了 `src/` 文案，跑 `npx vitest run --dir src`。

- [ ] **Step 5: Commit**

```bash
git add AGENTS.md CLAUDE.md .cursor/rules
git commit -m "$(cat <<'EOF'
docs: describe local tasks as the plan source

EOF
)"
```

---

## Self-review

**Spec coverage:** §5 模型 → T1–3；§6 解析 → T2/T9；§7 UI → T8–10；§8 今日 → T5/T11；§9 快照 → T4；§10 备份 → T6；§11 删除 TickTick → T7；§12 命令 → T3；§13 不做 → 各任务约束；§14 测试分布在 T1–11。

**Placeholders:** 无 TBD。TickTick 拆除按编译错误清引用，文件名单已列。

**Types:** `DayTask` / `DayTaskView` / `TaskBoardView` / `list_task_board` / `reschedule_task(start, end)` / `create_list(name, role)` / `UNSCHEDULED_DROP_SECS = 1800` 前后一致。`list_tasks` 删除后前端不得再调用。
