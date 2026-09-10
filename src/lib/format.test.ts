import { describe, expect, it } from "vitest";
import { formatEstimatedMinutes } from "./format";

describe("formatEstimatedMinutes", () => {
  it("hides seconds", () => {
    expect(formatEstimatedMinutes(5 * 3600 + 23 * 60 + 17)).toBe("5h 23m");
  });

  it("zero", () => {
    expect(formatEstimatedMinutes(0)).toBe("0h 0m");
  });
});
