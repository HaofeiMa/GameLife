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
