import { describe, expect, it } from "vitest";
import { listRoleLabel, taskTimeLabel } from "./taskBoard";

describe("listRoleLabel", () => {
  it("names the four list roles", () => {
    expect(listRoleLabel("mainline")).toBe("主线");
    expect(listRoleLabel("side")).toBe("支线");
    expect(listRoleLabel("longterm")).toBe("长期");
    expect(listRoleLabel("chore")).toBe("杂项");
  });

  it("falls back to 支线 for unknown roles", () => {
    expect(listRoleLabel("ignore")).toBe("支线");
  });
});

describe("taskTimeLabel", () => {
  it("formats a local clock range", () => {
    const start = Date.UTC(2026, 8, 15, 7, 0, 0) / 1000;
    const end = Date.UTC(2026, 8, 15, 8, 0, 0) / 1000;
    expect(taskTimeLabel(start, end)).toMatch(/^\d{2}:\d{2}–\d{2}:\d{2}$/);
  });
});
