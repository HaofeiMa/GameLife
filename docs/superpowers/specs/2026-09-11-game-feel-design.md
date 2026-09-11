# GameLife C：游戏手感与可用商店

日期：2026-09-11  
状态：已确认（用户授权跳过规格文件审阅，直接进入实现）  
范围：反馈层 + 商店可用性（F3）。不改 Judge、采样、观测引擎、Quest 文案、coin/XP 公式、Gold Day 封顶、连胜重算。

产品仍是科研主线时间校验器。C 学 Habitica 的「完成瞬间」，不学打卡、扣血、等级、掉落。Habitica 为 GPL，禁止拷贝其源码或资源。

本文覆盖并补充 `2026-09-10-gamelife-design.md` 第 8–10 节的**呈现与商店操作**，不改写那些经济不变量。

## 1. 问题

经济规则已经在 `gamelife-core`：Coin / XP tick、阶梯、早开始、连胜、商店单事务兑换。薄的是：

- 发奖没有瞬间：数字刷新，没有「刚刚发生了什么」
- 商店不能当产品用：愿望要手写进数据库，兑换没有回执
- 娱乐 XP 有时长字段，但兑换后没有任何倒计时
- 主窗口关闭时托盘只显示 `5h 23m / 8h`

## 2. 成功标准（F3）

| 能力 | 必须 |
| --- | --- |
| 愿望 | 商店页创建、改名/改价、停用；空商店有「添加愿望」；不预置货架 |
| 兑换 | 现有单事务 + UNIQUE 保持；错误码回到 UI |
| 娱乐倒计时 | 带时长的 XP 兑换后，主窗口与托盘显示剩余时间；到点提醒（仅窗口开着时 toast） |
| 同时一段 | 未结束会话存在时，再兑带时长 XP 失败，不扣第二次 |
| 反馈 | 窗口开着：对新账本行 toast（tick 合并）；关窗：不弹系统通知，只改托盘 |
| 判定 | 倒计时期间 Bilibili 等仍按原规则，不是豁免券 |

明确不算失败：关着窗口错过 toast；Cursor 等与本规格无关。

## 3. 不变量

1. 不改 `tick_keys_for_credited`、Gold Day 后不再产生 Coin/XP、连胜重算、冻结额度。
2. 硬币只 earn/spend，最低 0。XP 按日 SUM，禁止清零负分。
3. 兑换仍在一个 SQLite 事务内：余额 → `redemptions` → `shop_spend:<redemption_id>`。失败整单 rollback。
4. 同一 `redemption_id` 再提交 → 已处理，不双扣、不插第二段会话。
5. `sample_once` / Judge / `CaptureContext` 本波不动。
6. 无 macOS 系统通知，不申请通知权限，无音效。
7. 同时最多一段 `now < ends_at` 的娱乐会话。Coin 愿望无时长，不受这把锁。
8. `document_path` 与本规格无关。禁止从标题伪造路径（既有禁令，C 不触及）。
9. 测试禁止新增 `std::env::set_var("HOME", …)`。
10. 改 `src-tauri/` 后 `cargo test --offline -p gamelife` 必须编译并跑过。改 `src/` 后 `npx vitest run` 必须绿。
11. 不把浏览器自动化、辅助功能做成商店前置。

## 4. 架构

```
槽决议 / 连胜里程碑 / 兑换
  → 现有 ledger / redemptions
  → 带时长 XP 兑换额外写入 entertainment_sessions

主窗口已打开
  → 约 5s 轮询 TodayView
  → 第一次轮询只记账本水位，不 toast 历史
  → 之后 ts > 水位 的行交给 coalesce；关窗丢水位

主窗口关闭
  → 无 toast、无系统通知
  → 托盘：`5h 23m / 8h`，有未结束会话则 `· {name} {Nm}`

娱乐倒计时只改 UI 和托盘，不改 hint / credited / distraction
```

会话落盘：`ends_at` 续算。过期不删行。打开窗口时若已过期：商店/今日显示「已结束」，不补 toast。窗口一直开着、剩余从 >0 变成 0 → 可以 toast「到点了」。

## 5. 模块

### 5.1 `gamelife-core` 商店规则

保留 `validate_redeem` / `xp_shop_unlocked`。

新增：

- `validate_wish(name, kind, price) -> Result<(), WishError>`
  - 名称 trim 后非空，长度 ≤ 80
  - `price > 0`
  - XP：必须 `duration_minutes ≥ 5`
  - Coin：不得带时长（`duration_minutes` 为 Some 则失败）
- `has_entertainment_timer(kind) -> bool`：`Xp { Some(d) } if d >= 5`
- `can_start_entertainment(now, active_ends_at: Option<i64>) -> bool`：`Some(ends) if now < ends` 则为 false
- `entertainment_remaining_secs(now, ends_at) -> i64`：`ends_at - now`（可负）
- `tray_entertainment_minutes(remaining_secs) -> Option<u32>`：`<= 0` → None；否则 `(remaining_secs / 60).max(1)`

`RedeemError` 增加 `EntertainmentInProgress`、`WishArchived`、`WishMissing`。`FinalSlotImmutable` 仍不用于兑换。

既有库里 `duration_minutes = None` 的 XP 愿望：仍可按 `validate_redeem` 兑换，**不**开倒计时（grandfather）。新创建的 XP 必须有时长。

### 5.2 `gamelife-core` 反馈合并

`coalesce_feel_events(rows: &[LedgerSlice]) -> Vec<FeelNotice>`

`LedgerSlice { key, coin, xp }`（不含 ts；调用方已按水位过滤）。

规则：

- `validated_coin:` → 累加 coin 到 Coins
- `validated_xp:` / `xp_support:` / `xp_admin:` → 累加 xp 到 Xp
- `ladder:6h:` → Chest
- `ladder:8h:` → GoldDay
- `ladder:2h:` / `ladder:4h:` → 其 coin 并进 Coins（不单独命名）
- `early_start:` → EarlyStart { coins: 该行 coin }
- `streak_milestone:` → Streak { n }（解析最后一个 `:` 后的整数；现网 key 为 `streak_milestone:{n}`）
- `shop_spend:` → Redeem；**不要**把负的 coin/xp 加进 Coins/Xp 合计

输出顺序：先按输入顺序发出具名事件（Chest / GoldDay / EarlyStart / Streak / Redeem），最后若 Coins>0、Xp>0 各一条。同一批多个 tick 不得变成多条 Coins/Xp toast。

### 5.3 SQLite

`migrate` **每次**都执行（不依赖再 bump `user_version`）：

- `wishes.archived INTEGER NOT NULL DEFAULT 0`（`add_column_if_missing`）
- `redemptions.name TEXT`、`redemptions.duration_minutes INTEGER`（快照，可空以兼容旧行）
- `CREATE TABLE IF NOT EXISTS entertainment_sessions (
     redemption_id TEXT PRIMARY KEY,
     wish_id TEXT NOT NULL,
     name TEXT NOT NULL,
     started_at INTEGER NOT NULL,
     ends_at INTEGER NOT NULL
   )`

新库 SCHEMA 同步包含这些列/表。进行中：存在一行 `now < ends_at`。历史可多行。

### 5.4 `src-tauri` 命令与托盘

- `create_wish` / `update_wish` / `archive_wish`
- `redeem`：事务内校验 → 可选会话锁 → 写兑换与 spend → 可选 session。`now` 注入（生产用当前 unix 秒）。错误映射为稳定码字符串：`shop_locked` / `insufficient` / `entertainment_needs_duration` / `entertainment_in_progress` / `wish_missing` / `wish_archived` / `already_applied`
- `TodayView` 增加 `activeEntertainment: { name, endsAt, remainingSecs } | null`、`endedEntertainment: { name } | null`（最近一条 session 已过期且当前无进行中时）、`ledgerTail: { key, coin, xp, ts }[]`（今日全部账本行，按 ts 升序；一天最多约几十条）
- `WeekView` 增加未停用愿望、`redemptions` 最近 20 条（name 快照、ts、kind 价格不必）、同一 `activeEntertainment` / `endedEntertainment`
- 托盘：有进行中会话时在现有进度后追加 ` · {name} {n}m`。更新间隔仍 30 秒。

### 5.5 主窗口

`App` 可见时每 5 秒 `getToday()`（商店 Tab 同时 `getWeek()` 刷新愿望）。`src/lib/feel.ts` 实现与 core 相同的 coalesce + 水位（第一次轮询不 toast）。进行中倒计时：今日页与商店页都显示。带时长 XP 在有会话时 disable。

## 6. 数据流与失败

见对话中已确认的「数据流与失败处理」一节。摘要：

- 停用愿望：倒计时用会话副本名称；不能再兑
- 改价/改名不影响已写会话和旧 `redemptions` 快照
- 结束今天 / 暂停 / Gold Day / 午夜不取消会话
- IO/FULL/CORRUPT：未兑换、无会话
- 双 UUID：第二笔按余额或进行中失败

`endedEntertainment` 仅用于横幅。Toast「到点了」只由前端比较：上一轮 `remainingSecs > 0` 且本轮 `activeEntertainment === null`。首次挂载即使已过期也不 toast。

## 7. 测试

自动化不申请 TCC、不弹系统通知。

### 7.1 core 商店

- 空名、价格 0、XP 无时长、Coin 带时长 → `validate_wish` 失败
- 合法 XP 30 分钟、Coin 无时长 → 成功
- `now < ends_at` → `can_start_entertainment` false；相等或之后 true
- 59 秒剩余 → tray 分钟为 1；120 秒 → 2；0 → None

### 7.2 coalesce

- 10 条 `validated_xp` + 1 条 `validated_coin` → `[Coins(1), Xp(10)]` 或具名后的合计（无具名时仅这两条，Coins/Xp 在最后）
- 含 `ladder:6h` 与 ticks → 含 Chest，ticks 合并
- `shop_spend` 负 XP → Redeem，Xp 合计不含负数

### 7.3 DB

- 旧库无 `archived` 列：migrate 后有列且默认 0
- 带时长兑换写入 session，`ends_at = now + duration*60`
- 进行中再兑另一 UUID 带时长 → Rejected，ledger XP 不变
- 同一 redemption_id 两次 → AlreadyApplied，session 仍一行
- Coin 兑换不写 session
- 过期后可再开一段
- 停用后 load_wishes 不含该项

### 7.4 命令 / 托盘

- `tray_tooltip_for_today` 无会话 = 现有 `Xh Ym / 8h`；有会话追加 ` · 视频 12m`
- `build_today` 在 `now < ends_at` 时 `activeEntertainment` 非空

### 7.5 前端

- `nextFeelNotices(null, rows)` 返回空 notices 且水位为 max ts
- 第二次只对更新行 coalesce
- `sessionJustEnded(prevRemaining, active)`：prev>0 且 active 空 → true；prev 为 null → false

### 7.6 手工（实现后）

- 创建「30 分钟视频」并兑换：今日与商店倒计时、托盘有剩余
- 关主窗口：无系统通知，托盘仍有倒计时
- 再兑另一个时长愿望：失败
- 到点（或把系统时间理解为过期）：可再兑；窗口开着会 toast
- Chest / Gold Day：窗口开着有 toast，关着只有格子变化

## 8. 明确不做

独立 watcher、系统通知、音效、等级、头像、随机掉落、预置愿望、把娱乐改成合法 distraction、暂停采样、改 Quest 体验（那是 D）、改判定（那是 B）。

## 9. 与 A/B/D 的关系

- **A 观测、B 判定**：C 不依赖它们收完；也不改它们。
- **D 今日主线**：Quest 输入本波不动。
