import { describe, expect, it } from "vitest";
import { GUIDE_PLACEHOLDERS, savedCategoryGuides } from "./guides";
import type { CategoryGuides } from "./api";

describe("GUIDE_PLACEHOLDERS", () => {
  it("matches spec copy", () => {
    expect(GUIDE_PLACEHOLDERS.mainline).toContain("Overleaf");
    expect(GUIDE_PLACEHOLDERS.entertainment).toContain("YouTube");
  });
});

describe("savedCategoryGuides", () => {
  it("trims and never copies placeholders into policy", () => {
    const saved = savedCategoryGuides({
      mainline: "  ",
      side: " 打磨仓库 ",
      admin: "\n",
      entertainment: "",
    });
    expect(saved).toEqual({
      mainline: "",
      side: "打磨仓库",
      admin: "",
      entertainment: "",
    } satisfies CategoryGuides);
    expect(saved.mainline).not.toBe(GUIDE_PLACEHOLDERS.mainline);
    expect(saved.admin).not.toBe(GUIDE_PLACEHOLDERS.admin);
    expect(saved.entertainment).not.toBe(GUIDE_PLACEHOLDERS.entertainment);
  });
});
