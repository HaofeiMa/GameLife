# GameLife：跨平台观测

日期：2026-09-14  
状态：待用户审阅  
范围：把观测从「只在 macOS 上存在」扩到 Windows 与 Linux **Xorg** 会话；约定三端的数据目录、应用身份、永不截屏内置项、以及 hint 规则表怎么对齐。经济、判定顺序、缺口规则、TickTick 只读、视觉 fail-closed 仍以前文为准。

本文覆盖并取代：

- `2026-09-10-gamelife-design.md` §2「明确不做」里的 **Windows**（**iPhone / 手机端仍不做**）。
- 运行时假设「只有 macOS 观测、另外两端是空 stub」。`observe::imp` 现在按 `cfg` 接到 `macos/` / `windows/` / `linux/`。
- `2026-09-11-observation-engine-design.md` 里「osascript / ScreenCaptureKit 是唯一后端」的范围——该文对 **macOS** 的采集契约仍有效；另外两端是新后端，不是把那份 macOS 实现搬过去。

未提及的行为仍以 2026-09-10、Wave 1、观测引擎、落地判定、活动监测器为准。硬规则顺序、观测优先于计划、保护窗口不截不传不付，三端相同。

## 1. 问题

GameLife 是活动监视器：每 15 秒采前台窗口，判 15 分钟槽，发硬币与能量。排程在 TickTick，只读。在第二台电脑上「进程能跑、但永远采不到窗口」会让统计看起来像整天没工作，而不是「这台机器看不见」。

同时，判定读的是 **应用名 / 标题 / URL / 文档路径**。macOS 给出 `Google Chrome` 和 `com.google.Chrome`；Windows 给出 `chrome`（`chrome.exe` 去扩展名）；Linux X11 给出 `WM_CLASS` 的 class（`Google-chrome`、`Firefox`、`Code`）。默认娱乐 / 杂项 / 支线 / 永不截屏表今天全是 macOS 显示名。不补身份对齐，Windows / Linux 上大部分负载会掉进灰区，娱乐网站在 **没有 URL** 时也不会命中 host 规则。

## 2. 成功标准

| 项 | 必须 |
| --- | --- |
| 三端能跑 | macOS、Windows 10/11、Ubuntu 22.04/24.04 **Xorg** 都能启动托盘（或托盘不可用时显示窗口）并按 15 秒采样 |
| 看不见就说看不见 | 没有观测能力时 UI 明确说，不装成「今日主线 0」的空闲日。Linux Wayland 提示选「Ubuntu on Xorg」 |
| macOS 数据不搬家 | `~/Library/Application Support/GameLife` 路径锁定。动了等于丢弃既有数据库、`config.json`、`secrets.json` |
| 读不到就空着 | 没有的通道返回空 / `false` / `None`，不编造 `document_path`、不把窗口标题写成路径、不把 0 秒 idle 当成「刚活动过」以外的含义 |
| 规则表可对齐 | `matches_app_identity` 能把三端对同一应用的不同叫法收到同一张名单上（见 §6），而不是往默认名单里塞 `chrome` 这种会误伤的子串 |
| 交叉检查 | `tools/platform-check` 的 `contract()` 在 Windows / Linux target 上 `cargo check` 通过。主 crate 因 `rusqlite` bundled 不能为目标平台编译，这条回路是唯一的调用面证明 |

明确不算失败：两端暂时没有浏览器 URL 和文档路径（与 macOS 拒绝「自动化」时同形）；Linux 锁屏是 best-effort；两端从未在真机跑过之前，交叉编译通过只证明能编。

## 3. 支持范围

| 平台 | 观测 | 会话 | 数据根 |
| --- | --- | --- | --- |
| macOS 14+ | 完整：辅助功能元数据、ScreenCaptureKit 截图、Chrome / Safari / Arc 当前标签 URL | 不适用 | `~/Library/Application Support/GameLife` **不可改** |
| Windows 10/11 | 前台窗口、标题、可执行文件名、idle、工作站锁屏、GDI 截图 | 不适用 | `%APPDATA%\GameLife`（`APPDATA` 空则 `%USERPROFILE%\AppData\Roaming\GameLife`） |
| Linux **Xorg** | `_NET_ACTIVE_WINDOW`、标题、`WM_CLASS`、可选 `_GTK_APPLICATION_ID`、XScreenSaver idle、`GetImage` 截图 | 登录选 Ubuntu on Xorg | `$XDG_DATA_HOME/GameLife`（必须是绝对路径），否则 `~/.local/share/GameLife` |
| Linux **Wayland** | **不观测**。`observation_status = {supported: false, backend: "Wayland"}` | XWayland **仍算 Wayland**（`DISPLAY` 在不等于能看见合成器窗口） | 同上；进程可跑，分类为空，不计 credited |
| iPhone / 其他 | 不做 | — | — |

默认浏览器：macOS `open`；Windows `cmd /C start "" <url>`；Linux `xdg-open`。系统隐私面板深链只在 macOS；其它平台明确报错，不把 `x-apple.systempreferences:` 丢给 opener。

托盘建失败时 **显示主窗口**。托盘-only 且窗口默认隐藏时，没托盘就永远打不开。这主要是没有 AppIndicator 宿主的 Linux 桌面。

## 4. 分层与模块

```
sampler（15s）
  → observe::imp::{snapshot, idle, lock, capture, url, …}
       macos/   Accessibility + ScreenCaptureKit + osascript URL
       windows/ Win32 前台窗口 + PrintWindow/BitBlt
       linux/   X11（x11rb，不链 libX11）
  → samples + heartbeat → 既有 finalize / judge / ledger

platform.rs     数据目录 + 打开 URL（纯函数，三端约定在 macOS 上就能测）
observe/state   共享 FrontmostSnapshot；窗口句柄统一 u64
observe/session Linux 会话判定（测试放共享模块，macOS 上就能跑）
```

`macos/` 的采集语义不变，只经过 `observe::imp` 再导出。`src-tauri/src/macos/state.rs` 并入 `observe/state.rs`。

主 crate **不为** Windows / Linux target 做完整编译：`rusqlite` 的 `bundled` 需要目标 C 工具链。后端只依赖 `std` / `gamelife-core` / 平台 crate。`tools/platform-check/` 已从 workspace `exclude`，用 `#[path]` 包含真实源码，在 rustup 工具链 + 独立 `CARGO_TARGET_DIR` 下 `cargo check --target …`。**禁止**在仓库根对那两个 target 跑 `cargo check`（会编整个 app、混两个编译器进 `target/`）。后端每增一个 sampler 会调的名字或改签名，必须同步改 `contract()`。

Homebrew 的 cargo 编主项目；rustup 工具链 **只**给交叉检查。两者不得共用 `target/`。不要改 PATH / shell profile 来「统一」它们。

## 5. 三端观测契约

Sampler 看到的表面在三端签名相同。某端做不到的事必须 **空着**，禁止用猜测填。

| 通道 | macOS | Windows | Linux Xorg |
| --- | --- | --- | --- |
| `app` | 显示名（`Google Chrome`） | 可执行文件 stem（`chrome.exe` → `chrome`） | `WM_CLASS` 的 class，否则 instance（`Firefox`、`Code`、`Google-chrome`） |
| `bundle_id` | 真实 bundle id | **始终 `None`** | `_GTK_APPLICATION_ID`（有则用之，如 `org.gnome.Nautilus`），否则 `None` |
| `title` | AX 标题 | `GetWindowTextW` | `_NET_WM_NAME`，回退 `WM_NAME` |
| `document_path` | `AXDocument` | **空**（UIA `ValuePattern` 未接） | **空**（X11 无对应属性） |
| `url` | Chrome / Safari / Arc 的 osascript | **空** | **空** |
| idle | `CGEventSourceSecondsSinceLastEventType` | `GetLastInputInfo`（含 49.7 天回绕） | XScreenSaver；扩展缺失时报 0（当成「正在用」，只可能漏掉 idle→away，不发明离开） |
| 锁屏 | 系统锁屏 | `OpenInputDesktop` | best-effort（XScreenSaver 不是锁 API）。真正兜底是 hint 的 idle ≥ 180s → away |
| 安全输入 | 有 | **没有等价物，恒 `false`** | 同 Windows |
| 截图 | ScreenCaptureKit，前台 `CGWindowID` | `PrintWindow` + `PW_RENDERFULLCONTENT`，失败回落 `BitBlt` | `GetImage`，按窗口自己的 visual 解码 |
| 权限 | 辅助功能 / 屏幕录制，缺则 `unobserved` + 橙条 | 无权限模型，谓词报已授权；权限条不出现 | 同 Windows |
| 窗口句柄 | `CGWindowID` as `u64` | `HWND` as `u64` | XID as `u64` |

`PrintWindow` 在 `windows-sys` 里位于 `Win32::Storage::Xps`（微软 metadata 分组），不是笔误。纯 `BitBlt` 对浏览器 / Electron 的 GPU 合成窗口经常是黑的。

浏览器 URL 与文档路径双端都空：降级后果与 macOS 拒绝自动化 / 读不到 `AXDocument` 相同——采样继续，host 规则与落地路径规则少一条证据，不因此把槽打成 away。

**禁止**为了「两端也有路径」而从窗口标题伪造 `document_path`。标题仍可进 haystack 做任务词匹配。

## 6. 应用身份与规则表

### 6.1 现状（会错）

`hint` 的娱乐 / 杂项 / 支线 / 永不截屏名单匹配的是 **子串**（`matches_app_name`：`app.to_ascii_lowercase().contains(rule)`）。默认表写的是 macOS 显示名。

因此：

- Windows `chrome` **不含** `google chrome` → 默认信任名单、以应用名为键的娱乐/杂项目都打不中。
- Linux `Google-chrome` 中间是连字符，同样打不中 `Google Chrome`。
- Linux VS Code 的 class 是 `Code`，名单上是 `Visual Studio Code`。
- 娱乐默认规则是 **host**（`youtube.com` 等）。没有 URL 时，Chrome 里的 YouTube 不会被娱乐规则抓住——与 macOS 关掉「自动化」时相同。这是证据缺失，不是后端 bug。
- 当前 `hint_sample` 仍要求应用对上 `trusted_apps` 才可能成为 `CoreCandidate`（活动监测器一文 §5.1；设置里叫「主线应用」）。Windows 上的 `chrome` 对不上名单里的 `Google Chrome` 时，**连主线候选都进不去**，只会停在 `Unsure`——除非娱乐 / 杂项 / 支线先命中。所以别名表在这里不是优化，是两端判定能否开始工作的前提。

### 6.2 做法：扩展身份表，不往名单里塞短名

**不要**把 `chrome`、`Code` 写进 `trusted_apps` / `admin_apps`。`contains` 会让任何名字里带这些音节的窗口误命中。

正确做法与现有 `known_app_identities()`（bundle id → 显示名）同一条路：每个已知应用增加 **Windows stem** 与 **Linux `WM_CLASS` class**（大小写不敏感）。`matches_app_identity` 在 `bundle_id` 对不上时，用这两列把 `chrome` / `Google-chrome` 解析成显示名 `Google Chrome`，再去对名单。

设置 UI 继续展示、继续编辑 **显示名**。使用者不必为三端各填一遍。catalog 里没有的应用，仍只按原始 `app` 字符串做 `contains`——使用者在 Windows 上把 `chrome` 加进娱乐名单仍然生效，只是那条规则在 macOS 上也会匹配任何包含 `chrome` 的名字；这与今天「名单即子串」的语义一致，由使用者自己负责。

### 6.3 第一批必须进 catalog 的别名

显示名保持现有 `known_app_identities` / 默认名单里的写法。Windows 列为 `image_display_name`（stem），Linux 列为常见 `WM_CLASS` class。一词多 class 的全部列出。

| 显示名 | Windows stem | Linux class（常见） | 备注 |
| --- | --- | --- | --- |
| Cursor | `Cursor` | `Cursor` | |
| Visual Studio Code | `Code` | `Code` | 短名只活在 catalog，不进名单 |
| Google Chrome | `chrome` | `Google-chrome`, `google-chrome` | |
| Microsoft Word | `WINWORD` | `WINWORD`（有则） | |
| Zotero | `zotero` | `Zotero` | |
| MATLAB | `matlab` | `MATLAB` | |
| PyCharm | `pycharm64` | `jetbrains-pycharm`, `jetbrains-pycharm-ce` | |
| JupyterLab | `JupyterLab` | `JupyterLab` | |
| 1Password | `1Password` | `1Password` | 永不截屏 |
| Bitwarden | `Bitwarden` | `Bitwarden` | 永不截屏 |

macOS 专有、**不**在另外两端编造对应物：

| 显示名 | 处理 |
| --- | --- |
| Safari / Preview / Pages / Keychain Access / Terminal / iTerm2 / TeXShop / PDF Expert / Warp | 继续只靠 macOS 显示名 + bundle id。Windows / Linux 不会出现这些进程，不必加假名 |
| Keychain Access | **永不截屏内置项，仅 macOS**。不要把 Windows「凭证管理器」或 Linux `seahorse` / gnome-keyring 做成内置永不截屏——那些不是「前台密码箱窗口」的等价物，做成内置会误伤 |

`never_capture_removable` 已经按子串匹配内置名。Windows 上 `1Password.exe` → `1Password`，Linux class `1Password`，与内置项相同，**不必改字符串也能挡住这两个**。Bitwarden 同理。缺的是 Chrome / Code 这类 **显示名与 stem/class 拼写不同** 的应用。

侧线内置 `GameLife`：三端进程名都含 GameLife 即可；不另造别名。

微信 / 系统邮件等 **不在默认表里** 的应用：使用者自己加时，应加该端实际看到的 `app` 字符串，或等 catalog 后续增补。本文不把「微信」默认表扩到 `WeChat` / `wechat`——那是使用名单，不是平台契约。

### 6.4 `bundle_id` 在 hint 里怎么用

- macOS：继续用 catalog 的 bundle id 抗本地化显示名（「光标」仍能对上 Cursor）。
- Windows：字段恒空。身份只走 stem → 显示名 → 名单。
- Linux：有 `_GTK_APPLICATION_ID` 时，把它当作 bundle id 参与 `matches_app_identity` 的第一支。catalog 可为 GTK 应用补 reverse-DNS（例如 Nautilus）。没有该属性的 Electron / Chrome 仍只靠 `WM_CLASS`。

Policy JSON 字段名不变。不新增 `windows_apps` / `linux_apps` 配置键——那会让设置变成三份名单。别名只存在于 `known_app_identities`。

## 7. UI

- `observation_status` 由后端回答 `{supported, backend}`（`macOS` / `Windows` / `X11` / `Wayland` / `no X display`）。前端 **禁止** 再用 UA 猜「能不能观测」：XWayland 的 UA 看起来像能用的 Linux。
- `PlatformNotice`：`supported=false` 时显示。Wayland 文案要求选 Xorg；其它 backend 说明读不到前台窗口、采样仍跑、不计 credited。
- 权限条只在 macOS 有意义。设置 → 权限：macOS 显示 `PermissionPanel`，其它平台显示 `PlatformNotice`。
- `TRAFFIC_LIGHT_STRIP` 在非 macOS 为 0。Windows / Linux 自绘标题栏在 webview 上方，再留 28px 是死区。
- `data-tauri-drag-region="deep"` + `core:window:allow-start-dragging`。这是 ACL，不是 CSS。
- 红按钮关闭 = hide。macOS Dock 点图标必须应答 `RunEvent::Reopen`。

## 8. 打包

安装包在 **目标操作系统上** 用 `npm run tauri build` 打。本仓库的 Mac 开发机：

- **不能**交叉链接出可用的 Windows / Linux 安装包：`rusqlite` bundled 需要目标 C 工具链，Tauri 还要 WebView2 / webkit2gtk / NSIS。
- **能**做的是 `tools/platform-check` 的 `cargo check --target x86_64-pc-windows-msvc` 与 `x86_64-unknown-linux-gnu`。
- Linux 的变通：Colima 里跑 Ubuntu 22.04 容器，等于「在 Linux 上原生编」，走 `tools/linux-bundle/build.sh`。Apple Silicon 打出 `arm64` `.deb`。qemu 打 x86_64 目前会在 rustc/`cc` SIGSEGV，不算可用路径。Windows 没有对等路径。

Ubuntu 22.04/24.04（登录选 Ubuntu on Xorg）：

```bash
# 典型系统依赖（包名随发行版微调）
# libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
npm install && npm run tauri build
```

Windows 11：安装 WebView2 运行时与 NSIS / WiX 后同样 `npm install && npm run tauri build`。

macOS 签名身份不要写进已提交的 `tauri.conf.json`（公开仓库会带上开发者姓名 / Team ID）。本机把身份放在 gitignored 的 `src-tauri/tauri.conf.local.json`，打包时 `tauri build --config src-tauri/tauri.conf.local.json` 合并进去，这样 cdhash 仍然稳定。示例见 `src-tauri/tauri.conf.local.json.example`。

`[profile.release]` 只从 **workspace 根** `Cargo.toml` 读取。`panic = "abort"` 已生效：任何 panic 都会让进程消失（不是弹错误）。日界函数已按 DST gap/fold 写成全函数；若真机上 app 突然整体退出，当作 panic 排查。

## 9. 不变量（本规格新增或重申）

1. macOS 数据根不得移动。
2. 缺口不外推；进程死亡是 `unobserved` 不是 away。idle / 阅读桥 / `trusted_apps` 是否仍作 Core 候选门，以判定规格与当时的 `hint_sample` 为准——本规格不改那条顺序。
3. 保护窗口不截、不上传、不付。1Password / Bitwarden 内置不可删；Keychain Access 仅 macOS 有对应进程。
4. `document_path` 不从标题伪造；`screenshot_path` 不进 haystack。
5. 视觉 fail-closed。两端没有 URL / 文档时，该缺就缺，走灰区文本 AI / 视觉 / 待复核，不降级成「整槽娱乐」或「整槽主线」。
6. 测试禁止 `std::env::set_var("HOME" | "TZ", …)`。平台约定写成纯函数（`data_dir_for`、`kind_from`），DST 用合成时区。
7. 改 `src-tauri/` 后 `cargo test --offline -p gamelife` 必须 **跑过**。改 `tools/platform-check` 或 `windows/` / `linux/` 后两条交叉 `cargo check` 必须过。
8. `tools/platform-check` 不得加回 workspace `members`。

## 10. 明确不做

- Wayland 观测、xdg-desktop-portal + PipeWire 截图（另一个量级；需要另开规格）。
- iPhone / 手机端 / 云同步。
- 在 Mac 上交叉链接出 Windows / Linux 安装包。
- 把 `chrome` / `Code` 作为子串写进默认 Policy 名单。
- 为 Windows 伪造 bundle id，或把 HWND 十六进制当 bundle id。
- UIA `ValuePattern` 文档路径、Windows / Linux 浏览器当前标签 URL（可另开规格；V0 接受与「macOS 无自动化」同形）。
- D-Bus `org.freedesktop.login1` `LockedHint`（Linux 精确锁屏的下一步，需要 `zbus`）。
- 改变判定顺序、经济公式、TickTick 只读方向。

## 11. 测试要点

- `data_dir_for`：macOS 路径锁定；Windows 走 `APPDATA` 再回退 `AppData/Roaming`；Linux 忽略相对 / 空的 `XDG_DATA_HOME`。
- `opener_for` 三端命令行形状。
- `kind_from`：`XDG_SESSION_TYPE=wayland` 即使有 `DISPLAY` 也是 Wayland；`WAYLAND_DISPLAY` + `DISPLAY` 且 type 未设 → Wayland（XWayland 陷阱）。
- `matches_app_identity`：`chrome` + 空 bundle 在 catalog 补齐后能对上名单里的 `Google Chrome`；把 `chrome` 直接放进名单不是本规格要求的测试。
- `never_capture`：`1Password` / `Bitwarden` 在三端原始 `app` 字符串上仍不可删；`Keychain Access` 仍不可删。
- `contract()`：sampler 用到的每个方法按同样签名出现，缺名字或签名不对则 Windows / Linux check 失败。
- 日界 DST、拖拽 ACL、Dock `Reopen` 的既有测试保持绿。
- 真机（唯一能证明「能跑」的）：X11 visual/`GetImage` 边界、`PrintWindow` 在 Chrome / Electron 上是否非黑、`OpenInputDesktop` 锁屏、GNOME 托盘是否出现。

## 12. 实现状态

**已落地：** `platform.rs`、`observe/` 缝、`windows/`、`linux/`、`NativeSampleSource`、`observation_status`、`PlatformNotice`、`tools/platform-check`、托盘失败显示窗口、非 macOS 交通灯条为 0、稳定 macOS 签名身份、workspace 根 release profile、本地日界 DST 全函数。

**已落地（本规格 §6.3）：** `known_app_identities` 为 Windows stem / Linux `WM_CLASS` 提供精确、大小写不敏感别名；`matches_app_identity` 先解析到显示名再对名单。默认 Policy **没有** 把 `chrome` / `Code` 写成子串。两端后端仍未经真机运行。
