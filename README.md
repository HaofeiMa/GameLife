# GameLife V0.1

macOS 本地时间追踪与奖励应用（Tauri + React）。

## 安装与运行

```bash
npm install
npm run tauri dev
```

发布构建：`npm run tauri build`

数据目录：`~/Library/Application Support/ma.haofei.gamelife/`

## 系统权限（必须）

GameLife 需要以下 macOS 权限才能正常观测前台应用：

| 权限 | 用途 |
| --- | --- |
| **辅助功能** | 读取前台应用名与窗口标题 |
| **屏幕录制** | 灰区槽结束时截取前台窗口供视觉判定 |

在 **系统设置 → 隐私与安全性** 中分别授权。若缺少任一权限：

- 主窗口与时间轴会显示橙色横幅提示；
- 该时段记为 **unobserved / missing**，**不会**记为 Away；
- 心跳缺口同样记为 unobserved。

授权后请重启 GameLife。

## 钥匙串

OpenAI API Key 仅存 macOS 钥匙串（账号 `GameLife`，服务 `ma.haofei.gamelife.openai`），**不会**写入 `config.json`。在「设置」页输入并保存即可。

## 登录时启动

「登录时启动」**默认开启**（`loginAtStartup: true`）。可在设置页关闭。V0.1 仅持久化该选项；系统级登录项需用户自行在 macOS 登录项中添加 GameLife（若尚未实现 LaunchAgent）。

## 其他设置

- 采样间隔固定 **15 秒**，不可调。
- 样本保留默认 **7 天**，启动采样器时自动 `purge_old_samples`。
- 截图保留：none / 24h / 3d / 14d，由采样器清理过期文件。
- Never Capture 内置项（1Password、Bitwarden、Keychain Access）只读，不可删除。

## Spec §13 手工验收清单

以下项需在真实 macOS 上人工点击验证；本环境未执行长时间退出测试。

- [ ] 10:05 退出、11:30 启动：缺口为 unobserved，不是 away，credited=0
- [ ] Isaac Sim 15 分钟 unsure + 视觉 core：verified_core>0，credited>0 且 ≤ observed
- [ ] 两端 Core、中间 4.5 分钟无样本：中间 unobserved，不得 credited 那 4.5 分钟
- [ ] 半截「结束今天」槽：credited ≤ observed ≤ 实际时长 < 900
- [ ] 崩溃后 capture_scheduled_at 不变；已过点则 missed，不立即截
- [ ] 08:25 一分 + 10:30 补满 900s：early_start 时刻不是 08:25，奖金为 0
- [ ] 08:25–08:40 连续 900s：early_start 为 08:25 档
- [ ] 8m core + 7m side：周报两边都有，不是 Core+15
- [ ] 周一 failed 再 freeze：连胜恢复为冻前+1；周二 completed 为冻前+2
- [ ] 4 月 1 日冻 3 月 31 日：占 3 月额度
- [ ] UNIQUE 重试不双发；IOERR 不把槽标成已发奖
- [ ] 商店双击：最多一笔 spend
- [ ] 大量 12m partial：XP 按 90s 累计
- [ ] Gold Day 后再 Core：无新 coin/XP
- [ ] final 槽 Report misclassification 不改 ledger（自动化测试已覆盖）
- [ ] 内置 Never Capture 不可删
- [ ] 缺权限横幅显示，时段记 unobserved 非 Away

自动化测试：`cargo test -p gamelife --offline`
