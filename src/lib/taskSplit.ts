export const TASK_SPLIT_KEY = "gl-task-list-width";
export const LIST_MIN = 240;
export const LIST_DEFAULT = 320;
export const LIST_MAX = 520;

export function clampListWidth(value: number): number {
  if (!Number.isFinite(value)) return LIST_DEFAULT;
  return Math.min(LIST_MAX, Math.max(LIST_MIN, Math.round(value)));
}

export function readStoredListWidth(
  storage: Pick<Storage, "getItem">,
): number {
  try {
    const raw = storage.getItem(TASK_SPLIT_KEY);
    const parsed = raw == null ? NaN : Number(raw);
    return Number.isFinite(parsed) ? clampListWidth(parsed) : LIST_DEFAULT;
  } catch {
    return LIST_DEFAULT;
  }
}

export function writeStoredListWidth(
  storage: Pick<Storage, "setItem">,
  value: number,
): void {
  try {
    storage.setItem(TASK_SPLIT_KEY, String(clampListWidth(value)));
  } catch {
    /* quota / private mode */
  }
}
