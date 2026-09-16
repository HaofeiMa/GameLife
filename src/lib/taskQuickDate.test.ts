import { describe, expect, it } from "vitest";
import { unixAt } from "./taskBoard";
import { quickDateRange } from "./taskQuickDate";

const now = unixAt("2026-09-16", "14:07");

describe("quickDateRange", () => {
  it("keeps the clock when a timed task moves to today", () => {
    expect(
      quickDateRange(
        {
          start: unixAt("2026-09-10", "09:00"),
          end: unixAt("2026-09-10", "10:30"),
        },
        now,
        "today",
      ),
    ).toEqual({
      start: unixAt("2026-09-16", "09:00"),
      end: unixAt("2026-09-16", "10:30"),
    });
  });

  it("moves a timed task to tomorrow and next week from today", () => {
    const task = {
      start: unixAt("2026-09-10", "09:00"),
      end: unixAt("2026-09-10", "10:30"),
    };
    expect(quickDateRange(task, now, "tomorrow")).toEqual({
      start: unixAt("2026-09-17", "09:00"),
      end: unixAt("2026-09-17", "10:30"),
    });
    expect(quickDateRange(task, now, "nextWeek")).toEqual({
      start: unixAt("2026-09-23", "09:00"),
      end: unixAt("2026-09-23", "10:30"),
    });
  });

  it("keeps a multi-day span when shifting", () => {
    expect(
      quickDateRange(
        {
          start: unixAt("2026-09-10", "22:00"),
          end: unixAt("2026-09-11", "01:00"),
        },
        now,
        "today",
      ),
    ).toEqual({
      start: unixAt("2026-09-16", "22:00"),
      end: unixAt("2026-09-17", "01:00"),
    });
  });

  it("gives an unscheduled task the default 30-minute block on the target day", () => {
    const unset = { start: null, end: null };
    expect(quickDateRange(unset, now, "today")).toEqual({
      start: unixAt("2026-09-16", "14:00"),
      end: unixAt("2026-09-16", "14:30"),
    });
    expect(quickDateRange(unset, now, "tomorrow")).toEqual({
      start: unixAt("2026-09-17", "14:00"),
      end: unixAt("2026-09-17", "14:30"),
    });
    expect(quickDateRange(unset, now, "nextWeek")).toEqual({
      start: unixAt("2026-09-23", "14:00"),
      end: unixAt("2026-09-23", "14:30"),
    });
  });
});
