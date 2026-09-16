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

export function hourWashCategory(
  cells: (CategoryKey | null)[],
  hour: number,
): CategoryKey | null {
  const slice = cells.slice(hour * 4, hour * 4 + 4);
  const counted: CategoryKey[] = [];
  for (const cell of slice) {
    if (cell == null || cell === "unobserved") continue;
    counted.push(cell);
  }
  if (counted.length === 0) return null;
  const votes = new Map<CategoryKey, number>();
  for (const key of counted) {
    votes.set(key, (votes.get(key) ?? 0) + 1);
  }
  let max = 0;
  for (const n of votes.values()) max = Math.max(max, n);
  return counted.find((key) => votes.get(key) === max) ?? null;
}
