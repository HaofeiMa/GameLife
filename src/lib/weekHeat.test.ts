import { describe, expect, it } from "vitest";
import { heatTone } from "./weekHeat";

describe("heatTone", () => {
  it("no observe is empty", () => {
    expect(heatTone(0, 0)).toBe(0);
  });

  it("full core is 1", () => {
    expect(heatTone(3600, 3600)).toBe(1);
  });
});
