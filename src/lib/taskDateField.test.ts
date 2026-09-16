import { describe, expect, it } from "vitest";
import {
  applyScheduleDay,
  formatDaySlash,
  formatHmMeridiem,
  initialDayLock,
  monthMatrix,
  remindItemLabel,
  remindSummary,
  repeatItemLabel,
  shiftMonth,
} from "./taskDateField";

describe("formatDaySlash", () => {
  it("keeps year, month and day visible", () => {
    expect(formatDaySlash("2026-09-02")).toBe("2026/09/02");
    expect(formatDaySlash("2026-09-16")).toBe("2026/09/16");
  });
});

describe("formatHmMeridiem", () => {
  it("uses 上午 / 下午 without dropping the hour", () => {
    expect(formatHmMeridiem("00:00")).toBe("上午 12:00");
    expect(formatHmMeridiem("10:00")).toBe("上午 10:00");
    expect(formatHmMeridiem("12:00")).toBe("下午 12:00");
    expect(formatHmMeridiem("16:00")).toBe("下午 4:00");
    expect(formatHmMeridiem("17:30")).toBe("下午 5:30");
  });
});

describe("remindSummary", () => {
  it("labels offsets the way the date card lists them", () => {
    expect(remindItemLabel(0)).toBe("准时");
    expect(remindItemLabel(5)).toBe("提前 5 分钟");
    expect(remindItemLabel(60)).toBe("提前 1 小时");
  });

  it("joins selected reminders, or 无", () => {
    expect(remindSummary([])).toBe("无");
    expect(remindSummary([5])).toBe("提前 5 分钟");
    expect(remindSummary([60, 0])).toBe("准时、提前 1 小时");
  });
});

describe("repeatItemLabel", () => {
  it("covers the four engine keys", () => {
    expect(repeatItemLabel("none")).toBe("无");
    expect(repeatItemLabel("daily")).toBe("每天");
    expect(repeatItemLabel("weekly")).toBe("每周");
    expect(repeatItemLabel("monthly")).toBe("每月");
  });
});

describe("monthMatrix", () => {
  it("starts weeks on Monday and includes adjacent-month days", () => {
    const cells = monthMatrix(2026, 9);
    expect(cells.length % 7).toBe(0);
    expect(cells[0]).toEqual({
      iso: "2026-08-31",
      inMonth: false,
      date: 31,
    });
    const fifteenth = cells.find((c) => c.iso === "2026-09-15");
    expect(fifteenth).toEqual({ iso: "2026-09-15", inMonth: true, date: 15 });
  });
});

describe("applyScheduleDay", () => {
  it("moves both days until the other field is edited", () => {
    const same = { startDay: "2026-09-16", endDay: "2026-09-16" };
    const lock = initialDayLock(same.startDay, same.endDay);
    const first = applyScheduleDay(same, lock, "start", "2026-09-20");
    expect(first).toEqual({
      startDay: "2026-09-20",
      endDay: "2026-09-20",
      lock: { startTouched: true, endTouched: false },
    });
    const again = applyScheduleDay(first, first.lock, "start", "2026-09-21");
    expect(again.startDay).toBe("2026-09-21");
    expect(again.endDay).toBe("2026-09-21");
    const split = applyScheduleDay(again, again.lock, "end", "2026-09-22");
    expect(split).toEqual({
      startDay: "2026-09-21",
      endDay: "2026-09-22",
      lock: { startTouched: true, endTouched: true },
    });
  });

  it("does not relink a task that already spans days", () => {
    const span = { startDay: "2026-09-16", endDay: "2026-09-18" };
    const lock = initialDayLock(span.startDay, span.endDay);
    const next = applyScheduleDay(span, lock, "start", "2026-09-20");
    expect(next.startDay).toBe("2026-09-20");
    expect(next.endDay).toBe("2026-09-18");
  });
});

describe("shiftMonth", () => {
  it("wraps December to January", () => {
    expect(shiftMonth(2026, 12, 1)).toEqual({ year: 2027, month: 1 });
    expect(shiftMonth(2026, 1, -1)).toEqual({ year: 2025, month: 12 });
  });
});
