import { addDays, dayStartUnix } from "./calendar";

export type CalSpan = 3 | 7;

export const CAL_HOUR_H = 36;
export const CAL_GUTTER = 40;

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

export function hitCalendarTs(
  days: string[],
  clientX: number,
  clientY: number,
  grid: { left: number; top: number; width: number; scrollTop: number },
): number | null {
  if (days.length === 0) return null;
  const x = clientX - grid.left;
  const y = clientY - grid.top + grid.scrollTop;
  if (x < 0 || x > grid.width) return null;
  const inner = grid.width - CAL_GUTTER;
  if (inner <= 0) return null;
  const col = Math.min(
    days.length - 1,
    Math.max(0, Math.floor((x - CAL_GUTTER) / (inner / days.length))),
  );
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
