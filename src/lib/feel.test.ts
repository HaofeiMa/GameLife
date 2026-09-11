import { describe, expect, it } from "vitest";
import {
  coalesceFeelEvents,
  entertainmentStillActive,
  nextFeelNotices,
  noticeText,
  sessionJustEnded,
  showEndedFromActive,
  trayEntertainmentMinutes,
  wishRejectedMessage,
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

describe("trayEntertainmentMinutes", () => {
  it("matches the tray minute rule", () => {
    expect(trayEntertainmentMinutes(0)).toBeNull();
    expect(trayEntertainmentMinutes(59)).toBe(1);
    expect(trayEntertainmentMinutes(120)).toBe(2);
  });
});

describe("entertainmentStillActive", () => {
  it("is active only while endsAt is strictly after now", () => {
    expect(entertainmentStillActive(100, 99)).toBe(true);
    expect(entertainmentStillActive(100, 100)).toBe(false);
    expect(entertainmentStillActive(100, 101)).toBe(false);
  });
});

describe("showEndedFromActive", () => {
  it("names the ended session when remaining is not positive", () => {
    expect(showEndedFromActive(1, "视频")).toBeNull();
    expect(showEndedFromActive(0, "视频")).toBe("视频 已结束");
    expect(showEndedFromActive(-5, "咖啡")).toBe("咖啡 已结束");
  });
});

describe("wishRejectedMessage", () => {
  it("maps create/update Rejected codes to the same Chinese copy", () => {
    expect(wishRejectedMessage("empty_name")).toBe("名称不能为空");
    expect(wishRejectedMessage("name_too_long")).toBe("名称最多 80 字");
    expect(wishRejectedMessage("non_positive_price")).toBe("价格必须大于 0");
    expect(wishRejectedMessage("entertainment_needs_duration")).toBe(
      "能量兑换时长至少 5 分钟",
    );
  });
});

describe("noticeText", () => {
  it("uses Chinese coin and energy copy", () => {
    expect(noticeText({ type: "xp", n: 10 })).toBe("+10 能量");
    expect(noticeText({ type: "coins", n: 1 })).toBe("+1 硬币");
  });
});
