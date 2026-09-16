# GameLife：任务详情、列表多选、日历小时底色

日期：2026-09-16  
状态：已确认  
范围：任务备注与共用详情框；列表拖动不再选出文字；分组色空心圆勾选；列表 Shift/⌘ 多选及批量移动/删除；任务页日历去掉右缘活动竖条，改为列缝 + 小时淡底。产品仍是活动监测器：勾选不发币，备注不进判定。

本文覆盖并取代：

- `2026-09-15-tasks-interaction-design.md` §2 成功标准里「日历｜列右缘活动竖条」——改为列间留缝、浅分割线、每小时淡底色；任务块比底色窄。
- 同文 §2「列表｜勾选色 = 该组角色色」的实现形态——改为手绘空心圆勾选框，描边/填充走 `--cat-*`，不用系统 `checkbox` 的 `accentColor`。
- 同文 §8 活动竖条画法（96 格窄轨贴列右缘）。`get_day_view` 的 slots 仍是数据源。
- 同文 §3.12、§10 的 `user_version = 4`——本波为 `notes` 升到 **5**。判定、重复、提醒、右键菜单条目、今日不新建任务，仍以该文为准。

未提及的行为仍以 2026-09-15 任务交互、本机任务页、云备份、判定硬规则为准。

## 1. 问题

执行任务时需要看备注（指标、会议链接、地点），列表和日历单击却打不开详情。列表拖排序会拖出文字选区。系统勾选框未完成时看不出分组色。日历右缘 96 格色条像梯子，干扰读计划。列表不能连选后批量换组或删除。

## 2. 成功标准

| 项 | 必须 |
| --- | --- |
| 拖动 | 列表拖排序时不出现文字选中残影 |
| 勾选 | 空心圆，颜色与该分组圆点一致；完成态实心 + 对勾。列表、详情顶栏同一套 |
| 单击 | 任务页列表、任务页日历块、今日计划列：几乎无移动的 pointer 打开同一详情框。勾选框只完成，不打开。拖过约 4px 只拖不打开 |
| 详情 | 可改名称、备注（轻量 Markdown）、时间（含重复/提醒）、分组；右下角「…」打开现有任务右键菜单。不套娃对话框 |
| 备注 | 存 `tasks.notes`；复制 / 创建副本 / 完成后续写带上备注；不进判定快照与 AI |
| 多选 | 仅左侧列表。Shift 按可见顺序连选（可跨分组，折叠组不计）。⌘/Ctrl 点选切换。多于一条时关掉详情。右键可批量「移动到」「放弃」 |
| 日历 | 无右缘竖条。列间约 10px 缝，缝中 1px `--hour-line`。每小时淡底（空/未观测不涂）。任务块左右各收 6px |
| 今日 | 计划列单击打开同一详情；右键、拉边、复制不变；不画小时底色；不新建任务 |

明确不算失败：关窗时详情关掉；备注里的链接点不开系统浏览器也可以先当纯文本；Windows / Linux 上 ⌘ 键位用 Ctrl。

## 3. 不变量

1. 勾选、改备注、改期、多选移动/放弃都不发币。观测仍优先于计划。
2. `TaskSnapshot` 仍是 `id` / `title` / `role`。`notes` 不得进入 hint haystack、文本 AI prompt、视觉摘要。
3. 不引入 FullCalendar、dnd-kit、Radix、富文本编辑器库、Markdown 编辑器库。颜色只来自 `--cat-*` / `theme.ts`。
4. 页面只经 `src/lib/api.ts` 调 `invoke()`。对话框不嵌套：详情打开时「更改日期」只把焦点送到本框时间区；放弃确认可先关详情再走现有确认框。
5. `user_version` **5**。`device_id` 仍不是版本号。不删 `ticktick_cache`。
6. 测试禁止 `std::env::set_var("HOME", …)`。改 `src-tauri/` 必须跑过 `cargo test --offline -p gamelife`；改 core 跑 `gamelife-core`；改 `src/` 跑 `npx vitest run --dir src`。
7. 空任务板、关同步、关窗：已有任务仍能打开详情；无备注则为空字符串。

## 4. 数据

`tasks` 增加：

| 列 | 类型 | 含义 |
| --- | --- | --- |
| `notes` | TEXT NOT NULL DEFAULT `''` | 轻量 Markdown 原文 |

上限 **8192 字节**（UTF-8）。超出 `upsert` 拒绝，错误码 `notes_too_long`，不截断。旧行缺列时 default `''`。

`list_task_board` / `upsert_task` / `duplicate_task` 的 `TaskView` 带 `notes`。`spawn_after_complete` 把 `notes` 抄到新行。剪贴板序列化带 `notes`；缺字段的旧剪贴板当 `''`。

自然语言一行解析不填备注。

云备份：`tasks` 已在快照里，不必新表。多机仍不合并任务表。

## 5. 详情框

`TaskDetailDialog`，任务页与今日共用。现有 `Dialog` 可加可选自定义顶栏，无障碍标题为「任务详情」。

- **顶栏：** 分组色空心圆勾选；日期+时间（复用改期控件：开始/结束、重复、提醒）；关闭。
- **名称：** 单行可编辑，失焦保存。空标题拒绝，沿用现有校验。
- **备注：** textarea 编辑；下方渲染预览。停按约 400ms 或失焦后 `upsert`。子集：段落换行、`**粗体**`、`-` 无序列表、`1.` 有序列表、`[文字](url)`。`url` 只接受 `http:` / `https:`。不渲染 HTML、图片、标题、行内代码。解析器放 `src/lib/taskNotesMd.ts`，纯函数可测。
- **底栏左：** 分组 `Select`（`moveTask` / `upsert` 换 `list_id`）。
- **底栏右：** 「…」在按钮处打开 `TaskActionMenu`（更改日期、移动到、创建副本、放弃）。更改日期 → 焦点到本框时间区。放弃/删除 → 关详情，再走现有确认。

单击判定：`pointerdown` 记录坐标；`pointerup` 时位移 ≤ 4px 且无 Shift/⌘/Ctrl 视为单击打开。日历拉边命中上下 6px 仍只缩放。

## 6. 列表

### 6.1 拖动

`listDrag` 期间列表容器 `select-none`；任务行 `pointerdown`（非勾选框）`preventDefault`，避免系统选字。

### 6.2 勾选

手绘圆，直径约 14px，`border` / 完成填充 = `roleDot` 所用 `categoryColor`。完成态画对勾（当前前景色，保证对比）。禁止在组件里写 hex。

### 6.3 多选

状态：`selectedIds: string[]`。只在任务页左侧列表。

可见顺序：按当前渲染的未折叠分组、组内 `sort`，`showDone` 开时含已完成。折叠组内的 id 不出现在该序列里。

| 操作 | 结果 |
| --- | --- |
| 无修饰单击 | `selectedIds = [这一条]`，打开详情 |
| Shift+单击 | 锚点到这一条之间（含端点）全部选中；关详情。无锚点则只选这一条 |
| ⌘/Ctrl+单击 | 切换这一条是否在集合里；锚点更新为这一条。`length ≠ 1` 时关详情；减到 1 条不自动打开 |
| 点列表空白 / 开始拖某条 / 切换 3·7 天 | 清空多选，关详情 |
| 点勾选框 | 只 `toggleTaskDone`，不改多选、不打开详情 |

高亮：`bg-accent/40`（已有 token）。

### 6.4 右键

若目标 id 已在 `selectedIds` 且 `length > 1`：菜单对全部选中项。否则先变成只选目标再走单条菜单。

多选菜单只保留：

- **移动到 ▸** 其它分组（每条 `moveTask`；目标已是该组则跳过）
- **放弃任务**（一条确认：「放弃 N 条任务？」然后逐条现有放弃命令）

不批量改期、不批量勾选、不批量复制。拖排序只移动被抓住的那一条。

详情「…」始终按单条。今日计划列无多选。

## 7. 日历底色

删除 `absolute right-0 w-1` 的 96 格竖条。

日列 flex 相邻处：约 10px 间隙，正中 1px `bg-hour-line`（即 `--hour-line`），线两侧都是空白。第一条日列与小时槽之间不加这根日缝。

每小时一条淡底：`categoryColorAt(key, 10%)`，高度 `CAL_HOUR_H`，铺满该列内容宽。

`hourWashCategory(cells, hour) -> CategoryKey | null`（放 `slotRibbon.ts` 或改名为同文件新函数）：看该小时 4 个 15 分钟格，**忽略 `null` 与 `unobserved`**，众数类别；平手取更早的格；四个都忽略则 `null`（不涂）。周末、未来日、今天尚未采样的后半天：slots 本就没有格，不涂。只读，`pointer-events-none`。

任务块：相对列左右各 inset 6px，底色从两侧露出。泳道算法不变。

今日计划列不画这套底色。

## 8. 分层

```
TaskDetailDialog（Tasks + Today）
  名称 / 备注 Markdown / 改期控件 / 分组 / TaskActionMenu

任务页列表
  空心圆勾选 · select-none 拖动 · Shift/⌘ 多选 · 批量移动/放弃

任务页日历
  列缝 + hour-line · 小时淡底 · 块 inset 6px · 单击详情
```

## 9. 命令与模块

| 层 | 职责 |
| --- | --- |
| core `task.rs` | `Task.notes`；`spawn_after_complete` 抄备注；快照仍不含 notes |
| shell `db.rs` | `user_version = 5`；`ALTER notes` |
| shell `commands.rs` | `upsert` 校验长度；`duplicate` 带 notes；错误码 `notes_too_long` |
| `api.ts` | `TaskView.notes` |
| `taskNotesMd.ts` | 子集解析 → 安全 React 节点 |
| `taskClipboard.ts` | 序列化 notes |
| `slotRibbon.ts` | `hourWashCategory`；竖条格子函数可留作内部 |
| `TaskDetailDialog.tsx` | 详情框 |
| `TaskCheckbox.tsx` | 空心圆勾选 |
| `Tasks.tsx` / `Today.tsx` | 单击、多选、日历缝与底色 |

## 10. 不做

- 子任务、评论流、附件、@、Checklist 独立表。
- 日历或今日上的 Shift 连选。
- 多选后整段一起拖排序。
- 备注进判定或自然语言解析。
- 新 Markdown/富文本 npm 依赖、Hex 色、FullCalendar/dnd-kit/Radix。
- 为竖条保留「可贴边的 4px 梯子」备选画法。

## 11. 测试

core：新任务 `notes` default 空；完成每天任务新行 notes 与旧行相同；`TaskSnapshot` 无 notes 字段。

shell：migrate 到 5，旧库补列后读 `''`；`upsert` 8193 字节 → `notes_too_long`；8192 通过；`duplicate_task` 复制 notes。

前端：`taskNotesMd` 粗体/列表/链接；`javascript:` 链接当纯文本；超长不在前端静默截断（只展示错误）。`hourWashCategory`：四格未观测 → null；三格 mainline 一格 entertainment → mainline；平手取更早。多选：可见顺序跨组；折叠组打断「中间」。`npx vitest run --dir src`。
