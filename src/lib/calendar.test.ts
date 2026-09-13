import { describe, expect, it } from "vitest";
import {
  consumeStoredCalDay,
  peekStoredCalDay,
  pendingActivityMinutes,
  planBlocks,
  planMarksFromSnapshots,
  resolvedActivityMinutes,
  slotIndex,
} from "./calendar";

describe("calendar grid", () => {
  it("slot index 0 is midnight", () => {
    expect(slotIndex(0, 0)).toBe(0);
  });

  it("plan block uses 15m grid", () => {
    const b = planBlocks(
      [{ start: 9 * 3600, end: 11 * 3600, role: "mainline", title: "HDP" }],
      0,
    );
    expect(b[0].rowStart).toBe(9 * 4);
    expect(b[0].rowSpan).toBe(8);
  });

  it("keeps overlapping timed marks only", () => {
    const dayStart = Date.UTC(2026, 8, 13) / 1000;
    const marks = planMarksFromSnapshots(
      [
        { start: dayStart + 10 * 3600, end: dayStart + 11 * 3600, title: "A" },
        { start: null, end: null, title: "B" },
      ],
      dayStart,
    );
    expect(marks).toHaveLength(1);
    expect(marks[0].title).toBe("A");
  });

  it("peekStoredCalDay reads without removing", () => {
    const store = new Map<string, string>([["gl-cal-day", "2026-09-11"]]);
    const storage = {
      getItem: (k: string) => store.get(k) ?? null,
      removeItem: (k: string) => {
        store.delete(k);
      },
    };
    expect(peekStoredCalDay(storage)).toBe("2026-09-11");
    expect(storage.getItem("gl-cal-day")).toBe("2026-09-11");
    consumeStoredCalDay(storage);
    expect(storage.getItem("gl-cal-day")).toBeNull();
  });

  it("puts pending slot minutes only in pending, not category bars", () => {
    const slots = [
      {
        pending: false,
        activity: {
          core: 8,
          support: 0,
          admin: 0,
          side: 0,
          distraction: 0,
          away: 0,
          unobserved: 0,
        },
      },
      {
        pending: true,
        activity: {
          core: 5,
          support: 0,
          admin: 0,
          side: 0,
          distraction: 0,
          away: 0,
          unobserved: 7,
        },
      },
    ];
    const resolved = resolvedActivityMinutes(slots);
    expect(resolved.core).toBe(8);
    expect(resolved.unobserved).toBe(0);
    expect(pendingActivityMinutes(slots)).toBe(12);
  });
});
