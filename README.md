# GameLife

本地 **活动监视器**，不是计划器。每 15 秒采样前台窗口，把一天切成 15 分钟槽，判定主线 / 支线 / 杂项 / 娱乐 / 离开 / 未观测，并按计入的主线时间发放硬币和能量。日程仍放在 TickTick（只读）；这个应用负责观察、判定、结算、复盘。

> 下面的截图全部是 **虚构演示数据**，不是某个人的真实使用记录。

## 平台

| 系统 | 状态 |
| --- | --- |
| **macOS**（Apple Silicon） | 可用。安装包在 [Releases](https://github.com/HaofeiMa/GameLife/releases)。 |
| **Windows 11** | 观测后端已写，**尚未在真机打包 / 测试**，开发中。 |
| **Ubuntu（Xorg）** | 观测后端已写，**尚未在真机打包 / 测试**，开发中。Wayland 不观测。 |

macOS 关窗口只是藏到托盘，从托盘选「退出」才会结束进程。

## 界面

### 今日

采样时间轴、主线进展、分类去向。

![今日](docs/screenshots/today.png)

### 统计

按周看类别、按天堆叠、高效时段。月 / 节奏 / 应用同一页切换。

![统计](docs/screenshots/stats.png)

### 商店

硬币换物品，能量换一段娱乐。同时只能进行一段娱乐。

![商店](docs/screenshots/shop.png)

### 设置

外观、启动、名单、视觉模型、TickTick（只读）、权限。

![设置](docs/screenshots/settings.png)

## 安装（macOS）

1. 从 [Releases](https://github.com/HaofeiMa/GameLife/releases) 下载 `GameLife_0.1.0_aarch64.dmg`（Apple Silicon）。
2. 拖到「应用程序」。
3. 打开后到 **系统设置 → 隐私与安全性**，允许：
   - **辅助功能** — 读前台应用和窗口标题
   - **屏幕录制** — 灰区槽结束时截前台窗口
   - 自动化（可选）— Chrome / Safari / Arc 当前标签 URL

缺少辅助功能或屏幕录制时，该时段记为 **未观测**，不会记成离开，也不会发奖。授权后请重启 GameLife。

本机开发签名身份不要写进仓库。若你自己打包并想固定 cdhash（避免每次重建都要重授权限），把身份放到 gitignored 的 `src-tauri/tauri.conf.local.json`（见 `src-tauri/tauri.conf.local.json.example`），然后：

```bash
npx tauri build --config src-tauri/tauri.conf.local.json
```

## 开发

```bash
npm install
npm run tauri dev
```

数据目录：`~/Library/Application Support/GameLife/`（`gamelife.db`、`config.json`、`secrets.json`，可选 `screenshots/`）。

API Key、TickTick token **只写 `secrets.json`（mode 0600）**，不进 `config.json`，也不进 Git。

门禁：

```bash
cargo test --offline -p gamelife-core
cargo test --offline -p gamelife
npx vitest run --dir src
```

## 不会做的事

- 不把窗口标题、截图、报告同步到 TickTick 或其它云。
- 不做 Windows / Linux 的 GitHub Actions 打包（要在目标系统上 `npm run tauri build`）。
- 保护窗口（1Password、Bitwarden、钥匙串访问）不截、不上传、不付。
