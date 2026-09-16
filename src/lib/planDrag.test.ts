import { describe, expect, it } from "vitest";
import { moveSameDay, resizeSameDay } from "./planDrag";

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

describe("resizeSameDay", () => {
  it("resizeSameDay clamps into the local day", () => {
    const day = 1_000_000;
    const r = resizeSameDay(day, day + 1800, "end", day + 86400, day);
    expect(r.end).toBeLessThanOrEqual(day + 96 * 900);
  });
});
