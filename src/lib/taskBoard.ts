import { dayStartUnix } from "./calendar";
import { rejectedCode } from "./feel";

export const COLLAPSED_KEY = "gl-task-collapsed";

const PRESET_LIST_IDS = [
  "list-mainline",
  "list-side",
  "list-chore",
  "list-longterm",
] as const;

export function listRoleLabel(role: string): string {
  switch (role) {
    case "mainline":
      return "主线";
    case "side":
      return "支线";
    case "longterm":
      return "长期";
    case "chore":
      return "杂项";
    default:
      return "支线";
  }
}

export function taskTimeLabel(start: number, end: number): string {
  const fmt = (ts: number) =>
    new Date(ts * 1000).toLocaleTimeString("zh-CN", {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
  return `${fmt(start)}–${fmt(end)}`;
}

export function isPresetListId(id: string): boolean {
  return (PRESET_LIST_IDS as readonly string[]).includes(id);
}

export function parseCollapsed(raw: string | null): string[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((id): id is string => typeof id === "string");
  } catch {
    return [];
  }
}

export function toggleCollapsed(ids: string[], id: string): string[] {
  return ids.includes(id) ? ids.filter((item) => item !== id) : [...ids, id];
}

export function localDayOf(ts: number): string {
  const d = new Date(ts * 1000);
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${mm}-${dd}`;
}

export function localTimeOf(ts: number): string {
  const d = new Date(ts * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

export function unixAt(day: string, hm: string): number {
  const [h, min] = hm.split(":").map(Number);
  const hours = Number.isFinite(h) ? h : 0;
  const minutes = Number.isFinite(min) ? min : 0;
  return dayStartUnix(day) + hours * 3600 + minutes * 60;
}

export function shiftRangeToDay(
  start: number,
  end: number,
  oldDayStart: number,
  newDayStart: number,
): { start: number; end: number } {
  const delta = newDayStart - oldDayStart;
  return { start: start + delta, end: end + delta };
}

export function taskCommandError(err: unknown): string {
  const code = rejectedCode(err);
  switch (code) {
    case "too_many_judgment_tasks":
      return "当天已排期任务超过 20，请先完成、改期或放弃。";
    case "empty_title":
      return "标题不能为空。";
    case "empty_name":
      return "名称不能为空。";
    case "list_not_empty":
      return "分组里还有任务，无法删除。";
    case "preset_locked":
      return "预置分组不能删除。";
    case "no_mainline":
      return "至少保留一个主线分组。";
    case "invalid_role":
      return "请选择分组角色。";
    case "need_start_and_end":
      return "开始和结束时间要一起填。";
    default:
      return code;
  }
}
