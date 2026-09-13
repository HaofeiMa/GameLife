import { describe, expect, it } from "vitest";
import { calendarCells, monthHeatCell } from "./monthGrid";

describe("calendarCells", () => {
  it("pads September 2026 leading blanks to Tuesday-start? compute from Date", () => {
    const cells = calendarCells(2026, 9);
    expect(cells.length % 7).toBe(0);
    expect(cells.filter((c) => c.day === "2026-09-13").length).toBe(1);
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
