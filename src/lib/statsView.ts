export type StatsSegment = "week" | "month" | "rhythm" | "app";

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
