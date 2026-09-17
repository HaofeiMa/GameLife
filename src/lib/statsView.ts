import { CATEGORY_LABELS, categoryOf } from "./theme";

export type StatsSegment = "week" | "month" | "rhythm" | "app";

export function weekRangeLabel(
  byDay: { day: string }[],
  fallbackMonday: string,
  fallbackSunday: string,
): string {
  // The label comes from byDay's own span, never from the paged anchor.
  const start = byDay.length >= 2 ? byDay[0]!.day : fallbackMonday;
  const end = byDay.length >= 2 ? byDay[byDay.length - 1]!.day : fallbackSunday;
  // The mockup writes the end short when it shares the start's year:
  // "2026-09-07 至 09-13".
  return `${start} 至 ${shortEnd(end, start)}`;
}

/** Drops a shared leading year: ("2026-09-07", "2026-09-13") → "09-13". */
function shortEnd(end: string, start: string): string {
  return end.slice(0, 4) === start.slice(0, 4) ? end.slice(5) : end;
}

export function weekHasObservation(categoryMinutes: number): boolean {
  return categoryMinutes > 0;
}

export function dayStackCaption(_weekend: boolean, stackMinutes: number): string {
  if (stackMinutes <= 0) return "—";
  return `${stackMinutes}m`;
}

export function monthCellNote(isFuture: boolean, coreMinutes: number): string {
  if (isFuture) return "未来";
  if (coreMinutes > 0) return `${coreMinutes}m`;
  return "无观测";
}

export function monthShowsLedgerCards(hasObservation: boolean): boolean {
  return hasObservation;
}

export const MONTH_CALENDAR_MODE_KEY = "gl-month-calendar-mode";
export type MonthCalendarMode = "heat" | "detail";

export function readMonthCalendarMode(
  storage: Pick<Storage, "getItem">,
): MonthCalendarMode {
  try {
    return storage.getItem(MONTH_CALENDAR_MODE_KEY) === "detail"
      ? "detail"
      : "heat";
  } catch {
    return "heat";
  }
}

export function writeMonthCalendarMode(
  storage: Pick<Storage, "setItem">,
  mode: MonthCalendarMode,
): void {
  try {
    storage.setItem(MONTH_CALENDAR_MODE_KEY, mode);
  } catch {
    /* quota / private mode */
  }
}

export function hourCellTitle(
  day: string,
  hour: number,
  category: string | null,
): string {
  const clock = `${hour}:00`;
  if (!category) return `${day} · ${clock}`;
  return `${day} · ${clock} ${CATEGORY_LABELS[categoryOf(category)]}`;
}

/** Hour ticks on the start-time chart. 0 sits at the top, 24 at the bottom. */
export const START_HOUR_Y_TICKS = [0, 6, 12, 18, 24] as const;

/** Offset from the top of a 24-hour column. Midnight is 0, end-of-day is 100. */
export function startHourYPercent(hour: number): number {
  return (hour / 24) * 100;
}

export function startHourBandBox(
  startHour: number,
  endHour: number,
): { top: number; height: number } {
  const top = startHourYPercent(startHour);
  const height = startHourYPercent(endHour) - top;
  return { top, height };
}

/**
 * Date labels under the start-hour chart. A week prints every MM-DD; a month
 * of weekdays keeps about five so the axis cannot collide with itself.
 */
export function startHourXLabel(
  day: string,
  index: number,
  count: number,
): string | null {
  const label = day.slice(5);
  if (count <= 9) return label;
  if (index === 0 || index === count - 1) return label;
  const step = Math.max(1, Math.round((count - 1) / 4));
  if (index % step === 0 && count - 1 - index >= Math.ceil(step / 2)) {
    return label;
  }
  return null;
}
