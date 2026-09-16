import { addDays, dayStartUnix } from "./calendar";
import { localDayOf, shiftRangeToDay } from "./taskBoard";
import { defaultRange } from "./taskSchedule";

export type QuickDateKind = "today" | "tomorrow" | "nextWeek";

const OFFSET: Record<QuickDateKind, number> = {
  today: 0,
  tomorrow: 1,
  nextWeek: 7,
};

export function quickDateRange(
  task: { start: number | null; end: number | null },
  nowSec: number,
  kind: QuickDateKind,
): { start: number; end: number } {
  const today = localDayOf(nowSec);
  const target = addDays(today, OFFSET[kind]);
  if (task.start != null && task.end != null) {
    return shiftRangeToDay(
      task.start,
      task.end,
      dayStartUnix(localDayOf(task.start)),
      dayStartUnix(target),
    );
  }
  const range = defaultRange(nowSec);
  return shiftRangeToDay(
    range.start,
    range.end,
    dayStartUnix(today),
    dayStartUnix(target),
  );
}
