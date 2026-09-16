import { describe, expect, it } from "vitest";
import { CLICK_SLOP_PX, withinClickSlop } from "./taskPointer";

describe("withinClickSlop", () => {
  it("treats four pixels as a click and five as a drag", () => {
    expect(CLICK_SLOP_PX).toBe(4);
    expect(withinClickSlop(0, 4)).toBe(true);
    expect(withinClickSlop(3, 4)).toBe(false);
  });
});
