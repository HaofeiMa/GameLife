# GameLife B：落地证据与灰区判定

日期：2026-09-11  
状态：已确认  
范围：只改判定（hint、自动 Core 门、默认 distraction、policy seed v2）。不改观测引擎、15 秒槽、截图调度、账本、Quest / 商店 / 游戏手感、视觉 prompt、`verified_core` 窗口族、阅读应用列表。

本文覆盖并取代：

- `2026-09-10-gamelife-design.md` §7.3 中「idle≥10 分钟且非阅读类 → `away`」。
- 同文 §7.4 中「元数据强 Core：`strong_core ≥ 13 分钟`」——自动 Core 改为看 **`grounded_strong_core ≥ 13 分钟`**。灰区在视觉/人工确认后的 credited 公式仍用原来的 `strong_core`。

未提及的行为仍以 2026-09-10 设计、Wave 1、A1 为准，尤其是：缺口不外推、`unobserved` ≠ Away、Quest 为空则 credited=0、Never Capture fail-closed、不得从标题伪造 `document_path`、判定不得依赖「一定有 URL」。

实现排在 A1 观测引擎之后，做在同一工作树（`feat/a1-observation-engine`），因为判定吃的是已拆开的 `document_path` / `url`。

## 1. 问题

观测层换成原生 API 之后，判定仍按 2026-09-10 的 hint 在跑，两类误判都在：

- **漏判：** 非阅读应用空闲 10 分钟记成 Away。在 Cursor 里想实验、在浏览器里读论文，都会把整段判走。
- **误判 / 空转：** Quest 关键词只对标题和 `document_path`，不对 URL。Overleaf / arXiv 进不了主线；反过来，标题碰巧含 `HDP` 的闲逛标签，却可能攒满 13 分钟自动 Core。默认 `distraction_rules` 为空，B 站只能进灰区，不能自动 Distraction。

B 的取向（已确认）：漏判和误判都要压，但**宁可多进灰区等人或视觉**，也不要靠空闲自动 Away，也不要靠标题-only 自动 Core。13 分钟落地活跃仍可自动 Core。明显娱乐站占优仍可自动 Distraction。

## 2. 成功标准

| 场景 | B 必须 |
| --- | --- |
| Cursor 停下来想实验（idle 长、Trusted、标题或路径像主线） | 不是 Away；落地不足则灰区 |
| Preview 阅读桥接 | 与现网相同：5 分钟 `core_reading`，再长 `unsure_reading` |
| 标题含 Quest 关键词、无路径无 URL，活跃 13 分钟 | `core_candidate`，**不**自动 Core，进灰区 |
| 路径含关键词（如 `.../HDP/train.py`），活跃 13 分钟、杂事少 | 自动 Core |
| Overleaf / arXiv / Scholar 等 H2 站点，活跃 13 分钟 | 即使 URL 无关键词，也可自动 Core（须 Trusted） |
| 同一科研站点开着发呆 13 分钟 | 不自动 Core（idle≥3 分钟不进 `grounded_strong_core`） |
| YouTube / B 站等 D2 占优 | 自动 Distraction，credited=0 |
| 无 Quest | credited=0，即使 H2 落地 |
| 锁屏 / 暂停 | 仍是 Away |

明确不算失败：Chrome/Safari 仍不是阅读应用；Arc 仍不在默认 Trusted，故 Arc 上的 H2 站点本波仍是 Unsure。

## 3. 不变量

1. 只有 `gamelife-core` 的判定 + `scheduler` 的 `metadata_decidable` / policy seed。不改 A1 采集。
2. 采样间隔仍为 15 秒；每槽仍一张随机截图。
3. `Hint` 枚举不增加变体。落地与否是样本布尔 `is_grounded_core_sample`。
4. `document_path` 仍不得从标题伪造。
5. 判定不得要求「一定有 URL」。URL 空或 host 解析失败：不当 H2，关键词只看标题和路径。
6. 无辅助功能 / 整槽无样本：仍 `unobserved`。
7. Quest 为空 → credited=0。H2 命中可以把样本标成 `core_candidate`，但不能发币。
8. 自动 Core：`grounded_strong_core ≥ 780` 且 `side+distraction ≤ 60`。credited = `grounded_strong_core + reading_bridge`（仍 cap 在 observed / 实际时长 / 900）。
9. 灰区 + 视觉 Core（窗口族一致）或人工 Core：credited 仍为 `strong_core + reading_bridge + verified_core`。标题-only 在人/视觉背书后可以计入。`verified_core` 仍只升级无负向证据的 `unsure` / `unsure_reading`，窗口族规则与 Wave 1 相同。
10. Distraction 规则先于 Side、先于 Core。YouTube 标题含 HDP 仍是 Distraction。
11. 科研 host 是代码常量，不写入 Policy JSON，设置页本波不暴露。
12. `policy_seed_version=2` 之后，使用者清空的 distraction **不再**被补回。

## 4. 架构

采样线程与 `judge_slot` 入口不变。内部改为：

```
hint_sample
  锁屏/暂停 → Away
  D2 / 使用者 distraction → Distraction
  GameLife 等 Side → Side
  阅读应用空闲 → CoreReading / UnsureReading
  Trusted + (关键词命中标题|路径|URL 或 H2 host) → CoreCandidate
  否则 Unsure
  ※ 删除：idle≥10 分钟且非阅读 → Away

analyze_slot_evidence
  strong_core：CoreCandidate 且 idle<180（含标题-only）
  grounded_strong_core：同上且 is_grounded_core_sample
  reading_bridge：CoreReading，单槽 ≤300

judge_slot 自动 Core
  grounded_strong_core ≥ 780 且 side+distraction ≤ 60
  → Core，credited = grounded_strong_core + reading_bridge
```

`metadata_decidable` 必须与自动 Core 条件一致（看 `grounded_strong_core`），否则标题-only 满 13 分钟会跳过视觉。

## 5. 模块

### 5.1 `url`：host

在 `strip_url_query_fragment` 之后：

```
pub fn url_host(url: &str) -> Option<String>
pub fn host_is_research(host: &str) -> bool
```

`url_host`：小写、去端口。无法解析（无 host、不是 http/https）→ `None`。

`host_is_research` 用 DNS 后缀，**禁止**对整段 URL 做 `nature` 子串匹配。

H2 基名：

| 基名 | 匹配 |
| --- | --- |
| `overleaf.com` | host 等于或后缀 `.overleaf.com` |
| `arxiv.org` | 同上 |
| `ieee.org` | 等于或后缀 `.ieee.org`（含 `ieeexplore.ieee.org`） |
| `acm.org` | 等于或后缀 `.acm.org`（含 `dl.acm.org`） |
| `nature.com` | 等于或后缀 `.nature.com` |
| `sciencedirect.com` | 同上规则 |
| `webofscience.com` | 同上规则 |
| `pubmed.ncbi.nlm.nih.gov` | 等于或后缀 `.pubmed.ncbi.nlm.nih.gov` |
| Scholar | 完整 host 匹配 `scholar.google.com`，或 `scholar.google.` + 常见 TLD（`[a-z]{2,3}` 可选再加 `.[a-z]{2}`），例如 `scholar.google.com`、`scholar.google.co.uk`、`scholar.google.com.hk`。`scholar.google.com.evil.example` 不匹配 |

`https://github.com/nature/foo` 不是科研。

### 5.2 `policy`：D2

```
pub fn default_distraction_rules() -> Vec<String>
```

固定为：`bilibili.com`、`youtube.com`、`twitter.com`、`x.com`、`douyin.com`、`tiktok.com`。走现有 `matches_any_rule` 子串（URL 里出现这些 host 即可）。

`default_v01()` 的 `distraction_rules` 用该函数。科研 host **不**进 Policy。

### 5.3 `hint`

```
pub fn hint_sample(...) -> Hint   // 删除 AWAY_IDLE 分支
pub fn is_grounded_core_sample(sample: &Sample, quests: &[Quest]) -> bool
```

`is_core_candidate`：

1. 标题为 README/设置 → **忽略标题关键词**，仍检查路径、URL 关键词、H2 host。
2. 任一 Quest 关键词（大小写不敏感）出现在标题、`document_path`、或去 query 后的 URL → 候选。
3. 或 `url_host(url)` 为 H2 → 候选。

成为 `CoreCandidate` 仍须 Trusted（bundle 或显示名，现有 `matches_app_identity`）。非 Trusted 即使 H2 也是 `Unsure`。

`is_grounded_core_sample`：

- `document_path` 含任一 Quest 关键词，或
- URL（去 query）含任一 Quest 关键词，或
- host 为 H2。

标题命中不算落地。无 Quest 时关键词两条为假，H2 仍可落地。

阅读桥接、Side、Distraction 顺序与现网相同。删除：

```
idle ≥ 600 && !reading → Away
```

### 5.4 `judge`

`SlotEvidence` 增加 `grounded_strong_core_seconds: i64`。

自动 Core 分支把 `strong_core >= 780` 换成 `grounded_strong_core >= 780`。该分支 credited 用 `grounded_strong_core + reading_bridge`，不用标题-only 的 strong 秒。

其余分支：

- 整槽 unobserved、Away 占优（`away ≥ 600 && strong_core < 300`）、Side/Distraction 占优：阈值不变。Away 占优仍看 **`strong_core`**（不是 grounded），因为锁屏产生的 Away 与标题-only 活跃不应互相改口径以外的既有 Away 规则。
- 灰区视觉/人工：不改。

`credited_core_spans`：本波**不改**（仍含标题-only `CoreCandidate` 且 idle<180）。Early Start 只在槽已经 `final` 且 credited>0 时按该 credited 回溯；自动 Core 的 credited 已不含标题-only，回溯上限是 credited，不会把未入账的标题-only 秒发成早开始。若实现时发现会多算，再把 auto-core 槽的 spans 限制为落地样本——那是实现细节，不改变产品数字。

### 5.5 `scheduler` seed 与可决

`metadata_decidable(..., grounded_strong_core, ...)`：自动 Core 那一臂用 `grounded_strong_core`。签名若因此增加参数，所有调用点一起改。

`seed_default_policy_if_needed`：

| 当前 `policy_seed_version` | 行为 |
| --- | --- |
| 无 / 0 | 插入 `default_v01()`（已含 D2），写 version **2** |
| 1 | 若最新政策 `distraction_rules` **为空**：再插入一行政策（其余字段原样，distraction=D2），然后 version=2。若 distraction **非空**：只写 version=2，不改政策 |
| ≥2 | 返回，不改政策 |

v1 时空的 distraction 视为「当时默认就是空」，升级时补 D2。无法区分「有意清空」与「从未有过 D2」：接受升级一次。version=2 之后再清空则不再补。

插入新政策行，禁止 UPDATE 旧 `policy_versions` 行（已开始的槽仍绑定旧 id）。

JSON 损坏：现有 Fatal，不标成已处理。

## 6. 数据流与失败对照

见 §5.3–5.5。摘要：

| 失败 | 采样 | 判定 |
| --- | --- | --- |
| 无辅助功能 | unobserved | 本槽 unobserved |
| URL 空 / host 解析失败 | 样本照写 | 不当 H2；关键词看标题和路径 |
| 标题-only 13 分钟活跃 | 样本有 app/标题 | 灰区 / pending，credited=0 |
| H2 活跃 13 分钟、有 Quest、Trusted | 有 URL | 自动 Core |
| H2 发呆 13 分钟 | idle 大 | 不自动 Core |
| D2 占优 | 有 URL | 自动 Distraction |
| 无 Quest | — | credited=0 |
| 非 Trusted 打开 arXiv | 可有 URL | Unsure → 灰区 |
| v2 后 distraction=`[]` | — | 保持空，不补 D2 |

## 7. 测试

自动化不碰真屏幕、不申请 TCC。禁止新测试使用 `std::env::set_var("HOME", …)`。

改 `crates/gamelife-core`：`cargo test --offline -p gamelife-core` 必须绿。  
改 `src-tauri/`：`cargo test --offline -p gamelife` 必须编译并跑过。

### 7.1 `url`

- `https://arxiv.org/abs/1?x=2` → host `arxiv.org`，科研。
- `https://www.overleaf.com/project/abc`、`https://ieeexplore.ieee.org/document/1`、`https://scholar.google.com/scholar?q=a`、`https://scholar.google.co.uk/scholar` → 科研。
- `https://github.com/nature/foo`、`https://www.youtube.com/watch?v=1`、空串、非 URL → 非科研。
- `scholar.google.com.evil.example` → 非科研。

### 7.2 `hint`

- Cursor 标题 `train.py — HDP`、路径空、idle=700 → `CoreCandidate`，非 Away；未落地。
- 路径 `/Users/me/HDP/train.py`、idle=10 → `CoreCandidate` 且落地。
- Safari Overleaf URL、无关键词、Trusted → `CoreCandidate` 且落地。
- 同上、Chrome 不在 Trusted → `Unsure`。
- YouTube URL、标题含 HDP → `Distraction`。
- 标题 README + Overleaf URL → `CoreCandidate`。
- Preview 阅读桥接：现有测试保持语义。

### 7.3 `judge`

- 13 分钟落地活跃、杂事少、无视觉：自动 Core，不 pending，credited≥780（cap 前）。
- 13 分钟仅标题命中：pending，credited=0。
- 2 分钟落地 + 10 分钟同窗发呆：不自动 Core。
- YouTube 占优：Distraction，credited=0。
- 无 Quest + 13 分钟 Overleaf：credited=0。
- 现有 Isaac Sim `verified_core`、窗口族冲突、灰区无图 pending：保持绿。

### 7.4 seed 与 `metadata_decidable`

- 空库：政策含 D2，`policy_seed_version=2`。
- v1 且 distraction 空 → 新政策行含 D2，version=2。
- v1 且 distraction 非空 → JSON 不变，version=2。
- v2 且 distraction=`[]` → 再调用 seed 不插入。
- `metadata_decidable`：标题-only 13 分钟为假；落地 13 分钟为真。

## 8. 明确不做

- 独立判定进程；嵌入 ActivityWatch / Habitica 经济。
- Chrome / Safari / Arc 当阅读应用；把 Arc 补进默认 Trusted。
- GitHub / Hugging Face / Notion / ChatGPT 进科研白名单。
- 改视觉 prompt、置信度 0.7、`verified_core` 窗口族。
- 设置页编辑科研 host 或 distraction 编辑器。
- 加快采样、每槽多张图、改账本 / Quest / 商店。
- 把浏览器自动化缺失做成阻断观测。

## 9. 与后续子项目的关系

- **A1 观测：** 必须已在同一分支。B 不回退 osascript。
- **D 今日主线、C 游戏手感：** 不依赖本规格，但数字应建立在本波判定上再加厚。
- 科研 host / distraction 的设置页编辑：独立规格。
