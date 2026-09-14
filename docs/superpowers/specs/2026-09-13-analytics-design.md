# GameLife G：监测统计（日报 / 周报 / 月报 / 习惯 / 应用）

日期：2026-09-13  
状态：待用户审阅  
范围：把「本周」升格为 **统计** 页，并在今日页放入成品级日报。所有数字来自本机观测与账本，不从 TickTick 习惯/专注接口取数（官方 Open API 无此能力）。不改采样间隔、周末不采样、`activity_seconds` 跨槽求和、缺口不外推。

依赖 `2026-09-13-activity-monitor-design.md` 的外壳（导航「统计」、今日页去掉计划本）。商店视觉见 `2026-09-13-shop-desire-design.md`。

覆盖并取代 `2026-09-11-planner-shell-design.md` §8 对本周页「三块面板即完成」的上限：三块保留并升级，另外增加月报、节奏分析、应用分析。未提及的经济与判定仍以前文为准。

## 1. 问题

监测器若只显示「今日 +3h / 8h」，使用者没有理由打开窗口。需要能回答：今天时间去哪了、这周是否稳定、哪类 App 在偷时间、通常几点能进入主线。这些必须能从 `samples` / `slots` / `ledger` 算出来，不能假装有「习惯对象」。

## 2. 成功标准

| 面 | 必须 |
| --- | --- |
| 日报 | 今日页：一行徽章之下，类别构成、8h/宝箱/黄金日、可点时间轴、当日 Top App。空状态（周末/尚未采样）是成品，不是空白 div |
| 周报 | 统计 → 周：按类别、按天堆叠、高效时段热力（现有三块升级），加上周环比、娱乐占比、待复核率、主线达标日数 |
| 月报 | 统计 → 月：credited 热力日历、类别构成、硬币获得 vs 兑换、黄金日/连胜、月内冻结次数 |
| 节奏 | 统计 → 节奏：首次进入主线的时刻分布、工作日 6h/8h 达标率、连续娱乐槽、深时段（热力复用）。不出现「打卡习惯」列表 |
| 应用 | 统计 → 应用：按 App 的时间与主类别、浏览器按 host 的 Top、没被任何规则判到的新 App、保护（永不截屏）时长 |

**「名单」这一列取的是「这个应用今天真的被算成了哪一类」**，不是「用户把它填进了哪张表」。原因是 `distraction_rules` 与 `side_project_rules` 匹配的是窗口标题和网址 —— `bilibili.com` 永远匹配不上 app 名 `Google Chrome`，只用身份匹配的话 Chrome 会显示「未列入」，而它一下午明明是娱乐；更糟的是那会诱导用户把 `Google Chrome` 填进娱乐名单，从此所有 Chrome 窗口都变娱乐。所以字段取自该 App 自己的桶秒数（`side` / `admin` / `distraction` 取最大者，平票给 `hint_sample` 里更靠前的那条规则），没有桶时才回落到身份填写。同名的 `filed` 布尔表示这一列是不是真的写了 app 名：只有 `filed` 的行才给下拉菜单，标题/网址判出来的行只读显示「按标题/网址」。
| 合计规则 | 一律 `SUM(activity_*)` 或样本 15s 累加。禁止 `dominant × 15` 当周/月总量 |

明确不算失败：样本已按保留天数删除的日子，应用分解只靠日汇总表；汇总表出现前的历史日应用面板显示「无应用明细」。周末格在月历上标「未采样」而不是 0 主线。

## 3. 不变量

1. 与 2026-09-10 §6 相同：周/月按 `activity_seconds` 加总。
2. 未观测单独成项，不并入离开。
3. 日汇总在槽 **决议时** 递增写入，这样 `sample_keep_days` 仍可以是 7，月报应用分析仍成立。
4. 汇总只读已写入的 hint 分类秒数，不在报表里重跑 AI。
5. 永不截屏时段计入「保护」，不进主线、不进娱乐。
6. 改 `src-tauri/` 后 `cargo test --offline -p gamelife` 必须编译并跑过；前端 `npx vitest run --dir src`。
7. 测试禁止 `std::env::set_var("HOME", …)`。

## 4. 日汇总表

槽 finalize / resolve 成功后，用该槽样本更新（同一事务，失败则整槽判定事务失败，禁止「槽已 final 但汇总缺失」）：

```
app_day_stats (
  day TEXT NOT NULL,
  app TEXT NOT NULL,
  bundle_id TEXT NOT NULL DEFAULT '',
  samples INTEGER NOT NULL,
  idle_seconds INTEGER NOT NULL,
  core INTEGER NOT NULL,
  support INTEGER NOT NULL,
  admin INTEGER NOT NULL,
  side INTEGER NOT NULL,
  distraction INTEGER NOT NULL,
  away INTEGER NOT NULL,
  unobserved INTEGER NOT NULL,
  protected INTEGER NOT NULL,   -- 永不截屏或 secure input
  PRIMARY KEY (day, app, bundle_id)
)

host_day_stats (
  day TEXT NOT NULL,
  host TEXT NOT NULL,           -- url_host，无法解析则不下此表
  samples INTEGER NOT NULL,
  core INTEGER NOT NULL,
  support INTEGER NOT NULL,
  admin INTEGER NOT NULL,
  side INTEGER NOT NULL,
  distraction INTEGER NOT NULL,
  PRIMARY KEY (day, host)
)
```

每条样本 +15s 记入对应类别列（与 `hint_sample` 一致；`CoreCandidate`/`CoreReading` 记 `core`，`Unsure`/`UnsureReading` 记 0 类别秒但 `samples`+1，待槽 dominant 不回写样本级类别——**报表的 App 分解以 hint 为准**，槽级类别仍以 `slots.activity_json` 为准）。

两种口径在 UI 上写清楚：

- 槽级（日报时间轴、周类别、月热力）：`slots.activity_json` / `credited_core_seconds`
- App/host 级：hint 秒数。允许两者不完全相等（灰区后来被视觉改槽类别不会改写已入账的 hint 汇总）。不在 V0 做回填。

## 5. 今日 · 日报

今日页中部，不用两行小网格堆数字。布局：

1. **主线进度条**（一条）：`credited` / 480 分钟，旁注宝箱 `have/need`、黄金日 `have/need`。黄金日已达成则进度条满并显示既有黄金日文案。
2. **类别条**：主线 / 辅助 / 支线 / 杂项 / 娱乐 / 离开 / 未观测 / 待复核，整数分钟，颜色与现网 `--gl-*` 一致。
3. **时间轴**：见活动监测 spec §8.2。每块标题用类别中文名；有 TickTick 快照时轴顶用短竖线标计划时段（只读）。
4. **当日应用 Top 5**：来自 `app_day_stats`，显示名称与主色（该 App 秒数最大的类别色）。点开可看当日该 App 分钟。无数据时：「今天还没有应用明细」。
5. 待复核条数，点按滚到时间轴上第一块 pending。

翻日期：时间轴与类别条随 `get_day_view`；徽章（硬币/能量/连胜）始终是真实「今天」，不随历史日改写。

## 6. 统计页结构

统计页顶栏：日期范围 + 四个分段 **周 / 月 / 节奏 / 应用**。默认周。范围：

- 周：ISO 周一至周日，按钮 ‹ 本周 ›（与现网工作周一致；周末柱为空）。
- 月：自然月，‹ 本月 ›。
- 节奏 / 应用：沿用当前选中的周或月范围（分段条旁一个范围切换：按周 | 按月），默认按周。

所有图必须有：标题、一句话说明口径、图例、空状态、数值标注。禁止线框占位。

### 6.1 周报

保留并升级现有三块：

1. **按类别**：横条，分钟。增加「占观测比」。
2. **按天**：主线 / 支线 / 杂项堆叠；另用细线或第二轴表示娱乐（不堆进主线柱，以免看起来像工作）。周末显示「未采样」。
3. **高效时段**：8–21 点热力，值 = 该小时主线秒 / 观测秒（`heatTone` 现网）。无观测格子为灰。

新增一块 **本周数字**（不是第二套图，是四到六个大数）：

- 主线小时（credited 转小时，一位小数）
- 相对上周主线 Δ（首周无对比则「—」）
- 娱乐占观测比
- 待复核槽 / 已决议槽
- 工作日中 credited≥6h 的天数、≥8h 的天数
- 连胜（取今日快照，不按周重算另一套公式）

### 6.2 月报

1. **热力月历**：每个工作日一格，颜色按当天 `credited_core` 相对 480 分钟。周末与未来日样式不同。点格切换到「今日」路由并带上该日（时间轴与类别条为该日；硬币/能量/连胜徽章仍是真实今天）。
2. **月类别构成**：与周同一套类别，数据来自该月所有 `slots`。
3. **硬币**：本月 `ledger` 正增量合计 vs `redemptions` 硬币支出。能量只展示「本月获得」整数，不把未完成娱乐折算进去。
4. **月份徽章**：黄金日天数、当月冻结次数（`freeze_uses` 按被保护日的月份）、最长连胜只读展示当前 streak（不虚构历史最大 streak 除非已能从 `days.outcome` 重算；若 `days` 行完整则展示「本月 completed 日数」即可，不做未存储的最长 streak 考古）。

### 6.3 节奏（习惯分析的诚实版本）

没有习惯实体。只展示由槽推出的节奏：

1. **开工时刻**：每个工作日第一个 `credited_core>0` 的槽开始钟点，画 7 天（或月内工作日）的点/短柱。尚无主线日不画点。
2. **达标率**：范围内科 credited≥6h、≥8h 的工作日比例，大数字 + 辅助说明「周末不采样」。
3. **娱乐连段**：连续 ≥3 个槽 dominant 为娱乐的次数与总分钟。用于回答「是不是一滑就半小时」。
4. **深时段**：复用热力数据，列出范围内科主线占比最高的三个钟点。

不做：TickTick 习惯打卡、目标管理、睡眠、手机。

### 6.4 应用

1. **App 表**：范围内 `SUM(app_day_stats)`，列：名称、总分钟、主类别（core/support/admin/side/distraction/away 中秒最大者；全 0 则显示「未分」）、是否已在某一名单（主线/支线/杂项/娱乐/阅读/永不截屏）。
2. **新面孔**：范围内出现过、但**既不在任何生效名单里、也没有拿到任何被分类的秒数**（`core + support + admin + side + distraction + away` 全为 0）的 App，置顶提示「考虑加进名单」。只提示，不自动写入 Policy。这条从「不在名单里」改成了「没被任何规则判到」，否则每个跑主线的 App 都会变成新面孔，而主线已经不再是一张名单了。
3. **Host 表**：浏览器相关，Top 15 host，类别同 hint。
4. **保护**：`SUM(protected)` 分钟，单独一行说明「永不截屏 / 密码框，不发币」。

排序默认总分钟降序。无汇总的历史范围：空状态说明从本版本上线后的槽才有 App 明细。

## 7. API

`get_today` 增加当日 `appTop`（最多 5 条）与待复核计数（也可从已有 slots 推，不必新字段硬塞）。

新命令：

- `get_week_report(anchorDay)`：在现有 `get_week` 上扩展字段（环比、达标日、娱乐比、复核率）。保持旧字段以免商店页现有调用断裂；商店仍可只读 `get_week` 的愿望与余额。
- `get_month_report(year, month)`
- `get_rhythm_report(range)`：`range` 为 `{ kind: "week"|"month", anchor: string }`
- `get_app_report(range)`

聚合放 `src-tauri` 查询 + 少量 `gamelife-core` 纯函数（达标、连段、热力）。禁止在 React 里扫全部原始 samples。

## 8. 错误处理

| 情况 | 行为 |
| --- | --- |
| 某日无槽 | 日报类别全 0，时间轴空，文案「这一天没有监测记录」 |
| 汇总事务失败 | 槽不得标 final（与账本同一事务） |
| 月报跨样本已删除 | 槽级图仍在；App 表可能偏短，脚注说明 |
| 除零 | 热力与占比分母为 0 时显示「无观测」，不是 NaN |

## 9. 测试要点

- 8m 主线 + 7m 支线的槽，周报两类都出现，不是 15m 主线。
- 槽决议后 `app_day_stats` 行的秒数 = 该槽样本数 × 15 按 hint 分列之和。
- 周末不出现虚假 0 主线柱，而是未采样。
- 保护样本进 `protected`，不进 `distraction`。
- `heatTone` 与现网一致（观测 0 → 0）。
- 前端统计四段都能在无数据时渲染空状态。

## 10. 明确不做

- 把周报 Markdown 写进 TickTick。
- 预测「明天能完成 8h」。
- 社交排名、导出 PDF（本波 UI 内看完即可）。
- 回填本版本之前的 `app_day_stats`。
