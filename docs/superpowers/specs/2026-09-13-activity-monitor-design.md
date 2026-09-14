# GameLife F：活动监测器（判定输入 + 本机外壳）

日期：2026-09-13  
状态：待用户审阅  
范围：把 GameLife 从「自建计划本」改成 **AI 驱动的活动监测器**。计划在 TickTick；本机负责采样、判定、发币、复核、统计入口、商店与设置。本文覆盖判定流水线、应用名单、一句话类别规则、TickTick 只读接入、导航与今日页外壳。统计细目见 `2026-09-13-analytics-design.md`，商店视觉见 `2026-09-13-shop-desire-design.md`。三份一起构成一次交付，实现可按第 13 节并行，但必须先落地本文的 Policy / 快照契约。

产品仍是科研主线时间校验器，不是 GTD，不是 TickTick 客户端。不拷贝 TickTick / Habitica 源码或资源。运行时 **不依赖** 本机安装 `ticktick-cli`：Rust 直连官方 Open API。

本文覆盖并取代：

- `2026-09-11-planner-shell-design.md` 中 **今日页计划本呈现**（任务输入、自建列表、1 天计划|实际自绘日历）、**「无当天任务则 credited=0」**、以及用本机 `tasks` 表作为新槽判定集合。该文的发币折扣、槽钉快照、文本 AI → 视觉回退、四页导航（今日 / 统计 / 商店 / 设置）仍有效，统计页由原「本周」升格。
- 对话中提出的 FullCalendar / dnd-kit / 任务右键 / 拖进日历：**不做**。日历交互属于 TickTick。

未提及的行为仍以 2026-09-10、Wave 1、观测引擎、落地判定、游戏手感为准。硬规则（离开、娱乐、永不截屏、落地自动 Core 的 *观测* 条件）仍先于 AI。

## 1. 问题

自建清单和日历永远追不上 TickTick，却占满今日页。真正只能在本机做的是：15 秒采样、永不截屏、按窗口判定「刚才是主线还是刷网」、发币与连胜。使用者已经在 TickTick 里排期；GameLife 应读取那些有时段的任务当上下文，而不是再做一个计划本。

同时，判定不能只靠任务。没有排期时，仍应根据应用名单、网站规则和使用者自己写的一句话定义来分类。空 TickTick 不再把整天主线 credited 打成 0。

## 2. 成功标准

| 项 | 必须 |
| --- | --- |
| 身份 | 本机无任务输入、无自建列表入口、无自绘日历。今日页是日报 + 复核，不是 GTD |
| 应用名单 | 设置可维护主线 / 支线 / 杂项 / 娱乐（应用名或 host）。现有「信任应用」在 UI 改称「主线应用」，JSON 字段 `trusted_apps` 保持兼容 |
| 一句话规则 | 四类各一个输入框，灰字样稿；空则不把样稿送进模型 |
| TickTick | OAuth 只读；按清单角色（及标题 `#主线` 等）把当天有时段的任务钉进槽快照。断网用缓存。从不把截图或样本标题写回 TickTick |
| 冲突 | **观测优先于计划**。YouTube 在娱乐规则里时，即使 TickTick 该小时是主线会议，也是娱乐、0 币 |
| 无任务 | 无 TickTick 快照仍可自动 Core（落地证据 + 主线应用），也可进灰区走 AI/视觉。不再「无任务 ⇒ credited=0」 |
| 导航 | 左侧图标栏：今日、统计、商店、设置。小字标签可在设置关闭 |
| 复核 | 今日页时间轴（只读 15 分钟块）点开：待复核可分类；已终结只能报告误判 |
| 并行 | 统计与商店可并行做 UI；不得在 Policy 字段未就绪时改 `judge_slot` 签名两次 |

明确不算失败：TickTick 未连接时只靠应用名单 + 一句话 + 落地规则；官方 API 看不到已完成任务；关窗错过 toast。

## 3. 不变量

1. 15 秒采样；缺口不外推；进程死亡是未观测，不是离开。`credited ≤ observed ≤ 实际槽长`。
2. 一天最多 96 个 15 分钟槽。周末不采样。
3. 永不截屏：不截图、不把该窗口标题/路径/URL 送进模型、该段不发币。内置项不可删。
4. 账本 `UNIQUE reward_event_key`。已 `final` 的槽不改经济。报告误判只留记录。
5. Gold Day 之后不再产生硬币或能量。折扣门槛与 planner-shell §6 相同。
6. 视觉 fail-closed：`call_vision_api` 只接受 `SanitizedVisionContext`。保护窗口禁止整段 HTTP（含 JPEG）。
7. `document_path` 不得从窗口标题伪造。截图路径永不进文本 haystack。
8. 无系统通知、无音效、不申请通知权限。
9. 测试禁止 `std::env::set_var("HOME", …)`。改 `src-tauri/` 后 `cargo test --offline -p gamelife` 必须编译并跑过。
10. API Key 与 TickTick token 只进钥匙串，不进 `config.json`。
11. 不引入子任务、重复规则、双向同步、在 TickTick 里完成任务来兑奖、用非官方同步协议。

## 4. 分层

```
TickTick（计划本）
  清单 + 日历 + 自然语言 + 拖动改期

GameLife（监测器）
  采样 / 截图 / 硬规则 / 文本 AI / 视觉
  托盘 + 今日日报 + 统计 + 商店 + 设置
```

本机 SQLite 仍是唯一账本。TickTick 只提供「使用者声称这段时间在做什么」的上下文。

## 5. 判定流水线

槽结束仍是：硬规则 →（可选）自动 Core → 灰区文本 AI → 视觉 → 待复核。AI 不得覆盖硬规则已定性的娱乐 / 离开 / 未观测 / 永不截屏。

### 5.1 `hint_sample`（每条 15 秒样本）

顺序固定，先匹配先赢：

1. 锁屏或暂停 → `Away`
2. `distraction_rules`（娱乐应用名或 host 子串，含默认 B 站 / YouTube 等）→ `Distraction`
3. 空闲 ≥ `LOW_INPUT_IDLE_SECS`（180 秒）→ `Away`
4. `admin_apps` 命中应用身份**或** haystack（标题 / URL / 文档路径）→ `Admin`（**本波新增 `Hint::Admin`**，计入 `activity.admin`）
5. `all_side_project_rules`（内置 GameLife + 使用者支线名单）→ `Side`
6. Core 候选 → `CoreCandidate`（**不再要求 app 在主线名单里**，见下）
7. 否则 `Unsure`

**第 3 条的两个位置都是刻意的。** 放在娱乐之后：挂着视频不动鼠标仍然是娱乐，不会变成幽灵缺席。放在杂项与支线之前：人走开时留在前台的窗口不再继续记账 —— 否则把微信或 GameLife 窗口留在最前面，每个槽都会算杂项，人不在也能拿满每天的杂项能量。

**这一条取代了「阅读应用空闲 → `CoreReading` / `UnsureReading`」。** 那条的触发条件是 `idle_seconds >= 180`，正好是第 3 条抢先的全部样本；实现里已删除该分支，`reading_apps` 与 `reading_bridge_seconds` 现在是死配置（恒为 0），`Hint::CoreReading` / `UnsureReading` 枚举值保留但没有生产者。

Core 候选（与落地判定同一精神，**不**只靠窗口标题）：

- 下列之一：H2 科研 host、`document_path` 非空且像工作路径、窗口标题/路径/URL 命中 **本槽快照里主线任务标题的证据词**（按空白和标点切开，长度 ≥ 2 的片段；快照为空则不做标题子串）。

**`trusted_apps` 不再参与判定。** 它曾经是这道门的准入条件 —— 结果是「按标题判定」只对名单里的 app 成立，名单外的 app 标题里写什么都不看。现在任何 app 只要过了前面的硬规则都可以成为候选，把窗口挡在外面的责任交给娱乐 / 杂项 / 支线这三张表（它们都读 app 名）。副作用：名单字段仍在 `Policy` 与设置页里往返，但已经不影响任何判定；`/day` 的「当日应用」卡片仍用它渲染 listed_as 标签，那个标签因此不再代表实际判定。

标题-only、无路径无 URL 无 H2：仍是候选但不进 `grounded_strong_core`，不能自动 Core。

### 5.2 `judge_slot`

- 整槽未观测 → `unobserved`，credited 0。
- 离开占优且强 Core 不足 → `break_away`。
- **自动 Core：** `grounded_strong_core ≥ 780` 且 `side + admin + distraction ≤ 60`。  
  **删除**「判定集合为空则 credited=0」。无 TickTick 任务时，主线应用 + 落地证据仍可自动 Core。
- 支线 / 杂项 / 娱乐占优：按秒数最大者定 `dominant`（杂项用 `activity.admin`）。
- 否则灰区：文本 AI，失败或不够自信再视觉，再失败 `pending_review`。

人工复核与视觉 Core 的 credited 公式仍是 `strong_core + reading_bridge + verified_core`，且 cap 在 observed / 实际时长。

### 5.3 文本 AI

Prompt 只含：未保护样本摘要、本槽 TickTick 快照（id / 标题 / 角色 / 起止）、四句类别规则（空句省略）、当前政策里的名单摘要（主线/支线/杂项/娱乐各至多 20 项）。不发送截图、不发送 TickTick `content`/`desc`。

JSON：

- 快照非空：`{"task_id": string|null, "confidence": number}`。校验、0.7 门槛、`apply_task_match` 与现网相同。
- 快照为空：`{"category": "core_research"|"research_support"|"side_project"|"admin"|"distraction"|null, "confidence": number}`。`null`、非法、超时、无密钥 → 当失败，走视觉或待复核。  
  `core_research` 仍要求本槽 `strong_core > 0` 才落地为已决议主线，否则待复核（防止一句话规则把随便一个窗口说成主线）。  
  `research_support`：只增加 `activity.support`，`credited_core = 0`，不进 8h/宝箱/连胜。  
  `side_project` / `admin`：按 planner-shell §6 折扣发币。`distraction`：0 币。

娱乐硬规则已命中的槽 **不调用** 文本 AI。

## 6. 应用名单与一句话规则

### 6.1 Policy

```
Policy {
  trusted_apps,              // UI：主线应用
  admin_apps,                // 新，默认 []
  side_project_rules,        // UI：支线应用
  distraction_rules,         // UI：娱乐应用 / 网站
  reading_apps,
  never_capture_apps,
  category_guides: {
    mainline: String,
    side: String,
    admin: String,
    entertainment: String,
  }
}
```

旧政策 JSON 缺新字段时按空默认反序列化，**禁止**因缺字段丢弃整份政策。新政策行在使用者保存设置时插入（与现网 `policy_versions` 相同：已开始的槽仍用开槽时的版本）。

**保存时机**：是否插入新的 `policy_versions` 行，由「判定读到的字段有没有变」决定（前端 `policySignature` 对 `distractionRules` / `sideProjectRules` / `adminApps` / `neverCaptureApps` / `categoryGuides` 取指纹，保存前后比对）。不要再用「当前在哪个页签」来判断 —— 那样在别的页签保存名单编辑会只写 config.json、判定读到旧政策，而在名单页签反复保存会不断产生内容相同的新版本（2026-09-13 一次就产生了 9 个空版本）。

`admin_apps` 先试 `matches_app_identity`（显示名或已知 bundle），再退回 haystack 子串 —— 所以 Chrome 里标题含 `GameLife` 的标签页、iTerm2 里名为 `gamelife-ui-redesign` 的会话、以及 `~/Desktop/GameLife-UI方案2.html` 这类路径都会进杂项，而身份匹配只看得到名为 `gamelife` 的那个 app。`distraction_rules` 仍是 haystack 子串，可写 `Slack` 也可写 `youtube.com`。

**代价**：haystack 条目比身份条目宽得多。`Mail` 会命中任何标题含 "mail" 的窗口（`Gmail - Chrome` 也算），短词要收紧。`distraction_rules` 与 `side_project_rules` 一直如此。

### 6.2 设置 UI（名单 + 规则）

「名单」板块只留**三张名单** + 永不截屏（内置只读 + 额外）：

| UI 名称 | 含义 | 样例占位 |
| --- | --- | --- |
| 支线应用 | 工具、个人项目、打磨本应用 | `GameLife` 内置不可当主线 |
| 杂项应用 / 网站 | 邮件、日历、报销、行政 | `Mail`、`日历` |
| 娱乐应用 / 网站 | 先于计划生效 | `bilibili.com` |

**主线应用与阅读两张名单已删除。** 主线应用曾是 Core 候选的准入条件，删掉之后任何 app 都能成为候选（见 5.1 第 6 步）；阅读的编辑器随 `CoreReading` 分支一起消失（见 5.1 第 3 条）。两者的 JSON 字段（`trusted_apps` / `reading_apps`）保留在 `Policy` 与 `AppSettings` 里继续往返，但没有编辑器、也不影响任何判定，纯属兼容。统计页「当日应用」的名单下拉与 `listed_as_for` 也只报告仍在生效的三张名单，`trusted_apps` 与 `reading_apps` 不再产生标签。

其下四条「类别说明」，各限 500 字。输入框灰字样稿（未保存进 Policy，空字符串表示不用）：

- 主线：`当天 TickTick 主线清单里的科研任务，以及在 Cursor、论文 PDF、Overleaf 上写代码、改稿、看文献。`
- 支线：`与当天主线课题无直接关系的工具、个人项目、整理仓库、打磨 GameLife。`
- 杂项：`邮件、报销、填表、组会行政、改个人主页。`
- 娱乐：`视频、社交媒体、购物、无目的刷网。B 站和 YouTube 默认算娱乐。`

保存时只写使用者实际输入。清空则下次开槽不再把该句送进模型。

## 7. TickTick 只读接入

### 7.1 授权

设置 →「TickTick」：Client ID、Client Secret（钥匙串）、连接 / 断开。系统浏览器走 OAuth PKCE；回跳 `http://127.0.0.1:<临时端口>/callback`。Access / refresh token 钥匙串服务 `ma.haofei.gamelife.ticktick`。只申请官方提供的任务读取能力；即使 scope 含写，客户端也不得 `POST` 创建、完成、改期。

未填 Client ID 时该板块说明：到 TickTick 开发者中心建应用，Redirect URI 填连接时显示的回跳地址。个人使用允许自己填自己的应用凭证。

### 7.2 清单 → 角色

连接成功后 `GET /open/v1/project`。每个清单一个下拉：忽略 / 主线 / 支线 / 长期 / 杂项。映射存在 `config.json` 的 `ticktickProjectRoles`，**不**进 Policy（角色已钉在槽快照里）。

标题尾部或任意位置的 `#主线` `#支线` `#长期` `#杂项`（及「主线任务」等现有别名）覆盖清单默认角色。无法解析的 `#标签` 忽略。

### 7.3 判定集合

拉取映射不为「忽略」的清单 `GET /open/v1/project/{id}/data`。进入当天判定集合当且仅当：

- 未完成（`status == 0`）；
- `startDate` 与 `dueDate` 都存在且 `start < day_end && due > day_start`；
- 角色不是长期，**或者**长期但已有具体时段（与 planner-shell 相同）。

无起止的任务不进快照、不出现在本机今日页（本机本来也不再展示任务）。仍最多 20 条；超出则本槽文本 AI 跳过任务匹配并记一条可忽略的日志，判定改走「快照为空」分支（应用名单 + 一句话 + 视觉），**不**拒绝采样。设置里提示「当天有时段任务超过 20，请在 TickTick 勾完或改期」。

时间对齐仍 `align_range` 到 15 分钟。时区用任务 `timeZone`，缺省用本机。

### 7.4 缓存与钉快照

表名固定为 `ticktick_cache`，列：任务 id、清单 id、标题、角色、start、end、`fetched_at`。刷新：设置里手动同步；今日页打开；槽开始时若缓存超过 5 分钟。拉取超时 2 秒则用旧缓存；无缓存则快照为空，不阻塞采样。

槽开始把当前判定集合写入 `slots.task_snapshot_json`，形状仍是 `TaskSnapshot { id, title, role }`。id 使用 TickTick 任务 id，前缀 `tt-` 以免与旧本机任务碰撞。已开始的槽不因后来改期而改快照。

### 7.5 本机 `tasks` 表

本波 **不删除** 表，今日页与新槽判定 **不再读写**。旧测试改为注入快照，不再假设 UI 能加任务。`parse_task_line` / `create_list` 命令可保留但前端不调用。

## 8. 本机外壳

### 8.1 导航

左侧窄栏四个图标（内联 SVG，不用字母「今」）：今日、统计、商店、设置。`config.json`：`showRailLabels` 默认 `true`；`false` 时只显示图标 + `title`/`aria-label`。

主区仍浅底。统计页取代原「本周」路由，见分析 spec。

### 8.2 今日页

整页一张日报，窗口高度内自身滚动。删除：自然语言加任务、列表分组、新建列表、计划|实际日历、硬币/能量/连胜拆成两行网格、Progress32 那两行「宝箱/黄金日」重复网格（改进行日报数字，不丢数据）。

保留并重排：

1. 权限条、娱乐倒计时条。
2. **一行**徽章：硬币（今日新增 · 余额）、能量（今日获得，未解锁商店则标注锁定）、连胜圆环。
3. 日报摘要：估计有效主线 `Xh Ym / 8h`、宝箱与黄金日进度、是否黄金日、早开始文案。图表与时间轴细节见分析 spec 的「日报」。
4. 只读时间轴：当天 96 槽实际判定（颜色与类别一致）。当前槽描边「正在识别」。红线表示现在。点块打开现有复核模态。可翻到其他日期看历史实际（未来日为空）。计划块若有 TickTick 快照，在轴上用细线或小点标出时段，**不可拖**。
5. 「保护连胜」默认收成一行下拉，点开才见日期选择和冻结按钮。
6. 「结束今天」。

### 8.3 设置页板块

基础（含 `showRailLabels`）/ API / 名单与类别说明 / TickTick。Chrome URL 自动化说明保留。

## 9. 错误处理

| 情况 | 行为 |
| --- | --- |
| TickTick 未连接或 token 失效 | 快照空；设置红字；采样与硬规则继续 |
| 刷新超时 / 429 | 用缓存；指数退避；不重试打爆 API |
| 一句话规则 AI 把无关窗口判主线 | 无落地证据则待复核，不自动 Core |
| 当天有时段任务 >20 | 见 §7.3 |
| 永不截屏 / 保护历史 | 与 Wave 1 相同：红acted 历史不阻断未保护截图 |
| OAuth 取消 | 保持未连接，不写坏 token |

## 10. 模块与数据流

```
设置保存 → policy_versions 新行 + config.json 映射 / showRailLabels
TickTick OAuth → 钥匙串
槽开始 → 刷新缓存（可失败）→ task_snapshot_json
槽结束 → hint → judge_slot → 文本 AI（可选）→ 视觉（可选）→ resolve_slot
今日 / 统计 → 只读 slots / ledger / 日汇总表（分析 spec）
```

纯函数仍在 `gamelife-core`：新 `Hint::Admin`、Core 候选证据词、空快照时的 category JSON 校验、guides 截断。HTTP / OAuth / SQLite 在 `src-tauri`。前端只经 `src/lib/api.ts`。

## 11. 测试要点

- 自动 Core：`side + admin + distraction ≤ 60`（杂项也挡自动主线）。
- 空快照 AI `research_support` → 辅助秒增加且 credited 0。
- 娱乐 host 先于 TickTick 主线快照：dominant 娱乐，credited 0。
- 无快照 + Cursor + 落地路径 13 分钟 + 杂事少 → 自动 Core。
- 无快照 + 仅标题像任务 → 不自动 Core。
- `Hint::Admin` 命中 Mail 类名单 → `activity.admin` 增加。
- 旧 Policy JSON 无 `admin_apps` / `category_guides` 仍能反序列化。
- 快照 id `tt-`；文本 AI 返回未知 id → 当解析失败。
- 空 guides 不出现在 prompt 字符串里。
- 保护样本不进 `sample_summary`。
- 前端无加任务表单、无新建列表、无「计划|实际」双列日历。
- 现有缺口、unobserved、账本幂等、Gold Day、永不截屏回归仍绿。

## 12. 明确不做

- FullCalendar、dnd-kit、本机任务右键菜单、拖任务改期。
- 把周报/商店写成 TickTick 清单或笔记。
- 非官方 TickTick 全量同步。
- 用完成 TickTick 任务触发兑换。
- 把样稿占位符当成使用者已配置的规则存盘。

## 13. 一次交付的并行切分

共享落地（必须先于或包含在判定改动里）：Policy 新字段与 serde 默认、`Hint::Admin`、自动 Core 去掉「空任务」门、设置名单文案。

之后三条可并行：

1. **判定：** TickTick OAuth/缓存/钉快照、文本 AI prompt、今日页去掉计划本并接时间轴复核。
2. **统计：** `2026-09-13-analytics-design.md`（依赖槽数据与 §分析中的日汇总表；不依赖 TickTick 写回）。
3. **商店：** `2026-09-13-shop-desire-design.md`（不改兑换事务）。

合并前三条都要对各自测试负责；`judge` 与 Policy 的契约以本文为准，禁止各写一套字段名。
