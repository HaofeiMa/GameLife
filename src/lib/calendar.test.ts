import { describe, expect, it } from "vitest";
import { planBlocks, planMarksFromSnapshots, slotIndex } from "./calendar";

describe("calendar grid", () => {
  it("slot index 0 is midnight", () => {
    expect(slotIndex(0, 0)).toBe(0);
  });

  it("plan block uses 15m grid", () => {
    const b = planBlocks(
      [{ start: 9 * 3600, end: 11 * 3600, role: "mainline", title: "HDP" }],
      0,
    );
    expect(b[0].rowStart).toBe(9 * 4);
    expect(b[0].rowSpan).toBe(8);
  });

  it("keeps overlapping timed marks only", () => {
    const dayStart = Date.UTC(2026, 8, 13) / 1000;
    const marks = planMarksFromSnapshots(
      [
        { start: dayStart + 10 * 3600, end: dayStart + 11 * 3600, title: "A" },
        { start: null, end: null, title: "B" },
      ],
      dayStart,
    );
    expect(marks).toHaveLength(1);
    expect(marks[0].title).toBe("A");
  });
});
