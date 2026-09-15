import { describe, expect, it } from "vitest";
import { moveSameDay } from "./planDrag";

describe("moveSameDay", () => {
  it("clamps a dragged block inside the day", () => {
    const day = 1_000_000;
    const moved = moveSameDay(day + 3600, day + 7200, day, -10);
    expect(moved.start).toBe(day);
    expect(moved.end - moved.start).toBe(3600);
  });

  it("snaps to fifteen-minute slots and keeps duration", () => {
    const day = 0;
    const moved = moveSameDay(3600, 5400, day, 1);
    expect(moved.start).toBe(4500);
    expect(moved.end).toBe(6300);
  });
});
