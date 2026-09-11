# GameLife D：今日主线（Quest 契约 + 今日页）

日期：2026-09-11  
状态：已确认（用户授权跳过文件审阅，直接进入实现）  
范围：Quest 从「三行输入、整句切词」变成最多 3 条带显式证据的日快照，其中一条是英雄主线；今日页可沿用上一个工作日，并显示当前窗口是否命中。不改 15 秒槽、缺口规则、灰区视觉分支、账本、商店、连胜。

本文覆盖并取代：

- `2026-09-10-gamelife-design.md` 中「1–3 条 Quest」的**输入与匹配实现**（产品语义仍以该文为准：无 Quest 则 credited 只能为 0；Quest 修改只影响尚未开始的槽）。
- 该文 §10「今日：Quest」的呈现：改为英雄主线 + 证据 + live 命中，而不是三行 placeholder。

未提及的行为仍以 2026-09-10 设计为准，尤其是：Judge 顺序、Trusted 之后才可能 Core、README/设置不算 Core、已开始槽不重算、8h 进度只有一条、不显示截图时刻。

判定调优（灰区、verified_core、阅读桥接阈值）是后续 **B**，不在本规格。游戏手感是 **C**。

## 1. 问题

今日页是三个空输入框。保存时把整句按空格切开当 keyword，`Finish HDP tactile ablation` 会让 `Finish` 误伤。命中只看标题和 `document_path`，不用 URL，Overleaf / arXiv 对不上。新的一天 Quest 为空，不沿用昨天，credited 只能是 0，直到再保存一次。

D 的目标不是做成 Super Productivity。产品仍是科研主线时间校验器。只把「今天要盯什么」写成可匹配的契约，并让今日页看得出当前窗口是不是这条主线。

## 2. 成功标准

| 项 | D 必须 |
| --- | --- |
| 条数 | 每天 0–3 条；全部都能匹配 |
| 英雄 | 恰好一条 `hero=true`（0 条时无 hero）；页面以它为英雄卡 |
| 文案 vs 证据 | `text` 只给人看；匹配只用 `evidence` token，禁止再从文案切词 |
| 命中字段 | 标题、`document_path`、URL（已去 query/fragment）大小写不敏感子串 |
| 无证据 | 没有任何一条带非空 evidence → 与无 Quest 相同，credited 强制 0 |
| 沿用 | 新工作日先空着；可一键沿用上一个有 Quest 的日期（含 hero 与 evidence） |
| Live | 今日页显示当天最近一条 sample 是否命中某条 Quest；未 Trusted 须标明不会记入主线 |
| 8h | 仍是一条估计有效主线，不按 Quest 拆 credited |

明确不算失败：Cursor 的 `document_path` 为空但标题命中；用户拒绝浏览器自动化导致 URL 为空（可用标题 token）。

## 3. 不变量

1. 只有一个 GameLife 进程。不引入任务库、项目、子任务、重复任务、专注模式、自报计时。
2. 采样间隔仍为 15 秒；每槽仍一张随机截图。Live 命中读库里最近样本，不另打 `snapshot()` / ScreenCaptureKit。
3. `gamelife-core` 的 `CaptureContext` 不加 Quest 字段。
4. 1–3 条 Quest 的 evidence 在 Judge 里地位相同。hero 只影响今日页强调和视觉 prompt 的 `[main]` 前缀。
5. `document_path` 仍不得从窗口标题伪造。evidence 可以是用户手写的路径片段（如 `/HDP/`），去已有 `document_path` 字段里找。
6. Hint 顺序不变。Distraction / Side / Away 优先于 Core。未 Trusted 的应用即使标题含 evidence 也是 `Unsure`（或 Side/Distraction），不是 `CoreCandidate`。
7. README / Settings / `设置` 窗口标题仍不是 `CoreCandidate`。
8. 改 Quest 只影响尚未开始的槽。已开始与历史不重算。
9. 周末或今天已结束：拒绝 `set_quests` 与沿用。
10. 不把 credited 按 Quest 归因。不做 B/C。

## 4. 架构

```
今日页
  → set_quests / continue_previous_workday
      → 追加 quest_versions（只影响尚未开始的槽）
  → get_today
      → 当日最新 Quest 快照
      → 当日最近一条 sample
      → 纯函数：哪条 evidence 命中 + 是否 Trusted

采样 / Judge（现有循环）
  → 开槽时钉死 quest_version_id
  → hint_sample：Trusted 之后用 evidence 扫标题 / path / url
```

## 5. 模块

### 5.1 `Quest`

```
Quest {
  text: String,
  evidence: Vec<String>,
  hero: bool,
}
```

删除 `keywords`。测试夹具改用 `evidence` + `hero`。

### 5.2 `crates/gamelife-core/src/quest.rs`

常量：`MAX_QUESTS = 3`，`MAX_EVIDENCE = 8`，`MIN_EVIDENCE_CHARS = 2`。

```
pub struct QuestDraft {
  pub text: String,
  pub evidence: Vec<String>,
  pub hero: bool,
}

pub enum QuestListError { TooMany, EmptyText }

pub fn normalize_evidence(tokens: &[String]) -> Vec<String>
pub fn normalize_quest_list(drafts: Vec<QuestDraft>) -> Result<Vec<Quest>, QuestListError>
pub fn quest_list_has_evidence(quests: &[Quest]) -> bool
pub fn matched_quest_index(title, document_path, url, quests) -> Option<usize>
pub fn parse_quest_versions_json(json: &str) -> Result<Vec<Quest>, String>
pub fn vision_quest_label(quest: &Quest) -> String
```

`normalize_evidence`：trim；空串丢弃；长度 < 2 丢弃；小写相同去重（保留第一次出现的原样）；最多 8 个。

`normalize_quest_list`：0 条 → `Ok([])`；>3 → `TooMany`；任一条 trim 后文案空 → `EmptyText`。evidence 走 `normalize_evidence`。恰好一条 hero：若有 `hero=true`，取列表中**第一个**为真的，其余强制 false；若都没有且列表非空，下标 0 为 true。

`quest_list_has_evidence`：任一条 `evidence` 非空。

`matched_quest_index`：对每条 Quest 的每个 token，在 `window_title`、`document_path`、`url` 上做 ASCII 小写子串。返回第一条命中的下标。不匹配 `app` 名。不看 Trusted / README（今日页用）。token 已规范化，匹配时再 `to_ascii_lowercase`。

`parse_quest_versions_json`：JSON 数组。每项 `text` 必填。`evidence` 优先；若缺则用 `keywords`。缺 `hero`：解析后按 `normalize_quest_list` 规则补（只有第一条 true）。解析失败返回 `Err`。

`vision_quest_label`：hero 则 `"[main] {text}"`，否则 `{text}`。

### 5.3 `hint.rs`

`is_core_candidate`：README/设置仍先否决；否则 `matched_quest_index(...).is_some()`。haystack 含 URL。

### 5.4 `judge.rs`

`quests_empty` 改为 `!quest_list_has_evidence(input.quests)`。有标题无 token → credited 强制 0，不能自动 `core_research`。灰区视觉仍可走现有 pending 路径，但 credited 仍受这道门约束。

### 5.5 视觉

`VisionContext.quests` 仍是 `Vec<String>`。槽决议在填入前用 `vision_quest_label`。prompt 仍打印 `1. {quest}`。

### 5.6 持久化（`src-tauri`）

写入：`[{ "text", "evidence", "hero" }]`。读取走 `parse_quest_versions_json`。

`set_quests(Vec<QuestDraft>)`：周末/已结束拒绝；`normalize_quest_list` 失败拒绝；成功插入新版本。允许空列表。

### 5.7 `continue_previous_workday`

查 `quest_versions` 中 `day < 今天` 的行，按 `day DESC, id DESC` 扫描，跳过无法解析或解析后长度为 0 的 JSON，取第一份长度 ≥ 1 的。将该列表再 `normalize_quest_list` 后作为今天的新版本插入。今天已有版本也可以再沿用。周末/已结束拒绝。找不到 → 错误 `no previous quests`。

### 5.8 `get_today`

```
quests: [{ text, evidence, hero }]
previousWorkday: Option<string>
live: Option<{
  app, title, documentPath, url,
  matchedQuestIndex: Option<number>,
  trusted: bool
}>
```

`live` 来自当天 `samples` 按 `ts DESC` 最新一行。没有样本 → `null`。`matchedQuestIndex` 用 `matched_quest_index`。`trusted` 用当前 policy 的 `matches_app_identity(app, bundle_id, trusted_apps)`。

### 5.9 今日页

英雄卡：文案、evidence chips、命中文案。最多两条次要 Quest（可编辑）。「沿用 {previousWorkday}」。保存。8h 条、冻结、结束今天不动。不显示截图时刻。

Live 文案（纯函数 `liveMatchLabel`）：

- `live == null` → `尚无观测`
- 未命中 → `当前窗口未命中 Quest`
- 命中且 trusted → `命中「{text}」`
- 命中且非 trusted → `命中「{text}」，当前应用不在 Trusted，不会记入主线`

## 6. 数据流与失败处理

**保存：** 周末/已结束 → 拒绝。规范化失败 → 拒绝，不写版本。单字符 token 丢掉后该条 evidence 可为空；只要文案非空即可保存。全表无 evidence → 等同无 Quest。

**沿用：** 找不到可用快照 → `no previous quests`。拷贝失败不删今天已有版本。

**判定：** 开槽钉版本。URL 缺失不把整拍打成 unobserved。未 Trusted 即使标题含 token 也不是 Core。

**Live：** 无样本不是「未命中」。锁屏/暂停样本仍按该行算匹配与 Trusted。

**旧 JSON：** 只有 `keywords` 的行当作 evidence。缺 `hero` 则第一条为 hero。整段无法解析 → 该日 Quest 视为空，`eprintln`，不崩溃。

## 7. 测试

自动化不碰真窗口。`cargo test --offline -p gamelife-core` 与 `cargo test --offline -p gamelife` 必须编译并跑过。禁止新测试 `set_var("HOME", …)`。

覆盖：规范化与 hero；标题/`Finish` 不误伤；URL 命中；未 Trusted 非 Core；空 evidence 强制 credited 0；沿用；`get_today.live`；`liveMatchLabel`。

手工：Preview 路径含课题名；Chrome Overleaf URL；Cursor 仅标题；新工作日未沿用 credited 0；沿用后主线回来；非 Trusted 应用显示「不会记入主线」。

## 8. 明确不做

独立任务表、AX/OCR 写 evidence、按 Quest 拆 credited、专注模式/时间盒、自动拷贝昨天、从标题伪造 `document_path`、改采样/截图/账本/商店。

## 9. 与后续子项目

- **B 判定：** 等 D 的证据在真机上可用后再调灰区。
- **C 游戏手感：** 不依赖本规格，但数字仍应建立在可信主线上。
- **A1 观测：** 本规格不依赖 ScreenCaptureKit；URL 命中受益于浏览器 URL，没有 URL 时仍可靠标题/路径。
