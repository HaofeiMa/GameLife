export type StatsSegment = "week" | "month" | "rhythm" | "app";

/** `getWeek()` has no historical anchor; only month / rhythm / app may page. */
export function weekRangePagingEnabled(segment: StatsSegment): boolean {
  return segment !== "week";
}

export function weekRangeLabel(
  byDay: { day: string }[],
  fallbackMonday: string,
  fallbackSunday: string,
): string {
  if (byDay.length >= 2) {
    return `${byDay[0].day} 至 ${byDay[byDay.length - 1].day}`;
  }
  return `${fallbackMonday} 至 ${fallbackSunday}`;
}

export function weekHasObservation(categoryMinutes: number): boolean {
  return categoryMinutes > 0;
}

export function dayStackCaption(weekend: boolean, stackMinutes: number): string {
  if (weekend) return "未采样";
  if (stackMinutes <= 0) return "—";
  return `${stackMinutes}m`;
}

export function monthShowsLedgerCards(hasObservation: boolean): boolean {
  return hasObservation;
}
