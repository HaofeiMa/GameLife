import { describe, expect, it } from "vitest";
import {
  formatScheduledDuration,
  overlapSeconds,
  scheduledSecondsToday,
  unfinishedCount,
} from "./taskHeaderStats";

describe("taskHeaderStats", () => {
  it("overlap is empty when the range misses the day", () => {
    const day = 1_000_000;
    expect(overlapSeconds(day - 3600, day, day)).toBe(0);
    expect(overlapSeconds(day + 86400, day + 90000, day)).toBe(0);
  });

  it("clips a range that crosses midnight to today only", () => {
    const day = 1_000_000;
    expect(overlapSeconds(day - 3600, day + 3600, day)).toBe(3600);
  });

  it("sums unfinished overlapping tasks", () => {
    const day = 1_000_000;
    expect(
      scheduledSecondsToday(
        [
          { done: false, start: day + 10 * 3600, end: day + 11 * 3600 },
          { done: true, start: day, end: day + 7200 },
          { done: false, start: null, end: null },
        ],
        day,
      ),
    ).toBe(3600);
  });

  it("counts unfinished including unscheduled", () => {
    expect(
      unfinishedCount([{ done: false }, { done: true }, { done: false }]),
    ).toBe(2);
  });

  it("formats duration per spec", () => {
    expect(formatScheduledDuration(0)).toBe("0 分钟");
    expect(formatScheduledDuration(1)).toBe("1 分钟");
    expect(formatScheduledDuration(59 * 60)).toBe("59 分钟");
    expect(formatScheduledDuration(3600)).toBe("1 小时");
    expect(formatScheduledDuration(5400)).toBe("1.5 小时");
    expect(formatScheduledDuration(7200)).toBe("2 小时");
  });
});
