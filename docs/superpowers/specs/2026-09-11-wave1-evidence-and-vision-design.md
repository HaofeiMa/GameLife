# GameLife Wave 1：证据路径、文档采集与 Vision 上下文

日期：2026-09-11  
状态：已确认（含 2026-09-11 计划评审增补，见 §11）  
范围：仅 Wave 1（路径拆分、`document_path` 采集、截图瞬间绑定的 capture context、Vision 上下文与 Never Capture 脱敏、默认 Policy、Vision JSON 校验、截图生命周期）。不包含愿望商店、设置编辑器、登录启动、通知、Pause Resume、macos ScreenCaptureKit。

本文覆盖并取代 `2026-09-10-gamelife-design.md` 中与下列事项冲突的部分：`samples.path` 一列两用、Vision 只传 app+截图、用附近周期 Sample 充当截图上下文、默认 Policy 为空列表、Vision 接受任意 `category`（含 `unknown`）、Never Capture 只挡截图不挡云端 metadata。未提及的行为仍以 2026-09-10 设计为准。

## 1. 问题

当前 `samples.path` 既被设计成「当前文档路径」，又被截图流程写成 `~/Library/Application Support/GameLife/screenshots/...`。`hint.rs` 把该字段放进 Side Project haystack。内置规则含 `GameLife`，于是一张 HDP 截图可能被判成 Side Project。

同时：采样器从不采集真实文档路径；Vision 不知道今日 Main Quest；若用「距离 `capture_scheduled_at` 最近的 Sample」当截图上下文，15 秒采样中间切到微信时会出现「图是微信、文字说 Cursor」；新安装 Trusted/Reading 为空；非法 `category` 仍可能变成 `final`；Never Capture 不截图，但仍可能把 `1Password · My Bank` 送进 Vision prompt。

## 2. 不变量

1. `samples.document_path` = 被观察对象的文档 / 文件 / workspace 路径 = **Judge 文本证据**。
2. `slots.screenshot_path` = 该槽随机截图的本地临时文件 = **Capture / Vision / 清理专用**。
3. `screenshot_path` **永远不得**进入 Judge 的文本 haystack。`hint.rs` 只看：`app`、`title`、`url`、`document_path`。
4. 不得从窗口标题伪造 `document_path`。拿不到 Accessibility 文档就写 `None`；Quest 仍可用 `title` 匹配。
5. **Vision 的 Screenshot Context 必须来自截图实际发生时同步采集的 context**，禁止用附近周期 Sample 推测。
6. Never Capture / secure-input 不仅禁止截图，也禁止其敏感 metadata 进入云端 Vision payload。本地 SQLite 仍可记下 `app = 1Password` 做时长统计。
7. `call_vision_api` **只能**接受 `sanitize_vision_context()` 的输出。
8. Vision 只输出六类之一；非法 JSON = 无视觉 → `pending_review`。非法输出不得产生 `final`。
9. Vision 分类语义，不创造观测时间。`credited` 仍由 Judge 按既有公式计算。Prompt 不得把 unobserved 当成已观测工作。
10. 旧列 `samples.path` **从第一天起就不得再当 evidence 读取**。禁止 `SELECT path` 填进 `document_path`。
11. `capture_context()` 只在真正即将截图时调用；与 `capture_frontmost_window` 紧挨着。禁止每个 15 秒采样都读 CaptureContext。
12. 受保护的 **截图本身**（Never Capture / secure-input）→ 禁止整个 Vision HTTP（含 jpg）。受保护的 **历史窗口** 只脱敏摘要，不阻止上传一张未受保护的截图。
13. `verified_core` 按窗口族保守匹配（app + document_path 或 title），禁止「同一 App 全部升级」。
14. `final` / `unknown` 清理时，`screenshot_path` 与 `capture_context_json` 一起丢掉。`settle_day` 把 pending 变成 unknown 后必须走同一套清理。

## 3. Schema

### 3.1 列与 `user_version`

新库 `CREATE TABLE` 直接包含下列列。已有库用 `PRAGMA user_version` 升级，**不要**「ALTER 失败就一律忽略」。

目标 `user_version = 1`（当前未设 version 的库视为 0）。

`samples` 增加：

- `document_path TEXT`
- `bundle_id TEXT`
- `secure_input INTEGER NOT NULL DEFAULT 0`（该次采样时是否处于 secure input；供 Vision 脱敏。本地 Judge 仍用 app/title 统计）

`slots` 增加：

- `screenshot_path TEXT`
- `captured_at INTEGER`
- `capture_context_json TEXT`

`samples` 可保留旧列 `path`，但 **代码不再 SELECT / INSERT / UPDATE `samples.path`**。

新增：

```sql
CREATE TABLE IF NOT EXISTS app_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

用于一次性 seed 标记，见 §7.2。

### 3.2 迁移步骤

```
migrate(conn):
  execute CREATE TABLE IF NOT EXISTS …（含新列的完整 SCHEMA）
  v = PRAGMA user_version
  if v < 1:
    for each (table, column, decl) in wave1_columns:
      if column not in PRAGMA table_info(table):
        ALTER TABLE … ADD COLUMN …   -- 失败必须向上传递
    CREATE TABLE IF NOT EXISTS app_meta …
    PRAGMA user_version = 1
```

`add_column_if_missing` 只在 `table_info` 确认缺失时 ALTER。禁止 `if let Err(_) = alter { swallow }`。若并发下仍撞上 duplicate-column，**仅当** rusqlite 错误信息表明 duplicate column 时忽略；`BUSY` 重试；`IOERR` / `FULL` / `CORRUPT` / syntax **全部向上传递**。

### 3.3 Rust 类型

```rust
pub struct Sample {
    pub ts: i64,
    pub app: String,
    pub window_title: String,
    pub url: Option<String>,
    pub document_path: Option<String>,
    pub bundle_id: Option<String>,
    pub idle_seconds: i64,
    pub screen_locked: bool,
    pub paused: bool,
    pub secure_input: bool,
}

pub struct CaptureContext {
    pub app: String,
    pub bundle_id: Option<String>,
    pub title: String,
    pub document_path: Option<String>,
    pub url: Option<String>,
    pub secure_input: bool,
}
```

删除 `Sample.path`。所有 `hint` / `judge` / `observe` 测试改用 `document_path`。

`capture_context_json` 示例：

```json
{
  "app": "Cursor",
  "bundle_id": "com.todesktop.230313mzl4w4u92",
  "title": "train.py — HDP",
  "document_path": "/Users/me/Projects/HDP/train.py",
  "url": null,
  "secure_input": false
}
```

拆成 `slots.capture_app` 等多列也可以，本波用 **一个 JSON 列**，避免再扩一串 ALTER。字段名固定为上面六个。

### 3.4 截图写入（P0）

删除 `attach_screenshot_path_to_latest_sample()`。

`tick_capture` 接收 `&dyn SampleSource`（或等价的 `capture_context` 回调），**不要**在 `sample_once` 里预先 `read_capture_context`。

到点且仍是 Scheduled、且有屏幕录制权限时，**同一函数内紧挨着**：

1. `ctx = source.capture_context()`（一次调用；macOS 尽量一次 AppleScript 取 app/title/bundle_id/AXDocument）。
2. 若 Never Capture / `ctx.secure_input` / 锁屏 / 暂停 → `Skipped`，不写 path、不写 context、**不截图**。
3. 否则立刻 `capture_frontmost_window`，成功则：

```sql
UPDATE slots SET
  screenshot_path = ?1,
  captured_at = ?2,
  capture_context_json = ?3,
  capture_status = 'Captured'
WHERE day = ?4 AND slot_start = ?5
```

找不到槽行则先 `ensure_slot` 再写。**禁止**为了存截图而 INSERT 一条空 app 的 sample。

槽结束做 Vision 时：Screenshot Context **只**来自该槽 `capture_context_json`。没有这份 JSON、或 JSON 损坏 → `vision = None`，灰区 pending。损坏 JSON **不得**让 scheduler Fatal。

**禁止** nearest-sample。**禁止**把旧 `samples.path` 填进 `document_path`。

### 3.5 不做猜测性 path backfill

**不把旧 `samples.path` 拷到任何新列。** 开发阶段宁可丢掉旧 debug 截图引用。

清理仍按 `screenshots/` 目录 mtime + retention 扫描，不依赖 DB 引用。

本波 **不写** samples.path → screenshot_path 的 backfill。

### 3.6 截图与 CaptureContext 生命周期

`screenshot_path` 与 `capture_context_json` 一起进、一起出。

- `pending_review`：保留文件、`screenshot_path`、`capture_context_json`。
- 槽变为 `final` 或 `unknown` 之后（含 `finalize_slot_end`、`review_pending_slot`、**`settle_day` 把 pending 改成 unknown 之后**），再按 retention 处理。
- 默认 retention `none`：决议后立刻删文件；删除成功则 `screenshot_path = NULL` **且** `capture_context_json = NULL`。`capture_status` 保持 `Captured`。
- retention `24h` / `3d` / `14d`：文件与 JSON 一起留；TTL 删文件成功后两者都 NULL。
- 删文件失败：保留路径与 JSON，下次再试。

`settle_day` 必须在 `convert_pending_to_unknown` 之后对这些槽调用同一套 cleanup，不能只靠 `finalize_slot_end`。

## 4. `document_path()` 采集

### 4.1 接口

```rust
pub trait SampleSource {
    // 现有方法保留
    fn document_path(&self) -> Option<String>;
    fn bundle_id(&self) -> Option<String>;
    fn capture_context(&self) -> CaptureContext;
}
```

周期采样：`insert_sample` 写入 `document_path`、`bundle_id`、`secure_input`。`FakeSampleSource` 增加对应字段，默认 `None` / `false`。

`capture_context()` **只**给截图路径用。周期 `sample_once` 不得调用它。Mac 实现尽量一次 AppleScript 取 app、title、bundle_id、AXDocument（URL / secure_input 可同一次或紧随其后）。不要 app、bundle、document 各起一次 osascript。

`document_path()` 仍只返回真路径，禁止从标题伪造。

### 4.2 macOS 取值

`document_path()` **只**返回真正拿到的文档：

1. Accessibility：前台窗口 `AXDocument`，或值为 `file:` URL 的 `AXURL`。
2. 否则 `None`。

**禁止**从 `train.py — HDP` 这类标题解析出路径或 repo 名写入 `document_path`。`window_title` 已经单独采样。

`bundle_id()`：前台进程的 bundle identifier（System Events `bundle identifier of p`，或等价 NSWorkspace）。失败则 `None`，采样不中断。

### 4.3 Normalize

```rust
pub fn normalize_document_path(raw: &str) -> Option<String>
```

顺序必须是：

1. Trim。空 → `None`。
2. 若以 `~` 或 `~/` 开头：把 `~` 扩成 `$HOME`（不是伪造路径）。无 HOME → `None`。
3. 若以 `file:` 开头（大小写不敏感）：按 URL 解析；host 为空或 `localhost`；percent-decode path。解码失败 → `None`。
4. 结果必须是以 `/` 开头的绝对路径。相对路径、裸文件名、`train.py — HDP` → `None`。
5. 去掉末尾 `/`，根路径 `/` 除外。
6. **不要** `realpath` / 跟随 symlink。

因此 `~/Projects/HDP` → `$HOME/Projects/HDP`，不会在「必须以 `/` 开头」那一步被提前丢掉。

`file:///Users/haofei/Projects/HDP/train.py` → `/Users/haofei/Projects/HDP/train.py`。

采样与截图写入 DB 前都要 normalize。`hint_sample` 使用已 normalize 的 `document_path`。

## 5. Hint haystack

```text
haystacks = [app, window_title, url?, document_path?]
```

`bundle_id`、`screenshot_path`、`capture_context_json` 都不进入 Side/Distraction 子串规则。Quest 匹配仍是：`window_title` 或 `document_path` 含子串 keyword；标题含 `README` / `Settings` / `设置` 则不是 CoreCandidate。

Trusted / Reading 匹配见第 7 节。Never Capture 匹配同样：bundle ID 优先，显示名 fallback。

本地 Judge **可以使用** 1Password 的 app 名做 Away/Unsure/时长；不得把 1Password 的 title/path/url 送云。

## 6. Vision

### 6.1 合法类别

仅此六个：`core_research`、`research_support`、`admin`、`side_project`、`distraction`、`break_away`。

`unknown` 不再合法。Prompt 不得再列出 `unknown`。

### 6.2 解析

```rust
pub enum VisionParseError {
    InvalidJson,
    InvalidCategory,
    InvalidConfidence,
    InvalidReason,
}

pub fn parse_vision_json(json: &str, context_app: Option<String>) -> Result<VisionResult, VisionParseError>
```

- JSON 必须是 object，含 `category`、`confidence`。
- `category` 必须是上述六者之一（精确小写 snake_case）。其它 → `InvalidCategory`。
- `confidence` 必须是 JSON number，且 `finite && 0.0 <= x && x <= 1.0`。NaN / Inf / 越界 / 非数字 → `InvalidConfidence`。
- `reason` 若出现必须是 string（可空字符串）。缺省当作 `""`。`null` 或非 string → `InvalidReason`。
- 解析失败或 API 失败：`vision = None` → Judge 灰区走既有「无视觉 → pending」。**不得**把非法 category 写成 slot.category 后 `final`。

`wants_core` 仅当 `category == "core_research"`。

```rust
pub struct VisionMatchContext {
    pub app: String,
    pub title: String,
    pub document_path: Option<String>,
}

pub struct VisionResult {
    pub wants_core: bool,
    pub confidence: f64,
    pub match_context: Option<VisionMatchContext>,
    pub category: String,
}
```

`match_context` 来自本地 `capture_context_json`（**未 sanitize**），供 Judge 升级 credited。HTTP prompt 另走 sanitize。删除旧字段 `context_app`。

`verified_core` 升级（非人工）保守匹配，按 span 绑定的 source sample：

1. App 不同（大小写不敏感）→ 不升级。
2. 两边都有 `document_path` → 路径相同才升级。
3. 否则两边都有非空 title → title 相同才升级。
4. 都没有更强 context → 才 fallback 到 app-only。

人工点 Core 仍升级全部无负向证据的 unsure（不要求窗口族）。

### 6.3 上下文

```rust
pub struct VisionContext {
    pub slot_start: i64,
    pub slot_end: i64,
    pub quests: Vec<String>,
    pub capture: CaptureContext, // 来自 slots.capture_context_json，不是 nearest sample
    pub activity_summary: ActivitySummary,
}

pub struct ActivitySummary {
    pub top_windows: Vec<WindowShare>,
    pub hint_seconds: HintSeconds,
    pub unobserved_seconds: i64,
}

pub struct WindowShare {
    pub app: String,
    pub title: String,
    pub seconds: i64,
    pub bundle_id: Option<String>,
    pub document_path: Option<String>,
    pub url: Option<String>,
    pub secure_input: bool,
}

pub struct HintSeconds {
    pub core_candidate: i64,
    pub core_reading: i64,
    pub unsure: i64,
    pub unsure_reading: i64,
    pub side: i64,
    pub distraction: i64,
    pub away: i64,
}
```

`top_windows` 与 `hint_seconds` **只统计 observed spans**（与 `spans_for_slot` 同一 2I 规则）。按 `(app, window_title)` 聚合秒数。unobserved 单独给出。

构建 summary 时，每个 share 的 `secure_input`：该组内任一样本 `secure_input == true` 则为 true。`document_path` / `url` 取该组出现过的值仅用于随后 sanitize（云端不会看到受保护项的这些字段）。

秒数格式：`8m40s`（整分钟 + 剩余秒）。0 秒的 hint 行可省略。

Judge 与 Vision summary 共用：

```rust
pub struct SlotEvidence {
    pub hints: Vec<Hint>,
    pub spans: Vec<Span>,
    pub activity: ActivitySeconds,
    pub strong_core_seconds: i64,
    pub reading_bridge_seconds: i64,
    pub observed_seconds: i64,
}

pub fn analyze_slot_evidence(
    samples: &[Sample],
    policy: &Policy,
    quests: &[Quest],
    slot_start: i64,
    slot_end: i64,
) -> SlotEvidence
```

`Span` 增加 `sample_index: Option<usize>`：Observed 指向 `samples` 下标；Unobserved 为 `None`。槽开头用第一条 sample 向前填充的那十几秒，`sample_index` 仍是第一条 sample（即使 `span.start < sample.ts`）。**禁止**用 `last sample where ts <= span.start` 去猜。

`activity_summary_for_vision` 只吃 `&SlotEvidence` + `&[Sample]`，不得再算一遍 hints。

### 6.4 `sanitize_vision_context()`（P0）

```rust
pub enum VisionPrivacyError {
    ProtectedCapture,
}

pub fn is_protected_frontmost(app: &str, bundle_id: Option<&str>, secure_input: bool, never_capture: &[String]) -> bool

pub fn sanitize_vision_context(
    ctx: VisionContext,
    never_capture: &[String],
) -> Result<SanitizedVisionContext, VisionPrivacyError>
```

若 **capture 本身** 受保护（Never Capture 或 `capture.secure_input`）→ `Err(ProtectedCapture)`。调用方：`vision = None`，灰区 `pending_review`，**不得上传 jpg**。不要把 app 改成 `[Protected App]` 后继续传图。

对 `top_windows` 里受保护的历史窗口：只脱敏 metadata（`[Protected App]`，去掉 title/path/url/bundle），**不**因此禁止整次请求。

`call_vision_api` / `analyze_screenshot` 只能接受 `SanitizedVisionContext`。sanitize 失败不得 POST。

`is_protected_frontmost`：`secure_input` 为真，或 app/bundle 命中 Never Capture。

### 6.5 Prompt 模板

本地时区格式化槽区间为 `HH:MM–HH:MM`。**必须先 sanitize 再格式化。** 示例：

```
Classify this macOS screenshot for productivity tracking.
Judge this 15-minute block 10:15–10:30. Return JSON only with keys category, confidence, reason.
category must be one of: core_research, research_support, admin, side_project, distraction, break_away.

Today's main quests:
1. Finish HDP tactile ablation
2. Revise GRF WP2

Observed activity in this block (do not treat unobserved time as work; credited time is computed separately):
Cursor · train.py — HDP    10m00s
[Protected App]             5m00s

Hint seconds: core_candidate 10m00s, unsure 0s.

Screenshot context:
app: Cursor
title: train.py — HDP
document_path: /Users/me/Projects/HDP/train.py
url: (none)
```

无 Quest 时写 `Today's main quests: (none)`。无 `document_path` / `url` 写 `(none)`。

`call_vision_api` / `analyze_screenshot` 只接收 `&SanitizedVisionContext`，不再只传 `Option<&str>` app。

## 7. 默认 Policy

### 7.1 `default_v01()`

Trusted 显示名（写入 `trusted_apps`，设置 UI 也用这些字符串）：

`Cursor`、`Visual Studio Code`、`Preview`、`Zotero`、`Google Chrome`、`Safari`、`Microsoft Word`、`Pages`、`PDF Expert`、`Terminal`、`iTerm2`、`Warp`、`TeXShop`、`MATLAB`、`PyCharm`、`JupyterLab`、`Jupyter`

Reading：`Preview`、`Zotero`、`PDF Expert`、`Microsoft Word`、`Pages`

`side_project_rules` 用户列表默认为空；`GameLife` 仍由 `builtin_side_project_rules()` 合并，不可删。

`distraction_rules` 默认空。`never_capture_apps` 仍为内置 1Password / Bitwarden / Keychain Access。Never Capture 目录至少包含：

| 显示名 | bundle ID |
| --- | --- |
| 1Password | `com.1password.1password`、`com.agilebits.onepassword7` |
| Bitwarden | `com.bitwarden.desktop` |
| Keychain Access | `com.apple.keychainaccess` |

`#[derive(Serialize, Deserialize)]` 加在 `Policy` 上，字段名 `trusted_apps` 等与现有 `policy_versions.json` 一致。seed 用 `serde_json::to_string(&default_v01())`，不要手写 JSON。

### 7.2 一次性 seed（不用「全空就重置」）

`app_meta` 键 `policy_seed_version`，值 `"1"` 表示已执行过 V0.1 默认政策 seed。

启动时：

1. `policy_seed_version >= 1`：不再自动插入默认政策。用户把 Trusted 清空后保持空，**不得**下次启动又填回来。
2. 标记缺失或 `< 1`：
   - `policy_versions` 无行 → 插入 `default_v01()`，写标记 `1`。
   - 最新一行四个用户列表（trusted / reading / distraction / 用户 side）全空 → 视为从未配置过的开发期空种子，插入 `default_v01()`，写标记 `1`。
   - 最新一行已有任何用户规则 → **不插入**，只写标记 `1`（不覆盖已配置政策）。

Wave 2 若把 Policy 改为 SQLite 唯一 SoT，继续沿用这个标记，不要改回「全空即默认」。

`config.rs` 的 `default_settings()` 使用同一套 Trusted / Reading 列表，避免 Wave 2 之前 UI 与 Judge 不一致。

### 7.3 App 匹配：bundle ID 优先，显示名 fallback

代码内维护只读目录 `known_app_identities`：每个显示名对应 0 个或多个 bundle ID。Trusted/Reading 至少包括：

| 显示名 | bundle ID |
| --- | --- |
| Cursor | `com.todesktop.230313mzl4w4u92` |
| Visual Studio Code | `com.microsoft.VSCode` |
| Preview | `com.apple.Preview` |
| Zotero | `org.zotero.zotero` |
| Google Chrome | `com.google.Chrome` |
| Safari | `com.apple.Safari` |
| Microsoft Word | `com.microsoft.Word` |
| Pages | `com.apple.iWork.Pages` |
| PDF Expert | `com.readdle.PDFExpert-Mac` |
| Terminal | `com.apple.Terminal` |
| iTerm2 | `com.googlecode.iterm2` |
| Warp | `dev.warp.Warp-Stable` |
| TeXShop | `edu.ucsd.cs.mmccrack.texshop` |
| MATLAB | `com.mathworks.matlab` |
| PyCharm | `com.jetbrains.pycharm`、`com.jetbrains.pycharm.ce` |
| JupyterLab | `org.jupyter.jupyterlab-desktop`（运行时 bundle 不同则只影响目录命中，显示名 fallback 仍有效） |
| Jupyter | 无强制 bundle，显示名 fallback |

`matches_app_identity(app, bundle_id, names)`：

1. 若 `bundle_id` 等于目录中某项（大小写不敏感），且该项的显示名能在 `names` 里按现有子串规则命中 → 匹配。
2. 否则对 `app` 做现有 `matches_app_name`（小写子串）。

Trusted、Reading、Never Capture 都用这一函数。

## 8. 必写回归测试

### 8.1 截图路径不得污染 Side Project

```
sample.app = "Cursor"
sample.window_title = "train.py — HDP"
sample.document_path = "/Users/me/Projects/HDP/train.py"
slot.screenshot_path = "/Users/me/Library/Application Support/GameLife/screenshots/xxx.jpg"
policy.side_project_rules 含 builtin "GameLife"
quests keyword = "HDP"
trusted 含 Cursor
```

期望：`hint_sample` = `CoreCandidate`。`Sample` 没有截图字段；测试夹具不得把 `screenshot_path` 写入 `document_path`。调度层测试断言：截图成功后 haystack 用的仍是 `document_path`（HDP），`slots.screenshot_path` 含 `GameLife/screenshots`。

正向：同一 Trusted/Quest，但 `document_path = "/Users/me/Projects/GameLife/src/App.tsx"` → `Hint::Side`。

### 8.2 禁止从标题伪造 document_path

```
AX document missing
title = "train.py — HDP"
document_path() = None
normalize_document_path("train.py — HDP") = None
```

期望：Quest keyword `HDP` 仍可通过 **title** 命中 CoreCandidate；`document_path` 必须是 `None`。

### 8.3 截图上下文必须是 Capture 瞬间（P0）

```
10:08:30 periodic sample = Cursor / train.py — HDP / document_path=HDP
10:08:37 capture context   = WeChat / title=...
10:08:45 periodic sample = Cursor / train.py — HDP
```

期望：`slots.capture_context_json.app == "WeChat"`。`VisionContext.capture.app`（sanitize 前）是 WeChat，**不得**因 nearest sample 变成 Cursor。`build_vision_prompt` 的 Screenshot context 行含 WeChat，不含把 HDP 当成截图 app。

### 8.4 Never Capture metadata 不得进 Vision payload（P0）

```
slot observed:
  1Password · "Bank Account Password"  5m  (document_path/url 任意非空)
  Cursor    · "train.py — HDP"      10m
screenshot occurs in Cursor（capture_context.app = Cursor）
Vision called
```

期望 sanitize 后的 prompt / HTTP body：

- 含 `[Protected App]` 与 `5m`
- 含 `Cursor · train.py — HDP` 与 `10m`
- **不得**出现 `Bank Account Password`、`1Password`、1Password 的 document_path 或 url
- Screenshot context 仍是 Cursor（未受保护）

另：`capture.secure_input == true` 或 capture.app 为 1Password → `sanitize_vision_context` 返回 `Err(ProtectedCapture)`，调用方不得构造 HTTP body。

### 8.5 Chrome 窗口族不得整 App 升级

```
Chrome · personal site     5m  unsure
Chrome · TensorBoard      10m  unsure
capture = TensorBoard（title=TensorBoard, document_path=None）
vision = core_research confidence≥0.7
```

expected：TensorBoard span → verified；personal-site span **不得** verified。

### 8.6 其它（同波必须有）

- `normalize_document_path("~/Projects/HDP")` 在 HOME=/Users/me 时 → `"/Users/me/Projects/HDP"`
- `normalize_document_path("file:///Users/me/My%20Project/a.py")` → `"/Users/me/My Project/a.py"`
- `normalize_document_path("file:///Users/me/HDP/")` → `"/Users/me/HDP"`
- `parse_vision_json`：合法六类通过；`unknown` / `whatever` → Err；`confidence: 1.1` / `-0.1` / `null` → Err；`reason: null` → Err；`reason: ""` 且合法 category → Ok
- `default_v01()`：Trusted 含 Cursor 与 Preview；Reading 含 Preview 与 Zotero
- 无 `policy_versions` 行时加载得到非空 Trusted，且 `app_meta.policy_seed_version = 1`
- 已有 seed 标记后把 Trusted 清空，再加载 **不会** 自动恢复默认
- 截图成功后 `samples.document_path` 仍为采样值（或 NULL），`slots.screenshot_path` 为 jpg 路径；新代码不得 UPDATE `samples.path` 为截图路径
- retention none：final 且删文件成功后 `screenshot_path IS NULL` 且 `capture_context_json IS NULL`；`pending_review` 两者仍在
- `settle_day` 把 pending→unknown 后同样 NULL 两者（retention none）
- 损坏的 `capture_context_json` → vision=None，pending，不 Fatal
- `PRAGMA user_version` 升级后为 1；`table_info` 已有列时不再 ALTER
- `~/Projects/HDP` 的 normalize 测试读取进程已有 `HOME`，禁止 `std::env::set_var("HOME", ...)`

## 9. 明确不做（本波）

愿望 CRUD、兑换快照、Side Project 设置 UI、Vision base URL/model 设置项、把 Policy 从 `config.json` 拆出（Wave 2）、登录启动、pending 通知、Pause Resume、First Core 时刻、Coin 余额拆分、ScreenCaptureKit。

macos 采集本波仍可用 osascript 读 AXDocument / bundle identifier；不替换截图实现。

## 10. 成功标准

- 在 Cursor 打开 HDP 文件并截图后，该槽不会只因为截图目录含 `GameLife` 而变 Side Project。
- 周期 Sample 是 Cursor、截图瞬间是微信时，Vision 的 Screenshot Context 是微信。
- 槽里出现过 1Password 窗口标题时，Vision POST 不含该标题。
- 无 AX 文档时 `document_path` 为空，标题仍能匹配 Quest。
- `~/Projects/HDP` 能 normalize 成绝对路径。
- 新安装 Trusted/Reading 非空；用户清空后不会在下次启动被静默填回。
- Vision 非法类别或非法 confidence 只能 pending，不能 final。
- 默认 retention 下，final / unknown（含 settle）后截图与 capture_context 一起清除；pending 不删。
- 1Password 截图不得上传 jpg；仅历史窗口出现 1Password 时只脱敏摘要。
- Chrome 个人页 + TensorBoard 同槽时，TensorBoard 截图不得把个人页升成 verified Core。
