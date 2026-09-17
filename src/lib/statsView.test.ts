import { describe, expect, it } from "vitest";
import {
  dayStackCaption,
  hourCellTitle,
  MONTH_CALENDAR_MODE_KEY,
  monthCellNote,
  monthShowsLedgerCards,
  readMonthCalendarMode,
  START_HOUR_Y_TICKS,
  startHourBandBox,
  startHourXLabel,
  startHourYPercent,
  weekHasObservation,
  weekRangeLabel,
  writeMonthCalendarMode,
} from "./statsView";

describe("week range paging", () => {
  it("labels the week from byDay, not a paged anchor", () => {
    expect(
      weekRangeLabel(
        [{ day: "2026-09-07" }, { day: "2026-09-13" }],
        "2026-08-31",
        "2026-09-06",
      ),
    ).toBe("2026-09-07 至 09-13");
  });

  it("does not treat a live week as empty just because the rhythm anchor moved", () => {
    expect(weekHasObservation(12)).toBe(true);
    expect(weekHasObservation(0)).toBe(false);
  });
});

describe("day stack caption", () => {
  it("does not call a weekday with no stack 0m", () => {
    expect(dayStackCaption(false, 0)).toBe("—");
    expect(dayStackCaption(false, 40)).toBe("40m");
  });

  it("shows weekend minutes the same as weekdays", () => {
    expect(dayStackCaption(true, 0)).toBe("—");
    expect(dayStackCaption(true, 25)).toBe("25m");
  });
});

describe("month cell note", () => {
  it("does not label a weekend as unscanned", () => {
    expect(monthCellNote(false, 0)).toBe("无观测");
    expect(monthCellNote(false, 12)).toBe("12m");
    expect(monthCellNote(true, 0)).toBe("未来");
  });
});

describe("month empty extras", () => {
  it("hides ledger and badge cards when the month has no observation", () => {
    expect(monthShowsLedgerCards(false)).toBe(false);
    expect(monthShowsLedgerCards(true)).toBe(true);
  });
});

describe("start hour chart labels", () => {
  it("prints every date in a 7-day range", () => {
    const days = [
      "2026-09-14",
      "2026-09-15",
      "2026-09-16",
      "2026-09-17",
      "2026-09-18",
      "2026-09-19",
      "2026-09-20",
    ];
    expect(days.map((day, i) => startHourXLabel(day, i, days.length))).toEqual([
      "09-14",
      "09-15",
      "09-16",
      "09-17",
      "09-18",
      "09-19",
      "09-20",
    ]);
  });

  it("thins a month of weekday dates so MM-DD labels cannot collide", () => {
    const days = Array.from(
      { length: 22 },
      (_, i) => `2026-09-${String(i + 1).padStart(2, "0")}`,
    );
    const labels = days.map((day, i) => startHourXLabel(day, i, days.length));
    const shown = labels.filter((label): label is string => label != null);
    expect(shown[0]).toBe("09-01");
    expect(shown[shown.length - 1]).toBe("09-22");
    expect(shown.length).toBeLessThanOrEqual(6);
    expect(shown.length).toBeGreaterThanOrEqual(4);
  });

  it("places noon halfway down a 0-at-top axis", () => {
    expect(startHourYPercent(0)).toBe(0);
    expect(startHourYPercent(12)).toBe(50);
    expect(startHourYPercent(24)).toBe(100);
  });

  it("ticks 0 at the top and 24 at the bottom", () => {
    expect([...START_HOUR_Y_TICKS]).toEqual([0, 6, 12, 18, 24]);
  });

  it("maps a morning band to the top quarter of the column", () => {
    expect(startHourBandBox(0, 6)).toEqual({ top: 0, height: 25 });
    expect(startHourBandBox(18, 24)).toEqual({ top: 75, height: 25 });
  });
});

describe("month calendar mode", () => {
  function memoryStorage(initial: Record<string, string> = {}) {
    const store = new Map(Object.entries(initial));
    return {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => {
        store.set(k, v);
      },
    };
  }

  it("defaults to the heat calendar and remembers the detailed view", () => {
    const storage = memoryStorage();
    expect(readMonthCalendarMode(storage)).toBe("heat");
    writeMonthCalendarMode(storage, "detail");
    expect(storage.getItem(MONTH_CALENDAR_MODE_KEY)).toBe("detail");
    expect(readMonthCalendarMode(storage)).toBe("detail");
  });
});

describe("hour cell title", () => {
  it("names the hour and category without calling an empty hour unobserved", () => {
    expect(hourCellTitle("2026-09-17", 9, "core")).toBe("2026-09-17 · 9:00 主线");
    expect(hourCellTitle("2026-09-17", 3, null)).toBe("2026-09-17 · 3:00");
  });
});
