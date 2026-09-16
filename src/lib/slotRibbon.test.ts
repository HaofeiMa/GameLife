import { describe, expect, it } from "vitest";
import { dominantToCategory, hourWashCategory } from "./slotRibbon";
import type { CategoryKey } from "./theme";

describe("dominantToCategory", () => {
  it("pending maps to pending category", () => {
    expect(dominantToCategory("core_research", true)).toBe("pending");
    expect(dominantToCategory("distraction", false)).toBe("entertainment");
  });
});

describe("hourWashCategory", () => {
  it("returns null when the hour is empty or unobserved", () => {
    const cells = Array.from({ length: 96 }, () => null as CategoryKey | null);
    cells[0] = "unobserved";
    cells[1] = "unobserved";
    cells[2] = null;
    cells[3] = null;
    expect(hourWashCategory(cells, 0)).toBeNull();
  });

  it("takes the majority of the four quarter-hour cells", () => {
    const cells = Array.from({ length: 96 }, () => null as CategoryKey | null);
    cells[0] = "mainline";
    cells[1] = "mainline";
    cells[2] = "mainline";
    cells[3] = "entertainment";
    expect(hourWashCategory(cells, 0)).toBe("mainline");
  });

  it("breaks ties toward the earlier cell", () => {
    const cells = Array.from({ length: 96 }, () => null as CategoryKey | null);
    cells[4] = "entertainment";
    cells[5] = "mainline";
    cells[6] = "entertainment";
    cells[7] = "mainline";
    expect(hourWashCategory(cells, 1)).toBe("entertainment");
  });
});
