export const TICKTICK_ROLE_COLUMNS = [
  ["mainline", "主线"],
  ["side", "支线"],
  ["longterm", "长期"],
  ["chore", "杂项"],
  ["ignore", "忽略"],
] as const;

export type TickTickRoleId = (typeof TICKTICK_ROLE_COLUMNS)[number][0];

export function normalizeProjectRole(role: string): TickTickRoleId {
  switch (role) {
    case "mainline":
    case "side":
    case "longterm":
    case "chore":
    case "ignore":
      return role;
    default:
      return "ignore";
  }
}

/**
 * Column roles are stored under `"{projectId}:{columnId}"` — the same key
 * shape the backend uses, so a column in one project never collides with a
 * same-named column in another.
 */
export function columnRoleKey(projectId: string, columnId: string): string {
  return `${projectId}:${columnId}`;
}

/**
 * Writing "ignore" removes the key instead of storing it: an absent entry
 * and an explicit ignore mean the same thing to the judge, and a smaller
 * map is easier to read.
 */
export function nextRoleMap(
  current: Record<string, string>,
  key: string,
  role: string,
): Record<string, string> {
  const next = { ...current };
  if (role === "ignore") {
    delete next[key];
  } else {
    next[key] = role;
  }
  return next;
}

export function ticktickRoleLabel(role: string): string {
  return TICKTICK_ROLE_COLUMNS.find(([id]) => id === role)?.[1] ?? "忽略";
}

export function ticktickSyncButtonLabel(syncing: boolean): string {
  return syncing ? "同步中…" : "同步任务";
}

export function ticktickSyncErrorMessage(err: string): string {
  const raw = err.toLowerCase();
  if (raw.includes("ticktick_backoff")) return "同步过于频繁，请稍后再试";
  if (raw.includes("ticktick_429")) return "TickTick 限流，请一分钟后再试";
  if (raw.includes("timeout") || raw.includes("timed out")) {
    return "同步超时，已保留能拉到的清单。请再试一次";
  }
  return err;
}

export function formatTicktickLastSync(
  lastSync: number | null,
  _now: number,
): string {
  if (lastSync == null || lastSync <= 0) return "尚未同步任务";
  const time = new Date(lastSync * 1000).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
  return `上次同步 ${time}`;
}
