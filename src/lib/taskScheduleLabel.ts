import { addDays, dayStartUnix } from "./calendar";
import { localDayOf, localTimeOf } from "./taskBoard";
import { mondayOf } from "./taskCalendar";

function clock(ts: number): string {
  const [h, m] = localTimeOf(ts).split(":").map(Number);
  const hour = Number.isFinite(h) ? h : 0;
  const minute = Number.isFinite(m) ? m : 0;
  const mm = String(minute).padStart(2, "0");
  if (hour >= 11 && hour < 13) return `中午 ${hour}:${mm}`;
  if (hour >= 18) return `晚上 ${hour}:${mm}`;
  if (hour >= 13) return `下午 ${hour}:${mm}`;
  return `上午 ${hour}:${mm}`;
}

function relativeDay(day: string, today: string): string | null {
  const diff = (dayStartUnix(day) - dayStartUnix(today)) / 86400;
  if (diff === 0) return "今天";
  if (diff === -1) return "昨天";
  if (diff === 1) return "明天";
  return null;
}

function monthDay(day: string, nowYear: number): string {
  const [y, m, d] = day.split("-").map(Number);
  const md = `${m}月${d}日`;
  return y !== nowYear ? `${y}年${md}` : md;
}

function dayChunk(day: string, today: string, nowYear: number): string {
  const rel = relativeDay(day, today);
  const md = monthDay(day, nowYear);
  return rel ? `${rel}，${md}` : md;
}

export function scheduleSummary(
  start: number | null,
  end: number | null,
  nowSec: number,
): string {
  if (start == null || end == null) return "未排期";
  const today = localDayOf(nowSec);
  const nowYear = new Date(nowSec * 1000).getFullYear();
  const startDay = localDayOf(start);
  const endDay = localDayOf(end);
  const t1 = clock(start);
  const t2 = clock(end);
  if (startDay === endDay) {
    return `${dayChunk(startDay, today, nowYear)}，${t1} – ${t2}`;
  }
  return `${dayChunk(startDay, today, nowYear)} ${t1} – ${dayChunk(endDay, today, nowYear)} ${t2}`;
}

export function scheduleOverdue(
  end: number | null,
  nowSec: number,
  done: boolean,
): boolean {
  if (done || end == null) return false;
  return end < nowSec;
}

export type ListScheduleTone = "muted" | "overdue" | "upcoming";

const WEEKDAY = ["一", "二", "三", "四", "五", "六", "日"] as const;

function weekdayName(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const wd = (new Date(y, m - 1, d).getDay() + 6) % 7;
  return WEEKDAY[wd] ?? "一";
}

function listDayLabel(day: string, today: string, nowYear: number): string {
  const diff = (dayStartUnix(day) - dayStartUnix(today)) / 86400;
  if (diff === 0) return "";
  if (diff === -1) return "昨天";
  if (diff === 1) return "明天";
  if (diff === 2) return "后天";
  const thisMonday = mondayOf(today);
  const thisSunday = addDays(thisMonday, 6);
  const nextSunday = addDays(thisMonday, 13);
  const name = weekdayName(day);
  if (day >= thisMonday && day <= thisSunday) return `周${name}`;
  if (day > thisSunday && day <= nextSunday) return `下周${name}`;
  return monthDay(day, nowYear);
}

export function listScheduleChip(
  start: number | null,
  end: number | null,
  nowSec: number,
  done: boolean,
): { label: string; tone: ListScheduleTone } {
  if (start == null || end == null) {
    return { label: "未排期", tone: "muted" };
  }
  const today = localDayOf(nowSec);
  const startDay = localDayOf(start);
  const nowYear = new Date(nowSec * 1000).getFullYear();
  const named = listDayLabel(startDay, today, nowYear);
  const label = named || clock(start);
  if (done) return { label, tone: "muted" };
  const overdue = end < nowSec || named === "昨天";
  return { label, tone: overdue ? "overdue" : "upcoming" };
}

export function repeatShortLabel(repeat: string): string | null {
  switch (repeat) {
    case "daily":
      return "每天";
    case "weekly":
      return "每周";
    case "monthly":
      return "每月";
    default:
      return null;
  }
}
