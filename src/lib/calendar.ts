export function slotIndex(ts: number, dayStart: number): number {
  return Math.floor((ts - dayStart) / 900);
}

export type PlanTask = {
  start: number;
  end: number;
  role: string;
  title: string;
};

export type PlanBlock = PlanTask & {
  rowStart: number;
  rowSpan: number;
};

export function planBlocks(tasks: PlanTask[], dayStart: number): PlanBlock[] {
  return tasks
    .filter((t) => t.end > t.start)
    .map((t) => {
      const rowStart = Math.max(0, slotIndex(t.start, dayStart));
      const rowEnd = Math.max(rowStart + 1, Math.ceil((t.end - dayStart) / 900));
      return {
        ...t,
        rowStart,
        rowSpan: Math.min(96 - rowStart, rowEnd - rowStart),
      };
    })
    .filter((b) => b.rowStart < 96 && b.rowSpan > 0);
}

export function weekdayLabel(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  if (!y || !m || !d) return day;
  const date = new Date(y, m - 1, d);
  const weeks = ["日", "一", "二", "三", "四", "五", "六"];
  return `${y}年${m}月${d}日 周${weeks[date.getDay()]}`;
}

export function addDays(day: string, delta: number): string {
  const [y, m, d] = day.split("-").map(Number);
  const date = new Date(y, m - 1, d + delta);
  const mm = String(date.getMonth() + 1).padStart(2, "0");
  const dd = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${mm}-${dd}`;
}

export function dayStartUnix(day: string): number {
  const [y, m, d] = day.split("-").map(Number);
  return Math.floor(new Date(y, m - 1, d).getTime() / 1000);
}

export function roleClass(role: string): string {
  switch (role) {
    case "mainline":
    case "core_research":
      return "role-mainline";
    case "side":
    case "side_project":
    case "custom":
    case "longterm":
      return role === "longterm" ? "role-longterm" : "role-side";
    case "chore":
    case "admin":
      return "role-chore";
    case "distraction":
      return "role-play";
    case "pending_review":
      return "role-pending";
    default:
      return "role-muted";
  }
}

export function dominantLabel(dominant: string): string {
  switch (dominant) {
    case "core_research":
      return "主线";
    case "research_support":
      return "辅助";
    case "admin":
      return "杂项";
    case "side_project":
      return "支线";
    case "distraction":
      return "娱乐";
    case "break_away":
      return "离开";
    case "unobserved":
      return "未观测";
    case "pending_review":
      return "待复核";
    default:
      return "未知";
  }
}
