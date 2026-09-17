import { describe, expect, it } from "vitest";
import { calendarCells, detailHourCells, hourChipKind, monthHeatCell } from "./monthGrid";

describe("calendarCells", () => {
  it("starts the month grid on Sunday so the 1st sits under 日一二三四五六", () => {
    const cells = calendarCells(2026, 9);
    expect(cells.length % 7).toBe(0);
    expect(cells[0]).toEqual({ day: null, weekday: 0 });
    expect(cells[1]).toEqual({ day: null, weekday: 1 });
    expect(cells[2]).toEqual({ day: "2026-09-01", weekday: 2 });
    const headers = ["日", "一", "二", "三", "四", "五", "六"];
    const first = cells.findIndex((c) => c.day === "2026-09-01");
    const sun = cells.findIndex((c) => c.day === "2026-09-06");
    expect(headers[first % 7]).toBe("二");
    expect(headers[sun % 7]).toBe("日");
  });
});

describe("monthHeatCell", () => {
  it("uses credited core seconds against 28800", () => {
    expect(monthHeatCell(0)).toBe(0);
    expect(monthHeatCell(14400)).toBe(0.5);
    expect(monthHeatCell(28800)).toBe(1);
    expect(monthHeatCell(40000)).toBe(1);
  });
});

describe("detailHourCells", () => {
  it("maps 24 hours into a 5x5 grid with the last cell empty", () => {
    const hours = Array.from({ length: 24 }, (_, h) => (h === 8 ? "core" : ""));
    const cells = detailHourCells(hours);
    expect(cells).toHaveLength(25);
    expect(cells[8]).toBe("core");
    expect(cells[0]).toBeNull();
    expect(cells[24]).toBeNull();
  });
});

describe("hourChipKind", () => {
  it("keeps the last cell and future days blank, and empty hours as plates not unobserved", () => {
    expect(hourChipKind(24, "core", false)).toBe("none");
    expect(hourChipKind(8, "core", true)).toBe("none");
    expect(hourChipKind(3, null, false)).toBe("empty");
    expect(hourChipKind(8, "core", false)).toBe("category");
  });
});
