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
