import { describe, expect, it } from "vitest";
import {
  dayStackCaption,
  monthShowsLedgerCards,
  weekHasObservation,
  weekRangeLabel,
  weekRangePagingEnabled,
} from "./statsView";

describe("week range paging", () => {
  it("freezes paging on the week tab", () => {
    expect(weekRangePagingEnabled("week")).toBe(false);
    expect(weekRangePagingEnabled("month")).toBe(true);
    expect(weekRangePagingEnabled("rhythm")).toBe(true);
    expect(weekRangePagingEnabled("app")).toBe(true);
  });

  it("labels the week from byDay, not a paged anchor", () => {
    expect(
      weekRangeLabel(
        [{ day: "2026-09-07" }, { day: "2026-09-13" }],
        "2026-08-31",
        "2026-09-06",
      ),
    ).toBe("2026-09-07 至 2026-09-13");
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
    expect(dayStackCaption(true, 0)).toBe("未采样");
  });
});

describe("month empty extras", () => {
  it("hides ledger and badge cards when the month has no observation", () => {
    expect(monthShowsLedgerCards(false)).toBe(false);
    expect(monthShowsLedgerCards(true)).toBe(true);
  });
});
