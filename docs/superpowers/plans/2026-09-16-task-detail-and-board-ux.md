# Task Detail and Board UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 任务备注进 `tasks.notes`（`user_version` 5）；列表/日历/今日共用详情框；列表空心圆勾选、拖动不选字、Shift/⌘ 多选与批量移动/放弃；日历去掉右缘竖条，改为列缝 + 小时淡底。

**Architecture:** `notes` 只活在 `Task` / SQLite / `TaskView`。`TaskSnapshot` 仍是 `id` / `title` / `role`，判定与 AI 看不到备注。Markdown 子集是纯函数 `src/lib/taskNotesMd.ts`。多选与小时底色也是纯函数。UI 仍手绘：不引入 FullCalendar、dnd-kit、Radix、Markdown 编辑器库。详情与改期不套娃。

**Tech Stack:** Tauri 2、Rust、React、TypeScript、vitest、SQLite。构建用 Homebrew `/opt/homebrew/bin/cargo`。

**Spec:** `docs/superpowers/specs/2026-09-16-task-detail-and-board-ux-design.md`（覆盖 2026-09-15 的日历右缘竖条、系统 checkbox `accentColor`、`user_version` 4）。

## Global Constraints

- 15 秒采样；缺口不外推；进程死亡是未观测。`credited ≤ observed ≤ 实际槽长`。
- 观测优先：锁屏 / 离开 / 娱乐硬规则压过任务标题匹配。勾选、改备注、改期、多选移动/放弃都不发币。
- `TaskSnapshot` 不含 `notes`。备注不得进入 hint haystack、文本 AI prompt、视觉摘要。
- 不做全天、时区、dnd-kit、Radix、FullCalendar、新 npm Markdown/富文本库、子任务、日历/今日上的 Shift 连选、多选整段拖排序。
- 界面中文。能量称「能量」。颜色只来自 `--cat-*` / `theme.ts`。禁止在组件里写 hex。
- 页面只经 `src/lib/api.ts` 调 `invoke()`。对话框不嵌套。
- `user_version` **5**。`device_id` 仍不是版本号。不删 `ticktick_cache`。
- 改 `src-tauri/`：`/opt/homebrew/bin/cargo test --offline -p gamelife` 必须跑过（不要设 `CARGO_TARGET_DIR`，以免 Tauri ACL 指向错清单）。
- 改 `gamelife-core`：`/opt/homebrew/bin/cargo test --offline -p gamelife-core`。
- 改 `src/`：`npx vitest run --dir src`。
- 测试禁止新增 `std::env::set_var("HOME", …)`。
- 每个任务一条英文 conventional commit，不 `--no-verify`。
- 工作区里已有无关脏文件（通知崩溃修复、rustfmt 等）。**只 `git add` 本任务 Files 列出的路径。** 不要把 `src-tauri/src/macos/notify.rs` 或其它未列入文件打进这些 commit。

---

## Spec coverage（执行前对照，禁止跳过）

| Spec 条款 | 任务 |
| --- | --- |
| §4 `tasks.notes`，8192 字节，`notes_too_long` | Task 1, 2, 3 |
| §3.2 / §4 快照与 AI 不含 notes；NL 不填备注；复制/续写带备注 | Task 1, 3, 4 |
| §5 详情框、不套娃、失焦/400ms 保存 | Task 8, 9 |
| §5 Markdown 子集、`javascript:` 当纯文本 | Task 5, 9 |
| §6.1 拖动 `select-none` + `preventDefault` | Task 10 |
| §6.2 空心圆勾选 | Task 8, 10, 9 |
| §6.3–6.4 Shift/⌘ 多选、批量移动/放弃 | Task 6, 10 |
| §7 去掉竖条、列缝、小时淡底、块 inset 6px | Task 7, 11 |
| §2 / §7 今日计划列同一详情、不画底色 | Task 12 |
| §3.5 `user_version` 5 | Task 2, 13 |
| §11 测试表 | 各任务步骤 |

---

## File Structure

```
crates/gamelife-core/src/task.rs          Task.notes、MAX_NOTES_BYTES、notes_ok、spawn 抄备注
crates/gamelife-core/src/lib.rs           导出 MAX_NOTES_BYTES、notes_ok
src-tauri/src/db.rs                      user_version 5；SCHEMA / load / ALTER notes
src-tauri/src/commands.rs                TaskView.notes；upsert 校验；persist / duplicate
src/lib/api.ts                           TaskView.notes
src/lib/taskClipboard.ts                 序列化 notes；缺字段 → ''
src/lib/taskNotesMd.ts                   新增：Markdown 子集 → AST
src/lib/taskListSelect.ts                新增：可见顺序、连选、点选
src/lib/taskPointer.ts                   新增：CLICK_SLOP_PX、withinClickSlop
src/lib/slotRibbon.ts                    hourWashCategory
src/lib/taskCalendar.ts                  CAL_DAY_GAP、hitCalendarTs 计入列缝
src/lib/taskBoard.ts                     notes_too_long 文案
src/lib/preview/fixtures.ts              夹具 notes
src/lib/preview/invoke.ts                upsert / duplicate 带 notes
src/components/ui/dialog.tsx             可选 header
src/components/TaskCheckbox.tsx          新增：14px 空心圆
src/components/TaskScheduleFields.tsx    从 TaskDateDialog 抽出开始/结束/重复/提醒
src/components/TaskDateDialog.tsx        改用 TaskScheduleFields
src/components/TaskDetailDialog.tsx      新增：共用详情
src/components/TaskActionMenu.tsx        增加 TaskBulkMenu
src/pages/Tasks.tsx                      列表多选/勾选/拖动；日历缝与底色；单击详情
src/pages/Today.tsx                      计划列单击同一详情
AGENTS.md / CLAUDE.md / .cursor/rules    user_version 5；详情与备注不进判定
```

---

### Task 1: core `Task.notes`，快照仍无备注

**Files:**
- Modify: `crates/gamelife-core/src/task.rs`
- Modify: `crates/gamelife-core/src/lib.rs`（`pub use task::{…}` 增加 `notes_ok`, `MAX_NOTES_BYTES`）
- Test: `crates/gamelife-core/src/task.rs` `mod tests`

**Interfaces:**
- Consumes: 现有 `Task` / `TaskSnapshot` / `spawn_after_complete`
- Produces:
  - `pub const MAX_NOTES_BYTES: usize = 8192;`
  - `pub fn notes_ok(notes: &str) -> bool` — `notes.len() <= MAX_NOTES_BYTES`（Rust `str::len` 是 UTF-8 字节）
  - `Task.notes: String`，`#[serde(default)]`
  - `spawn_after_complete` 把 `done.notes.clone()` 抄到新行
  - `TaskSnapshot` **不加** `notes`

- [ ] **Step 1: Write the failing tests**

在 `task.rs` 测试模块里，`timed()` 暂时不要加 `notes`（让编译红，或先加字段再写断言——本任务按 TDD：先写测试与常量/函数调用）。在现有 `spawn_none_repeat_is_none` 附近追加：

```rust
#[test]
fn notes_ok_is_utf8_bytes() {
    assert!(notes_ok(""));
    assert!(notes_ok(&"a".repeat(8192)));
    assert!(!notes_ok(&"a".repeat(8193)));
}

#[test]
fn new_task_notes_default_empty() {
    let t = timed("a", PRESET_MAINLINE_ID, 0, 1800);
    assert_eq!(t.notes, "");
}

#[test]
fn spawn_after_complete_copies_notes() {
    let mut t = timed("a", PRESET_MAINLINE_ID, 0, 1800);
    t.repeat = RepeatRule::Daily;
    t.notes = "指标 0.91".into();
    let next = spawn_after_complete(&t, "b".into(), 86_400, 1).expect("next");
    assert_eq!(next.notes, "指标 0.91");
    assert_ne!(next.id, t.id);
}

#[test]
fn snapshot_json_has_no_notes_field() {
    let mut open = timed("u", PRESET_MAINLINE_ID, 0, 1800);
    open.notes = "secret-never-judge".into();
    let snaps = snapshots_open(&[open], &lists());
    let json = serde_json::to_string(&snaps).unwrap();
    assert!(!json.contains("secret-never-judge"));
    assert!(!json.contains("notes"));
}

#[test]
fn task_json_missing_notes_deserializes_empty() {
    let t: Task = serde_json::from_str(
        r#"{"id":"a","list_id":"list-mainline","title":"x","done":false,"start":null,"end":null,"range":null}"#,
    )
    .unwrap();
    assert_eq!(t.notes, "");
}
```

`timed()` 和其它 `Task { … }` 字面量（约 `longterm_without_window`、`snapshots_open_includes_unscheduled_and_other_days`）必须补 `notes: String::new()`，否则 Rust 不编译。先写测试、再补字段时一起改字面量。

- [ ] **Step 2: Run test to verify it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core -- notes_ok_is_utf8_bytes --nocapture`

Expected: FAIL，`notes_ok` 未定义（或 `Task` 没有 `notes`）。

- [ ] **Step 3: Write minimal implementation**

`Task`：

```rust
    #[serde(default)]
    pub remind_offsets: Vec<i64>,
    #[serde(default)]
    pub notes: String,
```

```rust
pub const MAX_NOTES_BYTES: usize = 8192;

pub fn notes_ok(notes: &str) -> bool {
    notes.len() <= MAX_NOTES_BYTES
}
```

`spawn_after_complete` 的 `Some(Task { … })` 增加 `notes: done.notes.clone()`。

`snapshot_of` **不要**读 `task.notes`。

`lib.rs` 的 `pub use task::{…}` 加上 `notes_ok, MAX_NOTES_BYTES`。

每个 `Task {` 字面量补 `notes: String::new()`（`timed()` 一处即可覆盖大部分测试）。

- [ ] **Step 4: Run tests to verify they pass**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core`

Expected: PASS（含本任务新测试）。`src-tauri` 此时可能编不过（`TaskView`/`load_tasks` 还没 `notes`）——不要在本任务跑 `-p gamelife`，除非只跑 core。

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: store task notes on Task without pinning them in snapshots

EOF
)"
```

---

### Task 2: SQLite `user_version` 5 与 `tasks.notes`

**Files:**
- Modify: `src-tauri/src/db.rs`（`SCHEMA` 的 `CREATE TABLE tasks`、`TARGET_USER_VERSION`、`migrate`、`load_tasks`、所有 `user_version == 4` 断言、`tag_device_rows` 注释）
- Test: `src-tauri/src/db.rs` `mod tests`

**Interfaces:**
- Consumes: Task 1 的 `Task.notes`
- Produces: 新库与旧库 migrate 后 `PRAGMA user_version = 5`；`tasks.notes TEXT NOT NULL DEFAULT ''`；`load_tasks` 填 `notes`

- [ ] **Step 1: Write the failing test**

把 `migrate_new_db_sets_user_version_4` 改名为 `migrate_new_db_sets_user_version_5`，断言改为 `5`。新增：

```rust
#[test]
fn migrate_adds_task_notes_and_sets_version_5() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE tasks (
           id TEXT PRIMARY KEY, list_id TEXT, title TEXT, done INTEGER,
           start INTEGER, end INTEGER, range TEXT,
           sort INTEGER NOT NULL DEFAULT 0,
           repeat TEXT NOT NULL DEFAULT 'none',
           remind_json TEXT NOT NULL DEFAULT '[]'
         );
         INSERT INTO tasks (id, list_id, title, done, start, end, range)
         VALUES ('a','list-mainline','x',0,NULL,NULL,NULL);
         PRAGMA user_version = 4;",
    )
    .unwrap();
    crate::db::migrate(&conn).unwrap();
    assert_eq!(user_version(&conn), 5);
    let names = column_names(&conn, "tasks");
    assert!(names.iter().any(|c| c == "notes"));
    let loaded = load_tasks(&conn).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].notes, "");
}
```

现有 `migrate_adds_task_sort_repeat_remind_and_sets_version_4` 末尾断言改为 `5`，并 `assert_eq!(loaded[0].notes, "")`。

所有 `assert_eq!(user_version(&conn), 4)` 改为 `5`（约 6 处，含 v1 愿望迁移、Wave1 旧库、device_id 打标）。

- [ ] **Step 2: Run test to verify it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife -- db::tests::migrate_adds_task_notes_and_sets_version_5 --nocapture`

Expected: FAIL（`TARGET_USER_VERSION` 仍为 4，或 `load_tasks` 没有 `notes` 列）。若因 Task 1 的 `Task.notes` 导致 `load_tasks` 已无法编译，这就是红灯。

- [ ] **Step 3: Write minimal implementation**

`TARGET_USER_VERSION: i32 = 5`。

`SCHEMA` 的 `CREATE TABLE tasks` 在 `remind_json` 后加：

```sql
  notes TEXT NOT NULL DEFAULT ''
```

`migrate`：

```rust
    if version < 4 {
        add_column_if_missing(conn, "tasks", "sort", "INTEGER NOT NULL DEFAULT 0")?;
        add_column_if_missing(conn, "tasks", "repeat", "TEXT NOT NULL DEFAULT 'none'")?;
        add_column_if_missing(conn, "tasks", "remind_json", "TEXT NOT NULL DEFAULT '[]'")?;
    }
    if version < 5 {
        add_column_if_missing(conn, "tasks", "notes", "TEXT NOT NULL DEFAULT ''")?;
    }
    if version < TARGET_USER_VERSION {
        conn.pragma_update(None, "user_version", TARGET_USER_VERSION)
            .map_err(map_rusqlite)?;
    }
```

`load_tasks` 的 SELECT 末尾加 `notes`，映射 `notes: r.get(10)?`。

`tag_device_rows` 注释改为：task 列 `sort` / `repeat` / `remind_json` 把版本推到 4；`notes` 推到 5。`device_id` 仍不是版本号。

- [ ] **Step 4: Run tests to verify they pass**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife -- db::`

Expected: `db::` 测试 PASS。`commands.rs` 里 `Task {` / `TaskView {` 若还没 `notes`，整包可能编不过——下一任务立刻补。若本步已经编不过 `commands`，先不要 commit 半截：把 Task 3 的 struct 字段也在本步最小补上（`notes: String::new()` 与 `TaskView.notes` 默认 `""`），校验仍留到 Task 3。

优先：本任务只改 `db.rs`。若 `cargo test -p gamelife -- db::` 因其它模块编译失败，立即做 Task 3 Step 3 的字段补齐（仍不写 `notes_too_long` 校验），再跑 `db::`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db.rs
git commit -m "$(cat <<'EOF'
feat: migrate tasks.notes and bump sqlite user_version to 5

EOF
)"
```

若为了编译把 `commands.rs` 的字段也动了，一并 add，commit message 仍只说 migrate；校验逻辑不要混进来。

---

### Task 3: upsert 拒绝超长备注；duplicate / 续写抄 notes

**Files:**
- Modify: `src-tauri/src/commands.rs`（`TaskView`、`task_to_view`、`view_to_task`、`persist_task`、`upsert_task_in`、`sample_view`、所有 `Task {` / `TaskView {` 字面量）
- Test: `src-tauri/src/commands.rs` `mod tests`

**Interfaces:**
- Consumes: `notes_ok` / `MAX_NOTES_BYTES`；`load_tasks` 已有 `notes`
- Produces: `TaskView.notes: String`（`#[serde(default)]`，camelCase `notes`）；`upsert_task_in` 超长 → `DbOpError::Rejected("notes_too_long")`；`duplicate_task_in` 因 `clone` 自动带 notes；完成后续写因 core spawn 自动带 notes

- [ ] **Step 1: Write the failing tests**

扩展 `sample_view`：

```rust
    fn sample_view(id: &str, title: &str) -> TaskView {
        TaskView {
            id: id.into(),
            list_id: PRESET_MAINLINE_ID.into(),
            title: title.into(),
            done: false,
            start: None,
            end: None,
            range: None,
            sort: 0,
            repeat: "none".into(),
            remind_offsets: vec![],
            notes: String::new(),
        }
    }
```

追加：

```rust
    #[test]
    fn upsert_rejects_notes_over_8192_bytes() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let mut ok = sample_view("t1", "x");
        ok.notes = "a".repeat(8192);
        upsert_task_in(&conn, ok).unwrap();
        let mut bad = sample_view("t2", "y");
        bad.notes = "a".repeat(8193);
        let err = upsert_task_in(&conn, bad).unwrap_err();
        assert_eq!(err, DbOpError::Rejected("notes_too_long".into()));
    }

    #[test]
    fn duplicate_task_copies_notes() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let mut src = sample_view("src", "x");
        src.notes = "地点：A301".into();
        upsert_task_in(&conn, src).unwrap();
        let copy = duplicate_task_in(&conn, "src").unwrap();
        assert_ne!(copy.id, "src");
        assert_eq!(copy.notes, "地点：A301");
        assert!(!copy.done);
    }
```

改 `completing_daily_inserts_one_future`：persist 的 `Task` 设 `notes: "指标".into()`，断言 `spawned.notes == "指标"`。

- [ ] **Step 2: Run test to verify it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife -- commands::tests::upsert_rejects_notes_over_8192_bytes --nocapture`

Expected: FAIL（尚未校验，或字段还没有）。

- [ ] **Step 3: Write minimal implementation**

`TaskView`：

```rust
    #[serde(default)]
    pub remind_offsets: Vec<i64>,
    #[serde(default)]
    pub notes: String,
```

`task_to_view` / `view_to_task` 抄 `notes`。

`persist_task` 的 INSERT/UPDATE 增加 `notes`：

```sql
INSERT INTO tasks (id, list_id, title, done, start, end, range, sort, repeat, remind_json, notes)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
ON CONFLICT(id) DO UPDATE SET
  ...
  remind_json=excluded.remind_json,
  notes=excluded.notes
```

`upsert_task_in` 在 `remind_offsets_ok` 附近：

```rust
    if !notes_ok(&stored.notes) {
        return Err(DbOpError::Rejected("notes_too_long".into()));
    }
```

`use gamelife_core::{…, notes_ok}`（或现有 import 列表追加）。

所有 `Task {` / `TaskView {` 字面量补 `notes`。

不要截断字符串。

- [ ] **Step 4: Run tests to verify they pass**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife`

Expected: PASS（整包，含 db + commands）。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
feat: reject oversized task notes and copy them on duplicate

EOF
)"
```

---

### Task 4: 前端 `TaskView.notes`、剪贴板、预览、错误文案

**Files:**
- Modify: `src/lib/api.ts`
- Modify: `src/lib/taskClipboard.ts`
- Modify: `src/lib/taskClipboard.test.ts`
- Modify: `src/lib/taskSort.test.ts`（helper 补 `notes: ""`）
- Modify: `src/lib/taskBoard.ts` / `src/lib/taskBoard.test.ts`
- Modify: `src/lib/preview/fixtures.ts`
- Modify: `src/lib/preview/invoke.ts`（upsert 类型与 `notes: task.notes ?? ""`；duplicate 抄 `notes`）
- Modify: `src/pages/Tasks.tsx`（自然语言 `upsertTask` 与日历 preview 占位对象加 `notes: ""`）
- Modify: `src/pages/Today.tsx`（`viewForDayTask` 加 `notes: ""`）

**Interfaces:**
- Consumes: 后端 `notes` camelCase
- Produces: `TaskView.notes: string`；旧剪贴板缺字段 → `""`；`taskCommandError("notes_too_long")` → `备注最长 8192 字节。`

- [ ] **Step 1: Write the failing tests**

`taskClipboard.test.ts` 的 `sample` 加 `notes: "会 A301"`。现有 roundtrip 增加 `expect(back?.notes).toBe("会 A301")`。追加：

```ts
  it("treats a missing notes field as empty", () => {
    const raw = serializeTaskCopy(sample);
    const stripped = raw.replace(/,"notes":"会 A301"/, "");
    expect(parseTaskCopy(stripped)?.notes).toBe("");
  });
```

`taskBoard.test.ts`：

```ts
  it("maps notes_too_long", () => {
    expect(taskCommandError("notes_too_long")).toBe("备注最长 8192 字节。");
  });
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/taskClipboard lib/taskBoard`

Expected: FAIL（`notes` 不在类型上，或 parse 没有默认，或文案未映射）。

- [ ] **Step 3: Write minimal implementation**

`api.ts` `TaskView` 加 `notes: string`。

`taskClipboard.ts`：`parseTaskCopy` 在 `isTaskView` 通过后：

```ts
    const notes = typeof (parsed as TaskView).notes === "string" ? (parsed as TaskView).notes : "";
    return { ...(parsed as TaskView), notes };
```

`isTaskView` 不要要求 `notes` 存在（兼容旧剪贴板）。

`taskCommandError` 增加 `case "notes_too_long": return "备注最长 8192 字节。";`

夹具两行任务加 `notes: ""`（第二条可写一句短备注方便预览，例如 `"地点：A301"`）。`invoke.ts` upsert 补 `notes: task.notes ?? ""`；`duplicate_task` 的 copy 带 `notes: src.notes`。

页面里所有 `TaskView` 字面量加 `notes: ""`，否则 `tsc` 红。本任务不改点击打开详情。

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run --dir src`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/lib/api.ts src/lib/taskClipboard.ts src/lib/taskClipboard.test.ts src/lib/taskSort.test.ts src/lib/taskBoard.ts src/lib/taskBoard.test.ts src/lib/preview/fixtures.ts src/lib/preview/invoke.ts src/pages/Tasks.tsx src/pages/Today.tsx
git commit -m "$(cat <<'EOF'
feat: plumb task notes through the view model and clipboard

EOF
)"
```

---

### Task 5: 备注 Markdown 子集

**Files:**
- Create: `src/lib/taskNotesMd.ts`
- Create: `src/lib/taskNotesMd.test.ts`

**Interfaces:**
- Consumes: 无
- Produces:

```ts
export type NotesInline =
  | { type: "text"; value: string }
  | { type: "strong"; value: string }
  | { type: "link"; text: string; href: string };

export type NotesBlock =
  | { type: "p"; children: NotesInline[] }
  | { type: "ul"; items: NotesInline[][] }
  | { type: "ol"; items: NotesInline[][] };

export function parseTaskNotes(src: string): NotesBlock[];
export function notesByteLength(src: string): number; // TextEncoder UTF-8
```

规则（与 spec §5 一致，写进测试不要写进模糊注释）：

- 空行分隔块。连续 `- ` 行 → `ul`；连续 `/^\d+\.\s/` 行 → `ol`；其它非空行每行一个 `p`（单换行即新段）。
- 行内从左到右：`[text](url)` 仅当 `url` 以 `http://` 或 `https://` 开头才产出 `link`，否则整段当 `text`（含 `javascript:`）；`**bold**` 成对才产出 `strong`。
- 不产出 HTML、图片、标题、行内代码节点；`#` / `` ` `` / 裸 HTML 当普通文本。
- 不引入 npm 依赖。

- [ ] **Step 1: Write the failing test**

```ts
import { describe, expect, it } from "vitest";
import { notesByteLength, parseTaskNotes } from "./taskNotesMd";

describe("parseTaskNotes", () => {
  it("parses paragraphs, bold, lists, and https links", () => {
    const blocks = parseTaskNotes(
      "见 **指标**\n\n- 地点 A301\n- 带[纪要](https://example.com/a)\n\n1. 先开会",
    );
    expect(blocks).toEqual([
      { type: "p", children: [{ type: "text", value: "见 " }, { type: "strong", value: "指标" }] },
      {
        type: "ul",
        items: [
          [{ type: "text", value: "地点 A301" }],
          [
            { type: "text", value: "带" },
            { type: "link", text: "纪要", href: "https://example.com/a" },
          ],
        ],
      },
      { type: "ol", items: [[{ type: "text", value: "先开会" }]] },
    ]);
  });

  it("keeps javascript URLs as plain text", () => {
    const blocks = parseTaskNotes("[x](javascript:alert(1))");
    expect(blocks).toEqual([
      { type: "p", children: [{ type: "text", value: "[x](javascript:alert(1))" }] },
    ]);
  });

  it("does not emit heading or html nodes", () => {
    const blocks = parseTaskNotes("# 标题\n\n<img src=x>");
    expect(blocks.every((b) => b.type === "p")).toBe(true);
    expect(JSON.stringify(blocks)).not.toMatch(/"type":"html"/);
  });
});

describe("notesByteLength", () => {
  it("counts utf-8 bytes", () => {
    expect(notesByteLength("a".repeat(8192))).toBe(8192);
    expect(notesByteLength("你")).toBe(3);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/taskNotesMd`

Expected: FAIL，模块不存在。

- [ ] **Step 3: Write minimal implementation**

实现 `parseTaskNotes` / `notesByteLength`。链接扫描用正则 `\ [([^\]]+)\]\(([^)]+)\)`，`href` 必须 `startsWith("http://") || startsWith("https://")`。粗体 `/\*\*([^*]+)\*\*/`。先切块再切行内。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src lib/taskNotesMd`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/lib/taskNotesMd.ts src/lib/taskNotesMd.test.ts
git commit -m "$(cat <<'EOF'
feat: parse a safe markdown subset for task notes

EOF
)"
```

---

### Task 6: 列表可见顺序与 Shift/⌘ 选择

**Files:**
- Create: `src/lib/taskListSelect.ts`
- Create: `src/lib/taskListSelect.test.ts`
- Create: `src/lib/taskPointer.ts`
- Create: `src/lib/taskPointer.test.ts`

**Interfaces:**
- Consumes: 无
- Produces:

```ts
export function visibleTaskIds(
  listIds: string[],
  tasksByList: Map<string, { id: string }[]>,
  collapsed: string[],
): string[];

export function rangeSelect(
  visible: string[],
  anchorId: string | null,
  targetId: string,
): string[];
// 无锚点、或锚点/目标不在 visible → [targetId]
// 否则 visible 上两下标闭区间（含端点），折叠组已不在 visible 里故自动打断「中间」

export function toggleSelect(ids: string[], id: string): string[];
// 有则删，无则加；不排序

export const CLICK_SLOP_PX = 4;
export function withinClickSlop(dx: number, dy: number): boolean;
// Math.hypot(dx, dy) <= CLICK_SLOP_PX
```

- [ ] **Step 1: Write the failing tests**

```ts
import { describe, expect, it } from "vitest";
import { rangeSelect, toggleSelect, visibleTaskIds } from "./taskListSelect";

describe("visibleTaskIds", () => {
  it("walks expanded groups in list order and skips collapsed", () => {
    const map = new Map([
      ["g1", [{ id: "a" }, { id: "b" }]],
      ["g2", [{ id: "c" }]],
      ["g3", [{ id: "d" }]],
    ]);
    expect(visibleTaskIds(["g1", "g2", "g3"], map, ["g2"])).toEqual(["a", "b", "d"]);
  });
});

describe("rangeSelect", () => {
  const vis = ["a", "b", "d"];
  it("selects the closed visible range", () => {
    expect(rangeSelect(vis, "a", "d")).toEqual(["a", "b", "d"]);
  });
  it("selects only the target when there is no anchor", () => {
    expect(rangeSelect(vis, null, "d")).toEqual(["d"]);
  });
});

describe("toggleSelect", () => {
  it("adds and removes", () => {
    expect(toggleSelect(["a"], "b")).toEqual(["a", "b"]);
    expect(toggleSelect(["a", "b"], "a")).toEqual(["b"]);
  });
});
```

```ts
import { describe, expect, it } from "vitest";
import { CLICK_SLOP_PX, withinClickSlop } from "./taskPointer";

describe("withinClickSlop", () => {
  it("treats four pixels as a click and five as a drag", () => {
    expect(CLICK_SLOP_PX).toBe(4);
    expect(withinClickSlop(0, 4)).toBe(true);
    expect(withinClickSlop(3, 4)).toBe(false);
  });
});
```

`hypot(3,4)=5>4` → false；`hypot(0,4)=4` → true。

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/taskListSelect lib/taskPointer`

Expected: FAIL，模块不存在。

- [ ] **Step 3: Write minimal implementation**

按上面签名实现。`rangeSelect` 用 `visible.indexOf`；任一 `< 0` 则返回 `[targetId]`；`from = min(i,j)`, `to = max(i,j)`，`visible.slice(from, to + 1)`。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src lib/taskListSelect lib/taskPointer`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/lib/taskListSelect.ts src/lib/taskListSelect.test.ts src/lib/taskPointer.ts src/lib/taskPointer.test.ts
git commit -m "$(cat <<'EOF'
feat: add list range-select and click-slop helpers

EOF
)"
```

---

### Task 7: 小时淡底类别与日历列缝命中

**Files:**
- Modify: `src/lib/slotRibbon.ts`
- Modify: `src/lib/slotRibbon.test.ts`
- Modify: `src/lib/taskCalendar.ts`
- Modify: `src/lib/taskCalendar.test.ts`

**Interfaces:**
- Consumes: `ribbonCells` 仍返回 96 格 `CategoryKey | null`（内部可留着；UI 下一任务不再渲染竖条）
- Produces:

```ts
export function hourWashCategory(
  cells: (CategoryKey | null)[],
  hour: number, // 0–23
): CategoryKey | null;
```

该小时 4 格：忽略 `null` 与 `"unobserved"`；其余众数；平手取 **counted 序列里先出现** 的那一类（即更早的格）；四个都忽略 → `null`。

```ts
export const CAL_DAY_GAP = 10;
```

`hitCalendarTs`：日列实际宽度 = `(grid.width - CAL_GUTTER - CAL_DAY_GAP * (days.length - 1)) / days.length`。从左往右走列宽 + 缝；落在缝里时按缝中点归到较近的一列（左半上一列，右半下一列）。

- [ ] **Step 1: Write the failing tests**

`slotRibbon.test.ts` 追加：

```ts
import { hourWashCategory } from "./slotRibbon";
import type { CategoryKey } from "./theme";

describe("hourWashCategory", () => {
  const n = (v: CategoryKey | null) => v;

  it("returns null when the hour is empty or unobserved", () => {
    const cells = Array.from({ length: 96 }, () => null as CategoryKey | null);
    cells[0] = "unobserved";
    cells[1] = "unobserved";
    cells[2] = null;
    cells[3] = null;
    expect(hourWashCategory(cells, 0)).toBeNull();
  });

  it("takes the majority of the four quarter-hour cells", () => {
    const cells = Array.from({ length: 96 }, () => null as CategoryKey | null);
    cells[0] = "mainline";
    cells[1] = "mainline";
    cells[2] = "mainline";
    cells[3] = "entertainment";
    expect(hourWashCategory(cells, 0)).toBe("mainline");
  });

  it("breaks ties toward the earlier cell", () => {
    const cells = Array.from({ length: 96 }, () => null as CategoryKey | null);
    cells[4] = "entertainment";
    cells[5] = "mainline";
    cells[6] = "entertainment";
    cells[7] = "mainline";
    expect(hourWashCategory(cells, 1)).toBe("entertainment");
  });
});
```

`taskCalendar.test.ts` 追加（先 `import { CAL_DAY_GAP, CAL_GUTTER, hitCalendarTs }`）：

```ts
describe("hitCalendarTs", () => {
  it("accounts for the gutter and the 10px gaps between days", () => {
    const days = ["2026-09-14", "2026-09-15", "2026-09-16"];
    const col = 100;
    const width = CAL_GUTTER + col * 3 + CAL_DAY_GAP * 2;
    const grid = { left: 0, top: 0, width, scrollTop: 0 };
    const midDay1 =
      CAL_GUTTER + col + CAL_DAY_GAP + col / 2;
    const ts = hitCalendarTs(days, midDay1, 18, grid);
    expect(ts).not.toBeNull();
    // hour 0, halfway down 36px row → not day 0
    const day0 = hitCalendarTs(days, CAL_GUTTER + 10, 18, grid);
    expect(day0).not.toBeNull();
    expect(ts).toBeGreaterThan(day0 as number);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/slotRibbon lib/taskCalendar`

Expected: FAIL，`hourWashCategory` / `CAL_DAY_GAP` 不存在，或 `hitCalendarTs` 仍按无缝均分。

- [ ] **Step 3: Write minimal implementation**

`hourWashCategory`：`const slice = cells.slice(hour * 4, hour * 4 + 4)`；过滤；计数；`max` 票；平手时 `counted.find` 第一个达到 `max` 的 key。

`hitCalendarTs` 用 `CAL_DAY_GAP`。`x < CAL_GUTTER` 仍可命中？现逻辑 `x < 0` 才 null，gutter 内 `floor` 会落到 day 0——保持：若 `x < CAL_GUTTER` 当作第一列（与现在一致），不要在本任务改 gutter 语义。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src lib/slotRibbon lib/taskCalendar`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/lib/slotRibbon.ts src/lib/slotRibbon.test.ts src/lib/taskCalendar.ts src/lib/taskCalendar.test.ts
git commit -m "$(cat <<'EOF'
feat: derive calendar hour wash and hit-test column gaps

EOF
)"
```

---

### Task 8: 空心圆勾选、Dialog 顶栏槽、抽出改期字段

**Files:**
- Create: `src/components/TaskCheckbox.tsx`
- Create: `src/components/TaskScheduleFields.tsx`
- Modify: `src/components/ui/dialog.tsx`
- Modify: `src/components/TaskDateDialog.tsx`（改用 `TaskScheduleFields`，行为不变）

**Interfaces:**
- Consumes: `categoryColor`；现有 `Form` 状态（开始/结束日与时刻、重复、提醒）
- Produces:
  - `TaskCheckbox({ checked, color, label, onToggle })` — `role="checkbox"`，约 `size-[14px] rounded-full border-2`，`borderColor`/`background`（完成时）= `color`（调用方传入 `roleDot` / `categoryColor`）。完成对勾颜色 `hsl(var(--background))`。`onPointerDown` `stopPropagation`，避免列表拖动。无 hex。
  - `DialogProps.header?: ReactNode`。有 `header` 时不渲染默认标题行；仍设 `aria-labelledby`；用 `h2.sr-only` 放 `title`（无障碍标题）。关闭按钮由 `header` 自己带，或当无 `header` 时保持现有关闭钮。
  - `TaskScheduleFields`：接收 `form` / `setForm` / `disabled`，渲染开始、结束、提醒、重复（从 `TaskDateDialog` 原样搬 `Row` 与提醒按钮）。`TaskDateDialog` 仍有清除/确定 footer。

本任务不改产品行为，只抽结构。没有 RTL；用现有 vitest 回归。

- [ ] **Step 1: Write the failing test**

没有新单测文件。先改 `dialog.tsx` 类型：增加 `header?: ReactNode`。跑现有前端测试确认仍绿，再实现组件——若你要严格 TDD：在 `taskPointer` 已覆盖 slop 的前提下，本任务以「抽出后 `npx vitest run --dir src` 仍 PASS」为门。不要为 checkbox 引入 testing-library。

- [ ] **Step 2: Confirm baseline still passes**

Run: `npx vitest run --dir src`

Expected: PASS（尚未改行为）。

- [ ] **Step 3: Write minimal implementation**

`TaskCheckbox`：`<button type="button" role="checkbox" aria-checked={checked} aria-label={label} className="relative size-[14px] shrink-0 rounded-full border-2" style={{ borderColor: color, background: checked ? color : "transparent" }}>`。对勾用 lucide `Check`，`className="size-2.5"`，`style={{ color: "hsl(var(--background))" }}`（token，不是 hex）。

`Dialog`：`header` 有值时：

```tsx
        {header ? (
          <>
            <h2 id={titleId} className="sr-only">
              {title}
            </h2>
            {header}
          </>
        ) : (
          /* 现有标题行 */
        )}
```

`footer` 的默认 `justify-end` 保留；详情框下一任务会自己排底栏。

把 `TaskDateDialog` 里提醒 + 重复 + 两个 `Row` 挪到 `TaskScheduleFields`。`id` 仍用 `task-date-*` 以免改期可访问性回退。

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run --dir src`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/components/TaskCheckbox.tsx src/components/TaskScheduleFields.tsx src/components/ui/dialog.tsx src/components/TaskDateDialog.tsx
git commit -m "$(cat <<'EOF'
feat: add circle checkbox and reusable task schedule fields

EOF
)"
```

---

### Task 9: `TaskDetailDialog`

**Files:**
- Create: `src/components/TaskDetailDialog.tsx`
- Modify: `src/components/TaskActionMenu.tsx`（详情「…」仍用现有单条菜单；本任务只在详情内挂上）

**Interfaces:**
- Consumes: `TaskView`、`TaskListView[]`、`upsertTask`、`toggleTaskDone`、`moveTask`、`TaskCheckbox`、`TaskScheduleFields`、`parseTaskNotes`、`notesByteLength`、`taskCommandError`
- Produces:

```tsx
export function TaskDetailDialog({
  task,
  lists,
  onClose,
  onSaved,
  onError,
  onAbandon,
}: {
  task: TaskView | null;
  lists: TaskListView[];
  onClose: () => void;
  onSaved: () => Promise<void> | void;
  onError: (message: string) => void;
  onAbandon: (task: TaskView) => void;
}): JSX.Element;
```

`task == null` 时 `Dialog open={false}`。`key={task.id}` 包 body。

布局：

- `header`：左 `TaskCheckbox`（`color` 来自该 `listId` 的 role → `categoryColor`，映射与 `Tasks.tsx` `roleDot` 相同：mainline / longterm / chore→admin / 其余 side）；中 `TaskScheduleFields`（可折行，小字号）；右关闭（现有 X 按钮）。无障碍 `title="任务详情"`。
- 名称：`Input`，失焦 `upsertTask({ ...task, title })`。空标题走现有 `empty_title`。
- 备注：`Textarea` 编辑；下方把 `parseTaskNotes` 渲成 `<p>` / `<ul>` / `<ol>` / `<strong>` / `<a href target="_blank" rel="noreferrer">`。**禁止** `dangerouslySetInnerHTML`。停按 400ms 或失焦后 upsert。超长（`notesByteLength > 8192`）不截断，调用 `onError(taskCommandError("notes_too_long"))`，不发请求。
- 底栏左：分组 `Select`（`lists`），`onChange` → `moveTask` 或 `upsert` 换 `listId`。
- 底栏右：`…` `Button`，在按钮 `getBoundingClientRect()` 处打开 `TaskActionMenu`。`onDate`：**不**打开 `TaskDateDialog`，改为 `document.getElementById("task-detail-schedule")?.focus()`（给 `TaskScheduleFields` 最外层加 `id="task-detail-schedule" tabIndex={-1}`）。`onAbandon`：先 `onClose()` 再 `onAbandon(task)`。`onDuplicate` 调 `duplicateTask` 然后 `onSaved`。

改期字段变更：与备注一样 400ms debounce upsert（含 `start`/`end`/`repeat`/`remindOffsets`）。「清除」调用 `rescheduleTask(id, null, null)`。

不要在详情里再挂一个 `Dialog`。`className` 加宽，如 `max-w-lg`。

本任务先不接入页面（页面仍编译）。若 `tsc` 因未使用告警，从 `Tasks.tsx` 先挂上但 `detailTask={null}` 也可——优先完整组件、下一任务接线。

- [ ] **Step 1: Write the failing test**

无 RTL。门是组件文件存在且 `npx vitest run --dir src` 绿。可在 `taskNotesMd.test.ts` 已覆盖解析。实现时把链接渲成 `<a>`。

- [ ] **Step 2: Confirm parser tests still pass**

Run: `npx vitest run --dir src lib/taskNotesMd`

Expected: PASS。

- [ ] **Step 3: Write minimal implementation**

按上面布局写 `TaskDetailDialog`。role 色抽一个 4 行函数放在组件文件内（或 `taskBoard.roleDotColor`）——**不要**复制 hex。debounce 用 `useRef<number>` + `setTimeout` 400，unmount 时 `clearTimeout`。

`TaskScheduleFields` 根节点：`id="task-detail-schedule"` 仅当详情使用时传入 `id` prop，避免改期对话框出现重复 id。给 `TaskScheduleFields` 加可选 `id?: string`。

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run --dir src`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/components/TaskDetailDialog.tsx src/components/TaskActionMenu.tsx src/components/TaskScheduleFields.tsx
git commit -m "$(cat <<'EOF'
feat: add the shared task detail dialog

EOF
)"
```

---

### Task 10: 任务页列表 — 勾选、拖动不选字、单击详情、多选

**Files:**
- Modify: `src/pages/Tasks.tsx`
- Modify: `src/components/TaskActionMenu.tsx`（增加 `TaskBulkMenu`）

**Interfaces:**
- Consumes: Task 6 helpers、`TaskCheckbox`、`TaskDetailDialog`、`withinClickSlop`
- Produces: 列表行为符合 spec §6

状态：

```ts
const [selectedIds, setSelectedIds] = useState<string[]>([]);
const [selectAnchor, setSelectAnchor] = useState<string | null>(null);
const [detailTask, setDetailTask] = useState<TaskView | null>(null);
```

`visibleIds = visibleTaskIds(lists.map(l => l.id), tasksByList, collapsed)`。

列表容器：`listDrag` 非空时加 `select-none`。任务行 `onPointerDown`（非勾选）`preventDefault()`。

`beginListDrag` 的 `onUp` 在 `!armed` 时（现在直接 return）：

- 目标是 checkbox → 不改选中、不打开详情（勾选框自己 `toggleTaskDone`）。
- `shiftKey` → `setSelectedIds(rangeSelect(visibleIds, selectAnchor, task.id))`；`setDetailTask(null)`；若无锚点则 `setSelectAnchor(task.id)`。
- `metaKey || ctrlKey` → `toggleSelect`；更新锚点为这一条；`length !== 1` 时 `setDetailTask(null)`；减到 1 **不**自动打开。
- 否则 → `setSelectedIds([task.id])`；`setSelectAnchor(task.id)`；`setDetailTask(task)`；`setDateTask(null)`（防套娃）。

`armed` 开始时：`setSelectedIds([])`；`setDetailTask(null)`。拖排序仍只动抓住的那一条。

高亮：`selectedIds.includes(task.id)` → `bg-accent/40`。

勾选：换成 `TaskCheckbox`，`color={roleDot(list.role)}`。

点列表空白：列表 `Card` 内 `onPointerDown`，若 `event.target === event.currentTarget`（或点在分组以外的垫底）则清空 `selectedIds` 并关详情。切换 3/7 天（现有 `setCalDays`）同样清空并关详情。

右键：若 `selectedIds.length > 1 && selectedIds.includes(task.id)` → 打开 `TaskBulkMenu`，否则先 `setSelectedIds([task.id])` 再走现有 `TaskActionMenu`。

`TaskBulkMenu`：只要「移动到 ▸」（`lists` 全列出）和「放弃任务」。`onMove(listId)`：对 `selectedIds` 逐条 `moveTask`，已在该组则跳过。`onAbandon`：关菜单，打开现有确认框，文案 `放弃后不可恢复。确定放弃 N 条任务？`，确认后逐条 `deleteTask`。不要批量改期/勾选/复制。

详情打开时 `TaskDetailDialog` 的 `onAbandon` 关详情再设 `abandonTask`。`onDate` 已在详情内聚焦时间区；列表右键「更改日期」仍 `openDateDialog`，但若 `detailTask` 已打开则只聚焦、不挂第二个 Dialog。

`submitLine` 的 `upsertTask` 已有 `notes: ""`（Task 4）。

- [ ] **Step 1: Write the failing test**

选择逻辑已在 Task 6 测完。本任务以实现后 `npx vitest run --dir src` 为门。不要为 `Tasks.tsx` 加 RTL。

- [ ] **Step 2: Run helper tests (still the spec’s selection contract)**

Run: `npx vitest run --dir src lib/taskListSelect lib/taskPointer`

Expected: PASS。

- [ ] **Step 3: Write minimal implementation**

按上面改 `Tasks.tsx`。`TaskBulkMenu` 放在 `TaskActionMenu.tsx`：

```tsx
export function TaskBulkMenu({
  open, x, y, count, lists, onClose, onMove, onAbandon,
}: {
  open: boolean;
  x: number;
  y: number;
  count: number;
  lists: TaskListView[];
  onClose: () => void;
  onMove: (listId: string) => void;
  onAbandon: () => void;
}) {
  return (
    <ContextMenu open={open} x={x} y={y} onClose={onClose}>
      <ContextMenuSub label="移动到">
        {lists.map((list) => (
          <ContextMenuItem key={list.id} onSelect={() => onMove(list.id)}>
            {list.name}
          </ContextMenuItem>
        ))}
      </ContextMenuSub>
      <ContextMenuItem destructive onSelect={onAbandon}>
        放弃任务
      </ContextMenuItem>
    </ContextMenu>
  );
}
```

放弃确认：`abandonIds: string[]`（单条时 length 1，文案继续带标题；多条用 N）。

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run --dir src`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/pages/Tasks.tsx src/components/TaskActionMenu.tsx
git commit -m "$(cat <<'EOF'
feat: open task details from the list and support bulk select

EOF
)"
```

---

### Task 11: 任务页日历 — 去掉竖条，列缝 + 小时淡底，单击详情

**Files:**
- Modify: `src/pages/Tasks.tsx`（`TaskCalendar` / `CalendarDayColumn` / `beginCalDrag`）

**Interfaces:**
- Consumes: `hourWashCategory`、`CAL_DAY_GAP`、`withinClickSlop`、`categoryColorAt`、`bg-hour-line`
- Produces: spec §7 的日历外观与单击打开详情

删除 `absolute right-0 w-1` 的 96 格竖条。`ribbonCells` 仍可用于生成 96 格再交给 `hourWashCategory`（不要删函数）。

表头行与格子行都要在日列之间插入 `w-[10px] shrink-0` 的缝（`CAL_DAY_GAP`），缝正中 `absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-hour-line`。**第一列与小时槽之间不加缝。**

`CalendarDayColumn`：

- 小时淡底：`hours.map`，`pointer-events-none absolute inset-x-0`，`top: h * CAL_HOUR_H`，`height: CAL_HOUR_H`，`background: wash ? categoryColorAt(wash, 10) : undefined`。`wash = hourWashCategory(cells, h)`。
- 任务块相对列左右各 inset 6px：包一层 `absolute inset-0` 的底色，块层 `left/right` 加 6px。例如 `left: calc(${(mark.lane / lanes) * 100}% + 6px)`，`width: calc(${(span / lanes) * 100}% - 12px - 4px)`（保留原 4px 泳道间隙）。底色必须比块宽，从两侧露出。
- 今天列现有 `categoryColorAt("mainline", 8)` 整列底可保留在表头；格子里小时淡底叠在上面。不要把整列再刷一层挡住小时色。

单击 vs 拖动：拉边命中上下 `h-1.5`（6px）仍立刻 `beginCalDrag(..., edge)`，不打开详情。块主体 `pointerdown` **不要**立刻 `setCalDrag`：记下坐标；`pointermove` 超过 slop 再 `beginCalDrag`；`pointerup` 且未 armed、无 Shift/⌘/Ctrl → `setDetailTask(task)`（日历无多选）。空列点击不选任务。

今日列不在本任务改。

- [ ] **Step 1: Write the failing test**

`hourWashCategory` / `hitCalendarTs` 已在 Task 7。本任务接线。

- [ ] **Step 2: Re-run calendar helpers**

Run: `npx vitest run --dir src lib/slotRibbon lib/taskCalendar lib/taskPointer`

Expected: PASS。

- [ ] **Step 3: Write minimal implementation**

改 `TaskCalendar` 的表头 `days.map` 与 grid 的 `days.map`，用同一套缝。抽小组件 `function DayColumnGap()` 避免表头/格子不一致。

`beginCalDrag`：非 edge 时延迟到 slop；edge 保持立即。`pointerup` 无移动则打开详情。

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run --dir src`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/pages/Tasks.tsx
git commit -m "$(cat <<'EOF'
feat: replace the calendar ribbon with hour wash and column gaps

EOF
)"
```

---

### Task 12: 今日计划列打开同一详情

**Files:**
- Modify: `src/pages/Today.tsx`

**Interfaces:**
- Consumes: `TaskDetailDialog`、`withinClickSlop`
- Produces: 计划列几乎无移动的 pointerup 打开同一详情；右键、拉边、复制、改期对话框保持；**不**画小时底色；**不**新建任务；无多选

`PlanColumn`（或今日里画块的那段）`dragRef` 增加 `originX` / `originY` / `edge?: "start" | "end"`。`onUp`：若有 edge，现逻辑不变。若 `withinClickSlop(e.clientX - originX, e.clientY - originY)` 且 `current.start === drag.start && current.end === drag.end` → `onOpenDetail(task)`，不要 `onMovePlan`。超过 slop 仍改期。

页面状态 `detailTask: TaskView | null`。挂 `TaskDetailDialog`，`lists={data.lists}`，`onSaved` 刷新今日。打开详情时 `setDateTask(null)`。从右键「更改日期」来的 `TaskDateDialog` 仍可用；若详情已开则与任务页一样只聚焦时间区。

`viewForDayTask` 已有 `notes`（Task 4）。

- [ ] **Step 1: Write the failing test**

无新文件。门：vitest 绿 + 计划列 pointer 逻辑按 slop 分支。

- [ ] **Step 2: Run pointer helper tests**

Run: `npx vitest run --dir src lib/taskPointer`

Expected: PASS。

- [ ] **Step 3: Write minimal implementation**

`Today.tsx` 引入 `TaskDetailDialog`。给计划块 `onPointerDown` 记录 `originX/Y`。把现有 `onUp` 的 `if (current.start === drag.start && current.end === drag.end) return;` 改成：无 edge 且在 slop 内则打开详情。需要 `pointerup` 事件坐标：`onUp(e: PointerEvent)` 使用 `e.clientX/Y`（现签名是无参，改一下）。

不要给今日加 `hourWashCategory`。

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run --dir src`

Expected: PASS。另跑 `/opt/homebrew/bin/cargo test --offline -p gamelife`（本任务若没动 Rust 也应仍绿）。

- [ ] **Step 5: Commit**

```bash
git add src/pages/Today.tsx
git commit -m "$(cat <<'EOF'
feat: open the shared task dialog from today's plan column

EOF
)"
```

---

### Task 13: 文档与规则

**Files:**
- Modify: `AGENTS.md`（`user_version` 4 → 5，注明 `notes`）
- Modify: `CLAUDE.md`（架构段 `user_version = 4` → 5；不变量里 `user_version` 是 4 改为 5；规格列表已有 2026-09-16 则核对措辞）
- Modify: `.cursor/rules/tauri-shell.mdc`（`user_version` is 4 → 5）
- Modify: `.cursor/rules/frontend-ui.mdc`（今日计划列还共用 `TaskDetailDialog`；备注不进判定；无 FullCalendar）
- Modify: `.cursor/rules/specs.mdc`（若 2026-09-16 条目已在则只核对）
- 规格已在 `docs/superpowers/specs/2026-09-16-task-detail-and-board-ux-design.md`（状态：已确认）
- 本 plan：`docs/superpowers/plans/2026-09-16-task-detail-and-board-ux.md`

**Interfaces:**
- Consumes: 已实现行为
- Produces: 文档与 spec overlay 一致；仍写明备注永不进 `TaskSnapshot`

- [ ] **Step 1: 对照 spec 扫一遍实现**

确认：勾选/备注/多选不发币；详情与改期不套娃；日历无右缘竖条；今日无小时底；8192 拒绝不截断；`javascript:` 链接纯文本。缺了就停，回到对应任务补，不要在文档任务里塞行为补丁除非是一行文案。

- [ ] **Step 2: 改文档**

把所有「`user_version` 是 4（`sort` / `repeat` / `remind_json`）」改成「`user_version` 是 **5**（另加 `notes`；判定快照仍无备注）」。`frontend-ui.mdc` 加一句：列表/日历/今日单击打开 `TaskDetailDialog`；备注 Markdown 子集在 `taskNotesMd.ts`。

- [ ] **Step 3: 跑门禁**

```bash
/opt/homebrew/bin/cargo test --offline -p gamelife-core
/opt/homebrew/bin/cargo test --offline -p gamelife
npx vitest run --dir src
```

Expected: 全绿。

- [ ] **Step 4: Commit**

```bash
git add AGENTS.md CLAUDE.md .cursor/rules/tauri-shell.mdc .cursor/rules/frontend-ui.mdc .cursor/rules/specs.mdc docs/superpowers/specs/2026-09-16-task-detail-and-board-ux-design.md docs/superpowers/plans/2026-09-16-task-detail-and-board-ux.md
git commit -m "$(cat <<'EOF'
docs: record task notes, detail dialog, and user_version 5

EOF
)"
```

不要 add 通知崩溃修复或其它脏文件。

---

## Self-review

1. **Spec coverage:** §4 数据 → T1–4；§5 详情/Markdown → T5, T8, T9；§6 列表 → T6, T8, T10；§7 日历 → T7, T11；今日 → T12；版本与文档 → T2, T13；测试表落在各任务。未做：子任务、日历 Shift、批量拖排序（spec §10 不做）。
2. **Placeholders:** 无 TBD。错误码固定 `notes_too_long`。列缝 `CAL_DAY_GAP = 10`。slop `4`。debounce `400`。
3. **Types:** `Task.notes: String`、`TaskView.notes: string`、`hourWashCategory(cells, hour) -> CategoryKey | null`、`TaskDetailDialog` props 在 T9 定义、T10/T12 消费同一签名。
