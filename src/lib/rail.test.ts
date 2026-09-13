import { describe, expect, it } from "vitest";
import { railTabs } from "./rail";

describe("railTabs", () => {
  it("labels week tab as 统计", () => {
    expect(railTabs()[1].label).toBe("统计");
  });
});
