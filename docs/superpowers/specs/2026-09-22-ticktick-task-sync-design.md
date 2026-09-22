# GameLife：TickTick 导入并与任务板双向同步

日期：2026-09-22  
状态：已确认，待实现  
范围：设置页恢复 TickTick 标签，顶部总开关默认关闭。打开后，已映射清单中的未完成任务导入四个预设分组；每小时拉取一次；预设分组上的新建、改标题、改时段、完成、取消完成、删除立刻写回 TickTick。产品仍是活动监测器。计划本仍是本地任务板。TickTick 是这四组任务的外部镜像，不是判定输入。

本文覆盖并取代：

- `2026-09-15-local-tasks-design.md` §11：恢复设置页 TickTick 标签、OAuth、清单角色和同步命令；重新读取 `secrets.json` 里的 TickTick Client Secret 与令牌。计划来源仍是 `task_lists` / `tasks`。不恢复用 `ticktick_cache` 钉槽快照，槽开始也不再刷新该表。
- 同文 §13「把 TickTick 留作导入」：本波做导入，并做成双向镜像。
- `2026-09-13-activity-monitor-design.md` §7 全文（只读、禁止写回、`#主线` 覆盖清单角色、只把有时段的任务钉进 `ticktick_cache`、今日 TickTick 卡片）。分组只看清单角色下拉。判定仍读本地未完成任务。
- 同文成功标准里「不引入双向同步、不在 TickTick 里完成任务」。
- `AppSettings` 对 `ticktick_client_id`、`ticktick_project_roles` 的 `skip_serializing`：这两项重新写入 `config.json`。`ticktick_column_roles` 继续不读不写。
- `2026-09-16-task-detail-and-board-ux-design.md` 的 `user_version = 5`：本波升到 **6**。备注、快照形状、勾选不发币，仍以该文为准。

未提及的行为仍以本地任务、任务交互、判定硬规则、云备份为准。

## 1. 问题

TickTick 里的安排进不了任务板。本地勾完的任务也不会回到 TickTick。2026-09-15 拆掉接入之后，设置里没有 TickTick 页；令牌留在 `secrets.json` 里但不读取。

## 2. 成功标准

| 项 | 必须 |
| --- | --- |
| 总开关 | 设置 → TickTick 顶部「同步到任务板」，默认关。关掉时不拉取、不写回、不每小时对齐。已导入任务留在板上，链接保留 |
| 连接 | 开关关掉时仍可填写凭证、连接、断开、刷新清单。立即同步在开关关闭或未连接时不可用 |
| 导入 | 角色不是「忽略」的清单里，当前未完成任务进入对应预设分组。历史已完成不回灌。子任务不导入。重复规则不导入 |
| 分组 | 主线 → `list-mainline`，支线 → `list-side`，长期 → `list-longterm`，杂项 → `list-chore`。不看标题里的 `#主线` 等字样 |
| 写入清单 | 同一角色有多份清单时，新建任务写入 `sortOrder` 数值最小的那份；相同则清单 id 字典序较小者。该角色一份都没有时，新建只留在本机 |
| 立刻写回 | 预设分组里新建、改标题、改时段、完成、取消完成、删除，以及在四个预设分组之间移动，本地提交后马上在后台写回 |
| 每小时 | 开关打开且已连接时，距上次成功的全量拉取满 3600 秒再拉。打开开关时马上对齐一次 |
| 完成 | TickTick 上完成的已链接任务，拉取后在任务板上打勾。任务板上打勾或取消勾选，写回 TickTick |
| 删除 | 全部已映射清单和完成列表都拉成功，且 TickTick 任务 id 哪里都不在，才删本地。任一清单失败则本轮不删任何已链接任务 |
| 失败 | 写回失败保留本地修改和脏标记。网络失败不挡住采样、判定、结算 |

明确不算失败：未连接或令牌失效时任务板照常使用；关窗后每小时对齐仍跑（托盘进程还在）；两台机器同时打开开关时没有跨设备锁。

## 3. 不变量

1. 观测仍优先于计划。导入的未完成任务与手建任务一样进入槽快照（未完成即进入，时段不决定是否参与判定）。已开始的槽不因同步改写。勾选、同步、删除都不发币，也不改已结算的账。
2. `TaskSnapshot` 仍是 `id` / `title` / `role`。备注、TickTick 正文、`ticktick_task_id` 都不进入 hint、文本 AI、视觉摘要。
3. 本地主键仍用现有 `new_task_id()`。不用 `tt-` 前缀当主键。不调用 `ticktick_judgment_set`，不读不写 `ticktick_cache`。
4. 已链接任务的重复固定为「不重复」。`upsert` 若把已链接任务的重复改成别的，拒绝，错误码 `linked_repeat`。本机完成已链接任务时不 `spawn_after_complete`。TickTick 自己生成的下一次若是新的未完成任务 id，下一轮按新 id 导入。
5. 只改备注、只改同一分组内的排序，不标脏，不写回。
6. Client Secret、access token、refresh token 只进 `secrets.json`（0600）。不进 `config.json`，不回传给页面，不进云备份。不用 macOS 钥匙串。
7. 网络命令保持 `async`。打开设置页只读本地缓存。`ticktick_enabled = false` 时，同步路径不发出拉取或写回。采样循环只做「是否到点」的本地判断。
8. `user_version` **6**。`device_id` 仍不是版本号。不删 `ticktick_cache`。云备份仍不上传 `ticktick_cache`；`tasks` 继续进入快照，因此会带上 TickTick 任务 id 与 etag。
9. 只使用 TickTick 官方 Open API，主机 `https://api.ticktick.com/open/v1`。授权页 `https://ticktick.com/oauth/authorize`，范围 `tasks:write`。不使用非官方同步协议，不接滴答清单 `dida365.com`。
10. 测试禁止 `std::env::set_var("HOME", …)`。改 `src-tauri/` 必须跑过 `cargo test --offline -p gamelife`；改 core 跑 `gamelife-core`；改 `src/` 跑 `npx vitest run --dir src`。

## 4. 数据

`tasks` 增加：

| 列 | 类型 | 含义 |
| --- | --- | --- |
| `ticktick_task_id` | TEXT | 远端任务 id。未链接为 NULL |
| `ticktick_project_id` | TEXT | 远端清单 id。未链接为 NULL |
| `ticktick_etag` | TEXT NOT NULL DEFAULT `''` | 上次见到的远端 etag |
| `ticktick_dirty` | INTEGER NOT NULL DEFAULT 0 | 0 = 干净；1 = 待写回；2 = 创建结果不确定，自动轮次只查找、不再次创建 |
| `ticktick_all_day` | INTEGER NOT NULL DEFAULT 0 | 1 = 这条链接当前按全天对待 |

`ticktick_task_id` 非空时唯一。旧行这些列为空 / 0 / `''`。

`config.json` 重新写出：

| 字段 | 默认 | 含义 |
| --- | --- | --- |
| `ticktick_enabled` | false | 总开关 |
| `ticktick_client_id` | `""` | OAuth Client ID |
| `ticktick_project_roles` | `{}` | 清单 id → `ignore` / `mainline` / `side` / `longterm` / `chore` |

`ticktick_column_roles` 保持 `skip_serializing`，读到也丢弃。角色映射不进 Policy。

`app_meta` 缓存（不进 config，不进 Policy）：

| 键 | 含义 |
| --- | --- |
| `ticktick_projects_json` | 清单 id、名称、`sortOrder` |
| `ticktick_last_sync_at` | 上次全部已映射清单和完成列表都成功的 unix 秒。没有则为空 |
| `ticktick_last_result` | 给设置页看的中文结果 |

对账与「角色 → 预设分组 / 写入清单」是 `gamelife-core` 的纯函数，不访问网络、文件系统或时钟。HTTP、OAuth、调度在 `src-tauri`。核心函数的输入里带「现在」和时区偏移，不自己读系统时间。

写入清单：在映射为该角色的清单中，取 `sortOrder` 最小者；并列时取清单 id 字典序较小者。`sortOrder` 按有符号整数比较（TickTick 会给出负数）。

时间：

- 远端 `isAllDay = true`：本地 `start` 为本机时区该日 00:00，`end` 为次日 00:00，`ticktick_all_day = 1`。写回时发 `isAllDay = true`，不把这两个时间戳当成定时区间。
- 只有 `dueDate`：`end` 为该时刻，`start` 为空。
- 只有 `startDate`：`start` 为该时刻，`end` 为空。
- 两者都有且不是全天：都写入，`ticktick_all_day = 0`。
- 都没有：`start` / `end` 为空。
- 本机改了开始或结束：清掉 `ticktick_all_day`，之后按定时任务写回。

不复制远端 `content` / `desc` / `items` / `repeatFlag` / 提醒 / 标签。本地备注保持原样，写回也不带备注。

## 5. 设置页

新标签放在「云端」和「权限」之间，文案为 TickTick。版式与云端备份卡片相同：开关在卡片顶部。颜色只用现有 token。页面只经 `src/lib/api.ts` 调用。

顶部开关：

- 标题「同步到任务板」。
- 说明「关闭时不同步任务。已导入的任务留在任务板上。」
- 下方一行：上次同步时间，或中文失败原因。

开关下面，关闭时仍可编辑：

- Client ID。未填写时说明：到 TickTick 开发者中心建应用，Redirect URI 填连接时显示的回跳地址。
- Client Secret。只写入 `secrets.json`，页面不回显已存密钥。
- 连接 / 断开。系统浏览器，OAuth PKCE，回跳 `http://127.0.0.1:<临时端口>/callback`。授权范围 `tasks:write`。以前的只读令牌需要重新连接。
- 断开后停止同步。已导入任务和链接都留着。再次连上后按任务 id 对齐。若连的是另一个账号，旧链接在一次全量成功拉取里按「远端已不存在」删掉。

已连接后列出缓存的清单。每个清单一个下拉：忽略、主线、支线、长期、杂项。缓存里第一次出现的清单默认忽略，并立即把这个默认写进 `ticktick_project_roles`，避免下一轮把整份清单灌进来。

同一角色有多份清单时，在说明里写「新建任务写入「清单名」」。只展示，不能另选。

「刷新清单」「立即同步」才打网络。立即同步在总开关关闭或尚未连接时不可用。刷新清单在未连接时不可用，在总开关关闭时可用。

## 6. 立刻写回

任务命令先提交本地数据库并返回。写回放在后台，不让保存等待 HTTP。失败不把这次本地保存变成错误；置 `ticktick_dirty = 1`，设置页记下中文原因。

会写回的本地操作：

| 操作 | 远端 |
| --- | --- |
| 预设分组里新建，且该角色有写入清单 | `POST /open/v1/task`。成功后记下任务 id、清单 id、etag，清脏 |
| 预设分组里尚无 `ticktick_task_id` 的任务，且该角色后来有了写入清单 | 下一轮同样按新建推送 |
| 已链接任务改标题或时段 | `POST /open/v1/task/{taskId}` |
| 已链接任务标完成 | `POST /open/v1/project/{projectId}/task/{taskId}/complete` |
| 已链接任务取消完成 | `POST /open/v1/task/{taskId}`，body 含 `status: 0` 以及 id、projectId、title 和当前时段 |
| 已链接任务删除 | `DELETE /open/v1/project/{projectId}/task/{taskId}`。若已有未完成的更新，放弃更新，只删除 |
| 在四个预设分组之间移动 | 目标角色有写入清单时，移到该清单（`POST /open/v1/task/move`），并更新 `ticktick_project_id`。目标角色没有写入清单时，本地分组保持不动，toast 与「还没有 TickTick 清单」相同 |
| 从预设分组移到自定义分组 | 删除远端任务，成功后清空链接列和全天标记。失败则保持链接、`ticktick_dirty = 1`，本地分组仍是自定义。之后的写回只发删除，不再更新标题 |
| 复制已链接任务到预设分组 | 新行清空 `ticktick_task_id`、`ticktick_project_id`、`ticktick_etag`、`ticktick_all_day`，再按新建推送 |

该角色没有写入清单时，新建不标脏，并 toast「主线还没有 TickTick 清单，这条任务只保存在本机。」支线、长期、杂项用对应组名。

自定义分组里的手建任务不推送。

`ticktick_dirty = 1` 时，这一轮整条以本地为准推回去，不用远端的标题、时段或完成状态覆盖它。推成功后再清 0。

创建请求超时，或成功响应里没有任务 id：把 `ticktick_dirty` 设为 2，不立刻再 `POST` 一次。之后的每小时对齐对这种行只做查找：在写入清单的未完成任务里，按标题和时段找尚未被本地链接占用的远端任务。恰好一条则接上并把 dirty 清 0。零条或多条：不创建，`ticktick_last_result` 写「有任务需要手动确认」，dirty 保持 2。用户按「立即同步」时先查找，仍是零条才再 `POST` 一次。

完成、取消完成、删除若返回 404：清掉脏标记。下一轮成功的全量拉取再决定保持完成还是删除本地行。

## 7. 每小时对齐

采样循环沿用云备份的每 20 拍检查。检查本身只读本地的开关、令牌是否存在、`ticktick_last_sync_at`。到点或开关刚打开时，在后台跑一轮，不占用采样线程。

一轮的顺序：

1. 开关关闭，或没有令牌：直接返回，不发请求。
2. 先推 `ticktick_dirty = 1` 的任务。没有远端 id 且 dirty 为 1 的，按新建推送。dirty 为 2 的本步不 `POST`。
3. 拉取每个已映射（角色不是忽略）清单的未完成任务：`GET /open/v1/project/{projectId}/data`。忽略 `items`。
4. 若 `ticktick_last_sync_at` 为空：不请求完成列表，也不删除。只导入未完成任务。这一轮全量清单都成功后，才写下 `ticktick_last_sync_at`。
5. 否则请求 `POST /open/v1/task/completed`。`startDate` 为上次成功时间减 300 秒，`endDate` 为现在。完成列表只用来给已经链接的任务打勾，不插入本地没有的历史任务。

对账（在 core 里算出动作，shell 只执行）：

- 远端未完成、本地没有该 `ticktick_task_id`：插入对应预设分组。重复为不重复。备注为空。全天与时段按第 4 节。
- 远端未完成、本地干净、etag 变了：更新标题和时段；若清单角色变了，改 `list_id`。本地已完成而远端仍未完成时，把本地改回未完成，不写回。
- 远端完成列表含有该 id，且本地不脏：本地标完成，不写回。
- 任务出现在另一份已经拉成功的已映射清单里：改 `ticktick_project_id` 和预设分组，不删除。
- 本地有链接、本地不脏、全部已映射清单和完成列表都成功、该 id 不在其中任何一份：删除本地行。移到「忽略」或未映射清单按这一条处理。
- 本地 `ticktick_dirty = 1`：不套用这一轮的远端标题、时段和完成状态。
- 本地 `ticktick_dirty = 2`：只在该行所属角色的写入清单里，按标题和时段匹配尚未被占用的未完成任务。恰好一条则写入链接并清脏。不删除这行，也不在自动轮次里创建。

任一已映射清单或完成列表失败：不删除任何已链接任务；失败清单上的已链接任务不改标题和时段；不前移 `ticktick_last_sync_at`。已经拉成功的其他清单仍可更新自己的干净任务。

`ticktick_last_sync_at` 只在全部已映射清单和完成列表都成功之后前移。脏任务推送失败不阻止前移；脏标记留到下一轮再推。

清单角色从主线改成支线：下一轮成功拉取时，这些任务改到支线分组。改成忽略：在全量成功时按删除处理。

## 8. 失败

未连接、令牌失效、超时、429：采样、判定、结算继续。设置页用中文记下原因。令牌刷新失败时保留原令牌，状态为「需要重新连接」，不清除已导入任务。

OAuth 被用户关掉、state 不一致、超时：不改已存令牌。

两台都打开总开关时，各自立刻写回，再用拉取按脏标记和 etag 合并。没有租约。后写回成功的修改会在另一台下一轮拉取时覆盖那台尚未推送的干净副本。

云备份恢复会带上任务行上的 TickTick id。恢复本身不打开总开关，也不发起 TickTick 请求。

## 9. 测试

`gamelife-core`：写入清单的 `sortOrder` 与 id 并列；忽略和未映射不导入；新的未完成任务进入对应预设分组；etag 变化且本地干净时采用远端标题和时段；本地脏则保留本地；完成列表只给已有链接打勾；换清单时改分组；全部成功且 id 消失才删除；任一清单失败则不删除；子任务不产生本地行；全天落在调用方传入的本地日界；已链接任务重复为不重复。

`gamelife`：假 HTTP 客户端。总开关关闭时没有出站请求。打开开关会立刻对齐一次。预设分组的新建、改标题、改时段、完成、取消完成、删除、跨预设分组移动会写回。只改备注或排序不写回。创建超时后不会马上再建一条，下一轮只在恰好一条时接上。`user_version` 从 5 升到 6。Client Secret 与 token 不在 `config.json`。清单拉取失败不挡住采样。

前端：总开关关闭时「立即同步」不可用；新清单默认忽略；同一角色多份清单时展示写入哪一个。逻辑放在 `src/lib/` 的纯函数里测。预览夹具提供 TickTick 状态，测试不打网络。

## 10. 不做

- 看板列角色、标题 `#标签` 改角色、滴答清单独立域名、非官方协议。
- 同步正文、备注、子任务、重复规则、提醒、标签、清单内排序。
- 把历史已完成任务灌进任务板。
- 自定义分组与 TickTick 互相同步。
- 用同步结果改判定流水线、发币或已结束的槽。
- 多设备锁、Webhook、槽开始时刷新 TickTick。
