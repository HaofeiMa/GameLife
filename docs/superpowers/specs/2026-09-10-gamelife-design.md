# GameLife 设计文档

日期：2026-09-10  
状态：按第三轮评审修订，待再次确认后进入实现计划

本文定义第一期产品。实现计划另写；未写入本文的行为都不做。

## 0. 本轮修订要点

1. **`unobserved`：** GameLife 退出、崩溃、被强退、系统重启后，心跳缺口补成「当时没有可靠观测」，不是 Away，也不是空白。credited = 0，本周单独一行。
2. **`verified_core_seconds`：** 灰区里视觉或人工确认 Core 后，把与截图上下文一致、且无显式 side/distraction/away 的 `unsure` 升级为已核实 Core。`credited = strong_core + reading_bridge + verified_core`。Isaac Sim 这类未知工具可以拿到真实 credited，但一张论文图仍不能无脑补满 900 秒。
3. **采样缺口不外推：** 相邻样本间隔 > 2×15s 的中间段是 `unobserved`。恒有 `credited ≤ observed ≤ 该槽实际时长`。
4. **`capture_scheduled_at` 持久化。** 重启不重新抽签；错过则 `capture_missed`，禁止启动后立刻补截。
5. **Early Start 在当天第一个完整 900 秒 Validated Core 完成后才发**；时刻取构成这 900 秒的那一段连续有效 Core 的起点，不能 08:25 打卡 1 分钟再去吃饭。
6. **本周按 `activity_seconds` 加总**，不是 dominant category × 15。
7. **连胜由 completed | protected | failed 重算**；冻结额度按被保护日所属月份。
8. **DB：只有幂等键 UNIQUE 才当已处理；BUSY 重试；IO/FULL/CORRUPT 标未决议并通知。**
9. **商店单事务防双击。** XP 按每 90 秒累计；Gold Day 后不再产生任何游戏货币。
10. **已 final 的槽 V0.1 不改经济结果**；只能 Report misclassification。
11. Never Capture **内置项各自不可删**；采样间隔 V0.1 **固定 15s**；UI 显示「估计」有效主线，精确到分钟。

## 1. 问题与目标

使用者是科研人员。坐在电脑前时，容易刷手机，或做「看起来像工作」的杂事（个人主页、统计工具、整理 GitHub、甚至打磨 GameLife 本身），而不是当天的主线科研。

GameLife 是 macOS 常驻桌面应用。它 **估计** 有效主线时间，再用能量做当天即时反馈、用硬币做长期愿望，把工作日拉向稳定的 **480 分钟** 估计有效主线。

产品定位：**Research Time Validator**。底层是 15 秒采样 + 规则/视觉推断，不是秒级实录。

成功标准（工作日）：

- 无法靠「检查前几秒切回论文」把整格刷成 15 分钟有效 Core。
- 无法靠「退出 GameLife → 做杂事 → 再打开」让缺口消失或变成 Away。
- 读 PDF、看 figure、想实验设计，不会只因为几分钟没键鼠而整格作废。
- **不声称**能区分「Preview 里读论文」和「论文开着、人在刷手机」。
- 未知科研工具（Isaac Sim 等）在截图/人工确认后可以获得 credited，不是 category=core 且 credited=0。
- 下午才开始时，空白不会把硬币扣成负数。
- 本周能看见真实的 Side Projects 累计，不会被主类别吃掉。
- 关主窗口不会停止采集；只有托盘「退出」才退出。退出之后的时间记 unobserved。

## 2. 第一期范围

做：心跳、15 秒采样、灰区随机前台截图、1–3 条 Quest 快照、六类 + unobserved + 每槽 credited、能量与硬币、早开始、Chest、Gold Day、连胜与冻结、托盘 + 主窗口。

明确不做：健康数据、摄像头、手机端、Windows/iPhone、云同步、真下单、节假日日历、`support_counts_as_core`、系统扣币、早收工、9h 超额币、追溯改账、采样间隔可调、已 final 槽的经济冲正。

浏览器 URL **可选**。判定不得依赖「一定读到 URL」。

空闲：`CGEventSourceSecondsSinceLastEventType`（或等价），不记录按键内容。截图默认 `capture_scope = frontmost_window`。检测到系统 **secure input / 密码 UI** 时与 Never Capture 一样跳过截图。

## 3. 架构

**Tauri 2 + React + TypeScript + Rust**。本地 SQLite。

启动只出托盘；关窗口采集继续；托盘「退出」才结束进程。退出前写最后一次心跳。

数据根：`~/Library/Application Support/GameLife/` — `gamelife.db`、`config.json`（无 Key）、可选 `screenshots/`。API Key 只在钥匙串。

`last_heartbeat_at` 每次成功采样更新（也在干净退出时更新），存在 DB 或同目录小文件，崩溃后仍可读。

## 4. 模块

1. **Heartbeat / Sampler** — 工作日、进程在、未暂停：每 **15 秒**（V0.1 固定，设置里不出现）写样本。URL 去 query/fragment。
2. **Capture** — 开槽时写入 `capture_scheduled_at`（槽内第 4–13 分钟均匀随机）并持久化。到点截前台窗口。Never Capture、secure input、锁屏、暂停：不截。是否上传由灰区规则决定。
3. **Judge** — 用本槽开始时的 Quest/政策快照 + 观测到的样本（含缺口规则）+ 可选视觉，输出 category、`activity_seconds`、`credited_core_seconds`、status。
4. **Ledger** — 已决议槽的同一事务记账。商店兑换另见第 9 节。
5. **UI**

## 5. 一日节奏、心跳缺口、午夜

工作日采样；周末不采样。无 Quest 时仍采样，但 credited 只能为 0。

槽：`:00–:15` … `23:45–00:00`。

**同日进程消失（退出 / 崩溃 / 强退 / 系统重启）：**  
启动后读取 `last_heartbeat_at`。若心跳与现在是 **同一个自然日**：把 `(last_heartbeat, now)` 写入对应槽的 `unobserved`，**禁止**记成 `break_away`，**禁止**丢弃。这 85 分钟可能是 Bilibili 也可能是真工作，系统不知道。

**跨日重启：** 只把心跳补到 **心跳当天 24:00**（周末跳过）。新的一天从本次启动开始观测，**不要**把今天 00:00 到启动前填成 unobserved（否则每天早上都会多出数小时噪声）。

暂停中进程仍在：记 `break_away`。机器睡眠且进程仍在：`break_away` / `missed_sleep`。进程已经没了：只能是 unobserved。

**午夜：**

1. `00:00:00` 决议昨天 `23:45–00:00`。
2. `00:00:05` settle 昨天：pending → `unknown`（0 新奖励）；按第 8.5 节 **重算** 连胜；不写 XP 负数。
3. 开始新的一天。

**结束今天：** 确认对话框。决议当前半截槽（遵守「不外推」、截图未到则 `capture_missed`）→ 后续槽不采样 → 立刻 settle。

Quest/政策修改只影响 **尚未开始** 的槽。已开始与历史不重算。

## 6. 类别、观测、估计有效时间

| 字段 | 含义 |
| --- | --- |
| `category` | 时间轴用的主类别（dominant），可为空/pending/unknown/unobserved |
| `activity_seconds` | 长期保存：`core, support, admin, side, distraction, away, unobserved` |
| `credited_core_seconds` | 计入 8h 进度的估计有效主线秒数，0–900，且 **≤ observed_seconds ≤ 本槽实际时长** |

整槽被退出覆盖时，dominant 可以是 `unobserved`。它不是六类 Activity 之一，本周单独显示。

**8 小时 = 当天 `SUM(credited) ≥ 28800`（480 分钟）。** UI 写「估计有效主线」，显示 `5h 23m`，**不要**显示秒。

两套格子：

- **时间轴：** 时钟槽，显示 dominant + 该槽估计 `credited` 分钟（整分钟）。一天最多 96 槽。
- **8h 进度：** 32 个「估计 15 分钟」单位，按当天累计 credited 从左到右填。Chest = 24 格，Gold Day = 32 格。

**本周账单：对 `activity_seconds` 做跨槽求和。** 禁止 `dominant × 15 分钟`。8 分钟 Core + 7 分钟 Side 的槽，周报必须 Core+8m 且 Side+7m，即使 dominant 是 Core。pending/unknown 的未归类秒可进「未复核」；unobserved 单独一行。

Support/Admin 不折算进 480 分钟。

## 7. 槽如何决议

### 7.1 采样与外推禁令

采样间隔 `I = 15s`。相邻样本 `t0 < t1`：

- `t1 − t0 ≤ 2I`（30s）：`[t0, t1)` 为 **observed**，按 `t0` 的 hint 前向填充。
- `t1 − t0 > 2I`：`[t0, t0+I)` 为 observed（若仍 `< t1`），`(t0+I, t1)` 为 **unobserved**，不得因为两端都是 Core 就填满。

槽开头到第一条样本、最后一条样本到槽实际结束（含「结束今天」半截槽、中途启动）：同样用 2I 规则；超出部分 unobserved。

因此半截槽、重启后的槽都满足：

`0 ≤ credited_core_seconds ≤ observed_seconds ≤ actual_slot_duration ≤ 900`

### 7.2 截图时刻持久化

创建槽行时写入 `capture_scheduled_at`，之后 **禁止改抽签**。

重启时：

- 现在 < 计划时刻：等到点再截。
- 现在 ≥ 计划时刻且尚未截成功：`capture_status = missed`，**不要立即补截**（否则不再是随机抽样）。
- Never Capture / secure input / 锁 / 暂停：`skipped`，不是 missed。

### 7.3 样本 hint

Idle 只是证据。阅读类最多在上一次 `core_candidate` 有效交互后 **桥接 5 分钟** `core_reading`；再长无输入 → `unsure_reading`。电脑端无法区分读 PDF 和刷手机。

Hint 顺序：锁屏/睡眠/暂停 → `away`；Distraction 规则 → `distraction`；Side Project 规则（含 **不可删除的 GameLife 自身**）→ `side`；idle≥10 分钟且非阅读类 → `away`；未知 App → `unsure`；Trusted 且标题/路径命中本槽 Quest 快照 → `core_candidate`（README/设置页不算）；Trusted 无 Quest 证据 → `unsure`。

Distraction 与 Side Project 规则必须分开。个人主页、AI Usage、GameLife 只能走 Side Project。

Trusted 是信任列表，不是工作应用全集。

### 7.4 聚合、视觉、verified_core

先把 **observed** 秒按 hint 归入 `activity_seconds`（unobserved 已分开）。定义：

- `strong_core` = 有效交互下的 `core_candidate`
- `reading_bridge` = `core_reading`，单槽 ≤ 300
- `verified_core` = 灰区确认后升级的秒数，初始 0
- `credited = min(observed, actual_duration, strong_core + reading_bridge + verified_core)`

截图仍是额外证据：

1. 明显离开：away 多、strong_core 少 → dominant `away`，credited=0，不上传。
2. 元数据强 Core：`strong_core ≥ 13 分钟` 且 side+distraction ≤ 1 分钟 → `core_research`，`verified_core=0`，credited 用上式，不上传。`unsure_reading` 不自动计入。
3. 明显非 Core：side+distraction 占优 → 对应类别，credited=0，不上传。
4. **灰区**（未知 App、大量 unsure、混杂）：有截图则上传。  
   - 视觉 `core_research`（confidence≥0.7）：把 **与截图上下文一致**（同一前台应用/同一窗口族）且 **没有** 显式 side/distraction/away 证据的 `unsure` / `unsure_reading` 升级为 `verified_core`，并加进 `activity_seconds.core`。Isaac Sim 做满 15 分钟且图是实验 → 可 credited 接近 observed，不是 0。  
   - **禁止**把其他应用时段、unobserved、已标 side/distraction/away 的秒补进 credited。一张论文图不能把整槽补到 900。  
   - 视觉非 Core 且与元数据同向：按视觉类别，credited=0。  
   - **冲突**（元数据偏 core、图是微信；或元数据全 unsure、图是论文但应用对不上）：`pending_review`，credited=0。  
   - 无图 / missed / Never Capture / API 失败：pending。

人工点 Core：与视觉相同，把无负向证据的 unsure / unsure_reading 升为 `verified_core`（人在为阅读/未知工具背书）。点其他类别：credited=0，但 **仍按样本把 activity_seconds 写入**，周报不丢 Side。

日终 pending → `unknown`，credited=0。不扣币。

### 7.5 已决议槽

`status = final` 后 **V0.1 不得改 credited、类别经济结果、ledger**。提供 **Report misclassification**：存原因、槽 id、可选备注，供改规则，**不** 重发/扣 Coin、不重算 streak。完整 correction ledger 留 V0.2。

pending 被人工点选是「首次决议」，不是改 final。

### 7.6 隐私

默认截图用完即删。Debug 保留：不保留 / 24h / 3 天 / 14 天。

**Never Capture：** 内置 1Password、Bitwarden、Keychain Access、系统密码/secure-input **每一项都不可删除**。使用者可另增可删项。内置项误删保护不存在「删光才不允许」。

原始 samples 默认 7 天（3 或 14 可调）。长期只留 slot 摘要（含 `activity_seconds`）。

## 8. 经济

硬币只 earn/spend，最低 0。数值暂定，约 10 个工作日自测后校准。无早起无连胜的 Gold Day 目标总币 **35–45**。

### 8.1 货币

XP 按日 `SUM(xp_delta WHERE day=今天)`，禁止清零负分。商店：`SUM(credited) ≥ 3600` 后解锁。

**Gold Day（当天 credited ≥ 28800）之后：** 活动与 `activity_seconds` 继续记；**不再** 产生 Coin，也 **不再** 产生 XP（含 Support/Admin XP）。爆肝只进账单，不进游戏货币。文案：

> Gold Day completed. Additional work is recorded, but no more Coins or XP are earned.

### 8.2 累计发放

- Coin：每 **900** 秒 credited → +1，每天最多 32。`validated_coin:<date>:<n>`
- Core XP：每 **90** 秒 credited → +1，每天最多 320（480 分钟）。`validated_xp:<date>:<n>`  
  15 分钟估计有效 = 10 XP。禁止逐槽 `floor(10 × credited/900)`（会在 partial 槽上丢 XP）。
- Support：Gold Day 前每个 dominant=support 的 **final** 槽 +6 XP（`xp_support:<date>:<slot_start>`）
- Admin：Gold Day 前每天最多 4 槽 +2（`xp_admin:<date>:<slot_start>`）

达到 900/90 秒阈值时按当天累计补发缺失的 n，与槽边界无关。

### 8.3 阶梯（暂定）

2h +1、4h +2、6h +3（Chest）、8h +5（Gold Day）。无 9h。锁定态用分钟：`200/360 min`。

### 8.4 早开始

**不在** 第一次 credited>0 时发。

当当天累计 credited **首次达到 900** 时发一次 `early_start:<date>`。奖励时刻 = 构成这第一个 900 秒的 **连续估计有效 Core 段的起点**：从达到 900 的那一时刻沿 credited 时间向前回溯，遇到 >2I 的 unobserved/空洞则停止。

- 08:25 起连续做到 08:40 凑满 15 分钟 → 起点 08:25 → +8
- 08:25 做 1 分钟，10:30 才回来凑满 → 回溯停在 10:30 一段 → 按 10:30 落入 ≥10:00 → **0**，不得用 08:25

| 该起点 | 硬币 |
| --- | --- |
| < 08:30 | +8 |
| 08:30–09:00 | +6 |
| 09:00–09:30 | +4 |
| 09:30–10:00 | +2 |
| ≥ 10:00 | 0 |

### 8.5 连胜与冻结

每个已 settle 的工作日恰好一个结果：`completed`（credited≥21600）| `protected`（冻结）| `failed`。

**当前连胜不存成权威 `streak_length`。** 每次用工作日结果从近到远重算：从最近一个已 settle 工作日往回，`completed` 或 `protected` 则 +1，遇到 `failed` 停止。周末跳过不打断。

因此：周一 failed 把连胜显示为 0；周二给周一冻结 → 周一变为 `protected` → 连胜恢复为「周一之前的长度 +1」；周二再 `completed` → 再 +1（即原长度 +2）。禁止在 freeze 时手加一个已定死的整数导致 off-by-one。

`protected` 计入连胜长度，但不补 Chest/Gold/validated_coin，不追扣已发币。里程碑 3/5/10/20：当重算后的长度 **首次跨越** n 时尝试插入 `streak_milestone:<n>:<date>`，`date` 为造成跨越的那一天（冻结日或完成日）。UNIQUE，冻结重试不双发。里程碑允许在冻结日跨越时发（保护的就是连胜，不是当天工时奖）。

**冻结窗口：** 该日 settle 之后，到下一工作日 settle 之前。日间不能预冻。

**每月 2 张：** 额度按 **被保护日 `protected_date` 的日历月** 计，不按点击 Freeze 的月份。3 月 31 日在 4 月 1 日点冻结，仍占 3 月额度。

### 8.6 Ledger 冲突

- 预期幂等键 UNIQUE（`reward_event_key`、`slots(day,slot_start)`、`redemptions.id`）→ **查出已有行，视为已处理**
- `SQLITE_BUSY` / locked → 有限次退避重试
- `IOERR` / `FULL` / `CORRUPT` → 槽保持未决议，通知「记账失败，未写入」，**绝对不当成已记账**

决议槽：单事务写 slot 摘要 + 补发缺失的 validated_coin/xp + 阶梯。

## 9. 商店

自建愿望。娱乐 XP 奖必须有结束时长。

**兑换必须在一个 SQLite 事务内：** 读余额（Coin=全历史 SUM，XP=当日 SUM）→ 不足则 rollback → 插入 `redemptions`（主键 `redemption_id`，客户端每次点击新 UUID）→ 写 ledger `spend`（`shop_spend:<redemption_id>` UNIQUE）→ commit。

双击两次 = 两个 UUID：第二次若余额不足则失败；若设计成「同一愿望冷却」另议，V0.1 靠余额挡住连兑。UI 按钮在请求进行中 disable，防抖。

满 60 分钟估计有效 Core 前 XP 按钮锁定。

## 10. 界面

托盘：`5h 23m / 8h`（估计），不要秒。结束今天要确认。冻结入口仅 settle 后未满 6h。

今日：Quest、32 格估计进度、Chest/Gold、XP 锁、连胜（重算值）。不显示截图时刻。

时间轴：dominant、该槽估计分钟、activity 拆分摘要。final 槽提供 Report misclassification，不提供「改成 Core 并补币」。pending 提供首次分类按钮。

本周：Core / Support / Admin / Side / Distraction / Away / **Unobserved** / 未复核。Side 必须能到 8h35m 这种数。

设置：无采样间隔旋钮。Never Capture 内置项只读列出。无 `support_counts_as_core`。

## 11. 数据模型

- `quest_versions` / `policy_versions`
- `days`：`settled_at`，`outcome`（completed|protected|failed|null），**不把 streak_length 当权威**
- `samples` 短期；`heartbeat` 最新时间
- `slots`：`UNIQUE(day,slot_start)`；版本 id；`capture_scheduled_at`；`capture_status`；`category`；`status`；`activity_seconds` JSON；`credited_core_seconds`；`observed_seconds`；`used_vision`
- `ledger`：`reward_event_key` UNIQUE
- `wishes`；`redemptions(redemption_id PRIMARY KEY)`
- `freeze_uses(protected_date UNIQUE)`，额度用 `strftime('%Y-%m', protected_date)`
- `misclassification_reports`

## 12. 错误处理

| 情况 | 行为 |
| --- | --- |
| 缺权限 | 横幅；无观测能力时段走 unobserved 或 missing_permission，不伪装 Away |
| 心跳缺口 | unobserved |
| 截图 missed | 不补截；灰区 pending |
| 视觉冲突/失败 | pending |
| UNIQUE 幂等键 | 已处理 |
| BUSY | 重试 |
| IO/FULL/CORRUPT | 未决议 + 通知 |

## 13. 测试

必须覆盖：

- 10:05 退出、11:30 启动：缺口为 unobserved，不是 away，credited=0。
- Isaac Sim 15 分钟 unsure + 视觉 core：verified_core>0，credited 明显大于 0，且 ≤ observed。
- 两端 Core、中间 4.5 分钟无样本：中间 unobserved，不得 credited 那 4.5 分钟。
- 半截「结束今天」槽：credited ≤ observed ≤ 实际时长 < 900。
- 崩溃后 `capture_scheduled_at` 不变；已过点则 missed，不立即截。
- 08:25 一分 + 10:30 补满 900s：early_start 时刻不是 08:25，奖金为 0。
- 08:25–08:40 连续 900s：early_start 为 08:25 档。
- 8m core + 7m side：周报两边都有，不是 Core+15。
- 周一 failed 再 freeze：连胜恢复为冻前+1；周二 completed 为冻前+2；milestone UNIQUE。
- 4 月 1 日冻 3 月 31 日：占 3 月额度。
- UNIQUE 重试不双发；IOERR 不把槽标成已发奖。
- 商店双击：最多一笔 spend。
- 大量 12m partial：XP 按 90s 累计，不低于「逐槽 floor」那条错误公式。
- Gold Day 后再 Core：无新 coin/XP。
- final 槽 report 不改 ledger。
- 内置 Never Capture 不可删。

## 14. 实现切片

1. 心跳、固定 15s 采样、缺口=unobserved、槽对齐、午夜、UNIQUE。
2. Judge（不外推、credited 约束、activity_seconds、时间轴+周报求和）。
3. 持久化 `capture_scheduled_at`、missed、Never Capture/secure-input、灰区视觉与 verified_core、pending。
4. validated_coin/xp 累计、Gold Day 封顶、早开始连续段、商店事务。
5. 连胜重算、冻结窗口与月份额度、结束今天、misclassification 记录。

## 15. 以后

睡眠/运动另开规格。读 PDF vs 刷手机需要手机或摄像头，V0.1 不做。历史经济冲正留 V0.2。
