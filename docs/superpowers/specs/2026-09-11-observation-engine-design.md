# GameLife A1：进程内原生观测引擎

日期：2026-09-11  
状态：已确认  
范围：只替换 macOS 观测实现（前台窗口元数据、可选浏览器 URL、前台窗口截图、权限布尔值）。不改 Judge、缺口规则、账本、15 秒槽、随机截图频率、Quest / 商店 / 游戏手感。

本文覆盖并取代：

- `2026-09-10-gamelife-design.md` 中「用 osascript / `screencapture` 采集」的实现描述（产品语义仍以该文为准）。
- `2026-09-11-wave1-evidence-and-vision-design.md` 中「macos 采集本波仍可用 osascript；不做 ScreenCaptureKit」——A1 **做** ScreenCaptureKit，且实现须排在 Wave 1 schema 落地之后。

未提及的行为仍以 2026-09-10 设计与 Wave 1 为准，尤其是：`document_path` 与 `screenshot_path` 分离、不得从标题伪造路径、`capture_context` 只在即将截图时调用、Never Capture / secure input fail-closed、进程死亡是 `unobserved`。

## 1. 问题

当前 `macos.rs` 用 `osascript` 读前台应用、标题、`AXDocument` 和窗口 id，再用 `/usr/sbin/screencapture -l` 截图。这套实现慢、易失败，而且 AppleScript 的 window id 不是 `screencapture -l` 要的 `CGWindowID`，容易截到错误窗口。浏览器 URL 也走 AppleScript，却把「自动化」说成读窗口标题。

A1 的目标不是做成 ActivityWatch 或 screenpipe。产品仍是科研主线时间校验器。只把观测层换成进程内原生 API，让 Wave 1 已经定义的字段更稳。

## 2. 成功标准（S2）

同一组字段，来源更可靠：

| 字段 | A1 必须 |
| --- | --- |
| 前台 app、标题、bundle id | 辅助功能已授权时，原生应用与 Electron 都应尽量有 app；标题能拿则拿 |
| `document_path` | Preview / Safari（本地文件窗口）/ Pages 等暴露 `AXDocument` 或 `file:` URL 的原生应用要有真实路径；Electron（Cursor / VS Code）允许为空 |
| `url` | 前台是 Chrome / Safari / Arc 时尽量给出当前标签 URL（去 query/fragment）；读不到则 `NULL`。判定仍不得依赖「一定有 URL」 |
| idle / 锁屏 / secure input | 保持现有 FFI，行为不变 |
| 权限 | 辅助功能控制元数据是否可用；屏幕录制控制能否截图 |
| 截图 | 计划时刻截的是当时前台窗口的 JPEG，且与本次 `capture_context` 同一扇窗 |

明确不算失败：Cursor 的 `document_path` 为空；用户拒绝浏览器自动化导致 URL 为空。

## 3. 不变量

1. 只有一个 GameLife 进程，不嵌入 ActivityWatch / screenpipe，不另起 watcher。
2. 采样间隔仍为 15 秒；每槽仍一张随机前台窗口截图（第 4–13 分钟）。禁止事件驱动连拍，禁止每个 15 秒截图。
3. `gamelife-core` 的 `CaptureContext` **不加** `CGWindowID`。窗口 id 只存在 `src-tauri`。
4. `document_path` 只来自 `AXDocument` 或 scheme 为 `file:` 的 `AXURL`，再经 `normalize_document_path`。禁止从窗口标题、HTTP(S) URL、sandbox 乱码伪造路径。
5. `sample_once` 禁止调用 `capture_context()`。`capture_context()` 只在 `tick_capture` 真正即将截图时调用，并与 `capture_frontmost_window` 紧挨着。
6. Never Capture / secure input / 锁屏 / 暂停 / 无屏幕录制 → `skipped`：不截图、不写 `capture_context_json`、不上传。
7. 截图失败或超时 → `missed`。禁止补截、禁止改 `capture_scheduled_at`。
8. 无辅助功能 → 该拍 `unobserved`，不写 `samples` 行。不得在未授权时用 `NSWorkspace` 偷偷记 app。
9. 截图最低系统：macOS 14+（`SCScreenshotManager`）。更旧系统元数据照采，截图记 `missed`，禁止回退 `screencapture`。
10. 实现开始前，Wave 1 的 schema（`document_path` / `bundle_id` / `secure_input` / `screenshot_path` / `capture_context_json`）与 `SampleSource::capture_context` 必须已在主线。

## 4. 架构

采样线程不变。`MacSampleSource` 仍实现 `SampleSource`。内部改为：

```
15s sample_once
  → 一次 observe_window() = NSWorkspace + 一次 AX
  → 可选 browser URL（仅三款浏览器）
  → idle / lock / secure_input（现有 FFI）
  → 写 samples

tick_capture 到点
  → capture_context()：再一次 snapshot + 可选 URL，暂存 cg_window_id
  → 调度器做 Never Capture 等检查
  → capture_frontmost_window(path) 只用暂存的 cg_window_id
  → ScreenCaptureKit 写 JPEG
```

AppKit 若必须在主线程：用有超时的 dispatch。禁止在主线程同步等待 ScreenCaptureKit。采样线程可以等待截图超时；主窗口不能跟着卡住。

## 5. 模块

把 `src-tauri/src/macos.rs` 拆成目录 `src-tauri/src/macos/`。

### 5.1 `macos/snapshot`

`snapshot() -> FrontmostSnapshot`

```
FrontmostSnapshot {
  app: String,
  title: String,
  bundle_id: Option<String>,
  document_raw: Option<String>,
  pid: Option<i32>,
  cg_window_id: Option<u32>,
}
```

步骤：

1. `NSWorkspace.shared.frontmostApplication` → 本地化名、`bundleIdentifier`、PID。没有前台应用则空结构。
2. 对该 PID 建 `AXUIElement`，读 focused/main window 的标题、`AXDocument`；若文档空则读 `AXURL`，仅当 scheme 为 `file:` 时保留。AX 超时 **400ms**。超时后仍返回已有的 app/bundle，title 空，`document_raw = None`。
3. `CGWindowListCopyWindowInfo`（仅屏幕上、排除桌面元素），前到后第一扇 `kCGWindowOwnerPID == pid` 且 `kCGWindowLayer == 0` 的窗口 → `cg_window_id`。禁止把 AX window id 当作 `cg_window_id`。

本模块不读浏览器 URL、不截图。

### 5.2 `macos/browser`

`url_for(bundle_id, app) -> Option<String>`

仅当命中允许列表才发 Apple Event（ScriptingBridge 或等价），超时 **1000ms**。否则立刻 `None`，不得拉起脚本。

允许列表（bundle **或** 应用名）：

| bundle id | 应用名 |
| --- | --- |
| `com.google.Chrome` | `Google Chrome` |
| `com.apple.Safari` | `Safari` |
| `company.thebrowser.Browser` | `Arc` |

不包含 Canary、Edge、Brave、Chromium。失败、拒权、超时 → `None`。query/fragment 仍由现有 `strip_url_query_fragment` 处理。`Info.plist` 的 `NSAppleEventsUsageDescription` 改为：只为读取 Chrome / Safari / Arc 当前标签 URL，不再声称用它读窗口标题。

浏览器自动化缺失 **不** 进入权限横幅（URL 可选）。设置页可用一句话说明。

### 5.3 `macos/capture`

`capture_window(window_id: u32, path) -> Result<(), ()>`

`SCScreenshotManager` 截该 `CGWindowID`，写成 JPEG。超时 **2000ms**。API 错误、空文件、无法解码 → `Err`。不检测「全黑像素」。不读 Never Capture、不写数据库。

### 5.4 `macos/input`

idle、锁屏、secure input：保持现有 FFI / `ioreg`。A1 不换实现。读失败语义不变（idle=0、未锁屏、secure=false）。

### 5.5 `macos/permissions`

- 辅助功能 → `metadata_observation_available`
- 屏幕录制 → `capture_observation_available`

采样循环内只查询布尔值，不反复弹出系统授权。横幅仍只处理这两项；点按钮只请求屏幕录制（与现网一致）。

### 5.6 `macos/mod` 与 `SampleSource`

`MacSampleSource` 持有 `last: Mutex<Option<FrontmostSnapshot>>`。`observe_window()` 与 `capture_context()` 每次都会覆盖它。

对外给采样器：

- `observe_window() -> Result<ObservedWindow, ()>`：一次 snapshot，写入 `last`，返回 app、title、bundle_id、尚未规范化的 `document_path`。`sample_once` **只调这一次**，不再分别问 `frontmost_app` / `document_path` / `bundle_id`。
- `optional_browser_url()`：只读 `last` 的 bundle/app 做允许列表判断。`last` 为空则返回 `None`，不补打 AX，也不另读 `NSWorkspace`。`sample_once` 必须先 `observe_window` 再问 URL。
- `capture_context() -> CaptureContext`：再打 snapshot（覆盖 `last`）+ 按 `last` 决定是否问 URL + `secure_input_on()`。
- `capture_frontmost_window(path)`：用 `last.cg_window_id` 截图，用后把 `last` 的 `cg_window_id` 置空（元数据可留）。没有 id → `Err`（调度器记 `missed`）。

`CaptureContext` 字段不变。`FakeSampleSource` 实现 `observe_window`，继续不碰系统 API，也不走浏览器允许列表（测试里设定的 URL 原样返回）。trait 上若仍留 `frontmost_app` / `document_path` / `bundle_id`，仅测试兼容；`sample_once` 与 `MacSampleSource` 的生产路径不得调用它们。

## 6. 数据流与失败处理

### 6.1 采样

1. 辅助功能未授权 → `unobserved` + 心跳，不写 `samples`。
2. 已授权：`observe_window`。无前台应用：写入空 app/空标题样本（与现网 `frontmost_app` 失败一致），不升级成 unobserved。
3. 仅三款浏览器才问 URL；失败则 `url = NULL`。
4. `document_path` 经 `normalize_document_path`；不合格为 NULL。

### 6.2 截图

调度语义不变：未到点不调用 `capture_context`；无屏幕录制 / 锁屏 / 暂停 → `skipped`；Never Capture 或 secure input → `skipped`，不截、不写 JSON、不上传。

通过检查后：

1. `capture_context` 暂存 `cg_window_id`。
2. 无 id、超时、写文件失败 → `missed`。不写 `screenshot_path`，不写 `capture_context_json`。
3. 成功则同一事务式更新里同时写 `screenshot_path`、`captured_at`、`capture_context_json`、`capture_status = captured`。上下文与文件必须成对；失败则两者都不要。

重启错过计划时刻、进程死亡：仍走现有 `missed` / `unobserved`。A1 不改心跳。

### 6.3 失败对照

| 失败 | 采样 | 截图 |
| --- | --- | --- |
| 无辅助功能 | unobserved | 本拍不截 |
| 无屏幕录制 | 照常写样本 | `skipped` |
| 无浏览器自动化 | `url = NULL` | 若已通过 Never Capture 检查，仍可截 |
| AX 超时 | app/bundle 能填就填 | 若截图路径上同样超时导致无 `cg_window_id` → `missed` |
| 浏览器超时 | `url = NULL` | 不影响截图 |
| 截图超时 / 空文件 / 无法解码 | — | `missed` |
| 无暂存 `cg_window_id` | — | `missed` |

## 7. 测试

自动化不碰真屏幕、不申请 TCC。`gamelife-core` 不为 A1 加判定测试。凡改 `src-tauri/`：`cargo test -p gamelife` 必须编译并跑过（`--no-run` 不够）。

### 7.1 采样器

- `sample_once` 每拍只调一次 `observe_window()`。
- 标题 `train.py — HDP` 且文档空 → `document_path` 为 NULL。
- 无辅助功能 → 不写 `samples`，该拍 unobserved，心跳仍写。
- 计划时刻之前 `capture_context` 调用次数为 0（保留现有测试）。

### 7.2 纯函数夹具

- `CGWindowList` 假数据：同一 PID、layer 0、前到后 → 选中该窗；夹具里的 AX id 不得被当成 `cg_window_id`。
- `AXDocument` / `file:` URL 才进入规范化；`https:`、空、标题字符串 → `None`。
- `url_for`：只放行 §5.2 三款；Cursor、微信、空 bundle → 零次 ScriptingBridge。
- 无暂存 id 调用 `capture_frontmost_window` → `Err` → 调度器 `missed`。

系统绑定用测试替身；生产再接到 AppKit / ScreenCaptureKit。

### 7.3 真实 macOS（手工，可 `#[ignore]`）

`cargo test -p gamelife` 在开发机必须绿。依赖 TCC 的用例标 `#[ignore]`。

手工：

- Preview 打开本地 PDF：样本有真实 `document_path`。
- Chrome / Safari / Arc：有去 query 的当前 URL；关闭自动化后 URL 空、样本仍在。
- Cursor：有 app 与标题，`document_path` 允许空。
- 1Password：样本有 app，槽 `skipped`，无截图文件。
- 关闭屏幕录制：样本照写，槽 `skipped`。
- 关闭辅助功能：时段 `unobserved`。
- 到点截图：JPEG 是当时前台窗口。

## 8. 明确不做

- 独立 watcher、嵌入 ActivityWatch / screenpipe 库、AX Observer 事件缓存。
- 无障碍树全文、本地 OCR、从标题解析 Cursor 路径、VS Code / Cursor 插件。
- 加快采样间隔、每槽多张图、macOS 13 及以下的 `screencapture` 回退。
- 改 Judge / 账本 / Quest / 商店；判定与游戏手感是后续独立规格。
- 把浏览器自动化缺失做成阻断观测的横幅。

## 9. 与后续子项目的关系

- **B 判定**：等 A1 在真实机器上稳定后再调 hint / 灰区。
- **D 今日主线、C 游戏手感**：不依赖本规格，但数字仍应建立在 A1 的观测上再加厚。
