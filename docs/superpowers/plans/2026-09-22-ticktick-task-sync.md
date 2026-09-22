# TickTick Task Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 设置页 TickTick 总开关打开后，把已映射清单里的未完成任务导入四个预设分组，本机改标题、时段、完成、删除立刻写回，并每小时再拉取一次对齐。

**Architecture:** 角色映射、写入清单选择、时间落点和一轮对账是 `gamelife-core` 的纯函数，不碰网络和时钟。`src-tauri/src/ticktick.rs` 负责 OAuth、Open API、后台推送和每小时拉取。任务命令先提交 SQLite 再在后台写回。设置页只读 `app_meta` 缓存。

**Tech Stack:** Tauri 2、Rust、`reqwest` blocking、React、TypeScript、vitest、SQLite。构建用 Homebrew `/opt/homebrew/bin/cargo`。

**Spec:** `docs/superpowers/specs/2026-09-22-ticktick-task-sync-design.md`

## Global Constraints

- 总开关 `ticktick_enabled` 默认 false。关闭时不拉取、不写回、不每小时对齐。连接、断开、刷新清单在关闭时仍可用。立即同步在开关关闭或未连接时不可用。
- 分组只认清单角色：`mainline` → `list-mainline`，`side` → `list-side`，`longterm` → `list-longterm`，`chore` → `list-chore`。`ignore` 与未知值不导入。不看标题 `#主线`。不读 `ticktick_column_roles`。
- 写入清单：该角色下 `sortOrder` 有符号整数最小；并列时清单 id 字典序较小。没有则新建只留本机，不标脏。
- 预设分组只认 `PRESET_LIST_IDS` 这四个 id。其它 `list_id` 上的任务不推送。已链接任务移出这四个 id 时，写回只发删除。
- 已链接任务重复必须是 `none`。`upsert` 拒绝其它值，错误码 `linked_repeat`。完成已链接任务不 `spawn_after_complete`。
- 只改备注或同一分组内的排序不标脏、不写回。
- 历史已完成不插入。子任务 `items` 忽略。不复制 `content` / `desc` / `repeatFlag` / 提醒 / 标签。
- `ticktick_dirty`：0 干净，1 待写回，2 创建结果不确定（自动轮次只查找，不 `POST`）。
- 删除已链接本地行仅当：不是第一次成功拉取、完成列表成功、每个已映射清单都拉取成功、该 TickTick 任务 id 不在未完成集合也不在完成列表。
- 任一清单或完成列表失败：不删除；不前移 `ticktick_last_sync_at`；失败清单上的已链接任务不改标题和时段。
- 全天：调用方传入本地日界 `[day_start, next_day_start)`，`ticktick_all_day = 1`。本机改了开始或结束则清标记，之后按定时任务写回。
- 官方 API 主机 `https://api.ticktick.com/open/v1`，授权页 `https://ticktick.com/oauth/authorize`，scope `tasks:write`。不接 `dida365.com`，不用非官方协议。
- Client Secret、access token、refresh token 只进 `secrets.json`（0600）。不进 `config.json`，不回传给页面，不进云备份。不用 macOS 钥匙串。
- 网络命令 `async`。打开设置不打网络。采样失败不因 TickTick 停下。`user_version` 为 **6**。不删 `ticktick_cache`，也不读它。`TaskSnapshot` 仍是 `id` / `title` / `role`。
- 界面中文。能量称「能量」。颜色只用现有 token，组件里不写 hex。页面只经 `src/lib/api.ts` 调 `invoke()`。
- 改 `src-tauri/`：`/opt/homebrew/bin/cargo test --offline -p gamelife` 必须跑完（不要设 `CARGO_TARGET_DIR`）。
- 改 `crates/gamelife-core/`：`/opt/homebrew/bin/cargo test --offline -p gamelife-core`。
- 改 `src/`：`npx vitest run --dir src`。
- 测试禁止 `std::env::set_var("HOME", …)`。
- 每个任务一条英文 conventional commit，不 `--no-verify`。只 `git add` 该任务 Files 列出的路径。
- 观测仍优先于计划。同步不发币，不改已结束的槽。

## File Structure

| 路径 | 职责 |
| --- | --- |
| `crates/gamelife-core/src/ticktick_sync.rs` | 纯函数：角色、写入清单、时间、JSON 解析、对账、单条推送决策 |
| `crates/gamelife-core/src/lib.rs` | 导出上面的模块 |
| `crates/gamelife-core/src/task.rs` | `Task` 增加链接字段。已有字面量补默认值 |
| `src-tauri/src/db.rs` | `user_version` 6、列、`load_tasks` |
| `src-tauri/src/commands.rs` | 保存任务时保留链接；该标脏的操作标脏；拒绝已链接任务改重复 |
| `src-tauri/src/config.rs` | `ticktick_enabled`；重新写出 client id 与清单角色 |
| `src-tauri/src/keychain.rs` | 三个 TickTick 槽的读写 |
| `src-tauri/src/ticktick.rs` | HTTP、OAuth、推送、每小时拉取、Tauri 命令 |
| `src-tauri/src/sampler.rs` | 每 20 拍调用 `ticktick::maybe_spawn_sync` |
| `src-tauri/src/lib.rs` | `mod ticktick` 与命令注册 |
| `src/lib/ticktickSettings.ts` | 立即同步是否可点、新清单默认忽略、写入提示文案 |
| `src/lib/api.ts` | 设置字段与 TickTick 命令 |
| `src/lib/preview/fixtures.ts` | 预览夹具 |
| `src/pages/Settings.tsx` | TickTick 标签 |
| `CLAUDE.md`、`AGENTS.md`、`.cursor/rules/specs.mdc`、`.cursor/rules/tauri-shell.mdc` | 去掉「不再读取 TickTick 令牌」 |

---

### Task 1: 角色、写入清单、时间

**Files:**
- Create: `crates/gamelife-core/src/ticktick_sync.rs`
- Modify: `crates/gamelife-core/src/lib.rs`
- Test: `crates/gamelife-core/src/ticktick_sync.rs`（模块底部 `#[cfg(test)]`）

**Interfaces:**
- Consumes: `crate::task::{ListRole, PRESET_MAINLINE_ID, PRESET_SIDE_ID, PRESET_LONGTERM_ID, PRESET_CHORE_ID}`
- Produces:
  - `RemoteProject { id: String, name: String, sort_order: i64 }`
  - `fn parse_role(raw: &str) -> Option<ListRole>` — `ignore`、空、未知返回 `None`
  - `fn list_id_for_role(role: ListRole) -> Option<&'static str>` — `Custom` 返回 `None`
  - `fn write_target<'a>(projects: &'a [RemoteProject], roles: &BTreeMap<String, String>, role: ListRole) -> Option<&'a RemoteProject>`
  - `fn stamp_missing_roles(projects: &[RemoteProject], roles: &mut BTreeMap<String, String>)` — 缺的键写成 `"ignore"`
  - `fn map_times(all_day: bool, start: Option<i64>, end: Option<i64>, day_start: i64, next_day_start: i64) -> (Option<i64>, Option<i64>, bool)` — 全天时忽略 start/end，返回 `(Some(day_start), Some(next_day_start), true)`；否则第三项为 false，原样返回 start/end

- [ ] **Step 1: Write the failing test**

在新文件底部：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn proj(id: &str, sort_order: i64) -> RemoteProject {
        RemoteProject { id: id.into(), name: id.into(), sort_order }
    }

    #[test]
    fn write_target_picks_smallest_sort_order_then_id() {
        let projects = vec![proj("b", -3), proj("a", -3), proj("c", -10)];
        let mut roles = BTreeMap::new();
        roles.insert("a".into(), "mainline".into());
        roles.insert("b".into(), "mainline".into());
        roles.insert("c".into(), "mainline".into());
        roles.insert("z".into(), "ignore".into());
        assert_eq!(write_target(&projects, &roles, ListRole::Mainline).unwrap().id, "c");
        let only = vec![proj("b", -3), proj("a", -3)];
        assert_eq!(write_target(&only, &roles, ListRole::Mainline).unwrap().id, "a");
        assert!(write_target(&projects, &roles, ListRole::Side).is_none());
    }

    #[test]
    fn stamp_missing_roles_defaults_ignore_and_keeps_existing() {
        let projects = vec![proj("new", 1), proj("old", 2)];
        let mut roles = BTreeMap::new();
        roles.insert("old".into(), "side".into());
        stamp_missing_roles(&projects, &mut roles);
        assert_eq!(roles.get("new").map(String::as_str), Some("ignore"));
        assert_eq!(roles.get("old").map(String::as_str), Some("side"));
    }

    #[test]
    fn map_times_all_day_uses_caller_bounds() {
        assert_eq!(map_times(true, Some(1), Some(2), 100, 200), (Some(100), Some(200), true));
        assert_eq!(map_times(false, None, Some(5), 100, 200), (None, Some(5), false));
    }

    #[test]
    fn list_ids_match_presets() {
        assert_eq!(list_id_for_role(ListRole::Mainline), Some(PRESET_MAINLINE_ID));
        assert_eq!(list_id_for_role(ListRole::Chore), Some(PRESET_CHORE_ID));
        assert_eq!(list_id_for_role(ListRole::Custom), None);
        assert_eq!(parse_role("ignore"), None);
        assert_eq!(parse_role("longterm"), Some(ListRole::Longterm));
    }
}
```

`lib.rs` 增加 `pub mod ticktick_sync;`。

- [ ] **Step 2: Run test to verify it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core ticktick_sync::`

Expected: FAIL，`ticktick_sync` 找不到或函数未定义。

- [ ] **Step 3: Write minimal implementation**

`parse_role`：`mainline` / `side` / `longterm` / `chore` 映射到对应 `ListRole`，其余 `None`。

`write_target`：留下 `parse_role(roles.get(id)) == Some(role)` 的项目，按 `(sort_order, id)` 取最小。

`stamp_missing_roles`：`roles.entry(id).or_insert_with(|| "ignore".into())`。

`map_times` 按接口说明。

- [ ] **Step 4: Run test to verify it passes**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core ticktick_sync::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/ticktick_sync.rs crates/gamelife-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: map TickTick lists onto preset task groups

EOF
)"
```

---

### Task 2: 对账与单条推送决策

**Files:**
- Modify: `crates/gamelife-core/src/ticktick_sync.rs`
- Test: 同文件 `tests`

**Interfaces:**
- Consumes: Task 1 的类型与函数
- Produces:

```rust
pub struct RemoteTask {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub all_day: bool,
    pub etag: String,
}

pub struct LocalMirror {
    pub local_id: String,
    pub list_id: String,
    pub title: String,
    pub done: bool,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub ticktick_task_id: Option<String>,
    pub ticktick_project_id: Option<String>,
    pub ticktick_etag: String,
    pub ticktick_dirty: i64,
    pub ticktick_all_day: bool,
}

pub struct ProjectFetch {
    pub project_id: String,
    pub ok: bool,
    pub open: Vec<RemoteTask>,
}

pub enum ReconcileAction {
    Insert(LocalMirror),
    Update(LocalMirror),
    Delete { local_id: String },
    Link {
        local_id: String,
        ticktick_task_id: String,
        ticktick_project_id: String,
        etag: String,
    },
}

pub struct ReconcileInput<'a> {
    pub local: &'a [LocalMirror],
    pub projects: &'a [RemoteProject],
    pub roles: &'a BTreeMap<String, String>,
    pub fetches: &'a [ProjectFetch],
    pub completed_ids: &'a [String],
    pub completed_ok: bool,
    pub first_sync: bool,
    pub day_start: i64,
    pub next_day_start: i64,
}

pub fn reconcile(input: &ReconcileInput) -> Vec<ReconcileAction>;

pub enum PushKind { Create, Update, Complete, Reopen, Delete, Move }

pub struct PushOp {
    pub kind: PushKind,
    pub local_id: String,
    pub task_id: Option<String>,
    pub project_id: String,
    pub to_project_id: Option<String>,
    pub title: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub all_day: bool,
}

pub fn push_op(task: &LocalMirror, roles: &BTreeMap<String, String>, projects: &[RemoteProject]) -> Option<PushOp>;

pub fn parse_projects(value: &serde_json::Value) -> Vec<RemoteProject>;
pub fn parse_open_tasks(project_id: &str, value: &serde_json::Value) -> Vec<RemoteTask>;
pub fn parse_completed_ids(value: &serde_json::Value) -> Vec<String>;
```

`Insert` 的 `local_id` 用空字符串，由 shell 填 `new_task_id()`。`done` 为 false，`ticktick_dirty` 为 0。

- [ ] **Step 1: Write the failing test**

```rust
fn mirror(id: &str, tt: Option<&str>, project: &str, dirty: i64) -> LocalMirror {
    LocalMirror {
        local_id: id.into(),
        list_id: PRESET_MAINLINE_ID.into(),
        title: "写稿".into(),
        done: false,
        start: Some(10),
        end: Some(20),
        ticktick_task_id: tt.map(str::to_string),
        ticktick_project_id: Some(project.into()),
        ticktick_etag: "e1".into(),
        ticktick_dirty: dirty,
        ticktick_all_day: false,
    }
}

fn open(id: &str, project: &str, title: &str, etag: &str) -> RemoteTask {
    RemoteTask {
        id: id.into(),
        project_id: project.into(),
        title: title.into(),
        start: Some(10),
        end: Some(20),
        all_day: false,
        etag: etag.into(),
    }
}

#[test]
fn reconcile_inserts_open_task_and_skips_subtasks_json() {
    let projects = vec![proj("p", 1)];
    let mut roles = BTreeMap::new();
    roles.insert("p".into(), "mainline".into());
    let fetches = vec![ProjectFetch {
        project_id: "p".into(),
        ok: true,
        open: vec![open("t1", "p", "写稿", "e")],
    }];
    let actions = reconcile(&ReconcileInput {
        local: &[],
        projects: &projects,
        roles: &roles,
        fetches: &fetches,
        completed_ids: &[],
        completed_ok: true,
        first_sync: true,
        day_start: 0,
        next_day_start: 86_400,
    });
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        ReconcileAction::Insert(row) => {
            assert_eq!(row.list_id, PRESET_MAINLINE_ID);
            assert_eq!(row.ticktick_task_id.as_deref(), Some("t1"));
            assert!(!row.done);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn dirty_local_is_not_overwritten_and_clean_etag_change_updates() {
    let projects = vec![proj("p", 1)];
    let mut roles = BTreeMap::new();
    roles.insert("p".into(), "mainline".into());
    let local = vec![mirror("L1", Some("t1"), "p", 1), mirror("L2", Some("t2"), "p", 0)];
    let fetches = vec![ProjectFetch {
        project_id: "p".into(),
        ok: true,
        open: vec![open("t1", "p", "远端标题", "e2"), open("t2", "p", "新标题", "e9")],
    }];
    let actions = reconcile(&ReconcileInput {
        local: &local,
        projects: &projects,
        roles: &roles,
        fetches: &fetches,
        completed_ids: &[],
        completed_ok: true,
        first_sync: false,
        day_start: 0,
        next_day_start: 86_400,
    });
    assert!(actions.iter().all(|a| !matches!(a, ReconcileAction::Update(row) if row.local_id == "L1")));
    assert!(actions.iter().any(|a| matches!(a, ReconcileAction::Update(row) if row.local_id == "L2" && row.title == "新标题")));
}

#[test]
fn delete_only_when_every_mapped_fetch_and_completed_succeed() {
    let projects = vec![proj("p", 1), proj("q", 2)];
    let mut roles = BTreeMap::new();
    roles.insert("p".into(), "mainline".into());
    roles.insert("q".into(), "side".into());
    let local = vec![mirror("L1", Some("gone"), "p", 0)];
    let fetches = vec![
        ProjectFetch { project_id: "p".into(), ok: true, open: vec![] },
        ProjectFetch { project_id: "q".into(), ok: false, open: vec![] },
    ];
    let blocked = reconcile(&ReconcileInput {
        local: &local, projects: &projects, roles: &roles, fetches: &fetches,
        completed_ids: &[], completed_ok: true, first_sync: false, day_start: 0, next_day_start: 86_400,
    });
    assert!(blocked.iter().all(|a| !matches!(a, ReconcileAction::Delete { .. })));

    let fetches = vec![
        ProjectFetch { project_id: "p".into(), ok: true, open: vec![] },
        ProjectFetch { project_id: "q".into(), ok: true, open: vec![] },
    ];
    let deleted = reconcile(&ReconcileInput {
        local: &local, projects: &projects, roles: &roles, fetches: &fetches,
        completed_ids: &[], completed_ok: true, first_sync: false, day_start: 0, next_day_start: 86_400,
    });
    assert!(deleted.iter().any(|a| matches!(a, ReconcileAction::Delete { local_id } if local_id == "L1")));
}

#[test]
fn completed_list_marks_linked_task_and_does_not_insert() {
    let projects = vec![proj("p", 1)];
    let mut roles = BTreeMap::new();
    roles.insert("p".into(), "mainline".into());
    let local = vec![mirror("L1", Some("t1"), "p", 0)];
    let fetches = vec![ProjectFetch { project_id: "p".into(), ok: true, open: vec![] }];
    let actions = reconcile(&ReconcileInput {
        local: &local, projects: &projects, roles: &roles, fetches: &fetches,
        completed_ids: &["t1".into(), "historical".into()],
        completed_ok: true, first_sync: false, day_start: 0, next_day_start: 86_400,
    });
    assert!(actions.iter().any(|a| matches!(a, ReconcileAction::Update(row) if row.local_id == "L1" && row.done)));
    assert!(actions.iter().all(|a| !matches!(a, ReconcileAction::Insert(_))));
}

#[test]
fn first_sync_does_not_delete_missing_links() {
    let projects = vec![proj("p", 1)];
    let mut roles = BTreeMap::new();
    roles.insert("p".into(), "mainline".into());
    let local = vec![mirror("L1", Some("gone"), "p", 0)];
    let fetches = vec![ProjectFetch { project_id: "p".into(), ok: true, open: vec![] }];
    let actions = reconcile(&ReconcileInput {
        local: &local, projects: &projects, roles: &roles, fetches: &fetches,
        completed_ids: &[], completed_ok: false, first_sync: true, day_start: 0, next_day_start: 86_400,
    });
    assert!(actions.iter().all(|a| !matches!(a, ReconcileAction::Delete { .. })));
}

#[test]
fn task_seen_on_another_mapped_list_moves_group() {
    let projects = vec![proj("p", 1), proj("q", 2)];
    let mut roles = BTreeMap::new();
    roles.insert("p".into(), "mainline".into());
    roles.insert("q".into(), "side".into());
    let local = vec![mirror("L1", Some("t1"), "p", 0)];
    let fetches = vec![
        ProjectFetch { project_id: "p".into(), ok: true, open: vec![] },
        ProjectFetch { project_id: "q".into(), ok: true, open: vec![open("t1", "q", "写稿", "e1")] },
    ];
    let actions = reconcile(&ReconcileInput {
        local: &local, projects: &projects, roles: &roles, fetches: &fetches,
        completed_ids: &[], completed_ok: true, first_sync: false, day_start: 0, next_day_start: 86_400,
    });
    assert!(actions.iter().any(|a| matches!(a, ReconcileAction::Update(row)
        if row.local_id == "L1" && row.list_id == PRESET_SIDE_ID && row.ticktick_project_id.as_deref() == Some("q"))));
    assert!(actions.iter().all(|a| !matches!(a, ReconcileAction::Delete { .. })));
}

#[test]
fn dirty_two_links_exact_title_and_times_only() {
    let projects = vec![proj("p", 1)];
    let mut roles = BTreeMap::new();
    roles.insert("p".into(), "mainline".into());
    let mut local = mirror("L1", None, "p", 2);
    local.ticktick_project_id = None;
    local.ticktick_etag.clear();
    let fetches = vec![ProjectFetch {
        project_id: "p".into(),
        ok: true,
        open: vec![open("t1", "p", "写稿", "etag")],
    }];
    let actions = reconcile(&ReconcileInput {
        local: &[local], projects: &projects, roles: &roles, fetches: &fetches,
        completed_ids: &[], completed_ok: true, first_sync: false, day_start: 0, next_day_start: 86_400,
    });
    assert!(actions.iter().any(|a| matches!(a, ReconcileAction::Link { local_id, ticktick_task_id, .. }
        if local_id == "L1" && ticktick_task_id == "t1")));
}

#[test]
fn push_op_create_complete_move_and_skip_clean() {
    let projects = vec![proj("p", -5), proj("s", 1)];
    let mut roles = BTreeMap::new();
    roles.insert("p".into(), "mainline".into());
    roles.insert("s".into(), "side".into());
    let mut fresh = mirror("L1", None, "p", 1);
    fresh.ticktick_project_id = None;
    fresh.list_id = PRESET_MAINLINE_ID.into();
    let created = push_op(&fresh, &roles, &projects).unwrap();
    assert_eq!(created.kind, PushKind::Create);
    assert_eq!(created.project_id, "p");

    let mut done = mirror("L2", Some("t2"), "p", 1);
    done.done = true;
    assert_eq!(push_op(&done, &roles, &projects).unwrap().kind, PushKind::Complete);

    let mut moved = mirror("L3", Some("t3"), "p", 1);
    moved.list_id = PRESET_SIDE_ID.into();
    let op = push_op(&moved, &roles, &projects).unwrap();
    assert_eq!(op.kind, PushKind::Move);
    assert_eq!(op.to_project_id.as_deref(), Some("s"));

    let mut custom = mirror("L4", Some("t4"), "p", 1);
    custom.list_id = "list-custom".into();
    assert_eq!(push_op(&custom, &roles, &projects).unwrap().kind, PushKind::Delete);

    let clean = mirror("L5", Some("t5"), "p", 0);
    assert!(push_op(&clean, &roles, &projects).is_none());
    assert!(push_op(&fresh, &roles, &[]).is_none());
}

#[test]
fn parse_open_tasks_ignores_items_and_completed_ids() {
    let data = serde_json::json!({
        "tasks": [{
            "id": "t1",
            "title": "父",
            "etag": "e",
            "status": 0,
            "isAllDay": false,
            "items": [{"id": "sub", "title": "子", "status": 0}]
        }]
    });
    let tasks = parse_open_tasks("p", &data);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "t1");
    let done = serde_json::json!([{"id": "c1"}, {"id": "c2"}]);
    assert_eq!(parse_completed_ids(&done), vec!["c1".to_string(), "c2".to_string()]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core ticktick_sync::tests::reconcile_inserts_open_task_and_skips_subtasks_json -- --nocapture`

Expected: FAIL，`reconcile` 未定义。

- [ ] **Step 3: Write minimal implementation**

`reconcile` 规则：

1. 已映射清单 = `roles` 里 `parse_role` 为 `Some` 的 id。
2. `failed` = 已映射但 `fetches` 里没有 `ok: true` 的 id。
3. 把所有 `ok` 的 `open` 按任务 id 收成一张表。同一 id 出现多次时保留先见到的。
4. `allow_delete = !first_sync && completed_ok && failed.is_empty()`。
5. 对每条 `ticktick_dirty == 1` 的本地行：不产生 Update/Delete。
6. 对 `ticktick_dirty == 2`：在该行 `list_id` 对应角色的写入清单的 open 任务里，找 `title/start/end` 都相等、且 id 尚未被其它本地行占用的任务。恰好一条则 `Link`。零条或多条则跳过。不 Delete，不 Insert。
7. 对干净且已链接的行：若 id 在 open 表，etag 或清单 id 或标题或时段与本地不同，则 `Update`（清单角色变了就换 `list_id`）。若本地 `done` 且远端仍 open，`Update` 把 `done` 设为 false。若 id 在 `completed_ids` 且不在 open 表，`Update` 把 `done` 设为 true。若 `allow_delete` 且两边都没有，`Delete`。
8. open 表里本地没有的 id：`Insert`，`list_id` 用该清单角色。角色缺失则跳过。`map_times` 填时间。

`push_op`：

- `ticktick_dirty != 1` 返回 `None`。
- 无 `ticktick_task_id`：`list_id` 属于四个预设且 `write_target` 有值则 `Create`，否则 `None`。
- 有 id 且 `list_id` 不在四个预设：`Delete`。
- 有 id 且 `list_id` 的角色与 `ticktick_project_id` 的角色不同：目标有写入清单则 `Move`，没有则 `None`（shell 不改分组；分组是否回滚由 Task 6 在写库前检查）。
- 有 id 且 `done`：`Complete`。
- 有 id 且未完成、etag 行仍在原清单：若调用方把「原先 done 为 true」编码进这次调用——`push_op` 不看旧值。shell 在取消完成时传 `done: false` 且 dirty 1，这里若 `!done` 发 `Reopen`（标题或时段也一起带上）。同一轮里完成优先：`done == true` 永远是 `Complete`，不是 `Update`。

`parse_projects` 读数组，或 `{ "projects": [...] }` 两种。字段 `id`、`name`、`sortOrder`。缺 `sortOrder` 当 0。

`parse_open_tasks` 读 `tasks` 数组。不要读 `items`。`status == 2` 的任务跳过。`isAllDay` 缺省 false。`startDate` / `dueDate` 用 `chrono` 解析成 unix 秒；解析失败当 `None`。`dueDate` 写入 `end`，`startDate` 写入 `start`。

`parse_completed_ids` 读数组的 `id`，或 `{ "tasks": [...] }`。

- [ ] **Step 4: Run test to verify it passes**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core ticktick_sync::`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/ticktick_sync.rs
git commit -m "$(cat <<'EOF'
feat: reconcile TickTick tasks against the local board

EOF
)"
```

---

### Task 3: 任务表存链接

**Files:**
- Modify: `crates/gamelife-core/src/task.rs`
- Modify: `crates/gamelife-core/src/task_parse.rs`（补新字段）
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/commands.rs`（`Task` 字面量、`task_to_view` / `view_to_task` / `persist_task` / `load` 路径）
- Test: `src-tauri/src/db.rs` 现有 `user_version` 测试

**Interfaces:**
- Consumes: 无
- Produces: `Task` 新字段，默认不链接。`load_tasks` 读出这些列。`view_to_task` 从已有行复制链接，不从 `TaskView` 读。

`Task` 增加，全部 `#[serde(default)]`：

```rust
pub ticktick_task_id: Option<String>,
pub ticktick_project_id: Option<String>,
pub ticktick_etag: String,
pub ticktick_dirty: i64,
pub ticktick_all_day: bool,
```

`TaskView` **不加**这些字段。

- [ ] **Step 1: Write the failing test**

把 `src-tauri/src/db.rs` 里断言 `user_version(...) == 5` 的测试改成先失败的新断言：迁移后是 6，且能插入并读回链接列。在 `db.rs` 的 tests 模块加：

```rust
#[test]
fn migrate_user_version_6_roundtrips_ticktick_link() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    assert_eq!(user_version(&conn), 6);
    conn.execute(
        "INSERT INTO task_lists (id, name, sort, role) VALUES ('list-mainline', '主线任务', 0, 'mainline')",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO tasks (id, list_id, title, done, ticktick_task_id, ticktick_project_id, ticktick_etag, ticktick_dirty, ticktick_all_day)
         VALUES ('t', 'list-mainline', '写稿', 0, 'tt1', 'p1', 'e', 2, 1)",
        [],
    ).unwrap();
    let tasks = load_tasks(&conn).unwrap();
    assert_eq!(tasks[0].ticktick_task_id.as_deref(), Some("tt1"));
    assert_eq!(tasks[0].ticktick_dirty, 2);
    assert!(tasks[0].ticktick_all_day);
}
```

现有从版本 1/3/4 迁到 5 的断言改为迁到 6（列仍然要在）。

- [ ] **Step 2: Run test to verify it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife migrate_user_version_6_roundtrips_ticktick_link -- --nocapture`

Expected: FAIL，`user_version` 仍是 5，或插入未知列。

- [ ] **Step 3: Write minimal implementation**

`TARGET_USER_VERSION` 改为 6。在 `version < 5` 块之后：

```rust
if version < 6 {
    add_column_if_missing(conn, "tasks", "ticktick_task_id", "TEXT")?;
    add_column_if_missing(conn, "tasks", "ticktick_project_id", "TEXT")?;
    add_column_if_missing(conn, "tasks", "ticktick_etag", "TEXT NOT NULL DEFAULT ''")?;
    add_column_if_missing(conn, "tasks", "ticktick_dirty", "INTEGER NOT NULL DEFAULT 0")?;
    add_column_if_missing(conn, "tasks", "ticktick_all_day", "INTEGER NOT NULL DEFAULT 0")?;
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS tasks_ticktick_task_id
         ON tasks(ticktick_task_id) WHERE ticktick_task_id IS NOT NULL",
    ).map_err(map_rusqlite)?;
}
```

新库的 `SCHEMA` 里 `tasks` 也加上这五列和同一条部分唯一索引，避免只靠迁移。

`load_tasks` 的 SELECT 与 `persist_task` 的 INSERT/UPDATE 带上这五列。`ticktick_all_day` 用 `i64` 0/1 存。

仓库里每一个 `Task {` 补上：

```rust
ticktick_task_id: None,
ticktick_project_id: None,
ticktick_etag: String::new(),
ticktick_dirty: 0,
ticktick_all_day: false,
```

`view_to_task` 改为 `view_to_task(view, existing: Option<&Task>)`。链接五字段来自 `existing`，没有则默认。`upsert_task_in` 在写入前按 id 找到旧行再转换。

`duplicate_task_in` 复制后把这五字段重置为默认，避免两个本地行共用一个 TickTick id。

- [ ] **Step 4: Run test to verify it passes**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core && /opt/homebrew/bin/cargo test --offline -p gamelife migrate_user_version_6_roundtrips_ticktick_link -- --nocapture`

Expected: PASS。core 测试若因 `Task` 字面量缺字段失败，补完再跑，直到 `gamelife-core` 全绿。

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/task.rs crates/gamelife-core/src/task_parse.rs src-tauri/src/db.rs src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
feat: store TickTick identity on local tasks

EOF
)"
```

---

### Task 4: 开关、清单角色、密钥槽

**Files:**
- Modify: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/keychain.rs`
- Test: 两文件已有的 `mod tests`

**Interfaces:**
- Consumes: 无
- Produces:
  - `AppSettings.ticktick_enabled: bool`，`#[serde(default)]`，camelCase `ticktickEnabled`
  - `ticktick_client_id` 与 `ticktick_project_roles` 去掉 `skip_serializing`
  - `ticktick_column_roles` 保持 `skip_serializing`，并加 `skip_deserializing`，字段保留以免删掉结构体成员时漏改默认值；读到旧 JSON 的这一键直接丢掉
  - `keychain::TICKTICK_CLIENT_SECRET`、`TICKTICK_ACCESS_TOKEN`、`TICKTICK_REFRESH_TOKEN`
  - `get_ticktick_secret(slot)` / `set_ticktick_secret(slot, value)` / `delete_ticktick_secret(slot)`，内部走现有 `get_in` / `set_in` / `delete_in`

- [ ] **Step 1: Write the failing test**

`config.rs` tests：

```rust
#[test]
fn ticktick_switch_and_roles_roundtrip_and_column_roles_are_dropped() {
    let raw = r#"{
        "screenshotRetention":"none","sampleKeepDays":7,"loginAtStartup":true,
        "trustedApps":[],"distractionRules":[],"sideProjectRules":[],
        "readingApps":[],"neverCaptureApps":[],
        "ticktickEnabled": true,
        "ticktickClientId": "cid",
        "ticktickProjectRoles": {"p":"mainline"},
        "ticktickColumnRoles": {"c":"side"}
    }"#;
    let parsed: AppSettings = serde_json::from_str(raw).unwrap();
    assert!(parsed.ticktick_enabled);
    assert_eq!(parsed.ticktick_client_id, "cid");
    assert_eq!(parsed.ticktick_project_roles.get("p").map(String::as_str), Some("mainline"));
    let out = serde_json::to_value(&parsed).unwrap();
    assert_eq!(out.get("ticktickEnabled").and_then(|v| v.as_bool()), Some(true));
    assert!(out.get("ticktickColumnRoles").is_none());
    assert_eq!(policy_snapshot_json(&parsed), policy_snapshot_json(&default_settings()));
}
```

`default_settings()` 里 `ticktick_enabled: false`。不要把整份 `policy_snapshot_json(&parsed)` 和 `default_settings()` 比较（两边的 provider 不同）。另造一份 `default_settings()`，只改 `ticktick_enabled`、`ticktick_client_id`、`ticktick_project_roles`，断言它和未改的 `policy_snapshot_json` 相等。

`keychain.rs` tests：

```rust
#[test]
fn ticktick_slots_roundtrip_in_temp_file() {
    let path = std::env::temp_dir().join(format!("gamelife-tt-secrets-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    set_in(&path, TICKTICK_ACCESS_TOKEN, "tok").unwrap();
    assert_eq!(get_in(&path, TICKTICK_ACCESS_TOKEN).unwrap(), "tok");
    delete_in(&path, TICKTICK_ACCESS_TOKEN).unwrap();
    assert!(get_in(&path, TICKTICK_ACCESS_TOKEN).is_err());
    let _ = std::fs::remove_file(&path);
}
```

用 `temp_dir` + pid，不要 `set_var("HOME")`。

- [ ] **Step 2: Run test to verify it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife ticktick_switch_and_roles_roundtrip_and_column_roles_are_dropped -- --nocapture`

Expected: FAIL，未知字段或没有 `ticktick_enabled`。

- [ ] **Step 3: Write minimal implementation**

按接口改 `AppSettings` 与 `default_settings()`。`policy_snapshot_json` 继续不读取 TickTick 字段（现有实现若是挑字段序列化，就不要把新字段加进去）。

`old_config_json_defaults_rail_labels_true` 补一句 `assert!(!parsed.ticktick_enabled)`。

- [ ] **Step 4: Run test to verify it passes**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife ticktick_ -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs src-tauri/src/keychain.rs
git commit -m "$(cat <<'EOF'
feat: persist the TickTick switch and list roles

EOF
)"
```

---

### Task 5: 本地编辑标脏，写回决策留到下一任务

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Test: `src-tauri/src/commands.rs` 的任务测试模块

**Interfaces:**
- Consumes: `Task` 链接字段；`gamelife_core::task::PRESET_LIST_IDS`；`ListRole`
- Produces: `fn note_ticktick_edit(before: &Task, after: &mut Task)`，供 `upsert` / `toggle` / `reschedule` / `move` / `reorder` 使用。本任务不发 HTTP。

行为：

- 无 `ticktick_task_id`：不改 dirty。预设分组里的新任务保持 dirty 0，等 Task 6 在开关打开时再推。
- 已链接，且标题、start、end、done、`list_id` 有变：`ticktick_dirty = 1`。start 或 end 变了则 `ticktick_all_day = false`。
- 只有 notes 或 sort 变：dirty 保持原值。
- 已链接且 `repeat != None`：`upsert_task_in` 返回 `DbOpError::Rejected("linked_repeat".into())`，不写库。
- `toggle_task_done_in`：已链接任务不调用 `spawn_after_complete`。
- `move_task` 与 `reorder_task`：目标 `list_id` 不在 `PRESET_LIST_IDS`，或目标预设角色没有写入清单时，**若任务已链接**则拒绝，错误码 `ticktick_no_list`，不改 `list_id`。未链接任务照旧移动。写入清单判断需要项目缓存：读 `app_meta['ticktick_projects_json']` 与 `settings.ticktick_project_roles`，调用 `write_target`。缓存缺失视为没有写入清单。
- 移到另一个预设且写入清单存在：改 `list_id`，`ticktick_dirty = 1`。

- [ ] **Step 1: Write the failing test**

在 `commands.rs` 测试里用现有的内存库帮手（与 `delete_task_removes_row` 同一套 `open` + `migrate`）。若该帮手是局部函数，把新测试放在它旁边，自己 `Connection::open_in_memory` + `migrate` + 插入预设分组。

```rust
#[test]
fn linked_repeat_is_rejected_and_notes_do_not_dirty() {
    let conn = Connection::open_in_memory().unwrap();
    migrate(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO task_lists (id, name, sort, role) VALUES
           ('list-mainline','主线任务',0,'mainline'),
           ('list-side','支线任务',1,'side');
         INSERT INTO tasks (id, list_id, title, done, start, end, repeat, notes, ticktick_task_id, ticktick_project_id, ticktick_dirty)
         VALUES ('t','list-mainline','写稿',0,10,20,'none','', 'tt','p',0);",
    ).unwrap();
    let err = upsert_task_in(&conn, TaskView {
        id: "t".into(),
        list_id: "list-mainline".into(),
        title: "写稿".into(),
        done: false,
        start: Some(10),
        end: Some(20),
        range: None,
        sort: 0,
        repeat: "daily".into(),
        remind_offsets: vec![],
        notes: "".into(),
    });
    assert!(matches!(err, Err(DbOpError::Rejected(msg)) if msg == "linked_repeat"));
    upsert_task_in(&conn, TaskView {
        id: "t".into(),
        list_id: "list-mainline".into(),
        title: "写稿".into(),
        done: false,
        start: Some(10),
        end: Some(20),
        range: None,
        sort: 0,
        repeat: "none".into(),
        remind_offsets: vec![],
        notes: "只改备注".into(),
    }).unwrap();
    let row = load_tasks(&conn).unwrap().into_iter().find(|t| t.id == "t").unwrap();
    assert_eq!(row.ticktick_dirty, 0);
    assert_eq!(row.notes, "只改备注");
}
```

`TaskView` 若在测试模块外是私有，测试放在 `commands.rs` 的 `mod tests` 内即可访问。

- [ ] **Step 2: Run test to verify it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife linked_repeat_is_rejected_and_notes_do_not_dirty -- --nocapture`

Expected: FAIL，重复被写进去，或 dirty 被改掉。

- [ ] **Step 3: Write minimal implementation**

实现 `note_ticktick_edit` 并接到 `upsert_task_in`、`toggle_task_done_in`、`reschedule_task_in`、`move_task`、`reorder_task`。`reorder_task` 只在 `list_id` 变化时走移动规则；只改 sort 时不调用标脏。

`move_task` 目前是裸 SQL。改成读出 `Task`、检查目标、改字段、`persist_task`，这样链接列不会丢。

- [ ] **Step 4: Run test to verify it passes**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife linked_repeat_is_rejected_and_notes_do_not_dirty -- --nocapture`

Expected: PASS

再跑：`/opt/homebrew/bin/cargo test --offline -p gamelife`

Expected: 全绿。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
feat: mark linked tasks dirty without pushing yet

EOF
)"
```

---

### Task 6: HTTP 镜像（假客户端）

**Files:**
- Create: `src-tauri/src/ticktick.rs`
- Modify: `src-tauri/src/lib.rs`（只加 `pub mod ticktick;`，命令注册留到 Task 7）
- Modify: `src-tauri/src/commands.rs`（本地提交成功后调用 `ticktick::spawn_push_one`）
- Test: `src-tauri/src/ticktick.rs`

**Interfaces:**
- Consumes: `push_op`、`reconcile`、`parse_projects`、`parse_open_tasks`、`parse_completed_ids`、`meta_get` / `meta_set`、`load_tasks`、`load_settings`、密钥槽
- Produces:
  - `trait TickTickApi { fn request(&mut self, method: &str, path: &str, body: Option<serde_json::Value>) -> Result<TickTickResponse, TickTickError>; }`
  - `struct TickTickResponse { pub status: u16, pub body: serde_json::Value }`
  - `fn push_one(api: &mut dyn TickTickApi, task: &Task, projects: &[RemoteProject], roles: &BTreeMap<String, String>) -> PushResult`
  - `enum PushResult { Unchanged, Synced { task_id: String, project_id: String, etag: String }, Ambiguous, Failed(String), Cleared }`
  - `fn pull_round(api: &mut dyn TickTickApi, local: &[Task], projects: &[RemoteProject], roles: &BTreeMap<String, String>, first_sync: bool, now: i64, last_sync_at: Option<i64>, day_start: i64, next_day_start: i64) -> PullRound`
  - `struct PullRound { pub actions: Vec<ReconcileAction>, pub advance_sync_at: bool, pub error: Option<String> }`
  - `fn apply_push_result(task: &mut Task, result: &PushResult)`
  - `pub fn spawn_push_one(task_id: String)` — 读库、若开关关闭或无 token 则返回；否则后台 `push_one` 并写回该行
  - `pub fn run_pull(now: i64) -> Result<(), String>` — Task 7 的命令和采样循环调用它

`PushResult::Cleared` 用于删除成功或 404。`Ambiguous` 把 dirty 设为 2，不填 task id。

HTTP 路径（path 相对于 `https://api.ticktick.com/open/v1`）：

| PushKind | 方法与路径 |
| --- | --- |
| Create | `POST /task`，body 含 `title`、`projectId`、`isAllDay`、有值才带 `startDate`/`dueDate` |
| Update / Reopen | `POST /task/{id}`，Reopen 的 body 含 `"status": 0` |
| Complete | `POST /project/{projectId}/task/{taskId}/complete`，body 空 |
| Delete | `DELETE /project/{projectId}/task/{taskId}` |
| Move | `POST /task/move`，body `{"fromProjectId","toProjectId","taskId"}`（数组里一条也行：`[{"fromProjectId":...,"toProjectId":...,"taskId":...}]`）。用对象数组，与官方 move 示例一致 |

日期写成 `chrono` UTC 的 `2019-11-13T03:00:00+0000` 这种固定格式。全天任务不写 start/due，只写 `isAllDay: true` 与 `startDate` 为本地日界的 `day_start`（shell 已存在 `Task.start`）。

状态码：200–299 且 create 的 JSON 有 `id` → `Synced`。create 超时用 `TickTickError::Transport`，或 200 但没有 `id` → `Ambiguous`。404 → `Cleared`。其它 → `Failed`，dirty 保持 1。

`pull_round`：对每个已映射清单 `GET /project/{id}/data`。`first_sync` 时不请求完成列表，`completed_ok` 传 false，`first_sync` 传 true。否则 `POST /task/completed`，body：

```json
{ "projectIds": ["..."], "startDate": "<last_sync_at - 300>", "endDate": "<now>" }
```

有任一清单失败或完成列表失败：`advance_sync_at = false`，`error` 为「同步失败」。全部成功：`advance_sync_at = true`。然后 `reconcile`。

`run_pull` 在 `ticktick_enabled` 为 false 或没有 access token 时直接 `Ok(())`，且不调用 `api.request`。开关打开时，先对每条 `ticktick_dirty == 1` 的任务调用 `push_one` 并写回结果，再拉清单。成功且 `advance_sync_at` 时 `meta_set(ticktick_last_sync_at)`。无论成败都 `meta_set(ticktick_last_result)`。把 `Insert` 写成新 `Task`（`new_task_id()`，`repeat: None`，`notes: ""`）。`Update` / `Link` / `Delete` 改现有行。应用动作前再次跳过 `ticktick_dirty == 1` 的行（对账已经跳过，这里再守一层）。`ticktick_dirty == 2` 的行只接受 `Link`，自动轮次不 `POST /task`。

`spawn_push_one` 用 `std::thread::spawn`。开关关闭时不发请求，dirty 留着。

真实 HTTP 用 `reqwest::blocking`，`Authorization: Bearer <token>`。401 时用 refresh token `POST https://ticktick.com/oauth/token`（form：`grant_type=refresh_token`、`client_id`、`client_secret`、`refresh_token`）。刷新失败返回 `Failed`，不删本地任务，`ticktick_last_result` = 「需要重新连接」。刷新成功写回两个 token 再重试一次。

- [ ] **Step 1: Write the failing test**

在 `ticktick.rs` 里写 `FakeApi { calls: Vec<(String, String)>, script: Vec<TickTickResponse> }`，`request` 把 `method + path` 记进 `calls` 并弹出 `script`。

```rust
#[test]
fn push_create_timeout_does_not_post_twice_in_pull() {
    let mut api = FakeApi::timeout_on("POST", "/task");
    let task = linked_task_without_remote_id();
    let projects = vec![RemoteProject { id: "p".into(), name: "主线".into(), sort_order: 1 }];
    let mut roles = BTreeMap::new();
    roles.insert("p".into(), "mainline".into());
    let result = push_one(&mut api, &task, &projects, &roles);
    assert!(matches!(result, PushResult::Ambiguous));
    assert_eq!(api.calls.iter().filter(|(m, p)| m == "POST" && p == "/task").count(), 1);
}

#[test]
fn disabled_run_shape_is_pull_round_without_calls_when_switch_off() {
    let mut api = FakeApi::empty();
    let round = pull_round(&mut api, &[], &[], &BTreeMap::new(), true, 1_000, None, 0, 86_400);
    assert!(api.calls.is_empty());
    assert!(!round.advance_sync_at);
}
```

`linked_task_without_remote_id` 造一个 `Task`，`list_id = list-mainline`，`ticktick_dirty = 1`，`ticktick_task_id = None`。

再加一个测试：两个清单其中一个 `project_data` 返回 500 时，`advance_sync_at` 为 false，且 `actions` 里没有 `Delete`。假数据里本地有一条链接、远端 open 为空。

- [ ] **Step 2: Run test to verify it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife ticktick:: -- --nocapture`

Expected: FAIL，模块或函数不存在。

- [ ] **Step 3: Write minimal implementation**

按接口实现。`commands.rs` 在 `upsert_task`、`toggle_task_done`、`delete_task`、`move_task`、`reorder_task`、`reschedule_task`、`duplicate_task` 成功返回前，若该任务 `ticktick_dirty == 1` 或刚被删除且删除前有 `ticktick_task_id`，调用 `spawn_push_one`。删除要在行消失前把 id 交给后台；`spawn_push_one` 若行已不在，改为只发 DELETE（把 `project_id` 与 `task_id` 作为参数：`spawn_push_deleted(project_id, task_id)`）。

预设分组里新建、或复制出的新行：尚无 `ticktick_task_id`。开关打开且 `write_target` 有值时，把 `ticktick_dirty` 设为 1 并 `spawn_push_one`。开关关闭时不标脏、不推送。没有写入清单时不标脏。`upsert_task` 与 `duplicate_task` 改为返回 `Result<TaskWriteResult, String>`：

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskWriteResult {
    pub task: Option<TaskView>,
    pub warning: Option<String>,
}
```

`upsert_task` 的 `task` 为 `None`。没有写入清单时 `warning` 为「主线还没有 TickTick 清单，这条任务只保存在本机。」组名按预设分组用「主线」「支线」「长期」「杂项」。其它成功路径 `warning` 为 `None`。`duplicate_task` 的 `task` 仍是新行。

`src/lib/api.ts`：`upsertTask` 返回 `Promise<TaskWriteResult>`，`duplicateTask` 同样。`Tasks.tsx`、`Today.tsx` 在 `warning` 非空时调用已有的 `addToast`。`TaskDetailDialog` 与 `TaskDateCard` 增加可选 `onWarning?: (text: string) => void`，由这两个页面传入 `addToast`。Task 8 改这些调用。

- [ ] **Step 4: Run test to verify it passes**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife ticktick:: -- --nocapture && /opt/homebrew/bin/cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ticktick.rs src-tauri/src/lib.rs src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
feat: push and pull the TickTick task mirror

EOF
)"
```

---

### Task 7: 命令、OAuth、每小时检查

**Files:**
- Modify: `src-tauri/src/ticktick.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/sampler.rs`
- Modify: `src-tauri/src/commands.rs`（`save_settings` 发现开关从 false 变 true 时调用 `ticktick::spawn_pull`）
- Test: `src-tauri/src/ticktick.rs`、`src-tauri/src/sampler.rs` 若已有循环测试则只加纯函数测试，不要起真实采样线程

**Interfaces:**
- Consumes: `run_pull`、`spawn_push_one`、密钥槽、`meta_get` / `meta_set`
- Produces: 下列 `#[tauri::command]`，全部 `async`：
  - `ticktick_status() -> TickTickStatus`
  - `ticktick_set_client_secret(secret: String) -> Result<(), String>`
  - `ticktick_disconnect() -> Result<(), String>`
  - `ticktick_refresh_projects() -> Result<TickTickStatus, String>`
  - `ticktick_sync_now() -> Result<TickTickStatus, String>`
  - `ticktick_connect() -> Result<TickTickStatus, String>`
  - `pub fn maybe_spawn_sync(conn: &Connection, now: i64)`
  - `pub fn authorize_url(client_id: &str, redirect: &str, state: &str, challenge: &str) -> String`

`TickTickStatus`（serde camelCase）：

```rust
pub struct TickTickStatus {
    pub enabled: bool,
    pub connected: bool,
    pub client_id: String,
    pub projects: Vec<TickTickProjectView>, // id, name, sort_order, role
    pub write_targets: BTreeMap<String, String>, // role -> project name
    pub last_sync_at: Option<i64>,
    pub last_result: String,
}
```

`ticktick_status` 只读 config、`app_meta`、以及「access token 是否非空」。不发 HTTP。`connected` 为 token 非空。

`ticktick_refresh_projects`：GET `/project`，`stamp_missing_roles` 后保存 `ticktick_project_roles`，项目 JSON 写入 `ticktick_projects_json`。未连接则 `Err("尚未连接")`。

`ticktick_sync_now`：开关关闭返回 `Err` 且不发请求，文案「同步到任务板已关闭」。未连接返回「尚未连接」。否则先 `run_pull`。拉取结束后，对仍然 `ticktick_dirty == 2` 的行各 `POST /task` 一次：成功则写上 id 并清脏；再次没有 id 则保持 2，`last_result` 为「有任务需要手动确认」。每小时的 `maybe_spawn_sync` 不跑这第二次创建。

`ticktick_disconnect`：删掉 access 与 refresh token。不删任务，不清 `ticktick_task_id`。

`ticktick_connect`：Client ID 为空则「请先填写 Client ID」。起 `127.0.0.1:0` 的一次性 TCP 监听，把实际端口放进 redirect `http://127.0.0.1:{port}/callback`。浏览器打开 `authorize_url`。scope 查询参数是 `tasks:write`。PKCE：verifier 是 64 字节随机的 base64url，challenge 是 SHA-256 后再 base64url。state 随机。回调校验 state，用 code 换 token，写入两个 token 槽。超时 180 秒，失败不改已有 token。换票 `POST https://ticktick.com/oauth/token`。

`authorize_url` 必须包含 `client_id`、`redirect_uri`、`code_challenge`、`code_challenge_method=S256`、`scope=tasks%3Awrite` 或未编码的 `tasks:write`、`state`。用测试锁住这个字符串，不听端口。

`maybe_spawn_sync`：`!ticktick_enabled` 或没有 token 则返回。`last_sync_at` 为空或 `now - last >= 3600` 则 `thread::spawn` 调用 `run_pull`。采样循环在现有 `SYNC_CHECK_EVERY_TICKS` 分支里加上这一句，紧挨 `sync::maybe_spawn_sync`。

`save_settings`：旧值 enabled false、新值 true 时 `ticktick::spawn_pull()`。

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn authorize_url_asks_for_task_write_and_s256() {
    let url = authorize_url("cid", "http://127.0.0.1:9/callback", "st", "ch");
    assert!(url.starts_with("https://ticktick.com/oauth/authorize?"));
    assert!(url.contains("client_id=cid"));
    assert!(url.contains("scope=tasks%3Awrite") || url.contains("scope=tasks:write"));
    assert!(url.contains("code_challenge=ch"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("state=st"));
}

#[test]
fn maybe_spawn_guard_is_due_only_after_an_hour() {
    assert!(!ticktick_due(Some(1_000), 1_000 + 3599));
    assert!(ticktick_due(Some(1_000), 1_000 + 3600));
    assert!(ticktick_due(None, 50));
}
```

把到点判断抽成 `pub(crate) fn ticktick_due(last: Option<i64>, now: i64) -> bool`，采样测试不必真的 spawn。

- [ ] **Step 2: Run test to verify it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife authorize_url_asks_for_task_write_and_s256 -- --nocapture`

Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

按接口实现。`lib.rs` 的 `generate_handler!` 加上六个命令。`use commands::...` 不要把 TickTick 命令塞进 `commands.rs`；在 `lib.rs` 里 `use ticktick::{ticktick_status, ...}`。

- [ ] **Step 4: Run test to verify it passes**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife authorize_url_asks_for_task_write_and_s256 ticktick_due -- --nocapture`

然后：`/opt/homebrew/bin/cargo test --offline -p gamelife`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ticktick.rs src-tauri/src/lib.rs src-tauri/src/sampler.rs src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
feat: connect TickTick and sync it hourly

EOF
)"
```

---

### Task 8: 设置页

**Files:**
- Create: `src/lib/ticktickSettings.ts`
- Create: `src/lib/ticktickSettings.test.ts`
- Modify: `src/lib/api.ts`（`upsertTask` / `duplicateTask` 改为返回 `TaskWriteResult`）
- Modify: `src/pages/Tasks.tsx`、`src/pages/Today.tsx`、`src/components/TaskDetailDialog.tsx`、`src/components/TaskDateCard.tsx`（`warning` 交给 `addToast` / `onWarning`）
- Modify: `src/lib/preview/fixtures.ts`
- Modify: `src/lib/preview/` 里分发 `invoke` 的表（搜索 `sync_status` 的那个文件）
- Modify: `src/pages/Settings.tsx`

**Interfaces:**
- Consumes: `TickTickStatus` 的 camelCase 形状与 Task 7 命令名
- Produces:

```typescript
export function syncNowDisabled(enabled: boolean, connected: boolean): boolean {
  return !enabled || !connected;
}

export function roleForProject(
  roles: Record<string, string> | undefined,
  projectId: string,
): "ignore" | "mainline" | "side" | "longterm" | "chore" {
  const raw = roles?.[projectId];
  if (raw === "mainline" || raw === "side" || raw === "longterm" || raw === "chore" || raw === "ignore") {
    return raw;
  }
  return "ignore";
}

export function writeTargetHint(projectName: string | null): string | null {
  if (!projectName) return null;
  return `新建任务写入「${projectName}」`;
}
```

`AppSettings` 增加 `ticktickEnabled: boolean`、`ticktickClientId: string`、`ticktickProjectRoles: Record<string, string>`。不要加 column roles。`PREVIEW_SETTINGS` 补上这三项，enabled 为 false，roles 为 `{}`。

- [ ] **Step 1: Write the failing test**

`src/lib/ticktickSettings.test.ts`：

```typescript
import { describe, expect, it } from "vitest";
import { roleForProject, syncNowDisabled, writeTargetHint } from "./ticktickSettings";

describe("ticktick settings", () => {
  it("disables immediate sync until the switch is on and the account is connected", () => {
    expect(syncNowDisabled(false, true)).toBe(true);
    expect(syncNowDisabled(true, false)).toBe(true);
    expect(syncNowDisabled(true, true)).toBe(false);
  });

  it("defaults an unseen list to ignore", () => {
    expect(roleForProject({ p: "mainline" }, "p")).toBe("mainline");
    expect(roleForProject({}, "new")).toBe("ignore");
  });

  it("names the write target", () => {
    expect(writeTargetHint("论文")).toBe("新建任务写入「论文」");
    expect(writeTargetHint(null)).toBeNull();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/ticktickSettings`

Expected: FAIL，模块不存在。

- [ ] **Step 3: Write minimal implementation**

实现上面三个函数。

`api.ts`：

```typescript
export interface TickTickProject {
  id: string;
  name: string;
  sortOrder: number;
  role: string;
}

export interface TickTickStatus {
  enabled: boolean;
  connected: boolean;
  clientId: string;
  projects: TickTickProject[];
  writeTargets: Record<string, string>;
  lastSyncAt: number | null;
  lastResult: string;
}

export function ticktickStatus(): Promise<TickTickStatus> {
  return invoke("ticktick_status");
}
export function ticktickSetClientSecret(secret: string): Promise<void> {
  return invoke("ticktick_set_client_secret", { secret });
}
export function ticktickConnect(): Promise<TickTickStatus> {
  return invoke("ticktick_connect");
}
export function ticktickDisconnect(): Promise<TickTickStatus> {
  return invoke("ticktick_disconnect");
}
export function ticktickRefreshProjects(): Promise<TickTickStatus> {
  return invoke("ticktick_refresh_projects");
}
export function ticktickSyncNow(): Promise<TickTickStatus> {
  return invoke("ticktick_sync_now");
}
```

预览分发表对这六个名字返回固定 `TickTickStatus`（`connected: false`，`projects: []`，`lastResult: ""`）。`ticktick_set_client_secret` 返回 `null`。

`Settings.tsx` 的 `TABS` 在 cloud 与 permissions 之间插入 `{ value: "ticktick", label: "TickTick" }`。`SettingsTab` 联合类型加上 `"ticktick"`。

新面板用现有 `Section`、`ToggleRow`、`Field`、`Input`、`Select`、`Button`。结构：

- 标题「TickTick」。说明「把已映射清单里的未完成任务同步到任务板的四个预设分组。」
- `ToggleRow` 标题「同步到任务板」，说明「关闭时不同步任务。已导入的任务留在任务板上。」绑定 `settings.ticktickEnabled`，保存走现有 `saveSettings`。
- 开关下方一行显示 `lastResult`；`lastSyncAt` 有值时再显示本地化时间。不要写 hex。
- Client ID 绑定 `ticktickClientId`。空时 hint：「到 TickTick 开发者中心建应用，Redirect URI 填连接时显示的回跳地址。」
- Client Secret 是空的密码框，placeholder「已保存则留空」。失焦且非空才 `ticktickSetClientSecret`。
- 按钮「连接」「断开」。断开仅 `connected` 时可用。
- 清单列表：每个项目一个 `Select`，选项忽略 / 主线 / 支线 / 长期 / 杂项，值就是 `ignore` 等五个英文角色。变更写入 `ticktickProjectRoles` 并 `saveSettings`。
- 每个角色若 `writeTargets[role]` 有名字，在清单区上面显示 `writeTargetHint`。
- 「刷新清单」在 `connected` 时可用，与总开关无关。「立即同步」的 `disabled` 用 `syncNowDisabled(settings.ticktickEnabled, status.connected)`。

进入该标签时调用 `ticktickStatus()`，不要调用 refresh 或 sync。

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src`

Expected: PASS

预览页面没有浏览器自动化要求：本任务不改布局行为以外的交互路径；vitest 覆盖纯函数。实现者打开 `npm run tauri dev` 时用眼睛看一眼 TickTick 标签是否在云端和权限之间，这一步不作为提交门槛。

- [ ] **Step 5: Commit**

```bash
git add src/lib/ticktickSettings.ts src/lib/ticktickSettings.test.ts src/lib/api.ts src/lib/preview src/pages/Settings.tsx
git commit -m "$(cat <<'EOF'
feat: add the TickTick settings switch

EOF
)"
```

---

### Task 9: 文档与规则

**Files:**
- Modify: `CLAUDE.md`
- Modify: `AGENTS.md`
- Modify: `.cursor/rules/specs.mdc`
- Modify: `.cursor/rules/tauri-shell.mdc`

**Interfaces:**
- Consumes: 规格路径
- Produces: 文档不再写「TickTick 令牌不读取、设置页没有 TickTick」

- [ ] **Step 1: 改这四处**

`CLAUDE.md` / `AGENTS.md`：计划本仍是本地任务。TickTick 在总开关打开时把已映射清单导入四个预设分组，并双向同步标题、时段、完成和删除。令牌在 `secrets.json`。`ticktick_cache` 仍不读、不上传。判定仍不读 TickTick。`user_version` 写 6。

`.cursor/rules/specs.mdc` 的规格列表加上 `2026-09-22-ticktick-task-sync-design.md`，并写明它覆盖 2026-09-15 §11 的「删除 TickTick」和 2026-09-13 §7 的只读限制。

`.cursor/rules/tauri-shell.mdc`：删掉「leftover TickTick secrets are not read」。改成开关关闭时 TickTick 模块不拉取、不写回；密钥只在 `secrets.json`。

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md AGENTS.md .cursor/rules/specs.mdc .cursor/rules/tauri-shell.mdc
git commit -m "$(cat <<'EOF'
docs: describe TickTick as a task-board mirror

EOF
)"
```

没有新的自动化测试。改完用搜索确认 `AGENTS.md` 里不再出现 “leftover TickTick secrets in secrets.json are not read”。
