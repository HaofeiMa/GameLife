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
