import { addDays, dayStartUnix } from "./calendar";
import { alignRange } from "./taskSchedule";

export type CalSpan = 3 | 7;
export type CalEdge = "start" | "end";

export const CAL_HOUR_H = 36;
export const CAL_GUTTER = 40;
export const CAL_DAY_GAP = 10;
export const CAL_BLOCK_INSET_LEFT = 2;
export const CAL_BLOCK_INSET_RIGHT = 6;
export const CAL_LANE_GAP = 4;

export function mondayOf(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const dt = new Date(y, m - 1, d);
  const wd = (dt.getDay() + 6) % 7;
  return addDays(day, -wd);
}

export function calendarDays(today: string, span: CalSpan): string[] {
  if (span === 3) {
    return [today, addDays(today, 1), addDays(today, 2)];
  }
  const monday = mondayOf(today);
  return Array.from({ length: 7 }, (_, i) => addDays(monday, i));
}

export function dropRange(dropTs: number): { start: number; end: number } {
  const start = Math.floor(dropTs / 900) * 900;
  let end = start + 1800;
  if (end <= start) end = start + 900;
  return { start, end };
}

export function moveRangeToDrop(
  start: number,
  end: number,
  dropTs: number,
  grabOffset: number,
): { start: number; end: number } {
  const dur = Math.max(900, end - start);
  const next = Math.floor((dropTs - grabOffset) / 900) * 900;
  return { start: next, end: next + dur };
}

export function resizeRange(
  start: number,
  end: number,
  edge: CalEdge,
  dropTs: number,
): { start: number; end: number } {
  const at = Math.floor(dropTs / 900) * 900;
  if (edge === "start") {
    return alignRange(Math.min(at, end - 900), end);
  }
  return alignRange(start, Math.max(at, start + 900));
}

export function hitCalendarTs(
  days: string[],
  clientX: number,
  clientY: number,
  grid: { left: number; top: number; width: number; scrollTop: number },
  headerH = 0,
): number | null {
  if (days.length === 0) return null;
  const x = clientX - grid.left;
  const y = clientY - grid.top + grid.scrollTop - headerH;
  if (x < 0 || x > grid.width || y < 0) return null;
  const n = days.length;
  const inner = grid.width - CAL_GUTTER - CAL_DAY_GAP * (n - 1);
  if (inner <= 0) return null;
  const colW = inner / n;
  let pos = x - CAL_GUTTER;
  let col = n - 1;
  for (let i = 0; i < n; i++) {
    if (pos < colW) {
      col = i;
      break;
    }
    pos -= colW;
    if (i < n - 1) {
      if (pos < CAL_DAY_GAP) {
        col = pos < CAL_DAY_GAP / 2 ? i : Math.min(n - 1, i + 1);
        break;
      }
      pos -= CAL_DAY_GAP;
    }
  }
  const day = days[col];
  if (!day) return null;
  const minutes = (y / CAL_HOUR_H) * 60;
  const clamped = Math.max(0, Math.min(24 * 60 - 1, minutes));
  return dayStartUnix(day) + clamped * 60;
}

export function dayColumnLabel(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const dt = new Date(y, m - 1, d);
  const weeks = ["日", "一", "二", "三", "四", "五", "六"];
  return `周${weeks[dt.getDay()]} ${m}/${d}`;
}
