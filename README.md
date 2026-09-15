# GameLife

本地 **活动监视器**，不是计划器。每 15 秒采样前台窗口，把一天切成 15 分钟槽，判定主线 / 支线 / 杂项 / 娱乐 / 离开 / 未观测，并按计入的主线时间发放硬币和能量。日程在本机「任务」页排；这个应用负责观察、判定、结算、复盘。

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

采样时间轴、主线进展、分类去向。计划列只读当天已排期任务，可拖块改当天钟点。

![今日](docs/screenshots/today.png)

### 任务

分组、自然语言输入、3/7 天日历。改期、放弃、移动分组都在这一页。

### 统计

按周看类别、按天堆叠、高效时段。月 / 节奏 / 应用同一页切换。

![统计](docs/screenshots/stats.png)

### 商店

硬币换物品，能量换一段娱乐。同时只能进行一段娱乐。

![商店](docs/screenshots/shop.png)

### 设置

外观、启动、名单、视觉模型、可选的云端备份、权限。

![设置](docs/screenshots/settings.png)

## 安装（macOS）

1. 从 [Releases](https://github.com/HaofeiMa/GameLife/releases) 下载 `GameLife_0.2.0_aarch64.dmg`（Apple Silicon）。
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

## 任务页

日程在本机排。自然语言例如「明天上午十点到十二点，写方法节 #主线」。无时段则进分组当未排期，可再拖进日历（默认 30 分钟）。勾选完成不发币；放弃即删除。槽开始钉当天未完成、已排期任务（最多 20 条）。没有任务时，窗口上已有落地证据仍会自动记主线。观测优先于计划。

## 云端备份（坚果云 WebDAV）

可选。把修剪过的库快照拷到**你自己的**网盘，换机或磁盘坏了能拿回来。远端只是副本：采样、判定永远读本机 `gamelife.db`。关着开关时，同步模块不出网。

默认范围 **仅判定与汇总**，不含窗口标题。`secrets.json` 和 `screenshots/` 任何档位都不上传。

### 1. 在坚果云生成应用密码

不要填网页登录密码。按[坚果云说明](https://help.jianguoyun.com/?p=2064)：

1. 登录 [坚果云](https://www.jianguoyun.com/)。
2. 右上角账户名 → **账户信息** → **安全选项**。
3. **第三方应用管理** → **添加应用密码**，名称填 `GameLife`，生成后立刻抄下来（只显示一次）。

### 2. 在 GameLife 里填写

打开 **设置 → 云端**：

| 字段 | 填什么 |
| --- | --- |
| 开启云端备份 | 打开 |
| 存储类型 | WebDAV（坚果云 / Nextcloud） |
| WebDAV 地址 | `https://dav.jianguoyun.com/dav/` |
| 账号 | 坚果云注册邮箱 |
| 密码 / 应用密码 | 上一步生成的应用密码 → **保存凭据** |
| 远端目录 | 默认 `gamelife` |
| 同步范围 | 保持「仅判定与汇总（不含窗口标题）」 |
| 同步间隔 | 默认 60 分钟即可 |

先点 **测试连接**（只写一个探针文件），再点 **立即同步**。成功后网盘里会出现 `gamelife/<设备号>/latest.db` 和 `gamelife/devices.json`。测试通过但立即同步 409，多半是旧版本没建父目录，请换 [v0.2.0](https://github.com/HaofeiMa/GameLife/releases/tag/v0.2.0) 以后的包。

第二台电脑填同一套 WebDAV，点立即同步即可。登记设备 ≥ 2 台时，当天硬币改为收齐后再结算（宽限期默认 36 小时）；统计读派生的 `merged.db`。「恢复」把某台设备的快照写成旁边的 `gamelife.restored.db`，**不会**覆盖正在用的库。要启用恢复结果，先退出 GameLife，再自己改名。

Nextcloud 把地址填到 WebDAV 根为止（常见是 `https://<主机>/remote.php/dav/files/<用户>/`），账号用 Nextcloud 用户名。S3 兼容（Cloudflare R2 / B2 / MinIO）改存储类型后填 Endpoint、Bucket、Region（R2 填 `auto`）和密钥。

![云端备份](docs/screenshots/settings-cloud.png)

## 开发

```bash
npm install
npm run tauri dev
```

数据目录：`~/Library/Application Support/GameLife/`（`gamelife.db`、可选 `merged.db`、`config.json`、`secrets.json`，可选 `screenshots/`）。

API Key、云备份凭据 **只写 `secrets.json`（mode 0600）**，不进 `config.json`，也不进 Git。

门禁：

```bash
cargo test --offline -p gamelife-core
cargo test --offline -p gamelife
npx vitest run --dir src
```

## 不会做的事

- 不做实时双向同步、不做服务端查询。云备份关着时，同步模块不出网。计划本会进入备份副本，多机不合并同一份任务表。
- 不做 Windows / Linux 的 GitHub Actions 打包（要在目标系统上 `npm run tauri build`）。
- 保护窗口（1Password、Bitwarden、钥匙串访问）不截、不上传、不付。
