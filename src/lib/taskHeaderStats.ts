const DAY = 86400;

export function overlapSeconds(start: number, end: number, dayStart: number): number {
  if (end <= start) return 0;
  const lo = Math.max(start, dayStart);
  const hi = Math.min(end, dayStart + DAY);
  return Math.max(0, hi - lo);
}

export function scheduledSecondsToday(
  tasks: { done: boolean; start: number | null; end: number | null }[],
  dayStart: number,
): number {
  let sum = 0;
  for (const task of tasks) {
    if (task.done || task.start == null || task.end == null) continue;
    sum += overlapSeconds(task.start, task.end, dayStart);
  }
  return sum;
}

export function unfinishedCount(tasks: { done: boolean }[]): number {
  return tasks.reduce((n, task) => n + (task.done ? 0 : 1), 0);
}

export function formatScheduledDuration(secs: number): string {
  const s = Math.max(0, secs);
  if (s === 0) return "0 分钟";
  if (s < 3600) return `${Math.ceil(s / 60)} 分钟`;
  const hours = (s / 3600).toFixed(1).replace(/\.0$/, "");
  return `${hours} 小时`;
}
