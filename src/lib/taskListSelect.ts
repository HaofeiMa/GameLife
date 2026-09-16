export function visibleTaskIds(
  listIds: string[],
  tasksByList: Map<string, { id: string }[]>,
  collapsed: string[],
): string[] {
  const ids: string[] = [];
  for (const listId of listIds) {
    if (collapsed.includes(listId)) continue;
    for (const task of tasksByList.get(listId) ?? []) {
      ids.push(task.id);
    }
  }
  return ids;
}

export function rangeSelect(
  visible: string[],
  anchorId: string | null,
  targetId: string,
): string[] {
  if (anchorId == null) return [targetId];
  const from = visible.indexOf(anchorId);
  const to = visible.indexOf(targetId);
  if (from < 0 || to < 0) return [targetId];
  const start = Math.min(from, to);
  const end = Math.max(from, to);
  return visible.slice(start, end + 1);
}

export function toggleSelect(ids: string[], id: string): string[] {
  return ids.includes(id) ? ids.filter((item) => item !== id) : [...ids, id];
}
