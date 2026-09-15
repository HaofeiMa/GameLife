import type { DayTask } from "./api";
import type { PlanBlock } from "./calendar";

export function splitTimedPlan(tasks: DayTask[]): DayTask[] {
  return tasks.filter((task) => task.end > task.start);
}

/** Right-hand actual column width. Geometry, stored in localStorage. */
export const TIMELINE_ACTUAL_KEY = "gl-timeline-actual-width";
/** Wide enough for 「主线」; dragging never switches layout. */
export const ACTUAL_MIN = 56;
/** Enough for the label, leaves the rest for long titles. */
export const ACTUAL_DEFAULT = 84;
export const ACTUAL_MAX = 220;
/** Old overlay layout lived at or below this; treat as unset. */
const LEGACY_OVERLAY_MAX = 40;

export function clampActualWidth(value: number): number {
  if (!Number.isFinite(value)) return ACTUAL_DEFAULT;
  return Math.min(ACTUAL_MAX, Math.max(ACTUAL_MIN, Math.round(value)));
}

export function readStoredActualWidth(
  storage: Pick<Storage, "getItem">,
): number {
  try {
    const raw = storage.getItem(TIMELINE_ACTUAL_KEY);
    const parsed = raw == null ? NaN : Number(raw);
    if (!Number.isFinite(parsed)) return ACTUAL_DEFAULT;
    if (parsed <= LEGACY_OVERLAY_MAX) return ACTUAL_DEFAULT;
    return clampActualWidth(parsed);
  } catch {
    return ACTUAL_DEFAULT;
  }
}

export function writeStoredActualWidth(
  storage: Pick<Storage, "setItem">,
  value: number,
): void {
  try {
    storage.setItem(TIMELINE_ACTUAL_KEY, String(clampActualWidth(value)));
  } catch {
    /* quota / private mode */
  }
}

export type PlanLaneItem = PlanBlock & { lane: number };

export function assignPlanLanes(blocks: PlanBlock[]): {
  items: PlanLaneItem[];
  laneCount: number;
} {
  const sorted = blocks
    .slice()
    .sort((a, b) => a.rowStart - b.rowStart || b.rowSpan - a.rowSpan);
  const laneEnds: number[] = [];
  const items: PlanLaneItem[] = [];
  for (const block of sorted) {
    let lane = laneEnds.findIndex((end) => end <= block.rowStart);
    if (lane < 0) {
      lane = laneEnds.length;
      laneEnds.push(0);
    }
    laneEnds[lane] = block.rowStart + block.rowSpan;
    items.push({ ...block, lane });
  }
  return { items, laneCount: laneEnds.length };
}

function rowsOverlap(a: PlanLaneItem, b: PlanLaneItem): boolean {
  return (
    a.rowStart < b.rowStart + b.rowSpan && b.rowStart < a.rowStart + a.rowSpan
  );
}

/** How many consecutive empty lanes to the right this block can occupy. */
export function planLaneSpan(
  item: PlanLaneItem,
  items: PlanLaneItem[],
  laneCount: number,
): number {
  let span = 1;
  for (let lane = item.lane + 1; lane < laneCount; lane++) {
    const taken = items.some(
      (other) => other.lane === lane && rowsOverlap(item, other),
    );
    if (taken) break;
    span += 1;
  }
  return span;
}
