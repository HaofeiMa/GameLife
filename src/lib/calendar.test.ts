import { describe, expect, it } from "vitest";
import { planBlocks, slotIndex } from "./calendar";

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
});
