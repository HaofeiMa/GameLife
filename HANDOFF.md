# GameLife 交接文档

日期：**2026-09-14**（续）
仓库：`/Volumes/MobileSSD/Program/My/GameLife`（外置盘 MobileSSD，**先确认已挂载**）
分支：`main`，**HEAD = `c457744`**
状态：跨平台六批 + spec **已提交**（`9550d96`…`c457744`）。工作区剩下的是 **UI / hint 重设计**，未纳入上述 commit。

> 上一版 HANDOFF 写「约 38 个文件未提交」——那些平台改动已经按批切进 `main`。规格见 `docs/superpowers/specs/2026-09-14-cross-platform-observation-design.md`。
> 项目规则看 **`CLAUDE.md`**；本文只写当前进度。

---

## 0. 一分钟版本

- 产品是活动监视器，不是计划器。macOS 完整可用。
- Windows / Linux **Xorg** 观测后端已提交，交叉编译检查通过，**从未在真机运行**。Wayland 不观测。
- 跨平台 spec 已写，§6.3 身份别名**已进** `known_app_identities`（`chrome` / `Code` 等精确别名，名单仍是显示名）。
- 本机 **打不出** Windows / Ubuntu 安装包（`rusqlite` bundled + WebView2 / webkit2gtk）。要在目标 OS 上 `npm run tauri build`。
- 下一步：**① 在 Windows 11 / Ubuntu Xorg 上打包并实测 ② 提交仍 dirty 的 UI/hint**。

---

## 1. 环境与命令（有几条不显然，先读这节）

### 主项目：必须用 Homebrew 的 cargo

```bash
command -v cargo        # 必须是 /opt/homebrew/bin/cargo（1.97.1 Homebrew）
```

本机同时装了两个 Rust 工具链：Homebrew 的（主项目用）和 rustup 的（**只给交叉检查用**，1.98.1）。
**两者绝不能共用一个 target 目录**——会互相判定为过期而全量重编。
`~/.cargo/bin` 不存在，PATH 里也没有，所以 rustup 抢不到主项目的 `cargo`。**别去改 PATH 或 shell profile。**

### 三条门禁（每次改动都要跑，和 `CLAUDE.md` 一致）

```bash
cargo test --offline -p gamelife-core     # 纯领域，~1s
cargo test --offline -p gamelife          # DB/调度/macOS/TickTick/AI，必须真的 *run*
npx vitest run --dir src                  # 前端（必须带 --dir src，否则会收集 .worktrees/ 里的副本）
npm run build                             # tsc + vite，前端类型检查
```

当前基线：`gamelife` **217 passed / 1 ignored**，`gamelife-core` **159 passed**，前端 **84 passed / 19 files**，tsc 干净。

### ★ 交叉编译检查回路（本次新建，非显然，别弄丢）

主 crate **无法**为目标平台编译：`rusqlite` 是 `bundled`，需要目标平台的 C 工具链。
所以后端的写法被约束成**只依赖 `std` / `gamelife-core` / 平台 crate**（不碰 tauri、rusqlite），
再由 `tools/platform-check/`（**已从 workspace `exclude`**）用 `#[path]` **直接包含真实源码**做 `cargo check`。

```bash
cd tools/platform-check
TC="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin"

# 必须显式把工具链 bin 放最前，否则 cargo 会调到 /opt/homebrew 的 rustc（它的 sysroot 里没这两个 target）
# CARGO_TARGET_DIR 必须独立，否则会污染主项目 target/ 并连 rusqlite、ring 一起编（必失败）
PATH="$TC/bin:$PATH" CARGO_TARGET_DIR=/tmp/pcheck-target cargo check --target x86_64-pc-windows-msvc
PATH="$TC/bin:$PATH" CARGO_TARGET_DIR=/tmp/pcheck-target cargo check --target x86_64-unknown-linux-gnu
```

⚠️ **绝对不要在仓库根目录跑 `cargo check --target ...`** —— 它会去编整个 app（撞 `ring` 的 build script），并在主 `target/` 里混进另一个编译器的产物。

`tools/platform-check/src/lib.rs` 里有个 **`contract()`**：它把 sampler 用到的**每一个调用按同样签名调一遍**。
app crate 没法为目标平台编译，所以**这是唯一能证明"后端真的满足 app 的调用面"的手段**。后端少一个名字或签名对不上，它会直接编不过。新增后端能力时，**同步更新它**。

工具链是本次装的（`brew install rustup`，keg-only）：
不想要了 `brew uninstall rustup && rm -rf ~/.rustup /tmp/pcheck-target` 即可回到原状。

---

## 2. 本次会话做了什么（六批，全部未提交）

### 第 0 批：本地打包 + 固定签名身份
- `src-tauri/tauri.conf.json` 加 `bundle.macOS.signingIdentity`（`Apple Development: Haofei MA (3762LQR944)`）。
  **不加的话每次重建 cdhash 都变，macOS 会把每次构建当成新 app，辅助功能/屏幕录制每次都要重授。**
- 产物：`target/release/bundle/dmg/GameLife_0.1.0_aarch64.dmg`（约 4.3 MB），签名有效。
- 未做：GitHub 仓库与 CI（用户明确说"先不要在 github 上打包"）。

### 第 1 批：跨平台骨架（第 1 层）
- **新增 `src-tauri/src/platform.rs`**：数据目录 + 默认浏览器 opener 的**纯函数**。
  `data_dir_for(Os, home, appdata, xdg)` / `opener_for(Os, url)` 都是纯的，**三端约定在 macOS 上就能跑测试**（`cfg` 扇出会让另外两个分支永远没验证过）。
  macOS 仍是 `~/Library/Application Support/GameLife`——**这条路径不能动**，动了等于丢弃所有既有数据库/config/secrets。
- `scheduler.rs` 的 `app_support_dir` / `screenshots_dir` 改为 `pub use crate::platform::…`（调用点不用动）；`db.rs` 不再自己拼一遍路径。
- `ticktick.rs` 的 `open_in_browser`、`commands.rs` 的 `open_privacy_settings` 走 `platform::open_url`（macOS `open` / Windows `cmd /C start "" <url>` / Linux `xdg-open`）；非 macOS 的隐私设置面板明确报错。
- `lib.rs`：**托盘建失败时显示窗口**（否则托盘挂了 + 窗口默认隐藏 = 进程永远打不开）。
- 前端：`TRAFFIC_LIGHT_STRIP` 在非 macOS 归 0；新增 `src/components/PlatformNotice.tsx` + `src/lib/platform.ts`。
- `src-tauri/src/macos/browser.rs` 里 shell 出 `sleep`/`kill` 的测试加 `#[cfg(unix)]`（Windows 必挂）。

### 第 2 批：两个 macOS bug
- **拖不动顶部**（两个缺陷叠加）：① `data-tauri-drag-region` **不带值时只认直接点在该元素自身**（Tauri 注入脚本判定 `el === composedPath[0]`），页头里全是子元素 → 必须写 `="deep"`；② `capabilities/default.json` 缺 **`core:window:allow-start-dragging`**——`core:window:default` 全是只读权限，`plugin:window|start_dragging` 被 ACL 静默拒掉。另外 `index.css` 里那段 `-webkit-app-region` 是 Electron/v1 写法，这个版本的 Tauri 根本不读，**正是它让问题看起来已处理**（已删并留注释）。
- **红按钮关闭后 Dock 点不开**：红色按钮走 `CloseRequested → prevent_close + hide`（窗口只是被藏起来），而 macOS 点 Dock 图标走 `applicationShouldHandleReopen` → `RunEvent::Reopen`，代码里**没人应答**。已加该分支（必须 cfg 到 macOS，`Reopen` 是 macOS 限定 variant）。

### 第 3 批：release profile 搬到 workspace 根
- `[profile.release]` 原本写在 `src-tauri/Cargo.toml`，**cargo 只从 workspace 根读 profile** → 那五行**一个都没生效**（`lto` / `codegen-units` / `panic=abort` / `strip` 全没上，`opt-level=3` 本来就是 release 默认）。
- 已移到根 `Cargo.toml`。实测：参数确实传到 rustc 了；**二进制 17.8 MB → 8.4 MB（−52.8%）**，符号 38 560 → 386，release 构建 82s → 135s。
- ⚠️ `panic = "abort"` 现在**是生效的**：任何 panic 都会让整个进程死掉。理由见 §5 的日期函数一条。

### 第 4 批：日界函数的夏令时崩溃
- `scheduler.rs` 的 `start_of_local_day` / `end_of_local_day` 原来对 `LocalResult` 直接 `.unwrap()`。
  chrono 明确建模了两种"本地时间不总存在"的情况：**gap**（春季前跳，本地午夜不存在）和 **fold**（秋季回拨，午夜出现两次）。`.single()` 对两者都返回 `None` → panic。**在午夜切换的时区（如 `America/Havana`），一年两天会崩**，而且 `end_of_local_day` 被 3 个**命令**调用（点界面就能碰到）。
- 已改成全函数，并**泛型化到 `Tz: TimeZone`**：gap → 缺口远侧的第一个存在时刻（那天 23 小时）；fold → 较早的午夜（那天 25 小时）；**普通日返回值完全不变**。
  泛型化是为了能测：测试不能设 `TZ`（并行会互相踩，和不能设 `HOME` 同理），所以写了个合成时区 `MidnightDst` 造出午夜 gap/fold。
- 顺带：`day_str_for_ts` 的 `.single()` → `.earliest()`（原来 fold 里的时间戳会被记成 **1970-01-01**）。
- 5 个新测试（`scheduler::local_day_tests`），含两年逐小时的全量扫描、以及一个专打 `i64::MIN/MAX` 的兜底测试——**它当场抓到我自己写的兜底会溢出**（`div_euclid` 向下取整后再乘越界，release 下静默回绕）。

### 第 5 批：第 2 层 —— 跨平台观测后端
新增模块：

| 路径 | 内容 |
|---|---|
| `src-tauri/src/observe/mod.rs` | **平台缝**：`observe::imp` 按 `cfg` 解析到 `macos/` / `windows/` / `linux/`；`ObservationStatus` + `status()` |
| `src-tauri/src/observe/state.rs` | **共享** `FrontmostSnapshot` + `ObservationState`（原 `macos/state.rs` 并入，**该文件已删除**）。窗口句柄统一 `u64`，同时容纳 CGWindowID / HWND / XID；`take_cg_window_id` → `take_window_id` |
| `src-tauri/src/observe/session.rs` | X11 / Wayland / Unknown 判定。**关键陷阱：XWayland 也算 Wayland**（Wayland 会话会留 XWayland 给 X 客户端，`DISPLAY` 是设着的，看着像能用的 X11 会话）。3 个测试在 macOS 上就能跑（所以放共享模块，不放 `linux/`） |
| `src-tauri/src/windows/mod.rs` | Windows 后端：`GetForegroundWindow` / `GetWindowTextW` / `OpenProcess`+`QueryFullProcessImageNameW` / `GetLastInputInfo`（处理 49.7 天回绕）/ `OpenInputDesktop` 判锁屏 / GDI `PrintWindow`+`PW_RENDERFULLCONTENT` 截图（失败回落 `BitBlt`），BGRA→RGBA→JPEG |
| `src-tauri/src/linux/mod.rs` | Linux X11 后端（纯 `x11rb`，无需 libX11）：`_NET_ACTIVE_WINDOW` / `_NET_WM_NAME`（带 `WM_NAME` 回退）/ `WM_CLASS` / `_NET_WM_PID` / `_GTK_APPLICATION_ID` → bundle_id / XScreenSaver 取 idle 与锁屏 / `GetImage` 截图，**按窗口自己的 visual** 解码（24/32bpp + scanline padding）。连接一次进程一个，失败下次重连 |

接线：
- `sampler.rs`：`MacSampleSource` → **`NativeSampleSource`**，17 处 `crate::macos::` 改为 `crate::observe::imp::`。**macOS 行为不变**（只是经过一层 re-export）。
- `lib.rs`：`#[cfg]` 声明 `windows` / `linux` 模块。
- `src-tauri/Cargo.toml`：目标依赖 `windows-sys`（注意 `Win32_Storage_Xps` 不是笔误，见 §5）/ `x11rb`。
- `commands.rs` + `lib.rs` + `src/lib/api.ts`：新命令 **`observation_status`**（返回 `{supported, backend}`）。
  **为什么需要它**：前端那条"当前平台暂无观测能力"原本按 UA 判 `IS_MACOS`，Linux/X11 真能用之后它会**假报**。现在由后端回答，Wayland 会明确提示"要选 Ubuntu on Xorg"。
- `src/components/PlatformNotice.tsx` 改为轮询该命令（**目前只轮询一次**，见 §6）。

### 第 6 批：验证回路本身
`tools/platform-check/`（含 `rust-toolchain.toml`、独立的 `src/observe.rs` 镜像真实 `observe/mod.rs` 的 re-export）+ 上面的 `contract()`。
这条回路本次共抓出 **13 个真实错误**，包括：`GetLastInputInfo` 不在 `SystemInformation` 而在 `KeyboardAndMouse`、`PrintWindow` 被 windows-sys 分到 `Storage::Xps`、`AtomEnum` 不能 `as i32`、screensaver 的 state 是 `u8`、`?` 用在返回 `String` 的函数里、块表达式做尾表达式不能直接接 `!=`、checker 漏带 `session` 模块、**以及 `#[tauri::command]` 被写重**（原文件 `get_permission_status` 前面本就有该属性，新函数插在它下面导致继承属性又自带一个）。

---

## 3. 工作区里还剩什么（都是 UI / hint，不是平台后端）

平台相关已提交：

```
9550d96 fix: let the header drag and the dock re-open the window
2fd9b93 chore: move the release profile to the workspace root
9171c7d fix: keep the local day boundary off the DST gap and fold
651e3f0 feat: resolve the data dir and the browser opener per platform
83dec5c feat: observe the frontmost window on Windows and X11
2c2c03f feat: add the cross-target check harness for the backends
91de7a2 chore: package the macOS app with a stable signing identity
c457744 docs: specify Windows and X11 observation, and the identity aliases
```

**仍 dirty（你的 UI 重设计 + hint 管线，未进上面那些 commit）：**
`crates/gamelife-core/src/{const,hint,judge}.rs`、`docs/superpowers/specs/2026-09-13-{activity-monitor,analytics}-design.md`、
`src/App.tsx`、`src/components/PageHeader.tsx`、`src/components/ui/{input,label,segmented,select}.tsx`、
`src/index.css`、`src/lib/{api,secretField,theme}.ts`、`src/pages/{Today,Stats,Shop,Settings}.tsx`、
`src-tauri/src/{commands,scheduler}.rs`、`CLAUDE.md`、`HANDOFF.md`

其中 `hint.rs` 去掉了 `trusted_apps` 门、并把 idle ≥ 180s 收成 `away`；`commands.rs` 的 `filed` / `listed_as` 跟统计页一起。**和已提交的 `hint_sample` 签名不同**，要一起交。

---

## 4. 必须知道的坑（都踩过）

1. **`data-tauri-drag-region` 不带值 ≠ 整块可拖**，且它是 **ACL 权限**问题不是 CSS 问题。要用 `="deep"`，并在 `capabilities/default.json` 放行 `core:window:allow-start-dragging`。
2. **`[profile.*]` 只从 workspace 根读**，写在成员 manifest 里会被忽略（只给一行 warning）。改完 profile 记得核实 rustc 实际收到的 `-C` 参数，别信配置文件。
3. **`PrintWindow` 在 windows-sys 里位于 `Win32::Storage::Xps`**（微软 metadata 的分组），不在 `Gdi` 也不在 `WindowsAndMessaging`。纯 `BitBlt` 对浏览器/Electron 的 GPU 合成窗口只能截到黑。
4. **`#[tauri::command]` 插函数时要看它上面有没有已经存在一个属性**，否则新函数会继承它又自带一个。
5. **测试不能设 `HOME`/`TZ`**（并行会互相踩）。需要环境相关的判断，就把**判定逻辑写成纯函数并参数化平台**，或造合成对象（如 `MidnightDst`）。
6. **`chrono` 的本地时间不总存在**：`gap` → `LocalResult::None`，`fold` → `Ambiguous`；`.single()` 对两者都返回 `None`。日界/时区相关代码一律别 `.unwrap()`。
7. **rustup 与 Homebrew 的 cargo 不能共用一个 target 目录**；交叉检查必须显式 `PATH="$TC/bin:$PATH"` + 独立 `CARGO_TARGET_DIR`。
8. **checker 里 `#[path]` 的相对基准是"声明所在模块的目录"**：内联在 `pub mod x { }` 里会多退一级，写在独立文件（如 `src/observe.rs`）里基准才是 `src/`。
9. **macOS 数据目录不能动**（`~/Library/Application Support/GameLife`），动了等于丢弃所有既有数据。`observe/state.rs` 里有专门守这条的测试。
10. **`tools/platform-check` 已从 workspace `exclude`**，别把它加回 `members`（否则 `windows-sys`/`x11rb` 会进主 lockfile）。
11. 本次会话里我有两次**替换脚本静默失败**（断言写错）导致半完成状态。改多文件时**每对替换都加断言，并核验真实内容**，别信脚本返回值。

---

## 5. 本次会话的副作用 / 环境变更

- 装了 **rustup**（keg-only）+ `stable` 工具链 + 两个 target（`x86_64-pc-windows-msvc`、`x86_64-unknown-linux-gnu`）。约 1.5 GB。**没有改 PATH，没有改 shell profile。**
- `Cargo.lock` 因新增目标依赖而更新（`windows-sys`、`x11rb`）——已随观测后端 commit 提交。
- `src-tauri/gen/schemas/capabilities.json` 是 Tauri 生成的，已随权限变更更新。
- `.gitignore` 加了 `tools/platform-check/target/`。
- 打包产物在 `target/release/bundle/`（gitignored），不参与提交。

---

## 6. 待办（按建议顺序）

### ① 真机打包 + 实测（现在最该做）
不能在这台 Mac 上交叉打出安装包。拿到 Windows 11 和 Ubuntu 22.04/24.04（登录选 **Ubuntu on Xorg**）：

```bash
# Ubuntu：libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
npm install && npm run tauri build
# Windows 11：WebView2 + NSIS/WiX 后同样
npm install && npm run tauri build
```

预计要调：X11 visual/`GetImage`、`PrintWindow` 在 Chrome/Electron 上是否非黑、`OpenInputDesktop` 锁屏、GNOME 托盘。

### ② 提交仍 dirty 的 UI / hint
见 §3。与已提交代码的 `hint_sample` 签名不同，要成套交。

### ③ 小尾巴
- `PlatformNotice` 目前只轮询一次 `observation_status`
- Linux 锁屏 best-effort；精确锁屏走 D-Bus `LockedHint`（`zbus`）另开规格

---

## 7. 明确的未知与风险（别当成已完成）

- **两个后端从未在真机上运行过。** 交叉编译通过只证明"能编"，不证明"能跑"。我只保证代码是朝"第一次跑就能调试"写的：读不到就**显式降级**（返回空/`false`/`None`），不静默给 0、不编造数据。
- **Wayland 观测不了**，且**截图在 Wayland 上更需要 xdg-desktop-portal + PipeWire**（另一个量级的工程）。目前的处理是 `observation_status` 报 `supported=false, backend="Wayland"`，UI 提示选 Xorg。
- **Windows / Linux missing 的通道**：`document_path` 双端都是空（macOS 读 `AXDocument`，Windows 要接 UIA `ValuePattern`，X11 没有对应属性）；浏览器 URL 双端都是空。降级后果与"macOS 上拒绝自动化权限"相同，采样不受影响。
- **`panic = "abort"` 现在生效**：好处是消除了 `Mutex` 中毒这一整类连锁（5 处 `.expect()` 因此不可达）；代价是任何 panic 都会让 app 整个消失。第 4 批已清掉已知的那类；**若测的时候 app 突然整体消失（而不是弹错误），那就是 panic，把当时在做什么告诉我/下一个 agent。**

---

## 8. 接手后建议的第一串命令

```bash
cd /Volumes/MobileSSD/Program/My/GameLife
git status --short                 # 确认未提交状态
cargo test --offline -p gamelife   # 基线应为 217 passed / 1 ignored

# 交叉检查回路（证明两个后端仍满足 app 的调用面）
cd tools/platform-check
TC="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin"
PATH="$TC/bin:$PATH" CARGO_TARGET_DIR=/tmp/pcheck-target cargo check --target x86_64-pc-windows-msvc
PATH="$TC/bin:$PATH" CARGO_TARGET_DIR=/tmp/pcheck-target cargo check --target x86_64-unknown-linux-gnu
```

然后：Windows / Ubuntu 真机打包（§6.①），或落地身份别名（spec §6.3）。
