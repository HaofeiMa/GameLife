import { describe, expect, it } from "vitest";
import {
  columnRoleKey,
  formatTicktickLastSync,
  nextRoleMap,
  normalizeProjectRole,
  ticktickSyncButtonLabel,
  ticktickSyncErrorMessage,
  ticktickTaskTimeLabel,
  groupTicktickTodayTasks,
} from "./ticktickBoard";

describe("normalizeProjectRole", () => {
  it("falls unknown roles into ignore so every list lands in a column", () => {
    expect(normalizeProjectRole("mainline")).toBe("mainline");
    expect(normalizeProjectRole("")).toBe("ignore");
    expect(normalizeProjectRole("support")).toBe("ignore");
  });
});

  describe("formatTicktickLastSync", () => {
    it("says not synced when there is no timestamp", () => {
      expect(formatTicktickLastSync(null, 1_000)).toBe("尚未同步任务");
    });

    it("shows local clock time for a unix timestamp", () => {
      expect(formatTicktickLastSync(1_768_305_600, 1_768_305_600)).toContain("上次同步");
    });
  });

describe("nextRoleMap", () => {
  it("omits ignore from the stored map and overwrites a previous role", () => {
    const next = nextRoleMap({ p1: "mainline", p2: "side" }, "p1", "ignore");
    expect(next).toEqual({ p2: "side" });
    expect(nextRoleMap({}, "p3", "longterm")).toEqual({ p3: "longterm" });
  });
});

describe("ticktickSyncButtonLabel", () => {
  it("shows in-progress copy while a sync is running", () => {
    expect(ticktickSyncButtonLabel(false)).toBe("同步任务");
    expect(ticktickSyncButtonLabel(true)).toBe("同步中…");
  });
});

describe("ticktickSyncErrorMessage", () => {
  it("translates backoff and 429 codes", () => {
    expect(ticktickSyncErrorMessage("ticktick_backoff")).toBe(
      "同步过于频繁，请稍后再试",
    );
    expect(ticktickSyncErrorMessage("ticktick_429")).toBe(
      "TickTick 限流，请一分钟后再试",
    );
  });
});

describe("columnRoleKey", () => {
  it("scopes a column id to its project", () => {
    expect(columnRoleKey("p1", "today")).toBe("p1:today");
    expect(columnRoleKey("p1", "today")).not.toBe(columnRoleKey("p2", "today"));
  });
});

describe("groupTicktickTodayTasks", () => {
  it("buckets titles by mapped role and skips unknown roles", () => {
    const groups = groupTicktickTodayTasks([
      {
        id: "1",
        title: "主线稿",
        role: "mainline",
        start: 100,
        end: 200,
        allDay: false,
      },
      {
        id: "2",
        title: "全天杂项",
        role: "chore",
        start: 0,
        end: 86400,
        allDay: true,
      },
      {
        id: "3",
        title: "不该出现",
        role: "core",
        start: 100,
        end: 200,
        allDay: false,
      },
    ]);
    expect(groups.mainline.map((t) => t.title)).toEqual(["主线稿"]);
    expect(groups.chore.map((t) => t.title)).toEqual(["全天杂项"]);
    expect(groups.side).toEqual([]);
    expect(groups.longterm).toEqual([]);
  });
});

describe("ticktickTaskTimeLabel", () => {
  it("says 全天 for all-day tasks and a clock range otherwise", () => {
    expect(
      ticktickTaskTimeLabel({
        start: 0,
        end: 86400,
        allDay: true,
      }),
    ).toBe("全天");
    const start = Date.UTC(2026, 8, 15, 7, 0, 0) / 1000;
    const end = Date.UTC(2026, 8, 15, 8, 0, 0) / 1000;
    const label = ticktickTaskTimeLabel({
      start,
      end,
      allDay: false,
    });
    expect(label).toMatch(/^\d{2}:\d{2}–\d{2}:\d{2}$/);
  });
});
