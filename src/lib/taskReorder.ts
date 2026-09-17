import { hasSchedule } from "./taskSort";

export function unscheduledBeforeId(
  dest: { id: string; start: number | null; end: number | null }[],
  dragId: string,
  hitBefore: string | null,
): string | null {
  const rest = dest.filter((task) => task.id !== dragId && !hasSchedule(task));
  if (hitBefore == null) return null;
  const hit = dest.find((task) => task.id === hitBefore);
  if (!hit || hasSchedule(hit)) return rest[0]?.id ?? null;
  return hitBefore;
}

export function ranksAfterDrag(
  ids: string[],
  dragId: string,
  beforeId: string | null,
): { id: string; sort: number }[] {
  const rest = ids.filter((id) => id !== dragId);
  let at = beforeId == null ? rest.length : rest.indexOf(beforeId);
  if (at < 0) at = rest.length;
  rest.splice(at, 0, dragId);
  return rest.map((id, i) => ({ id, sort: i * 10 }));
}

export function listDragShown(
  destIds: string[],
  dragId: string,
  beforeId: string | null,
): { shown: string[]; gapIndex: number } {
  const shown = destIds.filter((id) => id !== dragId);
  let gapIndex = beforeId == null ? shown.length : shown.indexOf(beforeId);
  if (gapIndex < 0) gapIndex = shown.length;
  return { shown, gapIndex };
}
