# GameLife：云端备份与多端账本

日期：2026-09-14
状态：待用户审阅
范围：把本地库变成可云端备份、并让多台设备的观测汇入同一份账本。新增 `device_id` 维度、快照传输、确定性的结算归属。经济数值、判定顺序、缺口规则、TickTick 只读、视觉 fail-closed、保护窗口不截不传不付，全部不变。

本文覆盖并取代：

- 无。这是新增能力，不改既有判定与经济规则。
- `platform.rs` 的「macOS 数据目录不可搬」仍然成立 —— 云端存的是**副本**，本地库始终是运行时的唯一真相源。云端恢复是「在另一台机器上铺一份新库」，不是把数据目录指过去。

## 1. 问题

现状：所有数据只在本机 `~/Library/Application Support/GameLife/`。`gamelife.db` 688 KB（3 天 / 6132 samples / 164 slots），加 `config.json`、`secrets.json`（0600）、`screenshots/`（1.1 MB）。磁盘故障或换机器即全丢。

同时跨平台观测已经落地（`2026-09-14-cross-platform-observation-design.md`），但两台机器各有一份库、各有一个钱包。要在第二台机器上继续同一场游戏，必须有设备维度。

三个既存事实决定了方案形状：

1. **没有设备维度。** `slots` 主键 `(day, slot_start)`、`samples` 主键 `ts`、账本事件键形如 `xp_admin:2026-09-14:1789353000` / `validated_chore_xp:2026-09-14:1` —— 键里只有 `day` 与 `slot_start`。两台机器对同一个 15 分钟槽会写出同一主键的两条不同事实（Mac 上是 Cursor、PC 上是 Chrome）；账本键相同则意味着同一段工作时间被付两次币，或互相覆盖。`samples` 更直接：15 秒采样边界是对齐的，两台机器很容易撞上同一个 `ts`。
2. **账本不可撤销**（「Final/unknown slots are immutable」）。所以「谁结算这个槽」必须在写入之前就确定，不能事后回填。
3. **`samples` 原样保存窗口标题。** 当前库 5469 条 `title`、550 条 `url`、124 条绝对 `document_path`；脱敏只发生在 AI 层（`text_ai.rs::sample_summary_lines` 的 `filter(|l| !l.protected)`、`vision.rs::sanitize_vision_context`），采样写入时只记 `secure_input` 不剔除。**把 `samples` 原样上云，等于把保护窗口的标题与路径上云**，与既有不变量冲突。上云范围必须显式收窄，不能靠默认。

## 2. 成功标准

| 项 | 必须 |
| --- | --- |
| 单机行为零变化 | 只有一台已登记设备时，采样、判定、结算、今日页的即时反馈与今天完全一致 |
| 数据不再单点 | 本机库丢失后，能从远端恢复出一份可运行的库（判定历史、账本、商店、配置） |
| 标题不出本机 | 默认档位下 `samples` 与 `screenshots/` 不进入任何上传负载；`secrets.json` 永不进入 |
| 不覆盖 | 两台设备各自上传，路径隔离，任一设备的快照都不会覆盖另一台的 |
| 不重复付币 | 同一 `(day, slot_start)` 全系统只结算一次，且所有设备算出同一个结果 |
| 失败不阻塞 | 网络不可达、凭据失效、远端 4xx/5xx 都不得阻塞采样、判定、结算；重试由下一轮定时器承担 |
| 可关闭 | `sync.enabled = false` 时没有任何出网请求 |

## 3. 决策

### 3.0 分两个阶段交付

| 阶段 | 内容 | 风险 |
| --- | --- | --- |
| 一 | 单机快照备份与恢复。零 schema 语义变更，纯新增 `sync.rs` + 设置卡片 | 低。不碰判定、结算、账本 |
| 二 | 多端合并。`device_id` 标签、`merged.db` 派生视图、按日收齐结算 | 中。引入新的结算时序 |

阶段一独立交付、独立可用 —— 只有一台机器时它就是完整答案。阶段二只在真的加了第二台设备之后才有意义。

章节归属：**阶段一** = §5.1 / §6 / §8 / §9 / §12 前四行；**阶段二** = §3.3 / §3.4 / §5.2 / §7 / §12 后四行。

### 3.1 传输用整库快照，不用行级推送

`VACUUM INTO` 拿到一致性快照后整文件 PUT。理由：

- 无服务端代码要维护，无鉴权体系要建，无 D1 schema 与 SQLite schema 的双份迁移（现在 `user_version = 3`，且 `db.rs::migrate` 还有 `add_column_if_missing` 的增量列，双份维护会持续漂移）。
- 数据量无压力：整库 688 KB，`samples` 按 `sample_keep_days` 默认 7 天裁剪后是稳定体积。按小时上传一年的历史量也在任何 WebDAV / R2 免费额度之内。
- 恢复保真：账本、商店、`app_meta`、`config.json` 全都在，一次到位。

行级同步到 Cloudflare D1 作为**后续可选阶段**保留（见 §11）：它唯一多出来的能力是「从服务端查询 / 网页看板」，在不需要之前不值得付维护成本。

### 3.2 必须引入 `device_id`

这是本规格的核心改动，也是多端共用账本的前提。没有它，§1 第 1 条的三处主键冲突无法解决。

### 3.3 结算权按槽分配，不指定特权机器

用户明确不要「账本主机」。替代方案是让**归属规则本身是确定性的**：每个槽的结算权由数据算出，不由机器身份决定。

> **归属规则**：槽 `(day, slot_start)` 归 `observed_seconds` 最大的设备；平局取 `device_id` 字典序最小者。

所有设备拉到同一批快照后，会算出**完全相同**的归属结果，因此每台设备可以独立结算自己拥有的槽，而不会重复付币。任何设备离线都不影响其它设备。

### 3.4 收齐后结算（本规格的主要风险点）

归属规则要求「所有设备的该日数据都已就位」，但账本不可撤销，两者在时序上会打架：一台离线设备回来后，它拥有的槽可能已经由别人结算过。

解决方式是按日收齐，并按设备数分级：

| 已登记设备数 | 结算时序 |
| --- | --- |
| 1 | **逐槽立即结算**，与今天完全一致。今日页的即时反馈不变 |
| ≥ 2 | 某日 D 满足「所有已登记设备都已上传覆盖 D 的快照」或「D 结束已超过 `SETTLE_GRACE`（默认 36h）」后，任一设备均可结算 D。今日页显示**待结算预览**（`activity_seconds` 实时，金币/能量待落账） |

宽限期之后才到的设备数据，只进统计视图，不改账本 —— 丢的是那台设备离线期间那几天的币。这是显式接受的损失，换来的是账本不可撤销。

**这是需要用户确认的一处产品行为变化**：双机时今日页的能量不再实时跳动。

## 4. 同步范围

| 档位 | 内容 | 说明 |
| --- | --- | --- |
| `aggregate`（**默认**） | `slots`（清空 `screenshot_path` / `capture_context_json`）、`ledger`、`days`、`app_day_stats`、`host_day_stats`、`wishes`、`redemptions`、`entertainment_sessions`、`freeze_uses`、`misclassification_reports`、`policy_versions`、`config.json` | 判定结果、钱包、商店、汇总。窗口标题、URL、文档路径一个字不出本机 |
| `samples` | 上一档 + `samples`（`app` / `title` / `url` / `document_path` / `bundle_id` 原样） | 需要跨设备重算历史判定时用。等价于把浏览记录上云，需用户显式开启 |
| 永不 | `secrets.json`、`screenshots/`、`heartbeat`、`app_meta`、`ticktick_cache`、`task_lists` / `tasks`（legacy，无 UI 调用） | 密钥与截图不设档位，任何情况下都不上传 |

`VACUUM INTO` 复制整库，所以「永不」档必须在副本上**按表名删除**，不能只是「不计数」。`app_meta` 是例外：裁剪到只留 `device_id` 一个键 —— 该值本来就是远端目录名，不是秘密，留着能让恢复回来的快照继续给行打标。

`ticktick_cache` 是每台设备各自的缓存，不合并；`app_meta` 除 `device_id` 外都是本机同步簿记，不合并。

## 5. 数据模型

### 5.1 阶段一：只加列，不重建任何主键

阶段一**不改本地 schema 的语义**，只加设备标签：

- `slots` / `ledger` / `samples` / `app_day_stats` / `host_day_stats` / `days` / `policy_versions` / `misclassification_reports` 各加一列 `device_id TEXT NOT NULL DEFAULT ''`。
- 既有行在迁移时回填为本机 `device_id`（首次启动生成 UUID，写 `app_meta['device_id']`）。
- **新行由触发器打标**，不靠各写入点自觉：`AFTER INSERT … WHEN NEW.device_id = ''` 时把 `device_id` 置为 `app_meta['device_id']`。生产代码里这八张表有十几处 `INSERT`，散在 `sampler.rs` / `scheduler.rs` / `resolve.rs` / `commands.rs`，靠人工逐处加列迟早漏一处，而漏掉的行会变成无人可归属的静默脏数据。触发器从 `app_meta` 读值而不把 id 写死，所以恢复别家快照后它会用快照自带的 id 继续打标（`VACUUM INTO` 会保留触发器，已验证）。
- 用 `db.rs` 已有的 `add_column_if_missing` 完成，`user_version` 保持 3 —— 加列是幂等的，不构成版本语义变更。

`db::local_device_id` 是本机身份的唯一实现，`sync::device_id` 转调它：远端目录名与行内 `device_id` 因此不可能不一致。

本地库的主键**不需要**设备维度：每台设备一份库，`(day, slot_start)` 在本机仍然唯一。设备维度只在合并时才有意义，所以不需要碰 `resolve.rs` / `scheduler.rs` 的任何查询。

### 5.2 阶段二：合并视图用独立的库文件

多端合并**不在活库上改主键**。合并结果写进 `~/Library/Application Support/GameLife/merged.db`，它有自己的一套主键：

| 表 | 主键 |
| --- | --- |
| `slots` | `(device_id, day, slot_start)` |
| `samples` | `(device_id, ts)` |
| `app_day_stats` | `(device_id, day, app, bundle_id)` |
| `host_day_stats` | `(device_id, day, host)` |
| `days` | `(device_id, day)` |
| `policy_versions` | `(device_id, id)` |
| `misclassification_reports` | `(device_id, id)` |

`merged.db` 是**只读派生数据**，由同步流程整体重建（写临时文件再原子替换），采样、判定、结算都不读它。判定链路与活库的主键完全不动 —— 这是把「多端合并」的风险从判定管线里摘出去的关键。

**`ledger.reward_event_key` 保持全局唯一、不加设备前缀** —— 这是结算互斥的关键机制，见 §7.3。

钱包侧的表主键已是全局唯一的 TEXT（`wishes.id` / `redemptions.redemption_id` / `entertainment_sessions.redemption_id` / `freeze_uses.protected_date`），合并时直接 union。`wishes` 新增 `updated_at INTEGER`，两设备改同一条时取新。

## 6. 传输层

远端布局：

```
<remote>/gamelife/devices.json                    设备登记表
<remote>/gamelife/<device_id>/latest.db           该设备最新快照
<remote>/gamelife/<device_id>/snapshots/<yyyymmdd>.db   滚动保留 keep_snapshots 份
<remote>/gamelife/merged/latest.db                合并视图（可选，供只读展示）
```

`devices.json` 记 `{device_id, label, platform, last_seen}`，每台设备上传快照时更新自己的条目。§3.4 的「已登记设备」就是这个文件里的集合。

快照生成顺序（不能颠倒）：

1. `VACUUM INTO <临时文件>` 得到一致性副本（目标已存在时 `VACUUM INTO` 会失败，先删）。**此时还没有裁剪**，本地库不受影响。
2. 打开临时副本，按 §4 档位裁剪（`aggregate` 档 `DELETE FROM samples`，并把 `slots.screenshot_path` / `capture_context_json` 置 NULL）。
3. 压缩后 PUT 到 `<remote>/gamelife/<device_id>/latest.db`，再按日期存一份滚动快照。
4. 删除临时文件。

`RemoteTarget` trait 两个实现，共用 `reqwest`（已在依赖里）：

- **WebDAV** —— `PROPFIND` / `PUT` / `GET` / `DELETE` / `MKCOL`，Basic 或 Digest 认证。覆盖坚果云、Nextcloud、Synology。
- **S3 兼容** —— SigV4，覆盖 Cloudflare R2、Backblaze B2、MinIO。

凭据只进 `secrets.json`：`sync-webdav-password` / `sync-s3-secret`，沿用 `keychain.rs` 既有的 `set_in` / `get_in`，模块名不改。

## 7. 合并与结算

### 7.1 合并视图

拉取所有已登记设备的 `latest.db` 到本地缓存目录，逐表 union 成合并视图。合并视图**只读**，供统计页、商店页展示，不参与判定。

### 7.2 规范槽集合

对每个 `(day, slot_start)`，按 §3.3 的归属规则选出唯一 owner，其余设备的同槽记录从合并视图里丢弃。丢弃的记录仍在各自设备的库里，不删。

### 7.3 账本互斥

`reward_event_key` 全局唯一，所以两台设备即使同时结算同一个槽，也只有一台的 `INSERT` 成功，另一台拿到 `DbOpError::AlreadyApplied` —— 这正是既有代码已经吞掉的唯一错误类型（「Only `DbOpError::AlreadyApplied` may be swallowed」）。归属规则保证两者算出的金额一致，所以谁先落账都不影响结果。

### 7.4 日结算

`days.settled_at` / `outcome` 由「该日拥有槽数最多的设备」写（平局取 `device_id` 最小者），同样是确定性规则。

## 8. 触发点

- 定时器：复用 `scheduler` 的 tick，间隔 `sync.interval_minutes`（默认 60）。
- 应用退出前（托盘 → 退出）。
- 某日结算完成之后。
- 设置页「立即同步」。

全部走异步路径，不在主线程做 HTTP —— 沿用既有约定（「Commands that touch the network must be `async`」）。

## 9. 设置与命令

`config.json` 新增：

```json
"sync": {
  "enabled": false,
  "target": "webdav",
  "url": "",
  "remotePath": "gamelife",
  "username": "",
  "intervalMinutes": 60,
  "scope": "aggregate",
  "keepSnapshots": 7,
  "deviceLabel": "",
  "settleGraceHours": 36
}
```

新增命令：`sync_status`、`sync_now`、`sync_test_connection`、`sync_set_credentials`、`sync_list_devices`、`sync_restore`（从远端铺一份新库，需重启生效）。

设置页新增「云端备份」卡片：开关、目标类型、地址、账号、凭据、同步范围、间隔、立即同步、设备列表、最近一次同步结果。凭据输入框只写不读回。

## 10. 不变量（新增）

- `secrets.json`、`screenshots/` 不进入任何上传负载，任何档位下都不例外。
- `scope = aggregate` 时 `samples` 不出本机。这一条要有一条测试直接断言上传负载里不含 `samples` 表与任何标题字符串。
- 快照是只读副本；本地库始终是运行时的唯一真相源。远端永远不是判定输入。
- `ledger.reward_event_key` 全局唯一、永不重写。
- 网络失败不阻塞采样、判定、结算；同步失败只记状态，不抛给采样循环。
- 关闭同步后进程不出网（TickTick 与视觉 API 除外，它们有自己的开关）。

## 11. 不做

- 不做实时双向同步。同步是定时的、单向的（本机 → 远端）。
- 不做服务端查询、网页看板、D1 行级同步 —— 留作后续阶段，只有真需要从服务端读数据时才做。
- 不搬 macOS 数据目录。
- 不上传 `screenshots/`。
- 不引入账号体系。只有设备标识 + 一个共享凭据。

## 12. 验收

| 场景 | 期望 |
| --- | --- |
| 单机开启同步 | 上传负载含 `slots` / `ledger` / 汇总 / 钱包，不含 `samples` 行、不含任何 `title` 字符串、不含 `secrets.json` |
| 单机行为回归 | `sync.enabled = false` 与今天逐字节等价；开启后采样、判定、今日页即时反馈不变 |
| 两台设备 | 各自 PUT 到自己的 `<device_id>/` 路径，互不覆盖；`devices.json` 出现两条 |
| 同槽冲突 | 构造两台设备对同一 `(day, slot_start)` 的观测，结算后账本只有一条，金额与 owner 规则一致 |
| 离线设备 | 设备 B 离线 3 天。A 在宽限期内不结算那几天；B 回来后收齐，结算结果与「两台一直在线」一致 |
| 宽限期后到达 | 超过 36h 才到，账本不改，合并视图更新，统计页可见 |
| 网络故障 | 远端 500 / 凭据失效 / DNS 失败，采样与判定照常，状态显示失败原因 |
| 恢复 | 空数据目录 + 远端凭据 → `sync_restore` 铺出新库，历史周报、账本余额、商店解锁全部正确 |

## 13. 未决与风险

1. **双机时今日页的能量不再实时跳动**（§3.4）。这是为了让账本不可撤销而付的代价。若用户不接受，唯一替代是放弃跨设备共用钱包，退化为「每台设备各自结算、各自钱包」，本规格的 §3.3 / §3.4 / §5 大部分可以删掉。
2. **`VACUUM INTO` 在大库上的耗时**未知。当前 688 KB 无感，需要确认库增长到几十 MB 时是否要挪到后台线程并限制频率。
3. **WebDAV 服务端的 `PUT` 原子性**不统一。部分实现直接覆写，中断会留下半个文件。缓解方式：先 PUT 到 `latest.db.tmp` 再 `MOVE` 覆盖（若服务端不支持 `MOVE`，回退为直接覆写并在下载侧校验 SQLite 头与 `PRAGMA integrity_check`）。
4. **`config.json` 的合并语义**未定。两台设备的规则表（`distraction_rules` 等）不同时，合并视图展示哪一份？当前倾向：合并视图用 owner 设备的规则，设置页只展示本机规则。
