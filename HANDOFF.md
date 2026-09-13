# GameLife UI 重设计 — 交接（2026-09-13）

> 起因：上一个会话（opencode + DeepSeek）历史累积 35 张截图，超过上游 30 张限制，
> 每条请求 400 死循环，无法继续。本文件把工程状态交接给新会话。

## 项目位置

- 仓库：`/Volumes/MobileSSD/Program/My/GameLife`（外置盘 MobileSSD，**注意要先挂载**）
- 分支：`main`，HEAD = `bd4c6f4`
- **工作区有大量未提交改动**（约 40 个文件 M/A/D）。整个 UI 重设计都在工作区里，还没 commit。

## 选定方案

用户从三套 mock 里选定 **① 对照**（`~/Desktop/GameLife-UI方案2.html`）：

- 「今日」页围绕 **计划 vs 实际** 组织：顶卡是「今天的主线任务对照」
  （TickTick 主线任务 × 实际推进时长 × 进度条 × 状态），配「时间去哪了」（全天成分条）
  和「攒到了」（硬币/能量/连胜/宝箱）。
- 整套 UI 换 **暖色「存档」语言**：暖白底 `#FAF6F0`、白卡、极淡暖阴影、18px 圆角。
- 页头垂直居中 + 「今日 │ 日期」分隔线（用户明确要 C 的页头样式，不要顶到顶部）。

**核心约束（不变）**：判定、账本、快照、TickTick 只读同步一个都不动；
类别色仍是 `--cat-*` 一处定义；`theme.ts` 仍是 TS 里唯一命名颜色的地方。

完整方案见 `~/.claude/plans/greedy-munching-piglet.md`。

## 已完成（工作区里已落地，已验证）

| 步骤 | 文件 | 状态 |
|---|---|---|
| 1. 暖色 token | `src/index.css`、`index.html` | ✅ 已落地（`--background: 36 50% 96%`、`--primary: 152 43% 46%`、pre-paint 已改暖色） |
| 2. 页头分隔线 + 侧栏 | `src/components/PageHeader.tsx`、`src/App.tsx` | ✅ 已落地（`subtitle` + `w-px` 竖线；侧栏 active 改白卡 pill） |
| 3. view-model | `src/lib/taskCompare.ts`、`taskCompare.test.ts` | ✅ 已落地，**11 个测试全绿** |

`taskCompare.ts` 导出：`compareDayTasks()`、`compareState()`、`COMPARE_STATE_LABEL`、
类型 `DayComparison` / `TaskComparison` / `TaskCompareState`。
「实际投入」用**时间窗口重叠**近似（slot 的 `creditedMinutes` 落在任务 `[start,end]` 内之和），
不依赖 AI 的任务匹配落库，无 TickTick 时自然降级。

## 未完成 / 需要修

### ⚠️ 第 4 步：`src/pages/Today.tsx` 改到一半断了（当前 tsc 不通过）

`npx tsc --noEmit` 报 10 个错，**全部是遗漏的收尾，不是设计问题**：

```
src/pages/Today.tsx(50,10):  'compareDayTasks' declared but never read
src/pages/Today.tsx(385,59): Cannot find name 'categoryColorAt'  ← 漏了 import
src/pages/Today.tsx(388,27): Cannot find name 'categoryColorAt'
src/pages/Today.tsx(449,10): 'TaskCompareCard' declared but never read  ← 定义了没接进 JSX
src/pages/Today.tsx(561,10): 'DayRibbon' declared but never read
src/pages/Today.tsx(592,10): 'LootTile' declared but never read
src/pages/Today.tsx(805,14): Cannot find name 'StatTile'  ← 漏了 import
src/pages/Today.tsx(812,14): Cannot find name 'StatTile'
src/pages/Today.tsx(901,31): Cannot find name 'categoryColorAt'
src/pages/Today.tsx(924,27): Cannot find name 'categoryColorAt'
```

关键事实：`categoryColorAt(key, pct)` 和 `StatTile` **都已经存在**
（`src/lib/theme.ts:28` 和 `src/components/ui/stat-tile.tsx`），只是 Today.tsx 里没 import。
`TaskCompareCard` / `DayRibbon` / `LootTile` 三个组件已经在文件里写好了，只是还没被渲染接进去。

**所以第 4 步的收尾是**：
1. 补 `categoryColorAt` 和 `StatTile` 的 import；
2. 把 `TaskCompareCard` / `DayRibbon` / `LootTile` 接进页面的 JSX（按上面的方案：顶卡对照 /
   中行时间+成分条 / 攒到了），顺手消掉那几个 "declared but never read"；
3. 重排上部内容时保持数据流不动（`getToday`/`getDayView`、`calDay` 切日、`SlotReview`、
   `ConfirmEndDay`、保护连胜）。

### 第 5、6 步：未开始

- 第 5 步：统计 / 商店 / 设置的皮肤微调（`Segmented` active 改白 pill 浮起等）。
- 第 6 步：验证——`npx vitest run --dir src`、`npm run build`、`npm run tauri dev` 目测四页 + 暗色。

## 建议的下一步动作

```bash
cd /Volumes/MobileSSD/Program/My/GameLife
git status                      # 确认工作区状态
npx tsc --noEmit                # 复现 Today.tsx 的 10 个错
# 修 Today.tsx 的 import + 接线，然后：
npx vitest run src/lib/taskCompare.test.ts
npm run build
```

修完 Today.tsx 后，建议**先把已完成的三步 + 第 4 步收尾一起 commit**，再往下做第 5、6 步——
工作区现在积了 40 个文件未提交，继续叠改动风险太大。

## 其他

- 上次会话为了截图临时改过 `src-tauri/tauri.conf.json` 的 `silentStart`（已改回，
  备份在 `/tmp/gamelife-config.bak`）。
- 顺带发现一个未查的 bug：把 `config.json` 的 `theme` 设成 `dark` 再重启，界面仍是亮的。
  `useTheme` 逻辑看起来没问题，怀疑是 `index.html` 的 pre-paint 脚本缓存的 `gl-theme` 覆盖了。
  值得单独查。
