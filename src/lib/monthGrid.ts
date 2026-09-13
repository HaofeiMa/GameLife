export function calendarCells(
  year: number,
  month: number,
): { day: string | null; weekday: number }[] {
  const first = new Date(year, month - 1, 1);
  const pad = (first.getDay() + 6) % 7;
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
