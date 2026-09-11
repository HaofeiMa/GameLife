import { describe, expect, it } from "vitest";
import {
  coalesceFeelEvents,
  nextFeelNotices,
  sessionJustEnded,
} from "./feel";

describe("coalesceFeelEvents", () => {
  it("merges ticks and keeps named", () => {
    const notices = coalesceFeelEvents([
      { key: "validated_xp:2026-09-10:1", coin: 0, xp: 1, ts: 1 },
      { key: "validated_xp:2026-09-10:2", coin: 0, xp: 1, ts: 1 },
      { key: "validated_coin:2026-09-10:1", coin: 1, xp: 0, ts: 1 },
      { key: "ladder:6h:2026-09-10", coin: 3, xp: 0, ts: 1 },
      { key: "xp_support:2026-09-10:1", coin: 0, xp: 6, ts: 1 },
    ]);
    expect(notices).toEqual([
      { type: "chest" },
      { type: "coins", n: 1 },
      { type: "xp", n: 8 },
    ]);
  });
});

describe("nextFeelNotices", () => {
  it("first poll is silent", () => {
    const rows = [
      { key: "validated_coin:d:1", coin: 1, xp: 0, ts: 50 },
    ];
    const first = nextFeelNotices(null, rows);
    expect(first.notices).toEqual([]);
    expect(first.maxTs).toBe(50);
    const second = nextFeelNotices(first.maxTs, [
      ...rows,
      { key: "validated_coin:d:2", coin: 1, xp: 0, ts: 60 },
    ]);
    expect(second.notices).toEqual([{ type: "coins", n: 1 }]);
  });
});

describe("sessionJustEnded", () => {
  it("toasts only after a live countdown hits zero", () => {
    expect(sessionJustEnded(null, null)).toBe(false);
    expect(sessionJustEnded(12, null)).toBe(true);
    expect(sessionJustEnded(12, { remainingSecs: 11 })).toBe(false);
  });
});
