import { categoryOf, type CategoryKey } from "./theme";

export function dominantToCategory(dominant: string, pending: boolean): CategoryKey {
  if (pending) return "pending";
  return categoryOf(dominant);
}

export function ribbonCells(
  slots: { start: number; dominant: string; pending: boolean }[],
  dayStart: number,
): (CategoryKey | null)[] {
  const cells: (CategoryKey | null)[] = Array.from({ length: 96 }, () => null);
  for (const slot of slots) {
    const i = Math.floor((slot.start - dayStart) / 900);
    if (i < 0 || i >= 96) continue;
    cells[i] = dominantToCategory(slot.dominant, slot.pending);
  }
  return cells;
}
