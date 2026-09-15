import { unixAt as unixAtDay } from "./taskBoard";

export const TIME_STEPS: string[] = Array.from({ length: 96 }, (_, i) => {
  const h = Math.floor(i / 4);
  const m = (i % 4) * 15;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
});

export const ALLOWED_REMIND_OFFSETS = [0, 5, 15, 30, 60] as const;

export function unixAt(dayIso: string, hhmm: string): number {
  return unixAtDay(dayIso, hhmm);
}

export function defaultRange(nowSec: number): { start: number; end: number } {
  const start = Math.floor(nowSec / 900) * 900;
  return { start, end: start + 1800 };
}

export function alignRange(start: number, end: number): { start: number; end: number } {
  const alignedStart = Math.floor(start / 900) * 900;
  let alignedEnd = Math.floor((end + 899) / 900) * 900;
  if (alignedEnd <= alignedStart) {
    alignedEnd = alignedStart + 900;
  }
  return { start: alignedStart, end: alignedEnd };
}

export function remindToggle(offsets: number[], offset: number): number[] {
  if (!(ALLOWED_REMIND_OFFSETS as readonly number[]).includes(offset)) {
    return offsets;
  }
  if (offsets.includes(offset)) {
    return offsets.filter((n) => n !== offset);
  }
  return [...offsets, offset];
}
