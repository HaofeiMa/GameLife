# Task Board Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 任务页列表拖排序出现抽走 / 上移 / 插入空隙；日历表头与列缝对齐并把任务块改成左 2px / 右 6px；详情与改期改成浅底 + 卡片；去掉页头三个点，改到列表空白右键；页头显示未完成、今日已排期、硬币与能量。

**Architecture:** 时长文案与拖动空隙下标是纯函数。列表仍用手写 pointer，不引入 dnd-kit / FLIP 库。日历把日期行放进与格子同一个 `overflow-auto`。对话框加 `chrome="plain"`，去掉灰条。页头只读已有 `getToday()`。不改判定、不升 `user_version`。

**Tech Stack:** Tauri 2、React、TypeScript、vitest。构建用 Homebrew `/opt/homebrew/bin/cargo`（本波不改 Rust 则不必跑 `gamelife`）。

**Spec:** `docs/superpowers/specs/2026-09-16-task-board-polish-design.md`（覆盖 2026-09-16 详情规格里的列表拖动只 `select-none`、任务块左右各 6px、页头三个点）。

## Global Constraints

- 15 秒采样；缺口不外推；进程死亡是未观测。`credited ≤ observed ≤ 实际槽长`。
- 观测优先。拖排序、改期、开详情、加分组、显隐已完成都不发币。
- `TaskSnapshot` 不含 `notes`。备注不得进入判定。
- 不引入 FullCalendar、dnd-kit、Radix、FLIP/动画 npm 库。颜色只来自 `--cat-*` / `theme.ts`。禁止在组件里写 hex。
- 页面只经 `src/lib/api.ts` 调 `invoke()`。对话框不嵌套。
- `user_version` **5**。`device_id` 仍不是版本号。
- 界面中文。能量称「能量」。
- 改 `src/`：`npx vitest run --dir src`。本波不改 Rust 则不必跑 `gamelife`；若误动 `src-tauri/`：`/opt/homebrew/bin/cargo test --offline -p gamelife`（不要设 `CARGO_TARGET_DIR`）。
- 测试禁止新增 `std::env::set_var("HOME", …)`。
- 每个任务一条英文 conventional commit，不 `--no-verify`。
- 工作区里已有无关脏文件（通知崩溃修复、rustfmt 等）。**只 `git add` 本任务 Files 列出的路径。** 不要把 `src-tauri/src/macos/notify.rs` 打进这些 commit。

---

## Spec coverage（执行前对照，禁止跳过）

| Spec 条款 | 任务 |
| --- | --- |
| §7 未完成 n、今日已排期相交秒数与文案 | Task 1, 3 |
| §4 `listDragShown` 空隙下标 | Task 2, 5 |
| §7 页头硬币 / 能量、去掉三个点 | Task 3 |
| §8 列表空白右键 | Task 4 |
| §4 浮动卡片 + 原位抽走 + 跨组插缝；拖到日历取消空隙 | Task 5 |
| §5 sticky 日期行、左 2px 右 6px | Task 6 |
| §6 详情 / 改期浅底 + `bg-card` | Task 7 |
| §3 `user_version` 5 不变；文档 overlay | Task 8 |
| §11 测试表 | Task 1, 2, 各任务 vitest |

---

## File Structure

```
src/lib/taskHeaderStats.ts               新增：相交秒数、未完成条数、时长文案
src/lib/taskHeaderStats.test.ts          新增
src/lib/taskReorder.ts                   增加 listDragShown
src/lib/taskReorder.test.ts              空隙下标
src/lib/taskCalendar.ts                  CAL_BLOCK_INSET_LEFT/RIGHT、CAL_LANE_GAP
src/components/ui/dialog.tsx             chrome?: "muted" | "plain"
src/components/TaskDetailDialog.tsx      浅底 + 卡片
src/components/TaskDateDialog.tsx        chrome plain + 时间卡片
src/pages/Tasks.tsx                      页头、空白菜单、拖动空隙、sticky 日历
AGENTS.md / .cursor/rules/frontend-ui.mdc / specs.mdc
```

不改 `Today.tsx` 计划列 inset。不改 Rust。

---

### Task 1: 今日已排期秒数与文案

**Files:**
- Create: `src/lib/taskHeaderStats.ts`
- Test: `src/lib/taskHeaderStats.test.ts`

**Interfaces:**
- Consumes: 无
- Produces:
  - `overlapSeconds(start: number, end: number, dayStart: number): number` — 区间 `[start, end)` 与 `[dayStart, dayStart+86400)` 相交秒数，`end <= start` 为 0
  - `scheduledSecondsToday(tasks: { done: boolean; start: number | null; end: number | null }[], dayStart: number): number` — 只加 `!done` 且 `start/end` 非 null 的相交
  - `unfinishedCount(tasks: { done: boolean }[]): number`
  - `formatScheduledDuration(secs: number): string` — spec §7

- [ ] **Step 1: Write the failing test**

Create `src/lib/taskHeaderStats.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  formatScheduledDuration,
  overlapSeconds,
  scheduledSecondsToday,
  unfinishedCount,
} from "./taskHeaderStats";

describe("taskHeaderStats", () => {
  it("overlap is empty when the range misses the day", () => {
    const day = 1_000_000;
    expect(overlapSeconds(day - 3600, day, day)).toBe(0);
    expect(overlapSeconds(day + 86400, day + 90000, day)).toBe(0);
  });

  it("clips a range that crosses midnight to today only", () => {
    const day = 1_000_000;
    expect(overlapSeconds(day - 3600, day + 3600, day)).toBe(3600);
  });

  it("sums unfinished overlapping tasks", () => {
    const day = 1_000_000;
    expect(
      scheduledSecondsToday(
        [
          { done: false, start: day + 10 * 3600, end: day + 11 * 3600 },
          { done: true, start: day, end: day + 7200 },
          { done: false, start: null, end: null },
        ],
        day,
      ),
    ).toBe(3600);
  });

  it("counts unfinished including unscheduled", () => {
    expect(
      unfinishedCount([
        { done: false },
        { done: true },
        { done: false },
      ]),
    ).toBe(2);
  });

  it("formats duration per spec", () => {
    expect(formatScheduledDuration(0)).toBe("0 分钟");
    expect(formatScheduledDuration(1)).toBe("1 分钟");
    expect(formatScheduledDuration(59 * 60)).toBe("59 分钟");
    expect(formatScheduledDuration(3600)).toBe("1 小时");
    expect(formatScheduledDuration(5400)).toBe("1.5 小时");
    expect(formatScheduledDuration(7200)).toBe("2 小时");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/taskHeaderStats`

Expected: FAIL（模块不存在）。

- [ ] **Step 3: Write minimal implementation**

Create `src/lib/taskHeaderStats.ts`:

```ts
const DAY = 86400;

export function overlapSeconds(start: number, end: number, dayStart: number): number {
  if (end <= start) return 0;
  const lo = Math.max(start, dayStart);
  const hi = Math.min(end, dayStart + DAY);
  return Math.max(0, hi - lo);
}

export function scheduledSecondsToday(
  tasks: { done: boolean; start: number | null; end: number | null }[],
  dayStart: number,
): number {
  let sum = 0;
  for (const task of tasks) {
    if (task.done || task.start == null || task.end == null) continue;
    sum += overlapSeconds(task.start, task.end, dayStart);
  }
  return sum;
}

export function unfinishedCount(tasks: { done: boolean }[]): number {
  return tasks.reduce((n, task) => n + (task.done ? 0 : 1), 0);
}

export function formatScheduledDuration(secs: number): string {
  const s = Math.max(0, secs);
  if (s === 0) return "0 分钟";
  if (s < 3600) return `${Math.ceil(s / 60)} 分钟`;
  const hours = (s / 3600).toFixed(1).replace(/\.0$/, "");
  return `${hours} 小时`;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src lib/taskHeaderStats`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/lib/taskHeaderStats.ts src/lib/taskHeaderStats.test.ts
git commit -m "$(cat <<'EOF'
feat: format today's scheduled duration for the task header

EOF
)"
```

---

### Task 2: 列表拖动空隙下标

**Files:**
- Modify: `src/lib/taskReorder.ts`
- Test: `src/lib/taskReorder.test.ts`

**Interfaces:**
- Consumes: 现有 `ranksAfterDrag(ids, dragId, beforeId)`
- Produces: `listDragShown(destIds: string[], dragId: string, beforeId: string | null): { shown: string[]; gapIndex: number }`
  - `shown` = `destIds` 去掉 `dragId`（保持相对顺序）
  - `gapIndex` = `beforeId == null` 或找不到 → `shown.length`；否则 `shown.indexOf(beforeId)`

- [ ] **Step 1: Write the failing test**

Append to `src/lib/taskReorder.test.ts`:

```ts
import { listDragShown, ranksAfterDrag } from "./taskReorder";

it("drops the dragged id and inserts a gap before the target", () => {
  expect(listDragShown(["a", "b", "c"], "a", "c")).toEqual({
    shown: ["b", "c"],
    gapIndex: 1,
  });
});

it("puts the gap at the end when beforeId is null", () => {
  expect(listDragShown(["x", "y"], "z", null)).toEqual({
    shown: ["x", "y"],
    gapIndex: 2,
  });
});

it("treats a missing beforeId as append", () => {
  expect(listDragShown(["a", "b"], "c", "missing")).toEqual({
    shown: ["a", "b"],
    gapIndex: 2,
  });
});
```

Keep the existing `ranksAfterDrag` test. Add `listDragShown` to the import (replace the current single import).

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --dir src lib/taskReorder`

Expected: FAIL（`listDragShown` 不是函数）。

- [ ] **Step 3: Write minimal implementation**

Add to `src/lib/taskReorder.ts`:

```ts
export function listDragShown(
  destIds: string[],
  dragId: string,
  beforeId: string | null,
): { shown: string[]; gapIndex: number } {
  const shown = destIds.filter((id) => id !== dragId);
  let gapIndex = beforeId == null ? shown.length : shown.indexOf(beforeId);
  if (gapIndex < 0) gapIndex = shown.length;
  return { shown, gapIndex };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --dir src lib/taskReorder`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/lib/taskReorder.ts src/lib/taskReorder.test.ts
git commit -m "$(cat <<'EOF'
feat: compute list-drag gap insertion index

EOF
)"
```

---

### Task 3: 页头统计与硬币能量

**Files:**
- Modify: `src/pages/Tasks.tsx`

**Interfaces:**
- Consumes: `unfinishedCount`、`scheduledSecondsToday`、`formatScheduledDuration`、`dayStartUnix`、`getToday`（`src/lib/api.ts` 已有）
- Produces: 页头 `任务 | 未完成 n · 今日已排期 d`；`actions` 里先 `◉ coinBalance` `⚡ xpToday`（class 与 `Shop.tsx` 商店页头相同：`text-[12.5px] font-semibold tabular-nums`，硬币 `text-btn-ink`，能量 `text-energy`），再 3 天 / 7 天。删除三个点按钮。`moreOpen` 状态先留着，Task 4 才删「更多」对话框。

字幕拼接：`` `未完成 ${n} · 今日已排期 ${formatScheduledDuration(secs)}` ``。`n` 用整板 `board.tasks`（空板 0）。`dayStart` = `dayStartUnix(todayIso())`。

`getToday` 在现有 `refresh` 里一起拉（`Promise.all` 或 refresh 后再调）。失败则硬币/能量那两段不渲染，字幕仍根据本地任务算。不要为任务页新增命令。

- [ ] **Step 1: Write the failing test**

无新文件。门是 vitest 绿 + 页头不再把三个点放在 `center`。

- [ ] **Step 2: Run existing helpers**

Run: `npx vitest run --dir src lib/taskHeaderStats`

Expected: PASS。

- [ ] **Step 3: Write minimal implementation**

`Tasks.tsx`：

1. Import `getToday` from `../lib/api`，类型用现有 `TodayView` 即可只取 `coinBalance` / `xpToday`。
2. State：`const [wallet, setWallet] = useState<{ coinBalance: number; xpToday: number } | null>(null);`
3. 在 `refresh` 里：`listTaskBoard()` 成功后 `getToday().then((t) => setWallet({ coinBalance: t.coinBalance, xpToday: t.xpToday })).catch(() => setWallet(null));` 不要让 `getToday` 失败毁掉整板。
4. 计算：

```ts
const boardTasks = board?.tasks ?? [];
const subtitle = `未完成 ${unfinishedCount(boardTasks)} · 今日已排期 ${formatScheduledDuration(
  scheduledSecondsToday(boardTasks, dayStartUnix(today)),
)}`;
```

`today` 已有 `todayIso()`。

5. `PageHeader`：`subtitle={subtitle}`。删掉 `center={三个点按钮}`。`actions`：

```tsx
<div className="flex items-center gap-2">
  {wallet && (
    <>
      <span className="flex items-center gap-[6px] text-[12.5px] font-semibold tabular-nums text-btn-ink">
        ◉ {wallet.coinBalance}
      </span>
      <span className="flex items-center gap-[6px] text-[12.5px] font-semibold tabular-nums text-energy">
        ⚡ {wallet.xpToday}
      </span>
    </>
  )}
  <Segmented
    aria-label="日历跨度"
    size="sm"
    value={calDays}
    onChange={(next) => {
      setCalDays(next);
      writeCalDays(next);
      clearListSelection();
    }}
    options={[
      { value: "3", label: "3 天" },
      { value: "7", label: "7 天" },
    ]}
  />
</div>
```

三个点的 `onClick={() => setMoreOpen(true)}` 先去掉。Task 4 再给空白右键接上 `setMoreOpen` / 直接打开添加分组。

- [ ] **Step 4: Run tests**

Run: `npx vitest run --dir src`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/pages/Tasks.tsx
git commit -m "$(cat <<'EOF'
feat: show task header stats and wallet

EOF
)"
```

---

### Task 4: 列表空白右键代替三个点

**Files:**
- Modify: `src/pages/Tasks.tsx`

**Interfaces:**
- Consumes: 现有 `setAddOpen` / `setShowDone` / `ContextMenu` / `ContextMenuItem`
- Produces: `MenuState` 增加 `{ kind: "board"; x: number; y: number }`。删除 `moreOpen` 状态和 title 为「更多」的 `Dialog`。列表滚动容器空白右键弹出「添加分组」「显示已完成」/「隐藏已完成」。

命中以下选择器则 **不要** `preventDefault`（让任务行 / 分组标题自己的 `onContextMenu` 走）：

- `[data-task-id]`
- `button`（分组标题是 button）
- `[role=checkbox]`
- `input` / `textarea`

其它（滚动容器空白、分组 section 里任务行以外的垫白）`preventDefault` + `setMenu({ kind: "board", x, y })`。

添加分组：与旧「更多」里相同，`setAddName(""); setAddRole("mainline"); setAddOpen(true)`。

- [ ] **Step 1: Write the failing test**

无新文件。门：源码不再渲染「更多」对话框标题。

- [ ] **Step 2: Confirm helpers still pass**

Run: `npx vitest run --dir src lib/taskReorder lib/taskHeaderStats`

Expected: PASS。

- [ ] **Step 3: Write minimal implementation**

1. `MenuState` 加 `| { kind: "board"; x: number; y: number }`。
2. 删 `const [moreOpen, setMoreOpen] = useState(false);` 以及

```tsx
<Dialog open={moreOpen} onClose={() => setMoreOpen(false)} title="更多">
  ...
</Dialog>
```

3. 列表滚动容器（现有 `min-h-0 flex-1 overflow-y-auto px-2 pb-3` 那个 div）加：

```tsx
onContextMenu={(e) => {
  const el = e.target;
  if (!(el instanceof HTMLElement)) return;
  if (el.closest("[data-task-id], button, [role=checkbox], input, textarea")) return;
  e.preventDefault();
  setMenu({ kind: "board", x: e.clientX, y: e.clientY });
}}
```

4. 在现有 list `ContextMenu` 旁加：

```tsx
<ContextMenu
  open={menu?.kind === "board"}
  x={menu?.kind === "board" ? menu.x : 0}
  y={menu?.kind === "board" ? menu.y : 0}
  onClose={() => setMenu(null)}
>
  {menu?.kind === "board" && (
    <>
      <ContextMenuItem
        onSelect={() => {
          setMenu(null);
          setAddName("");
          setAddRole("mainline");
          setAddOpen(true);
        }}
      >
        添加分组
      </ContextMenuItem>
      <ContextMenuItem
        onSelect={() => {
          setShowDone((v) => !v);
          setMenu(null);
        }}
      >
        {showDone ? "隐藏已完成" : "显示已完成"}
      </ContextMenuItem>
    </>
  )}
</ContextMenu>
```

日历、分割条、页头不要绑这套。若 `MoreHorizontal` 已无引用，删掉 lucide import。

- [ ] **Step 4: Run tests**

Run: `npx vitest run --dir src`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/pages/Tasks.tsx
git commit -m "$(cat <<'EOF'
feat: move add-list actions to the list blank menu

EOF
)"
```

---

### Task 5: 列表拖动空隙与浮动卡片

**Files:**
- Modify: `src/pages/Tasks.tsx`

**Interfaces:**
- Consumes: `listDragShown`、现有 `beginListDrag` / `ranksAfterDrag` / `CLICK_SLOP_PX`（4）
- Produces: `listDrag` 增补 `rowHeight: number; pointerX: number; pointerY: number; title: string`。拖过 slop 后原行不渲染；目标组按 `listDragShown` 在 `gapIndex` 插一块 `height: rowHeight` 的空缝（不要背景强调色）；`position: fixed` 浮动卡片跟随指针。`overListId` 整组 `bg-accent/40` 去掉。拖进日历格子：`setListDrag(null)` 后现有 `beginCalDragAt`。

`ListDrag` 类型改成：

```ts
const [listDrag, setListDrag] = useState<{
  id: string;
  overListId: string;
  beforeId: string | null;
  rowHeight: number;
  pointerX: number;
  pointerY: number;
  title: string;
} | null>(null);
```

`beginListDrag` 里 `armed` 时第一次 `setListDrag` 用 `row.getBoundingClientRect().height` 和 `task.title`。之后 `onMove` 更新 `overListId` / `beforeId` / `pointerX` / `pointerY`，保留 `rowHeight` / `title` / `id`。

渲染每个分组的任务：

```ts
const dest = listDrag?.overListId === list.id;
const rawIds = tasks.map((t) => t.id);
const layout = listDrag
  ? dest
    ? listDragShown(rawIds, listDrag.id, listDrag.beforeId)
    : { shown: rawIds.filter((id) => id !== listDrag.id), gapIndex: -1 }
  : { shown: rawIds, gapIndex: -1 };
const byId = new Map(tasks.map((t) => [t.id, t]));
```

用 `layout.shown` 映射 `byId.get` 渲染行。在 `gapIndex >= 0` 时，在该下标 **之前** 插入：

```tsx
<div
  key="gap"
  className="transition-[height] duration-150"
  style={{ height: listDrag.rowHeight }}
/>
```

若 `gapIndex === shown.length`，空隙在最后。

浮动卡片（`listDrag` 非 null 时，放在 `Tasks` 根 fragment 里即可）：

```tsx
<div
  className="pointer-events-none fixed z-50 max-w-xs rounded-[9px] border bg-card px-2 py-1.5 text-[13px] font-semibold shadow-lg"
  style={{
    left: listDrag.pointerX + 8,
    top: listDrag.pointerY + 8,
  }}
>
  {listDrag.title}
</div>
```

禁止 hex。`shadow-lg` 是 token 阴影。行上现有 `listDrag?.id === task.id && "opacity-50"` 删掉（原行已不在 `shown` 里）。

未 armed 的单击路径不要 `setListDrag`。`pointerup` 写库逻辑保持 `ranksAfterDrag` + `reorderTask` / `moveTask`。

- [ ] **Step 1: Write the failing test**

`listDragShown` 已在 Task 2。本任务接线。

- [ ] **Step 2: Re-run reorder helper**

Run: `npx vitest run --dir src lib/taskReorder`

Expected: PASS。

- [ ] **Step 3: Write minimal implementation**

按上面改 `Tasks.tsx`。给列表行加 `transition-[transform] duration-150` 可选；空隙高度过渡已够。

- [ ] **Step 4: Run tests**

Run: `npx vitest run --dir src`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/pages/Tasks.tsx
git commit -m "$(cat <<'EOF'
feat: preview list reorder with a gap and floating card

EOF
)"
```

---

### Task 6: 日历 sticky 表头与任务块留白

**Files:**
- Modify: `src/lib/taskCalendar.ts`（三个常量）
- Modify: `src/pages/Tasks.tsx`（`TaskCalendar` / `CalendarDayColumn`）

**Interfaces:**
- Consumes: 现有 `CAL_DAY_GAP`、`DayColumnGap`、`hitCalendarTs`
- Produces:
  - `CAL_BLOCK_INSET_LEFT = 2`
  - `CAL_BLOCK_INSET_RIGHT = 6`
  - `CAL_LANE_GAP = 4`
  - 日期行与 24h 格子同在一个 `overflow-auto`；日期行 `sticky top-0 z-20 bg-card`
  - 块：`left: calc(lane% + 2px)`，`width: calc(span% - 12px)`（2+6+4）

今日计划列（`Today.tsx`）**不要**改 inset。

- [ ] **Step 1: Write the failing test**

现有 `src/lib/taskCalendar.test.ts` 的 `hitCalendarTs` 必须继续绿。本任务把表头放进同一滚动容器，**不要**改 `hitCalendarTs` 公式。`gridRef` 仍绑在含 gutter+列的那层可滚动根上（现在是 `overflow-auto` 的 div）；sticky 之后 `gridRef` 继续指向这个滚动根，这样 `getBoundingClientRect` + `scrollTop` 与现有 `hitTs` 一致。

- [ ] **Step 2: Run calendar helpers**

Run: `npx vitest run --dir src lib/taskCalendar`

Expected: PASS。

- [ ] **Step 3: Write minimal implementation**

`taskCalendar.ts` 在 `CAL_DAY_GAP` 下增加：

```ts
export const CAL_BLOCK_INSET_LEFT = 2;
export const CAL_BLOCK_INSET_RIGHT = 6;
export const CAL_LANE_GAP = 4;
```

`TaskCalendar` 现结构是「外面日期行 + 里面 overflow 格子」。改成：

```tsx
<div className="flex h-full min-h-0 flex-col">
  <div ref={gridRef} className="min-h-0 flex-1 overflow-auto">
    <div className="relative" style={{ height: height + /* 日期行约 32：用 sticky 行自身高度，不要加进 24h */ }}>
      <div className="sticky top-0 z-20 flex bg-card">
        <div className="shrink-0" style={{ width: CAL_GUTTER }} />
        {days.map((day, i) => (
          <Fragment key={day}>
            {i > 0 ? <DayColumnGap /> : null}
            <div
              className="min-w-0 flex-1 py-1.5 text-center text-[11px] font-medium"
              style={
                day === today
                  ? { background: categoryColorAt("mainline", 16) }
                  : undefined
              }
            >
              {dayColumnLabel(day)}
            </div>
          </Fragment>
        ))}
      </div>
      <div className="relative flex" style={{ height }}>
        {/* 现有小时槽 + days.map CalendarDayColumn，含 DayColumnGap */}
      </div>
    </div>
  </div>
</div>
```

**不要**把 sticky 行的高度加进 `hitCalendarTs` 的 `y` 计算除非你同步改 helper。正确做法：`hitTs` 用的 `box.top` 是滚动根的顶，日期行 sticky 在滚动根顶部。格子的 `top` 比滚动根内容顶多了一行日期。这样 **现有 `hitCalendarTs` 会把 y=0 当成 00:00，实际点在日期行上**。

必须二选一，选 **A（推荐）**：

- 滚动根仍是 `gridRef`。
- `hitTs` 里把 `clientY` 先减去日期行高度。给日期行 `ref` 或固定高度：日期行 `h-8`（32px），`hitCalendarTs` 增加参数 `headerH: number`，`y = clientY - grid.top + scrollTop - headerH`。`y < 0` 仍 `null`（点在日期上不落格）。

改 `hitCalendarTs`：

```ts
export function hitCalendarTs(
  days: string[],
  clientX: number,
  clientY: number,
  grid: { left: number; top: number; width: number; scrollTop: number },
  headerH = 0,
): number | null {
  ...
  const y = clientY - grid.top + grid.scrollTop - headerH;
  if (y < 0) return null;
  ...
}
```

在 `taskCalendar.test.ts` **追加**（保留现有用例，它们不传 `headerH` 故默认 0，行为不变）：

```ts
it("ignores clicks on the sticky day header", () => {
  const days = ["2026-09-16", "2026-09-17"];
  const grid = { left: 0, top: 0, width: 40 + 200 + 10 + 200, scrollTop: 0 };
  expect(hitCalendarTs(days, 50, 10, grid, 32)).toBeNull();
});
```

`Tasks.tsx` 的 `hitTs` 传 `headerH: 32`，日期行 `className` 加 `h-8`。

`CalendarDayColumn` 任务块 style：

```ts
left: `calc(${(mark.lane / lanes) * 100}% + ${CAL_BLOCK_INSET_LEFT}px)`,
width: `calc(${(span / lanes) * 100}% - ${CAL_BLOCK_INSET_LEFT + CAL_BLOCK_INSET_RIGHT + CAL_LANE_GAP}px)`,
```

- [ ] **Step 4: Run tests**

Run: `npx vitest run --dir src lib/taskCalendar`

Expected: PASS。再跑 `npx vitest run --dir src`。

- [ ] **Step 5: Commit**

```bash
git add src/lib/taskCalendar.ts src/lib/taskCalendar.test.ts src/pages/Tasks.tsx
git commit -m "$(cat <<'EOF'
feat: align calendar header gaps and widen blocks

EOF
)"
```

---

### Task 7: 详情与改期浅底卡片

**Files:**
- Modify: `src/components/ui/dialog.tsx`
- Modify: `src/components/TaskDetailDialog.tsx`
- Modify: `src/components/TaskDateDialog.tsx`

**Interfaces:**
- Consumes: 现有 `Dialog` `header` / `footer`
- Produces: `Dialog` 增加可选 `chrome?: "muted" | "plain"`，默认 `"muted"`。`"plain"` 时默认顶栏 / 底栏 **不要** `bg-muted/30`（改用 `bg-background`）。`TaskDetailDialog` 与 `TaskDateDialog` 用 `chrome="plain"`；名称 / 备注 / 时间放进 `rounded-xl border bg-card p-3`。放弃、添加分组 Dialog 不传 `chrome`（保持灰条）。

`DialogProps` 增加：

```ts
chrome?: "muted" | "plain";
```

顶栏 class：

```ts
cn(
  "flex items-start justify-between gap-4 border-b px-5 py-4",
  chrome === "plain" ? "bg-background" : "bg-muted/30",
)
```

底栏同样切换。自定义 `header` 槽位仍由调用方自己写 class。

`TaskDetailDialog` 自定义 header 现有 `border-b bg-muted/30 px-5 py-3` 改成 `border-b bg-background px-5 py-3`。勾选 + `TaskScheduleFields` 包在：

```tsx
<div className="rounded-xl border bg-card p-3">
  ... checkbox + TaskScheduleFields + 清除时段 ...
</div>
```

children 里名称、备注各一块 `rounded-xl border bg-card p-3`。`Dialog` 传 `chrome="plain"`。

`TaskDateDialog`：`chrome="plain"`，children：

```tsx
<div className="rounded-xl border bg-card p-3">
  <TaskScheduleFields form={form} setForm={setForm} disabled={busy} />
</div>
```

行为（保存、清除、不套娃）一字不改。

- [ ] **Step 1: Write the failing test**

无新纯函数。门：vitest 全绿。

- [ ] **Step 2: Run frontend tests**

Run: `npx vitest run --dir src`

Expected: PASS（改前基线）。

- [ ] **Step 3: Write minimal implementation**

按上面改三个文件。`Dialog` 解构默认 `chrome = "muted"`。

- [ ] **Step 4: Run tests**

Run: `npx vitest run --dir src`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/dialog.tsx src/components/TaskDetailDialog.tsx src/components/TaskDateDialog.tsx
git commit -m "$(cat <<'EOF'
feat: restyle task dialogs with card sections

EOF
)"
```

---

### Task 8: 文档 overlay

**Files:**
- Modify: `.cursor/rules/specs.mdc`（列表追加本 spec）
- Modify: `.cursor/rules/frontend-ui.mdc`（列表拖动空隙；页头统计；空白右键；日历 sticky；详情卡片）
- Modify: `AGENTS.md`（任务页页头与列表拖动一句）
- 规格已在 `docs/superpowers/specs/2026-09-16-task-board-polish-design.md`
- 本 plan：`docs/superpowers/plans/2026-09-16-task-board-polish.md`

**Interfaces:**
- Consumes: 已实现行为
- Produces: 规则与 spec overlay 一致；仍写明不引入 dnd-kit、备注不进判定、`user_version` 5

- [ ] **Step 1: 对照 spec 扫一遍实现**

确认：列表原位抽走 + 空隙 + 浮动卡片；拖到日历取消空隙；表头与列缝对齐；块 2/6；详情/改期浅底卡片；无三个点；空白右键；页头未完成/已排期/硬币能量。缺了就停，回到对应任务补。

- [ ] **Step 2: 改文档**

`specs.mdc` 在 2026-09-16 详情那条后追加：

```
- `2026-09-16-task-board-polish-design.md` — list drag gap + floating card; sticky calendar header; block inset 2px/6px; detail/date dialogs as background + cards; task header stats and wallet; add-group on list blank context menu.
```

`frontend-ui.mdc` 加：列表拖过 4px 原行抽走、目标插缝、浮动 `bg-card` 卡片；页头 `未完成 n · 今日已排期`，硬币能量在 3·7 天左侧；添加分组在列表空白右键；日历日期行与格子同一滚动容器。禁止 dnd-kit。

`AGENTS.md` 产品规则里任务页那句补上：列表拖排序要有空隙预览；页头不放三个点。

- [ ] **Step 3: 跑门禁**

```bash
npx vitest run --dir src
```

Expected: 全绿。未改 Rust 不必跑 cargo。若 Tasks 以外的 `src-tauri` 被误动，再跑 `/opt/homebrew/bin/cargo test --offline -p gamelife`。

- [ ] **Step 4: Commit**

```bash
git add AGENTS.md .cursor/rules/frontend-ui.mdc .cursor/rules/specs.mdc docs/superpowers/specs/2026-09-16-task-board-polish-design.md docs/superpowers/plans/2026-09-16-task-board-polish.md
git commit -m "$(cat <<'EOF'
docs: record task board polish overlay

EOF
)"
```

不要 add 通知崩溃修复或其它脏文件。规格若已在 Task 0 提交过，本步 `git add` 它也不会出错。

---

## Self-review

1. **Spec coverage:** §4 拖动 → T2/T5；§5 日历 → T6；§6 对话框 → T7；§7 页头 → T1/T3；§8 空白菜单 → T4；§3 不变量 / 文档 → T8；§11 测试 → T1/T2/T6。未做：日历空隙动画、多选整段拖、放弃对话框改卡片（spec §10）。
2. **Placeholders:** 无 TBD。`headerH` 固定 32 与 `h-8` 绑定。`chrome` 默认 `muted`。
3. **Types:** `listDragShown` 返回 `{ shown, gapIndex }`；`listDrag` 含 `rowHeight/pointerX/pointerY/title`；`Dialog.chrome` 为 `"muted" | "plain"`。T5 使用 T2 的函数名，T3 使用 T1 的四个导出。
