# TickTick Column Roles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 刷新清单后，设置页能展开每一份 TickTick 清单并给分组指定角色；分组的显式角色覆盖整份清单，未设置的分组跟随清单。

**Architecture:** 最终角色、该拉哪些清单、分组排序和角色裁剪是 `gamelife-core` 的纯函数。`src-tauri/src/ticktick.rs` 在刷新时为每份清单读 `GET /project/{id}/data`，把 `columns` 写入现有的 `ticktick_projects_json`，同步时用最终角色决定导入、改分组或只删本机行。设置页只读这份缓存。授权 scope 改为 `tasks:read tasks:write`。

**Tech Stack:** Tauri 2、Rust、`reqwest` blocking、React、TypeScript、vitest、SQLite。构建用 Homebrew `/opt/homebrew/bin/cargo`。

**Spec:** `docs/superpowers/specs/2026-09-23-ticktick-column-roles-design.md`

## Global Constraints

- 本文只改规格列出的覆盖点。2026-09-22 的其余规则仍有效：总开关默认关闭；关闭时不拉取、不写回；只导入未完成任务；子任务、备注、重复规则不同步；判定不读 TickTick，不读 `ticktick_cache`；打开设置不打网络；令牌不进 `config.json`，不回传给页面。
- 官方主机仍是 `https://api.ticktick.com/open/v1`。授权页仍是 `https://ticktick.com/oauth/authorize`。scope 改为 `tasks:read tasks:write`（空格按 OAuth 百分号编码）。不接 `dida365.com`，不用非官方接口。不读取、不迁移旧槽 `ticktick-access`。
- 分组角色只写入 `config.json` 的 `ticktickColumnRoles`。合法值只有 `ignore` / `mainline` / `side` / `longterm` / `chore`。没有这把键表示跟随清单。不存储 `inherit`。新分组不写成忽略。
- 清单角色仍是那五个值。缺键的新清单在刷新时写成 `ignore`。
- 写入目标不变：该角色下 `sortOrder` 最小的清单，并列时清单 id 字典序较小。创建请求不带 `columnId`。
- 最终角色是忽略时，只在全量成功后删本机行。不发远端删除，也不把本机未写回的修改推上去。任一决定要拉的清单或完成列表失败：不删已链接任务，不前移 `ticktick_last_sync_at`。
- `user_version` 仍是 6。任务行不加分组列。判定快照仍是 `id` / `title` / `role`。
- 界面中文。能量称「能量」。组件里不写 hex。页面只经 `src/lib/api.ts` 调 `invoke()`。
- 改 `src-tauri/`：`/opt/homebrew/bin/cargo test --offline -p gamelife` 必须跑完。不要设 `CARGO_TARGET_DIR`。
- 改 `crates/gamelife-core/`：`/opt/homebrew/bin/cargo test --offline -p gamelife-core`。
- 改 `src/`：`npx vitest run --dir src`。
- 测试禁止 `std::env::set_var("HOME", …)`。刷新测试不得调用 `load_settings()` / `save_settings()`，以免写到用户的 `config.json`。
- 每个任务一条英文 conventional commit，不 `--no-verify`。只 `git add` 该任务 Files 列出的路径。不要提交 `.workbuddy-ai/`，也不要把工作区里与本任务无关的未提交改动加进去。
- 同步不发币，不改已结束的槽。

## File Structure

| 路径 | 职责 |
| --- | --- |
| `crates/gamelife-core/src/ticktick_sync.rs` | `RemoteColumn`、`effective_role`、`projects_to_fetch`、`parse_columns`、`sort_columns`、`kept_column_ids`、对账时按最终角色插入 / 改分组 / `DropIgnored` |
| `src-tauri/src/ticktick.rs` | 把分组角色传进拉取；刷新时合并 `columns`；权限不足与部分失败；授权 scope；本机删除不发远端 DELETE |
| `src-tauri/src/config.rs` | `ticktickColumnRoles` 读写 `config.json`，仍不进 policy 快照 |
| `src/lib/api.ts` | 设置与状态类型带上分组 |
| `src/lib/ticktickSettings.ts` | 跟随清单、空分组文案、分组排序 |
| `src/pages/Settings.tsx` | 清单展开和分组下拉 |
| `src/lib/preview/fixtures.ts` | 预览数据含分组 |
| `CLAUDE.md`、`AGENTS.md`、`.cursor/rules/specs.mdc`、`.cursor/rules/tauri-shell.mdc` | 指向新规格 |
| `docs/superpowers/specs/2026-09-22-ticktick-task-sync-design.md` | 文首加一句后续覆盖，避免执行者只读旧句 |

不新增模块，不升 `user_version`。

---

### Task 1: 最终角色与对账

**Files:**
- Modify: `crates/gamelife-core/src/ticktick_sync.rs`
- Modify: `src-tauri/src/ticktick.rs`（只为新字段和签名补编译；行为仍把空的分组角色传进去）
- Test: 同上两个文件里的既有测试模块

**Interfaces:**
- Consumes: 现有 `parse_role`、`list_id_for_role`、`write_target`、`ReconcileInput`、`ReconcileAction`
- Produces:
  - `pub struct RemoteColumn { pub id: String, pub name: String, pub sort_order: i64, pub project_id: String }`
  - `RemoteProject` 增加 `pub columns: Vec<RemoteColumn>`
  - `RemoteTask` 增加 `pub column_id: Option<String>`
  - `pub enum StoredRole { Ignore, Role(ListRole) }`
  - `pub fn parse_stored_role(raw: &str) -> Option<StoredRole>`
  - `pub fn effective_role(project_roles: &BTreeMap<String, String>, column_roles: &BTreeMap<String, String>, project_id: &str, column_id: Option<&str>) -> Option<ListRole>`
  - `pub fn projects_to_fetch<'a>(projects: &'a [RemoteProject], project_roles: &BTreeMap<String, String>, column_roles: &BTreeMap<String, String>) -> Vec<&'a RemoteProject>`
  - `pub fn sort_columns(columns: &mut [RemoteColumn])`
  - `pub fn parse_columns(project_id: &str, value: &serde_json::Value) -> Vec<RemoteColumn>`
  - `pub fn kept_column_ids(projects: &[RemoteProject]) -> BTreeSet<String>`
  - `pub fn prune_column_roles(kept: &BTreeSet<String>, column_roles: &mut BTreeMap<String, String>)`
  - `ReconcileAction::DropIgnored { local_id: String }`
  - `ReconcileInput` 增加 `pub column_roles: &'a BTreeMap<String, String>`
  - `pull_round(..., roles, column_roles, first_sync, ...)`
  - `run_pull_body_with(..., roles, column_roles)`

- [ ] **Step 1: Write the failing core tests**

在 `crates/gamelife-core/src/ticktick_sync.rs` 的 `mod tests` 末尾追加。先改两个帮手，否则新字段还没落地时测试写不下去；这一步只加测试，帮手和字段在 Step 3 一起补。若编译器在 Step 2 之前就要求字段存在，把 Step 3 的结构体字段先加上、函数保持未实现，让这些测试因 `effective_role` 未定义而失败。

```rust
fn column(id: &str, project: &str, sort_order: i64) -> RemoteColumn {
    RemoteColumn {
        id: id.into(),
        name: id.into(),
        sort_order,
        project_id: project.into(),
    }
}

#[test]
fn effective_role_column_overrides_list_including_ignore() {
    let mut lists = BTreeMap::new();
    lists.insert("p".into(), "mainline".into());
    let mut columns = BTreeMap::new();
    columns.insert("c-ignore".into(), "ignore".into());
    columns.insert("c-side".into(), "side".into());
    assert_eq!(
        effective_role(&lists, &columns, "p", Some("c-ignore")),
        None
    );
    assert_eq!(
        effective_role(&lists, &columns, "p", Some("c-side")),
        Some(ListRole::Side)
    );
    assert_eq!(
        effective_role(&lists, &columns, "p", Some("c-new")),
        Some(ListRole::Mainline)
    );
    assert_eq!(
        effective_role(&lists, &columns, "p", None),
        Some(ListRole::Mainline)
    );
    columns.insert("c-side".into(), "inherit".into());
    assert_eq!(
        effective_role(&lists, &columns, "p", Some("c-side")),
        Some(ListRole::Mainline)
    );
}

#[test]
fn effective_role_ignored_list_imports_explicit_column() {
    let mut lists = BTreeMap::new();
    lists.insert("p".into(), "ignore".into());
    let columns = BTreeMap::new();
    assert_eq!(effective_role(&lists, &columns, "p", Some("c-new")), None);
    let mut columns = BTreeMap::new();
    columns.insert("c-main".into(), "mainline".into());
    assert_eq!(
        effective_role(&lists, &columns, "p", Some("c-main")),
        Some(ListRole::Mainline)
    );
}

#[test]
fn projects_to_fetch_includes_ignore_list_only_for_mapped_column() {
    let mut project = proj("p", 1);
    project.columns = vec![column("c1", "p", 1), column("c2", "p", 2)];
    let projects = vec![project, proj("q", 2)];
    let mut lists = BTreeMap::new();
    lists.insert("p".into(), "ignore".into());
    lists.insert("q".into(), "side".into());
    let mut columns = BTreeMap::new();
    columns.insert("c1".into(), "ignore".into());
    assert_eq!(
        projects_to_fetch(&projects, &lists, &columns)
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        vec!["q"]
    );
    columns.insert("c2".into(), "mainline".into());
    assert_eq!(
        projects_to_fetch(&projects, &lists, &columns)
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        vec!["p", "q"]
    );
}

#[test]
fn write_target_ignores_column_roles() {
    let mut later = proj("b", 5);
    later.columns = vec![column("c-main", "b", 0)];
    let projects = vec![proj("a", 1), later];
    let mut lists = BTreeMap::new();
    lists.insert("a".into(), "mainline".into());
    lists.insert("b".into(), "side".into());
    let target = write_target(&projects, &lists, ListRole::Mainline).unwrap();
    assert_eq!(target.id, "a");
}

#[test]
fn parse_columns_and_task_column_id() {
    let data = serde_json::json!({
        "columns": [
            {"id": "c2", "name": "后", "sortOrder": 2, "projectId": "p"},
            {"id": "c1", "name": "先", "sortOrder": 1, "projectId": "p"}
        ],
        "tasks": [{
            "id": "t1",
            "title": "写稿",
            "status": 0,
            "columnId": "c1",
            "etag": "e"
        }]
    });
    let mut columns = parse_columns("p", &data);
    sort_columns(&mut columns);
    assert_eq!(columns[0].id, "c1");
    assert_eq!(columns[0].name, "先");
    assert_eq!(columns[0].sort_order, 1);
    assert_eq!(columns[1].id, "c2");
    let tasks = parse_open_tasks("p", &data);
    assert_eq!(tasks[0].column_id.as_deref(), Some("c1"));
}

#[test]
fn prune_column_roles_drops_ids_that_are_not_kept() {
    let mut roles = BTreeMap::new();
    roles.insert("keep".into(), "side".into());
    roles.insert("gone".into(), "mainline".into());
    let mut kept = BTreeSet::new();
    kept.insert("keep".into());
    prune_column_roles(&kept, &mut roles);
    assert_eq!(roles.get("keep").map(String::as_str), Some("side"));
    assert!(roles.get("gone").is_none());
}

#[test]
fn reconcile_drops_ignored_column_without_inserting_it() {
    let mut project = proj("p", 1);
    project.columns = vec![column("c-ignore", "p", 1), column("c-main", "p", 2)];
    let projects = vec![project];
    let mut lists = BTreeMap::new();
    lists.insert("p".into(), "mainline".into());
    let mut columns = BTreeMap::new();
    columns.insert("c-ignore".into(), "ignore".into());
    let local = mirror("L1", Some("t-ignore"), "p", 1);
    let fetches = vec![ProjectFetch {
        project_id: "p".into(),
        ok: true,
        open: vec![
            open_column("t-ignore", "p", "写稿", "e1", Some("c-ignore")),
            open_column("t-skip", "p", "不导入", "e3", Some("c-ignore")),
            open_column("t-new", "p", "新任务", "e2", Some("c-main")),
        ],
    }];
    let actions = reconcile(&ReconcileInput {
        local: &[local],
        projects: &projects,
        roles: &lists,
        column_roles: &columns,
        fetches: &fetches,
        completed_ids: &[],
        completed_ok: true,
        first_sync: false,
        day_start: 0,
        next_day_start: 86_400,
    });
    assert!(actions.iter().any(|a| matches!(
        a,
        ReconcileAction::DropIgnored { local_id } if local_id == "L1"
    )));
    assert!(actions.iter().all(|a| !matches!(a, ReconcileAction::Delete { .. })));
    assert!(actions.iter().all(|a| !matches!(
        a,
        ReconcileAction::Insert(row) if row.ticktick_task_id.as_deref() == Some("t-skip")
    )));
    let inserted = actions.iter().find_map(|a| match a {
        ReconcileAction::Insert(row) => Some(row),
        _ => None,
    });
    assert_eq!(inserted.unwrap().list_id, PRESET_MAINLINE_ID);
    assert_eq!(inserted.unwrap().ticktick_task_id.as_deref(), Some("t-new"));
}

#[test]
fn reconcile_ignored_list_inserts_explicit_mainline_column() {
    let mut project = proj("p", 1);
    project.columns = vec![column("c-main", "p", 1)];
    let projects = vec![project];
    let mut lists = BTreeMap::new();
    lists.insert("p".into(), "ignore".into());
    let mut columns = BTreeMap::new();
    columns.insert("c-main".into(), "mainline".into());
    let fetches = vec![ProjectFetch {
        project_id: "p".into(),
        ok: true,
        open: vec![open_column("t1", "p", "写稿", "e", Some("c-main"))],
    }];
    let actions = reconcile(&ReconcileInput {
        local: &[],
        projects: &projects,
        roles: &lists,
        column_roles: &columns,
        fetches: &fetches,
        completed_ids: &[],
        completed_ok: true,
        first_sync: false,
        day_start: 0,
        next_day_start: 86_400,
    });
    match &actions[0] {
        ReconcileAction::Insert(row) => assert_eq!(row.list_id, PRESET_MAINLINE_ID),
        other => panic!("expected insert, got {other:?}"),
    }
}

#[test]
fn reconcile_moves_list_only_when_the_round_fully_succeeds() {
    let mut project = proj("p", 1);
    project.columns = vec![column("c-side", "p", 1)];
    let other = proj("q", 2);
    let projects = vec![project, other];
    let mut lists = BTreeMap::new();
    lists.insert("p".into(), "mainline".into());
    lists.insert("q".into(), "side".into());
    let mut columns = BTreeMap::new();
    columns.insert("c-side".into(), "side".into());
    let local = mirror("L1", Some("t1"), "p", 0);
    let open = vec![open_column("t1", "p", "改标题", "e2", Some("c-side"))];
    let full = reconcile(&ReconcileInput {
        local: &[local.clone()],
        projects: &projects,
        roles: &lists,
        column_roles: &columns,
        fetches: &[
            ProjectFetch { project_id: "p".into(), ok: true, open: open.clone() },
            ProjectFetch { project_id: "q".into(), ok: true, open: vec![] },
        ],
        completed_ids: &[],
        completed_ok: true,
        first_sync: false,
        day_start: 0,
        next_day_start: 86_400,
    });
    match &full[0] {
        ReconcileAction::Update(row) => {
            assert_eq!(row.list_id, PRESET_SIDE_ID);
            assert_eq!(row.title, "改标题");
        }
        other => panic!("expected update, got {other:?}"),
    }
    let partial = reconcile(&ReconcileInput {
        local: &[local],
        projects: &projects,
        roles: &lists,
        column_roles: &columns,
        fetches: &[
            ProjectFetch { project_id: "p".into(), ok: true, open },
            ProjectFetch { project_id: "q".into(), ok: false, open: vec![] },
        ],
        completed_ids: &[],
        completed_ok: false,
        first_sync: false,
        day_start: 0,
        next_day_start: 86_400,
    });
    assert!(partial.iter().all(|a| !matches!(a, ReconcileAction::DropIgnored { .. })));
    match &partial[0] {
        ReconcileAction::Update(row) => {
            assert_eq!(row.list_id, PRESET_MAINLINE_ID);
            assert_eq!(row.title, "改标题");
        }
        other => panic!("expected title update, got {other:?}"),
    }
}
```

`open_column` 与现有 `open` 相同，多一个 `column_id: Option<&str>`。现有 `open` 改为调用它并传入 `None`。

每个已有的 `ReconcileInput {` 补上 `column_roles: &BTreeMap::new(),`。`proj` 和 `open` 补上新字段。

- [ ] **Step 2: Run the tests to verify they fail**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core effective_role_column_overrides_list_including_ignore -- --nocapture`

Expected: FAIL，`effective_role` 找不到。

- [ ] **Step 3: Implement the core functions and reconcile**

`RemoteProject` 增加 `pub columns: Vec<RemoteColumn>`。`RemoteTask` 增加 `pub column_id: Option<String>`。`parse_projects` 从对象的 `columns` 填这个字段，没有该键则为空向量。`parse_open_tasks` 读取非空 `columnId`。

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteColumn {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
    pub project_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredRole {
    Ignore,
    Role(ListRole),
}

pub fn parse_stored_role(raw: &str) -> Option<StoredRole> {
    match raw {
        "ignore" => Some(StoredRole::Ignore),
        "mainline" => Some(StoredRole::Role(ListRole::Mainline)),
        "side" => Some(StoredRole::Role(ListRole::Side)),
        "longterm" => Some(StoredRole::Role(ListRole::Longterm)),
        "chore" => Some(StoredRole::Role(ListRole::Chore)),
        _ => None,
    }
}

pub fn effective_role(
    project_roles: &BTreeMap<String, String>,
    column_roles: &BTreeMap<String, String>,
    project_id: &str,
    column_id: Option<&str>,
) -> Option<ListRole> {
    if let Some(cid) = column_id.filter(|s| !s.is_empty()) {
        if let Some(stored) = column_roles.get(cid).and_then(|raw| parse_stored_role(raw)) {
            return match stored {
                StoredRole::Ignore => None,
                StoredRole::Role(role) => Some(role),
            };
        }
    }
    project_roles
        .get(project_id)
        .and_then(|raw| parse_role(raw))
}

pub fn projects_to_fetch<'a>(
    projects: &'a [RemoteProject],
    project_roles: &BTreeMap<String, String>,
    column_roles: &BTreeMap<String, String>,
) -> Vec<&'a RemoteProject> {
    projects
        .iter()
        .filter(|project| {
            let list_mapped = project_roles
                .get(&project.id)
                .and_then(|raw| parse_role(raw))
                .is_some();
            if list_mapped {
                return true;
            }
            project.columns.iter().any(|column| {
                column_roles
                    .get(&column.id)
                    .and_then(|raw| parse_role(raw))
                    .is_some()
            })
        })
        .collect()
}

pub fn sort_columns(columns: &mut [RemoteColumn]) {
    columns.sort_by(|left, right| (left.sort_order, &left.id).cmp(&(right.sort_order, &right.id)));
}

pub fn parse_columns(project_id: &str, value: &serde_json::Value) -> Vec<RemoteColumn> {
    let Some(arr) = value.get("columns").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            let id = json_string_field(obj, "id")?;
            let name = json_string_field(obj, "name").unwrap_or_default();
            let sort_order = json_i64_field(obj, "sortOrder").unwrap_or(0);
            let project_id = json_string_field(obj, "projectId").unwrap_or_else(|| project_id.to_string());
            Some(RemoteColumn { id, name, sort_order, project_id })
        })
        .collect()
}

pub fn kept_column_ids(projects: &[RemoteProject]) -> BTreeSet<String> {
    projects
        .iter()
        .flat_map(|project| project.columns.iter().map(|column| column.id.clone()))
        .collect()
}

pub fn prune_column_roles(kept: &BTreeSet<String>, column_roles: &mut BTreeMap<String, String>) {
    column_roles.retain(|id, _| kept.contains(id));
}
```

`ReconcileAction` 增加 `DropIgnored { local_id: String }`。`ReconcileInput` 增加 `column_roles`。

`reconcile` 里把 `mapped` 改成 `projects_to_fetch(input.projects, input.roles, input.column_roles)` 的 id 集合。`failed` 仍是这批 id 里没有 `ok` 拉取的那些。`allow_delete` 的条件不变。

脏值为 2 的匹配循环保持原样，但候选里去掉 `effective_role(...) == None` 的远端任务，避免把忽略分组链到本机。

已链接行（`ticktick_task_id` 有值，含脏行）若出现在未完成集合里：

- `effective_role` 为 `None` 且 `allow_delete`：只发 `DropIgnored`，不发 `Delete`，不发 `Update`。
- `effective_role` 为 `None` 且不是全量成功：什么都不发。
- 有最终角色且 `ticktick_dirty != 0`：什么都不发（本轮脏行仍赢）。
- 有最终角色且脏值为 0：沿用现在的标题、时段、etag、完成变化。`apply_remote_fields` 用 `effective_role` 得到 `list_id`。不是全量成功时，把 `list_id` 改回原来的本机分组。只有这样算出的 `list_id` 变了，或者原来的那些字段变了，才发 `Update`。

远端 id 不在未完成集合里时，仍只处理脏值为 0 的行：完成列表和原来的 `Delete` 规则不变。不要对「远端还在、只是最终角色变成忽略」发 `Delete`。

插入循环用 `effective_role` 代替 `role_for_project`。`None` 就跳过。

`src-tauri/src/ticktick.rs`：

- 每处 `RemoteProject { ... }` 加 `columns: Vec::new()`。`mainline_roles` 和 `pull_round_partial_project_failure_does_not_advance_or_delete` 里的两处都算。
- `pull_round` 在 `roles` 后增加 `column_roles: &BTreeMap<String, String>`，传进 `ReconcileInput`。决定要拉的 id 改为 `projects_to_fetch(projects, roles, column_roles)`，空则仍直接返回、不打网络。完成列表的 `projectIds` 用这批 id。可以删掉只被这里使用的 `mapped_project_ids`。
- `run_pull_body_with` 增加同样的 `column_roles` 参数并传给 `pull_round`。`run_pull_body` 传入 `settings.ticktick_column_roles`（这一步映射还是空的，因为配置仍丢弃该字段）。
- 两处测试里的 `pull_round` 调用在 `roles` 后加 `&BTreeMap::new()`。`run_pull_body_with` 的调用补上 `&BTreeMap::new()`。
- `apply_reconcile_actions` 增加 `DropIgnored`：不看 `ticktick_dirty`，调用已有的 `delete_task_row`。`Update` / `Delete` 的脏行保护不要放宽。

- [ ] **Step 4: Run the tests to verify they pass**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife-core ticktick_sync::`

Expected: PASS

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife ticktick::`

Expected: PASS。既有拉取测试仍通过，因为传入的分组角色是空的。

- [ ] **Step 5: Commit**

```bash
git add crates/gamelife-core/src/ticktick_sync.rs src-tauri/src/ticktick.rs
git commit -m "$(cat <<'EOF'
feat: classify TickTick tasks by column role

EOF
)"
```

---

### Task 2: 把分组角色写入配置

**Files:**
- Modify: `src-tauri/src/config.rs`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/preview/fixtures.ts`
- Test: `src-tauri/src/config.rs` 的 `ticktick_switch_and_roles_roundtrip_and_column_roles_are_dropped`

**Interfaces:**
- Consumes: `AppSettings.ticktick_column_roles`
- Produces: JSON 键 `ticktickColumnRoles`，`Record<string, string>`。policy 快照仍不包含它。

- [ ] **Step 1: Write the failing test**

把 `ticktick_switch_and_roles_roundtrip_and_column_roles_are_dropped` 改成下面这个测试，并改名。

```rust
#[test]
fn ticktick_column_roles_roundtrip_and_stay_out_of_policy() {
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
    assert_eq!(
        parsed.ticktick_column_roles.get("c").map(String::as_str),
        Some("side")
    );
    let out = serde_json::to_value(&parsed).unwrap();
    assert_eq!(
        out.get("ticktickColumnRoles")
            .and_then(|v| v.get("c"))
            .and_then(|v| v.as_str()),
        Some("side")
    );
    let a = default_settings();
    let mut b = default_settings();
    b.ticktick_enabled = true;
    b.ticktick_client_id = "cid".into();
    b.ticktick_redirect_uri = "http://127.0.0.1:18789/callback".into();
    b.ticktick_project_roles.insert("p".into(), "mainline".into());
    b.ticktick_column_roles.insert("c".into(), "side".into());
    assert_eq!(policy_snapshot_json(&a), policy_snapshot_json(&b));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife ticktick_column_roles_roundtrip_and_stay_out_of_policy -- --nocapture`

Expected: FAIL。反序列化后 `c` 不在映射里，或序列化结果没有 `ticktickColumnRoles`。

- [ ] **Step 3: Persist the map**

去掉 `ticktick_column_roles` 上的 `skip_serializing` 和 `skip_deserializing`，保留 `#[serde(default)]`。`policy_snapshot_json` 继续用手写的政策字段对象，不要把整个 `AppSettings` 放进去。

`src/lib/api.ts` 的 `AppSettings` 增加：

```ts
/** columnId → ignore | mainline | side | longterm | chore. Missing key follows the list. */
ticktickColumnRoles: Record<string, string>;
```

`src/lib/preview/fixtures.ts` 的设置对象增加 `ticktickColumnRoles: {}`。

- [ ] **Step 4: Run the tests**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife ticktick_column_roles_roundtrip_and_stay_out_of_policy -- --nocapture`

Expected: PASS

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs src/lib/api.ts src/lib/preview/fixtures.ts
git commit -m "$(cat <<'EOF'
feat: store TickTick column roles in config

EOF
)"
```

---

### Task 3: 刷新分组并申请读取权限

**Files:**
- Modify: `src-tauri/src/ticktick.rs`
- Modify: `src/lib/api.ts`（`TickTickProject.columns`）
- Modify: `src/lib/preview/fixtures.ts`（状态里的 `columns`）
- Test: `src-tauri/src/ticktick.rs` 的测试模块

**Interfaces:**
- Consumes: `parse_columns`、`parse_projects`、`stamp_missing_roles`、`kept_column_ids`、`prune_column_roles`、`sort_columns`
- Produces:
  - `pub struct TickTickColumnView { pub id: String, pub name: String, pub sort_order: i64 }`
  - `TickTickProjectView.columns: Vec<TickTickColumnView>`
  - `fn insufficient_scope(status: u16, body: &Value) -> bool`
  - `fn plan_refresh(api: &mut dyn TickTickApi, previous: &Value, settings: &AppSettings) -> Result<PlannedRefresh, String>`
  - `fn apply_refresh(api: &mut dyn TickTickApi, conn: &Connection, settings: &mut AppSettings) -> Result<(), String>`
  - 常量文案 `有清单没能刷新，这些清单仍显示上次的分组。`
  - 错误文案 `TickTick 授权不足以读取清单，请重新连接。`
  - `authorize_url` 的 scope 为 `tasks:read tasks:write`

`PlannedRefresh` 含更新后的 `AppSettings`、要写入的项目 JSON 字符串、`partial: bool`。

- [ ] **Step 1: Write the failing tests**

```rust
const PARTIAL_REFRESH: &str = "有清单没能刷新，这些清单仍显示上次的分组。";
const SCOPE_REFRESH: &str = "TickTick 授权不足以读取清单，请重新连接。";

#[test]
fn authorize_url_asks_for_task_read_and_write() {
    let url = authorize_url("cid", "http://127.0.0.1:9/callback", "st", "ch");
    assert!(url.contains("scope=tasks%3Aread%20tasks%3Awrite"));
    assert!(!url.contains("scope=tasks%3Awrite&"));
    assert!(url.contains("code_challenge_method=S256"));
}

#[test]
fn refresh_keeps_failed_project_columns_and_does_not_touch_tasks() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrate(&conn).unwrap();
    let task = Task {
        id: "L1".into(),
        list_id: PRESET_MAINLINE_ID.into(),
        title: "写稿".into(),
        done: false,
        start: None,
        end: None,
        range: None,
        sort: 0,
        repeat: Default::default(),
        remind_offsets: vec![],
        notes: String::new(),
        ticktick_task_id: Some("tt1".into()),
        ticktick_project_id: Some("p2".into()),
        ticktick_etag: "e".into(),
        ticktick_dirty: 1,
        ticktick_all_day: false,
    };
    persist_task(&conn, &task).unwrap();
    meta_set(&conn, "ticktick_last_sync_at", "1000").unwrap();
    meta_set(
        &conn,
        "ticktick_projects_json",
        r#"[{"id":"p1","name":"甲","sortOrder":1,"columns":[{"id":"old-1","name":"旧甲","sortOrder":1,"projectId":"p1"}]},{"id":"p2","name":"乙","sortOrder":2,"viewMode":"list","columns":[{"id":"old-2","name":"旧乙","sortOrder":1,"projectId":"p2"}]}]"#,
    )
    .unwrap();
    let mut settings = crate::config::default_settings();
    settings.ticktick_project_roles.insert("p1".into(), "mainline".into());
    settings.ticktick_column_roles.insert("old-1".into(), "side".into());
    settings.ticktick_column_roles.insert("old-2".into(), "chore".into());
    let mut api = FakeApi::with_script(vec![
        Ok(TickTickResponse {
            status: 200,
            body: json!([
                {"id": "p1", "name": "甲", "sortOrder": 1, "viewMode": "list"},
                {"id": "p2", "name": "乙", "sortOrder": 2, "viewMode": "list"}
            ]),
        }),
        Ok(TickTickResponse {
            status: 200,
            body: json!({"columns": [{"id": "new-1", "name": "新甲", "sortOrder": 3, "projectId": "p1"}]}),
        }),
        Ok(TickTickResponse {
            status: 500,
            body: json!({"errorMessage": "boom"}),
        }),
    ]);
    apply_refresh(&mut api, &conn, &mut settings).unwrap();
    let raw = meta_get(&conn, "ticktick_projects_json").unwrap().unwrap();
    let cached: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let p1 = cached.as_array().unwrap().iter().find(|p| p["id"] == "p1").unwrap();
    let p2 = cached.as_array().unwrap().iter().find(|p| p["id"] == "p2").unwrap();
    assert_eq!(p1["columns"][0]["id"], "new-1");
    assert_eq!(p1["columns"][0]["name"], "新甲");
    assert_eq!(p1["columns"][0]["sortOrder"], 3);
    assert_eq!(p1["viewMode"], "list");
    assert_eq!(p2["columns"][0]["id"], "old-2");
    assert!(settings.ticktick_column_roles.get("old-1").is_none());
    assert_eq!(settings.ticktick_column_roles.get("old-2").map(String::as_str), Some("chore"));
    assert!(settings.ticktick_column_roles.get("new-1").is_none());
    assert_eq!(meta_get(&conn, "ticktick_last_sync_at").unwrap().as_deref(), Some("1000"));
    assert_eq!(meta_get(&conn, "ticktick_last_result").unwrap().as_deref(), Some(PARTIAL_REFRESH));
    assert_eq!(load_tasks(&conn).unwrap().len(), 1);
    assert_eq!(load_tasks(&conn).unwrap()[0].ticktick_dirty, 1);
}

#[test]
fn refresh_insufficient_scope_leaves_cache_and_roles() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrate(&conn).unwrap();
    meta_set(&conn, "ticktick_projects_json", r#"[{"id":"p","name":"甲","sortOrder":1}]"#).unwrap();
    meta_set(&conn, "ticktick_last_sync_at", "1000").unwrap();
    let mut settings = crate::config::default_settings();
    settings.ticktick_project_roles.insert("p".into(), "mainline".into());
    settings.ticktick_column_roles.insert("c".into(), "side".into());
    let mut api = FakeApi::with_script(vec![Ok(TickTickResponse {
        status: 500,
        body: json!({"errorCode": "client_exception", "errorMessage": "Insufficient scope for this resource"}),
    })]);
    let err = apply_refresh(&mut api, &conn, &mut settings).unwrap_err();
    assert_eq!(err, SCOPE_REFRESH);
    assert_eq!(
        meta_get(&conn, "ticktick_projects_json").unwrap().as_deref(),
        Some(r#"[{"id":"p","name":"甲","sortOrder":1}]"#)
    );
    assert_eq!(settings.ticktick_project_roles.get("p").map(String::as_str), Some("mainline"));
    assert_eq!(settings.ticktick_column_roles.get("c").map(String::as_str), Some("side"));
    assert_eq!(meta_get(&conn, "ticktick_last_sync_at").unwrap().as_deref(), Some("1000"));
}

#[test]
fn refresh_drops_columns_when_the_project_disappears() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrate(&conn).unwrap();
    meta_set(
        &conn,
        "ticktick_projects_json",
        r#"[{"id":"gone","name":"旧","sortOrder":1,"columns":[{"id":"c-gone","name":"组","sortOrder":1,"projectId":"gone"}]}]"#,
    )
    .unwrap();
    let mut settings = crate::config::default_settings();
    settings.ticktick_column_roles.insert("c-gone".into(), "mainline".into());
    let mut api = FakeApi::with_script(vec![
        Ok(TickTickResponse {
            status: 200,
            body: json!([{"id": "p", "name": "新", "sortOrder": 1}]),
        }),
        Ok(TickTickResponse {
            status: 200,
            body: json!({"columns": []}),
        }),
    ]);
    apply_refresh(&mut api, &conn, &mut settings).unwrap();
    assert!(settings.ticktick_column_roles.get("c-gone").is_none());
    assert_eq!(settings.ticktick_project_roles.get("p").map(String::as_str), Some("ignore"));
}
```

把现有的 `authorize_url_asks_for_task_write_and_s256` 删掉，由上面的新测试代替。`Task` 字面量按文件里已有测试的字段写全。

- [ ] **Step 2: Run the tests to verify they fail**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife refresh_keeps_failed_project_columns_and_does_not_touch_tasks -- --nocapture`

Expected: FAIL，`apply_refresh` 找不到。授权测试会因为 scope 仍是 `tasks%3Awrite` 而失败。

- [ ] **Step 3: Implement refresh and the scope change**

`authorize_url` 里的 scope 改为先 `percent_encode("tasks:read tasks:write")`，再放进查询串。编码结果是 `tasks%3Aread%20tasks%3Awrite`。

```rust
fn insufficient_scope(status: u16, body: &Value) -> bool {
    if status == 403 {
        return true;
    }
    body.to_string().to_ascii_lowercase().contains("insufficient scope")
}
```

`plan_refresh`：

1. `GET /project`。传输失败返回「同步失败」。权限不足返回 `SCOPE_REFRESH`，且不要改调用方的 settings。其它非 2xx 保持现在的 `http {status}`，同样不改缓存。
2. 对返回数组里的每个有 `id` 的清单 `GET /project/{id}/data`。某一份权限不足：立刻返回 `SCOPE_REFRESH`，不返回部分结果。其它失败记为这一份失败，继续下一份。
3. 成功的清单：以清单接口的原对象为底，写入 `columns`（数据里没有这个键就写 `[]`），保留 `viewMode` 等未知字段。失败且缓存里有同一 id：整份沿用缓存对象。失败且没有旧对象：用清单接口对象，`columns` 为 `[]`。`GET /project` 没再返回的清单不要出现在新数组里。
4. `parse_projects` 得到新列表。`stamp_missing_roles` 只补清单角色。`prune_column_roles(&kept_column_ids(&parsed), &mut column_roles)`。不要给新分组写角色。
5. 有任一清单数据失败时 `partial = true`。

`apply_refresh` 调用 `plan_refresh`。只有 `Ok` 时才替换 `settings`、写 `ticktick_projects_json`。`partial` 时把 `ticktick_last_result` 设为 `PARTIAL_REFRESH`。不是 partial，且当前 `ticktick_last_result` 正好是这句时，把它写成空字符串；其它同步结果不要动。不要写 `ticktick_last_sync_at`，不要写 `tasks`。

`refresh_projects_blocking` 改为：检查令牌、打开数据库、`LiveApi::from_settings()`、`load_settings()`、`apply_refresh`、`write_settings_file`、`build_status`。`apply_refresh` 失败时不要写设置文件。

`TickTickProjectView` 增加 `columns`。`build_status` 把 `RemoteProject.columns` 复制成 `TickTickColumnView`，先 `sort_columns`。`role` 仍是清单角色。

`src/lib/api.ts`：

```ts
export interface TickTickColumn {
  id: string;
  name: string;
  sortOrder: number;
}

export interface TickTickProject {
  id: string;
  name: string;
  sortOrder: number;
  role: string;
  columns: TickTickColumn[];
}
```

`PREVIEW_TICKTICK_STATUS` 的每个项目补 `columns: []`，否则预览类型对不上。这一步可以先空着，Task 5 再放示例分组。

- [ ] **Step 4: Run the tests**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife ticktick:: -- --nocapture`

Expected: PASS

Run: `npx vitest run --dir src`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ticktick.rs src/lib/api.ts src/lib/preview/fixtures.ts
git commit -m "$(cat <<'EOF'
feat: refresh TickTick columns and request read scope

EOF
)"
```

---

### Task 4: 同步按最终角色拉取，忽略分组只删本机

**Files:**
- Modify: `src-tauri/src/ticktick.rs`
- Test: `src-tauri/src/ticktick.rs`

**Interfaces:**
- Consumes: `projects_to_fetch`、`ReconcileAction::DropIgnored`、`run_pull_body_with`、`task_body`
- Produces: 全量成功时，最终角色为忽略的已链接任务在写回之前从推送集合去掉；`task_body` 仍不含 `columnId`

- [ ] **Step 1: Write the failing tests**

更新 `pull_keeps_push_failure_reason_over_ok` 的脚本顺序。拉取现在发生在写回之前，而且不是第一次同步才会要完成列表。这个测试没有 `ticktick_last_sync_at`，所以仍是第一次同步，只有一次 `GET /project/p/data`，然后才是写回的 400：

```rust
let mut api = FakeApi::with_script(vec![
    Ok(TickTickResponse {
        status: 200,
        body: json!({ "tasks": [] }),
    }),
    Ok(TickTickResponse {
        status: 400,
        body: json!({}),
    }),
]);
```

断言不变：脏值仍是 1，`ticktick_last_result` 仍是「写回被拒绝」。

再追加：

```rust
#[test]
fn create_body_omits_column_id() {
    let op = PushOp {
        kind: PushKind::Create,
        local_id: "L".into(),
        task_id: None,
        project_id: "p".into(),
        to_project_id: None,
        title: "写稿".into(),
        start: Some(10),
        end: Some(20),
        all_day: false,
    };
    let body = task_body(&op);
    assert!(body.get("columnId").is_none());
}

#[test]
fn full_sync_drops_ignored_column_locally_and_does_not_delete_remote() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrate(&conn).unwrap();
    meta_set(&conn, "ticktick_last_sync_at", "1000").unwrap();
    let mut task = linked_task_without_remote_id();
    task.ticktick_task_id = Some("tt1".into());
    task.ticktick_project_id = Some("p".into());
    task.ticktick_dirty = 1;
    persist_task(&conn, &task).unwrap();
    let mut project = RemoteProject {
        id: "p".into(),
        name: "甲".into(),
        sort_order: 1,
        columns: vec![gamelife_core::ticktick_sync::RemoteColumn {
            id: "c-ignore".into(),
            name: "忽略组".into(),
            sort_order: 1,
            project_id: "p".into(),
        }],
    };
    let projects = vec![project];
    let mut roles = BTreeMap::new();
    roles.insert("p".into(), "mainline".into());
    let mut column_roles = BTreeMap::new();
    column_roles.insert("c-ignore".into(), "ignore".into());
    let mut api = FakeApi::with_script(vec![
        Ok(TickTickResponse {
            status: 200,
            body: json!({
                "tasks": [{
                    "id": "tt1",
                    "title": "写稿",
                    "status": 0,
                    "columnId": "c-ignore",
                    "etag": "e",
                    "startDate": "1970-01-01T00:00:10+0000",
                    "dueDate": "1970-01-01T00:00:20+0000",
                    "isAllDay": false
                }]
            }),
        }),
        Ok(TickTickResponse {
            status: 200,
            body: json!({ "tasks": [] }),
        }),
    ]);
    run_pull_body_with(2_000, &mut api, &conn, &projects, &roles, &column_roles).unwrap();
    assert!(load_tasks(&conn).unwrap().is_empty());
    assert!(api.calls.iter().all(|(method, path)| method != "DELETE" && path != "/task/tt1"));
    assert_eq!(meta_get(&conn, "ticktick_last_sync_at").unwrap().as_deref(), Some("2000"));
}

#[test]
fn partial_sync_keeps_ignored_column_task_and_sync_clock() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrate(&conn).unwrap();
    meta_set(&conn, "ticktick_last_sync_at", "1000").unwrap();
    let mut task = linked_task_without_remote_id();
    task.ticktick_task_id = Some("tt1".into());
    task.ticktick_project_id = Some("p".into());
    task.ticktick_dirty = 0;
    persist_task(&conn, &task).unwrap();
    let projects = vec![
        RemoteProject {
            id: "p".into(),
            name: "甲".into(),
            sort_order: 1,
            columns: vec![gamelife_core::ticktick_sync::RemoteColumn {
                id: "c-ignore".into(),
                name: "忽略组".into(),
                sort_order: 1,
                project_id: "p".into(),
            }],
        },
        RemoteProject {
            id: "q".into(),
            name: "乙".into(),
            sort_order: 2,
            columns: Vec::new(),
        },
    ];
    let mut roles = BTreeMap::new();
    roles.insert("p".into(), "mainline".into());
    roles.insert("q".into(), "side".into());
    let mut column_roles = BTreeMap::new();
    column_roles.insert("c-ignore".into(), "ignore".into());
    let mut api = FakeApi::with_script(vec![
        Ok(TickTickResponse {
            status: 200,
            body: json!({"tasks": [{"id": "tt1", "title": "写稿", "status": 0, "columnId": "c-ignore", "etag": "e"}]}),
        }),
        Ok(TickTickResponse { status: 500, body: json!({}) }),
    ]);
    run_pull_body_with(2_000, &mut api, &conn, &projects, &roles, &column_roles).unwrap();
    assert_eq!(load_tasks(&conn).unwrap().len(), 1);
    assert_eq!(meta_get(&conn, "ticktick_last_sync_at").unwrap().as_deref(), Some("1000"));
    assert!(api.calls.iter().all(|(method, _)| method != "DELETE"));
}
```

`let mut project` 若没有后续修改，写成 `let project`。

- [ ] **Step 2: Run the tests to verify they fail**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife full_sync_drops_ignored_column_locally_and_does_not_delete_remote -- --nocapture`

Expected: FAIL。本机行还在，或出现了对 `/task/tt1` 的 POST。`pull_keeps_push_failure_reason_over_ok` 在改顺序之前也会失败。

- [ ] **Step 3: Pull before push, and skip rows that will be dropped**

`run_pull_body_with` 的顺序改为：

1. 读 `ticktick_last_sync_at`、任务和日界，先 `pull_round`。
2. 收集本轮 `DropIgnored` 的 `local_id`。
3. 再对脏值为 1、且不在这个集合里的任务做原来的 `push_one`。遇到「需要重新连接」时，写下这个结果并返回，不要 `apply_reconcile_actions`，也不要前移同步时间。
4. 然后 `apply_reconcile_actions`、按 `advance_sync_at` 写 `ticktick_last_sync_at`、保留「写回失败优先于 ok」的 `ticktick_last_result` 逻辑。

不要给 `task_body` 增加 `columnId`。`PushKind::Delete` 仍只用于用户把已链接任务移出四个预设分组，不要拿它表示「最终角色变成忽略」。

- [ ] **Step 4: Run the tests**

Run: `/opt/homebrew/bin/cargo test --offline -p gamelife ticktick:: -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ticktick.rs
git commit -m "$(cat <<'EOF'
fix: drop ignored TickTick columns locally without deleting them remotely

EOF
)"
```

---

### Task 5: 设置页展开分组

**Files:**
- Modify: `src/lib/ticktickSettings.ts`
- Modify: `src/lib/ticktickSettings.test.ts`
- Modify: `src/pages/Settings.tsx`
- Modify: `src/lib/preview/fixtures.ts`

**Interfaces:**
- Consumes: `TickTickProject.columns`、`AppSettings.ticktickColumnRoles`、`roleForProject`
- Produces:
  - `export const EMPTY_COLUMNS_COPY = "这个清单没有分组，任务按整份清单归类。"`
  - `export function columnRoleChoice(roles: Record<string, string> | undefined, columnId: string): "inherit" | "ignore" | "mainline" | "side" | "longterm" | "chore"`
  - `export function columnRolePatch(roles: Record<string, string> | undefined, columnId: string, choice: "inherit" | "ignore" | "mainline" | "side" | "longterm" | "chore"): Record<string, string>`
  - `export function sortedColumns<T extends { id: string; sortOrder: number }>(columns: T[]): T[]`

- [ ] **Step 1: Write the failing tests**

在 `src/lib/ticktickSettings.test.ts` 追加：

```ts
import {
  columnRoleChoice,
  columnRolePatch,
  EMPTY_COLUMNS_COPY,
  sortedColumns,
} from "./ticktickSettings";

it("missing column role follows the list", () => {
  expect(columnRoleChoice({}, "c1")).toBe("inherit");
  expect(columnRoleChoice({ c1: "nope" }, "c1")).toBe("inherit");
  expect(columnRoleChoice({ c1: "mainline" }, "c1")).toBe("mainline");
  expect(columnRoleChoice({ c1: "ignore" }, "c1")).toBe("ignore");
});

it("inherit deletes the stored key and a role writes it", () => {
  expect(columnRolePatch({ c1: "side", c2: "chore" }, "c1", "inherit")).toEqual({
    c2: "chore",
  });
  expect(columnRolePatch({}, "c1", "mainline")).toEqual({ c1: "mainline" });
});

it("empty columns use the empty-group sentence", () => {
  const columns: { id: string; sortOrder: number }[] = [];
  const copy = columns.length === 0 ? EMPTY_COLUMNS_COPY : "";
  expect(copy).toBe("这个清单没有分组，任务按整份清单归类。");
});

it("sorts columns by sortOrder then id", () => {
  expect(
    sortedColumns([
      { id: "b", sortOrder: 1 },
      { id: "a", sortOrder: 1 },
      { id: "c", sortOrder: 0 },
    ]).map((column) => column.id),
  ).toEqual(["c", "a", "b"]);
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npx vitest run --dir src lib/ticktickSettings`

Expected: FAIL，`columnRoleChoice` 找不到。

- [ ] **Step 3: Implement the helpers and the settings rows**

`ticktickSettings.ts` 按上面的签名实现。`columnRoleChoice` 只接受五个合法角色，其它值包括空字符串和 `inherit` 都返回 `"inherit"`。`columnRolePatch` 在 `"inherit"` 时删除键，否则写入该角色。`sortedColumns` 先按 `sortOrder` 升序，再按 `id` 字典序，不改原数组。

`Settings.tsx` 的清单映射里，每个清单保持现在的角色 `Select`。旁边加一个 `Button`（`size="sm"`、`variant="outline"`），文案在展开时是「收起」，收起时是「展开」。展开状态用 `useState<Record<string, boolean>>`，不要写入 `settings`。

展开后用 `sortedColumns(project.columns ?? [])`。数组为空时显示一段 `text-[13px] text-muted-foreground`，内容是 `EMPTY_COLUMNS_COPY`。否则每个分组一个 `Field`，标签是分组名称。下拉的第一项是 `{ value: "inherit", label: "跟随清单" }`，后面接现有的 `TICKTICK_ROLE_OPTIONS`。值用 `columnRoleChoice(settings.ticktickColumnRoles, column.id)`。变更时用 `columnRolePatch` 得到新映射，再走现在的 `persistBasic`，只改 `ticktickColumnRoles`。

`PREVIEW_TICKTICK_STATUS` 改成已连接，并带两份清单：一份有两个分组（`sortOrder` 故意乱序），一份 `columns: []`。设置夹具里给其中一个分组 id 写上 `mainline`，另一个不写。

- [ ] **Step 4: Run the tests and check the preview**

Run: `npx vitest run --dir src`

Expected: PASS

用 `VITE_PREVIEW=1` 打开设置的 TickTick 页。确认：有分组的清单可以展开和收起；分组按 sortOrder 再按 id 排列；没保存的分组显示「跟随清单」；选「忽略」或四个角色会留下对应键，改回「跟随清单」会删掉该键；没有分组的清单展开后是「这个清单没有分组，任务按整份清单归类。」预览服务器用完即停。

- [ ] **Step 5: Commit**

```bash
git add src/lib/ticktickSettings.ts src/lib/ticktickSettings.test.ts src/pages/Settings.tsx src/lib/preview/fixtures.ts
git commit -m "$(cat <<'EOF'
feat: set TickTick column roles from the settings list

EOF
)"
```

---

### Task 6: 文档指向新规格

**Files:**
- Modify: `CLAUDE.md`
- Modify: `AGENTS.md`
- Modify: `.cursor/rules/specs.mdc`
- Modify: `.cursor/rules/tauri-shell.mdc`
- Modify: `docs/superpowers/specs/2026-09-22-ticktick-task-sync-design.md`

**Interfaces:**
- Consumes: 新规格的覆盖范围
- Produces: 索引里能找到 2026-09-23 这篇，旧规格文首指出它被哪里覆盖

- [ ] **Step 1: Update the indexes**

`CLAUDE.md` 和 `.cursor/rules/specs.mdc` 在 2026-09-22 TickTick 那条后面加：

```markdown
- `2026-09-23-ticktick-column-roles-design.md` — 清单内分组的显式角色覆盖整份清单。刷新清单还会读 `GET /project/{id}/data`。授权 scope 为 `tasks:read tasks:write`。`user_version` 仍是 6。
```

`AGENTS.md` 产品规则里 TickTick 那句后面补上：分组的显式角色覆盖整份清单；没单独设置的分组跟随清单；新建任务的写入目标仍按整份清单选择。

`.cursor/rules/tauri-shell.mdc` 的 TickTick 句补上：分组角色在 `config.json` 的 `ticktickColumnRoles`；刷新清单会读取每份清单的 `columns`。

`docs/superpowers/specs/2026-09-22-ticktick-task-sync-design.md` 在文首「本文覆盖并取代」之前加一句：

```markdown
分组角色、刷新时读取分组，以及授权 scope，以后续的 `2026-09-23-ticktick-column-roles-design.md` 为准。
```

不要改那篇里已经被声明覆盖的旧句子，避免两篇同时改写同一条规则。

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md AGENTS.md .cursor/rules/specs.mdc .cursor/rules/tauri-shell.mdc docs/superpowers/specs/2026-09-22-ticktick-task-sync-design.md
git commit -m "$(cat <<'EOF'
docs: point TickTick column roles at the new spec

EOF
)"
```

这一任务没有行为测试。提交前确认只改了这五个文件里的说明句。
