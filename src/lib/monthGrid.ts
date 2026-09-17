/** Sunday-first headers for the month heat calendar. */
export const MONTH_WEEKDAY_HEADERS = ["日", "一", "二", "三", "四", "五", "六"] as const;

export function calendarCells(
  year: number,
  month: number,
): { day: string | null; weekday: number }[] {
  const first = new Date(year, month - 1, 1);
  const pad = first.getDay();
  const cells: { day: string | null; weekday: number }[] = [];
  for (let i = 0; i < pad; i++) {
    const dt = new Date(year, month - 1, 1 - pad + i);
    cells.push({ day: null, weekday: dt.getDay() });
  }
  const lastDate = new Date(year, month, 0).getDate();
  const mm = String(month).padStart(2, "0");
  for (let d = 1; d <= lastDate; d++) {
    const dt = new Date(year, month - 1, d);
    const dd = String(d).padStart(2, "0");
    cells.push({ day: `${year}-${mm}-${dd}`, weekday: dt.getDay() });
  }
  let trail = 1;
  while (cells.length % 7 !== 0) {
    const dt = new Date(year, month, trail++);
    cells.push({ day: null, weekday: dt.getDay() });
  }
  return cells;
}

/** creditedCore is seconds; 8h gold day = 28800. */
export function monthHeatCell(creditedCoreSeconds: number): number {
  return Math.min(1, Math.max(0, creditedCoreSeconds / 28800));
}

/** 5×5 day grid: hours 0–23 then a blank cell. Empty hours are null, not unobserved. */
export function detailHourCells(hours: readonly string[]): (string | null)[] {
  const cells: (string | null)[] = Array.from({ length: 25 }, () => null);
  for (let i = 0; i < 24; i++) {
    const cat = hours[i] ?? "";
    cells[i] = cat === "" ? null : cat;
  }
  return cells;
}

/** Last cell / future days stay blank; hours with no slots still occupy a plate. */
export function hourChipKind(
  index: number,
  category: string | null,
  future: boolean,
): "none" | "empty" | "category" {
  if (future || index >= 24) return "none";
  return category ? "category" : "empty";
}
