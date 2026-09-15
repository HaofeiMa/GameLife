# GameLife G：本机任务页（取代 TickTick）

日期：2026-09-15  
状态：待用户审阅  
范围：新增第五个轨道页「任务」，用本机分组 + 自然语言 + 自绘日历代替 TickTick，作为计划来源和槽快照。今日页仍是日报 + 计划|实际时间轴，只读当天任务。产品仍是活动监测器：观测优先于计划，勾选完成不发币。

本文覆盖并取代：

- `2026-09-13-activity-monitor-design.md` 中 **TickTick 作为计划本**（OAuth、清单角色映射、`ticktick_cache` 钉快照、设置页 TickTick 板块、今日「当天 TickTick」卡片、时间轴计划列读 TickTick）。判定流水线、应用名单、一句话规则、观测优先、无任务仍可自动 Core、四页之外的统计/商店，仍以该文为准。
- 该文「FullCalendar / dnd-kit / 任务右键 / 拖进日历：**不做**」——改为 **自绘** 日历与右键；仍禁止 FullCalendar 与 dnd-kit。
- `2026-09-11-planner-shell-design.md` 中 **恰好一个主线列表**、**今日页当计划本**、自建列表默认 `custom`。该文的列表角色发币档次、槽钉快照、自然语言规则解析、最多 20 条判定集合仍有效。
- `2026-09-14-cloud-sync-design.md` §4「永不」表里的 `task_lists` / `tasks`。这两张表现在进入快照副本；`ticktick_cache`、`heartbeat`、`secrets.json`、`screenshots/` 仍永不上传。

未提及的行为仍以 2026-09-10、Wave 1、观测引擎、落地判定、游戏手感、活动监测、跨平台观测、云备份为准。

## 1. 问题

今日页已经能并排看计划与实际，但计划仍绑在 TickTick OAuth 上：断网、限流、授权失败时计划列是空的，设置里还挂着一整页用不到的连接向导。本机 `task_lists` / `tasks` 和 `parse_task_line` 从计划本时代就在，只是 UI 和判定不再读写。

把计划搬回本机：一个「任务」页管分组与排期，今日只展示。判定快照改钉这些本地已排期任务。GameLife 仍然不靠勾选发币。

## 2. 成功标准

| 项 | 必须 |
| --- | --- |
| 导航 | 左侧：今日、任务、统计、商店、设置。小字标签仍可在设置关闭 |
| 任务页 | 左栏分组（可折叠，角色色点）+ 自然语言输入；右栏 3 天（今明后）或 7 天（本周一–周日）自绘日历；中间分割线可拖 |
| 加任务 | 规则解析中文日期/时段/`#分组或角色`，不为此打模型。无时段则进分组当未排期 |
| 改期 | 右键改日期；日历上拖块改日期/钟点；今日计划列拖块改当天钟点。对齐 15 分钟 |
| 放弃 | 右键「放弃任务」确认后删除。勾选完成不删除，默认从列表藏起来 |
| 判定 | 槽开始钉本地未完成、已排期、与当天相交的任务。观测仍优先于计划。无任务仍可自动 Core |
| TickTick | 设置页、OAuth、同步、槽开始刷新缓存全部去掉。今日与判定不再读 `ticktick_cache` |
| 备份 | `task_lists` / `tasks` 留在快照副本里，换机恢复后计划本还在。不做多机协同编辑 |

明确不算失败：自然语言漏解析时整句当未排期标题；关窗错过 toast；没配文本/视觉 API 时任务能加、灰区槽仍待复核。

## 3. 不变量

1. 15 秒采样；缺口不外推；进程死亡是未观测，不是离开。`credited ≤ observed ≤ 实际槽长`。
2. 一天最多 96 个 15 分钟槽。周末不采样。
3. **观测优先于计划**。娱乐规则命中就是娱乐、0 币，即使该小时日历上是主线。
4. 勾选完成、放弃、改期都不发币。发币只来自槽判定。
5. 账本 `UNIQUE reward_event_key`。已 `final` 的槽不改经济。改任务只影响尚未开始的槽（`task_snapshot_json` 的 `COALESCE` 钉死）。
6. 当天进入判定集合最多 20 条；超出须先完成、改期或放弃。写入路径拒绝，不把已超限的快照钉成空数组。
7. 无系统通知、无音效。不引入 FullCalendar、dnd-kit、Radix、子任务、重复规则、一条任务多个时段、全天无钟点条。
8. 界面中文。能量称「能量」。颜色只来自 `--cat-*` / `theme.ts`。
9. 页面只经 `src/lib/api.ts` 调 `invoke()`。
10. 改 `src-tauri/` 后 `cargo test --offline -p gamelife` 必须跑过；改 core 跑 `gamelife-core`；改 `src/` 跑 `npx vitest run --dir src`。
11. 测试禁止 `std::env::set_var("HOME", …)`。
12. `user_version` 保持 3。不删 `ticktick_cache` 表，以免升版本。

## 4. 分层

```
任务页（计划本）
  分组 + 自然语言 + 自绘日历 + 右键

今日页（监测器外壳）
  计划|实际时间轴（计划列只读本地已排期）+ 当天任务只读卡片 + 复核

判定
  槽开始钉本地任务快照 → 硬规则 → 文本 AI / 视觉
```

本机 SQLite 仍是唯一账本。任务标题是「使用者声称这段时间在做什么」的上下文，不是观测结果。

## 5. 数据模型

沿用现表，不改 schema：

```
TaskList { id, name, sort, role: mainline | side | longterm | chore }
Task     { id, list_id, title, done, start?, end?, range? }
```

不再给新列表写 `custom`。库里若还有 `custom` 行，判定与发币按支线（6 折）。`range` 列保留但不在本波 UI 露出（周/月范围以后再说）。

### 5.1 分组

预置四组，种子与现网一致，显示名：

| id | 名称 | role |
| --- | --- | --- |
| `list-mainline` | 主线任务 | mainline |
| `list-side` | 支线任务 | side |
| `list-chore` | 杂项 | chore |
| `list-longterm` | 长期规划 | longterm |

现网种子名是「长期计划」：迁移时若该行仍叫「长期计划」，改名为「长期规划」。id 不变。

规则：

- 可再建同角色的组（例如第二个主线组「实验」）。
- 任意时刻 **至少** 一个 `mainline` 组，不再要求恰好一个。
- 预置四组可改名、不可删除。
- 自建组：空组可删；组内还有任务则拒绝并 toast。
- 删除不得导致主线组数量变为 0。
- 角色在创建时选择，本波不提供改角色（避免已钉快照与列表定义对不上；要改就新建组再移动任务）。

发币档次（命中任务列表 role，与 planner-shell §6 相同）：

| role | 硬币 / 能量 | 8h、宝箱、连胜 |
| --- | --- | --- |
| mainline | 不打折 | 计入 |
| side | 6 折 | 不计入 |
| longterm（已排期） | 6 折 | 不计入 |
| chore | 3 折 | 不计入 |
| 未排期的 longterm | 不进判定 | — |

### 5.2 任务

- `done=false` 为未完成；勾选把 `done=true`。完成不删除。
- 放弃 = `DELETE` 该行。不可恢复。
- `start`/`end` 都有：已排期。写入时 `align_range`，最短 15 分钟。一条任务一个时段。
- 都空：未排期。不进日历、不进今日计划列、不进判定。
- 只写其中一个：拒绝。
- 不引入全天任务；今日页顶上的 TickTick 全天条删除。

**进入当天判定集合**（未完成，且 `[start, end)` 与当天相交）：长期规划未排期不进；长期规划一旦有时段则进，按 6 折。主线组可以有多个，全部按主线发币。

## 6. 自然语言

继续用 `parse_task_line`，不打模型。例：

`明天上午十点到十二点，写方法节 #主线`

→ 明天 10:00–12:00，标题「写方法节」，列表为第一个 `mainline` 组（或名称恰好为「主线」的组）。

支持：今天/明天/后天、周几、上午下午晚上、点/点半、`#主线` `#支线` `#杂项` `#长期` `#长期规划`、预置名、自建组名。`#长期计划` 仍识别为长期，兼容旧输入。

无 `#`：加入当前键盘焦点所在分组；都不在分组里时默认第一个主线组。

解析失败或没有时段：`parse_ok=false`，标题进目标分组，不排期，可再改期或拖进日历。

输入框在左栏顶上。键入时在框下显示解析碎片（分组、时段、标题）；回车提交。空标题拒绝。

当天判定集合将超过 20：拒绝添加/改期并 toast。

## 7. 任务页

外壳与其它页一致：`PageHeader` + `px-[22px]` 内容；整页一块 `--background`，页头无底边线。左右各一张 `Card`（无描边、软阴影），不要卡片小标题。中间分割线可拖，宽度写入 `localStorage`（窗口几何，不是 `config.json`）。

### 7.1 页头

```
任务 | [排序 ▾]  [···]                         [3 天 | 7 天]
```

排序（只作用于每个分组内部，不打乱分组顺序；选择写入 `localStorage`）：

- 按时间（默认）：有时段的按 `start`；未排期排在该组最后。
- 按标题：Unicode 序。

`···` 菜单：

- 添加分组 → 对话框：名称 + 角色（主线 / 支线 / 杂项 / 长期规划）。空名拒绝。
- 显示已完成 / 隐藏已完成（默认隐藏）。显示时已完成行打勾、划线，仍可取消勾选。

3 天 = 今天、明天、后天。7 天 = 本周一到周日，不是滚动 7 日。默认 7 天。选择写入 `localStorage`。周末可以排期，只是应用不采样。

分组标题右键：重命名；自建且为空时可删除。

### 7.2 左栏

自然语言输入。其下按 `sort` 列出全部分组。点标题折叠/展开（`localStorage`）。标题左侧角色色点：主线 `--cat-mainline`、支线 `--cat-side`、杂项 `--cat-admin`、长期 `--cat-longterm`。右侧未完成条数。

每条任务：勾选、标题、时段或「未排期」标记。右键：

1. 更改日期… → 对话框：日期；可选开始/结束时间。已排期只改日期则平移钟点、保持时长。未排期只选日期、不填时间：仍是未排期（日期本身不进入 schema）。未排期填了时间：变为已排期。
2. 移动到其它分组 → 子菜单列出全部组。
3. 放弃任务 → 确认后删除。

### 7.3 右栏日历

自绘，交互对齐今日时间轴（色块 = 类别淡底 + 左边线），24 小时可滚。只画未完成且已排期的任务。块的颜色跟所属分组角色走。

- 拖块到另一天：改日期，保持时长。
- 拖块到另一钟点：改 `start`/`end`，对齐 15 分钟。
- 从左栏把未排期拖进某格：以落点为开始，默认 **30 分钟**，对齐 15 分钟。已排期从左栏拖到日历：与拖块相同，按落点改期。
- 不实现边缘拉伸改时长（要改时长走「更改日期」对话框）。
- 不做全天条、不把未排期画进日历。

拖动用指针捕获，与侧栏/时间轴分割线相同，不引入 dnd-kit。

## 8. 今日页

- 计划列、`planMarks`、底部卡片改读本地未完成已排期任务（字段从 `ticktickTasks` 改名为 `dayTasks`，避免继续叫 TickTick）。`dayTasks` 无全天字段。
- 底部卡片标题「当天任务」，只读，无输入框。空态：「当天没有已排期任务。可在任务页添加。」
- 删除 TickTick 全天任务顶条。
- 计划列可拖块改 **当天** 钟点（15 分钟对齐）；换日期在任务页做。
- 实际列、复核、权限条、娱乐倒计时不变。
- 对比「计划 vs 实际」的逻辑从 TickTick 列表改为 `dayTasks`。

## 9. 判定与快照

`ensure_slot` / `pin_task_snapshot_json`：不再 `load_ticktick_cache`。改为 `load_task_lists` + `load_tasks` → `judgment_tasks` → `snapshot_of`。

快照形状不变：`TaskSnapshot { id, title, role }`。已开始的槽仍 `COALESCE` 不改写。

文本 AI 仍吃快照 id/标题/角色；证据词仍只从主线快照标题切开。无快照时类别匹配路径不变。无任务仍可 `grounded_strong_core` 自动 Core。

槽开始不再调用 `maybe_refresh_ticktick_cache`。

若库内某天已超过 20 条可判定任务（绕过写入校验的脏数据）：按 `start` 取前 20 条钉进去，并打日志。不要钉 `[]`。

## 10. 云备份

`NEVER_SYNCED_TABLES` 去掉 `task_lists` 和 `tasks`，保留 `heartbeat`、`ticktick_cache`。`app_meta` 仍裁到 `device_id`。

含义：上传的快照带计划本；恢复某份快照会带上那份计划。`merged.db` **不**合并任务表（没有设备维度，也不做协同）。两台机器各自改任务，活库互不影响；恢复是整份覆盖。这是备份，不是 TickTick 云同步。

`aggregate` 档仍然不上传 `samples` 标题；任务标题会随 `tasks` 表上传——使用者把计划本放进备份即视为接受。在设置的备份说明里加一句：计划本会进入备份。

## 11. 删除 TickTick

从前端和命令表移除：

- 设置页 TickTick 标签与全部连接/映射/同步 UI。
- `ticktick_*` Tauri 命令（status / OAuth / sync / tree / disconnect / set_client_secret）。
- `AppSettings` 里的 `ticktickClientId`、`ticktickProjectRoles`、`ticktickColumnRoles`：读旧 `config.json` 时忽略，保存时不再写出。
- 预览夹具与 `src/lib/ticktickBoard.ts` 中仅服务设置页的文案。时间标签等可迁到 `src/lib/taskBoard.ts`。

保留：

- `ticktick_cache` 空表（不升 `user_version`）。
- `secrets.json` 里已有的 TickTick 密钥：不主动删除，也不再读取。文件仍 0600。

文档：`AGENTS.md` / `CLAUDE.md` / `.cursor/rules` 里「计划在 TickTick」「禁止本地任务 CRUD」改为指向本文。

## 12. 模块与命令

| 层 | 职责 |
| --- | --- |
| `gamelife-core` `task.rs` | `validate_lists` 改为至少一条主线；`match_role_alias` 增加「长期规划」；分组删除/重命名校验 |
| `gamelife-core` `task_parse.rs` | 现有解析；补 `#长期规划` |
| `src-tauri` `scheduler.rs` | 钉本地快照；删除 TickTick 刷新 |
| `src-tauri` `commands.rs` | 见下 |
| `src-tauri` `sync.rs` | `NEVER_SYNCED` 调整 |
| `src/pages/Tasks.tsx` | 新页 |
| `src/lib/rail.ts` | 五页 |
| `src/pages/Today.tsx` | `dayTasks`；去掉 TickTick 文案 |
| `src/pages/Settings.tsx` | 去掉 TickTick 标签 |

命令：

- 保留：`parse_task_line`、`upsert_task`、`toggle_task_done`。
- `create_list(name, role)`：必须带角色，不再默认 custom。
- 新增：`rename_list`、`delete_list`、`delete_task`、`move_task`、`reschedule_task`、`list_task_board`（返回 lists + tasks）。删除易误导的 `list_tasks`（它现在返回 `TodayView`）。
- `get_day_view` / `get_today`：`ticktickTasks` → `dayTasks`。预览夹具同步改。

## 13. 不做

- 子任务、重复规则、提醒、标签云、智能清单（今天/最近 7 天）。
- 全天任务、一条任务多个时段、分组改角色。
- 任务页做实际识别列（那是今日的事）。
- 多机合并同一份任务表。
- 把 TickTick 留作导入。
- FullCalendar、dnd-kit、Radix。
- 为解析打模型。

## 14. 测试

core：`validate_lists` 允许多个主线、拒绝零主线；`judgment_tasks` 排除 done / 未排期 / 未排期长期；`parse_task_line` 的 `#长期规划` 与无时段 `parse_ok=false`；`align_range` 仍最短 15 分钟。

shell：钉快照读 tasks 不读 ticktick_cache；已开始的槽不随改期改写；超过 20 条写入拒绝；备份副本含 tasks、不含 ticktick_cache；TickTick 命令不再注册。

前端：rail 五页；3/7 天的日期范围；排序；解析碎片；放弃需确认；未排期拖入默认 30 分钟。`npx vitest run --dir src`。
