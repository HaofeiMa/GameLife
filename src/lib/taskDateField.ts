export function formatDaySlash(day: string): string {
  const [y, m, d] = day.split("-");
  if (!y || !m || !d) return day;
  return `${y}/${m}/${d}`;
}

export function formatHmMeridiem(hm: string): string {
  const [h, m] = hm.split(":").map(Number);
  const hour = Number.isFinite(h) ? h : 0;
  const minute = Number.isFinite(m) ? m : 0;
  const hour12 = hour % 12 === 0 ? 12 : hour % 12;
  const period = hour < 12 ? "上午" : "下午";
  return `${period} ${hour12}:${String(minute).padStart(2, "0")}`;
}

export function remindItemLabel(offset: number): string {
  if (offset === 0) return "准时";
  if (offset === 60) return "提前 1 小时";
  if (offset === 1440) return "提前 1 天";
  return `提前 ${offset} 分钟`;
}

export function remindSummary(offsets: number[]): string {
  if (offsets.length === 0) return "无";
  return [...offsets]
    .sort((a, b) => a - b)
    .map(remindItemLabel)
    .join("、");
}

export function repeatItemLabel(repeat: string): string {
  switch (repeat) {
    case "daily":
      return "每天";
    case "weekly":
      return "每周";
    case "monthly":
      return "每月";
    default:
      return "无";
  }
}

export function monthMatrix(
  year: number,
  month: number,
): { iso: string; inMonth: boolean; date: number }[] {
  const first = new Date(year, month - 1, 1);
  const pad = (first.getDay() + 6) % 7;
  const start = new Date(year, month - 1, 1 - pad);
  const cells: { iso: string; inMonth: boolean; date: number }[] = [];
  for (let i = 0; i < 42; i++) {
    const dt = new Date(start.getFullYear(), start.getMonth(), start.getDate() + i);
    const y = dt.getFullYear();
    const m = dt.getMonth() + 1;
    const d = dt.getDate();
    cells.push({
      iso: `${y}-${String(m).padStart(2, "0")}-${String(d).padStart(2, "0")}`,
      inMonth: y === year && m === month,
      date: d,
    });
  }
  while (cells.length > 35 && cells.slice(-7).every((c) => !c.inMonth)) {
    cells.splice(-7);
  }
  return cells;
}

export type ScheduleDayLock = {
  startTouched: boolean;
  endTouched: boolean;
};

export function initialDayLock(startDay: string, endDay: string): ScheduleDayLock {
  const linked = startDay === endDay;
  return { startTouched: !linked, endTouched: !linked };
}

export function applyScheduleDay(
  form: { startDay: string; endDay: string },
  lock: ScheduleDayLock,
  which: "start" | "end",
  day: string,
): { startDay: string; endDay: string; lock: ScheduleDayLock } {
  if (which === "start") {
    return {
      startDay: day,
      endDay: lock.endTouched ? form.endDay : day,
      lock: { ...lock, startTouched: true },
    };
  }
  return {
    startDay: lock.startTouched ? form.startDay : day,
    endDay: day,
    lock: { ...lock, endTouched: true },
  };
}

export function shiftMonth(
  year: number,
  month: number,
  delta: number,
): { year: number; month: number } {
  const dt = new Date(year, month - 1 + delta, 1);
  return { year: dt.getFullYear(), month: dt.getMonth() + 1 };
}
