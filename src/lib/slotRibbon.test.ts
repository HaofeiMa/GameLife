import { describe, expect, it } from "vitest";
import { dominantToCategory } from "./slotRibbon";

describe("dominantToCategory", () => {
  it("pending maps to pending category", () => {
    expect(dominantToCategory("core_research", true)).toBe("pending");
    expect(dominantToCategory("distraction", false)).toBe("entertainment");
  });
});
