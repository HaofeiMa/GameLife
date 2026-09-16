import { describe, expect, it } from "vitest";
import { calendarDays, CAL_DAY_GAP, CAL_GUTTER, dropRange, hitCalendarTs, resizeRange } from "./taskCalendar";

describe("calendarDays", () => {
  it("three days are today and the next two", () => {
    expect(calendarDays("2026-09-15", 3)).toEqual([
      "2026-09-15",
      "2026-09-16",
      "2026-09-17",
    ]);
  });

  it("seven days are Monday through Sunday", () => {
    expect(calendarDays("2026-09-15", 7)[0]).toBe("2026-09-14");
    expect(calendarDays("2026-09-15", 7)).toHaveLength(7);
    expect(calendarDays("2026-09-15", 7)[6]).toBe("2026-09-20");
  });
});

describe("dropRange", () => {
  it("drop range is thirty minutes snapped to a slot", () => {
    expect(dropRange(100)).toEqual({ start: 0, end: 1800 });
  });
});

describe("resizeRange", () => {
  it("resize start does not pass the end", () => {
    const r = resizeRange(0, 3600, "start", 4000);
    expect(r.end - r.start).toBeGreaterThanOrEqual(900);
    expect(r.end).toBe(3600);
  });
});

describe("hitCalendarTs", () => {
  it("accounts for the gutter and the 10px gaps between days", () => {
    const days = ["2026-09-14", "2026-09-15", "2026-09-16"];
    const col = 100;
    const width = CAL_GUTTER + col * 3 + CAL_DAY_GAP * 2;
    const grid = { left: 0, top: 0, width, scrollTop: 0 };
    const day0 = hitCalendarTs(days, CAL_GUTTER + 10, 18, grid);
    const inRightHalfOfFirstGap = CAL_GUTTER + col + CAL_DAY_GAP / 2 + 1;
    const ts = hitCalendarTs(days, inRightHalfOfFirstGap, 18, grid);
    expect(day0).not.toBeNull();
    expect(ts).not.toBeNull();
    expect(ts).toBeGreaterThan(day0 as number);
  });

  it("ignores clicks on the sticky day header", () => {
    const days = ["2026-09-16", "2026-09-17"];
    const grid = { left: 0, top: 0, width: 40 + 200 + 10 + 200, scrollTop: 0 };
    expect(hitCalendarTs(days, 50, 10, grid, 32)).toBeNull();
  });
});
