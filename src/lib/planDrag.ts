import { resizeRange, type CalEdge } from "./taskCalendar";

export function moveSameDay(
  start: number,
  end: number,
  dayStart: number,
  grabDeltaSlots: number,
): { start: number; end: number } {
  const dur = end - start;
  let next = Math.round((start + grabDeltaSlots * 900) / 900) * 900;
  const min = dayStart;
  const max = dayStart + 96 * 900 - dur;
  if (next < min) next = min;
  if (next > max) next = max;
  return { start: next, end: next + dur };
}

export function resizeSameDay(
  start: number,
  end: number,
  edge: CalEdge,
  dropTs: number,
  dayStart: number,
): { start: number; end: number } {
  const resized = resizeRange(start, end, edge, dropTs);
  const dayEnd = dayStart + 96 * 900;
  let nextStart = Math.max(dayStart, resized.start);
  let nextEnd = Math.min(dayEnd, resized.end);
  if (nextEnd - nextStart < 900) {
    if (edge === "end") {
      nextEnd = Math.min(dayEnd, nextStart + 900);
      nextStart = nextEnd - 900;
    } else {
      nextStart = Math.max(dayStart, nextEnd - 900);
      nextEnd = nextStart + 900;
    }
  }
  return { start: nextStart, end: nextEnd };
}
