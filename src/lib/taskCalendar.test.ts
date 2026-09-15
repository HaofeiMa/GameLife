import { describe, expect, it } from "vitest";
import { calendarDays, dropRange, resizeRange } from "./taskCalendar";

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
