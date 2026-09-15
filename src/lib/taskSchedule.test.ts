import { describe, expect, it } from "vitest";
import { TIME_STEPS, defaultRange, remindToggle } from "./taskSchedule";

describe("taskSchedule", () => {
  it("has 96 quarter-hour labels", () => {
    expect(TIME_STEPS).toHaveLength(96);
    expect(TIME_STEPS[0]).toBe("00:00");
    expect(TIME_STEPS[95]).toBe("23:45");
  });

  it("defaultRange is thirty minutes aligned", () => {
    const { start, end } = defaultRange(100);
    expect(start).toBe(0);
    expect(end).toBe(1800);
  });

  it("remindToggle adds and removes allowed offsets", () => {
    expect(remindToggle([0], 15)).toEqual([0, 15]);
    expect(remindToggle([0, 15], 0)).toEqual([15]);
  });
});
