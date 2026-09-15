import type { TaskView } from "./api";

const PREFIX = "gamelife/task-copy+json:";

let lastCopy: string | null = null;

function isTaskView(value: unknown): value is TaskView {
  if (!value || typeof value !== "object") return false;
  const row = value as Record<string, unknown>;
  return typeof row.id === "string" && typeof row.title === "string" && typeof row.listId === "string";
}

export function serializeTaskCopy(task: TaskView): string {
  const raw = `${PREFIX}${JSON.stringify(task)}`;
  lastCopy = raw;
  return raw;
}

export function parseTaskCopy(raw: string): TaskView | null {
  if (!raw.startsWith(PREFIX)) return null;
  try {
    const parsed: unknown = JSON.parse(raw.slice(PREFIX.length));
    return isTaskView(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

export function lastCopiedPayload(): string | null {
  return lastCopy;
}
