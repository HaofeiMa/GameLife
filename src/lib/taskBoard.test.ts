import { describe, expect, it } from "vitest";
import {
  isPresetListId,
  listRoleLabel,
  parseCollapsed,
  shiftRangeToDay,
  taskCommandError,
  taskTimeLabel,
  toggleCollapsed,
} from "./taskBoard";

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

describe("collapsed lists", () => {
  it("parses a json id array and ignores junk", () => {
    expect(parseCollapsed(null)).toEqual([]);
    expect(parseCollapsed('["list-side"]')).toEqual(["list-side"]);
    expect(parseCollapsed("{")).toEqual([]);
  });

  it("toggles an id in the collapsed set", () => {
    expect(toggleCollapsed([], "list-side")).toEqual(["list-side"]);
    expect(toggleCollapsed(["list-side"], "list-side")).toEqual([]);
  });
});

describe("taskCommandError", () => {
  it("maps the 20-task cap to the toast copy", () => {
    expect(taskCommandError("too_many_judgment_tasks")).toBe(
      "当天已排期任务超过 20，请先完成、改期或放弃。",
    );
  });
});

describe("shiftRangeToDay", () => {
  it("keeps the clock when moving to another day", () => {
    expect(shiftRangeToDay(100, 280, 0, 86400)).toEqual({
      start: 86500,
      end: 86680,
    });
  });
});

describe("isPresetListId", () => {
  it("locks the four seeded lists", () => {
    expect(isPresetListId("list-mainline")).toBe(true);
    expect(isPresetListId("list-lab")).toBe(false);
  });
});
