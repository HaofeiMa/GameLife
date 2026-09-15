import { describe, expect, it } from "vitest";
import type { DayTask } from "./api";
import { planBlocks } from "./calendar";
import {
  ACTUAL_DEFAULT,
  ACTUAL_MAX,
  ACTUAL_MIN,
  assignPlanLanes,
  clampActualWidth,
  planLaneSpan,
  readStoredActualWidth,
  splitTimedPlan,
  TIMELINE_ACTUAL_KEY,
  writeStoredActualWidth,
} from "./timelinePlan";

function task(
  p: Partial<DayTask> & Pick<DayTask, "id" | "title" | "role">,
): DayTask {
  return {
    start: 9 * 3600,
    end: 11 * 3600,
    ...p,
  };
}

describe("splitTimedPlan", () => {
  it("keeps every timed role whose end is after start", () => {
    const timed = splitTimedPlan([
      task({ id: "a", title: "论文", role: "mainline" }),
      task({
        id: "b",
        title: "GameLife",
        role: "side",
        start: 14 * 3600,
        end: 15 * 3600,
      }),
      task({
        id: "c",
        title: "组会",
        role: "chore",
        start: 16 * 3600,
        end: 17 * 3600,
      }),
      task({
        id: "d",
        title: "读综述",
        role: "longterm",
        start: 19 * 3600,
        end: 20 * 3600,
      }),
      task({
        id: "e",
        title: "买菜",
        role: "ignore",
        start: 12 * 3600,
        end: 13 * 3600,
      }),
      task({
        id: "f",
        title: "空时段",
        role: "chore",
        start: 10 * 3600,
        end: 10 * 3600,
      }),
    ]);
    expect(timed.map((t) => t.id).sort()).toEqual(["a", "b", "c", "d", "e"]);
  });
});

describe("assignPlanLanes", () => {
  it("puts overlapping tasks in different lanes", () => {
    const blocks = planBlocks(
      [
        { start: 9 * 3600, end: 11 * 3600, role: "mainline", title: "A" },
        { start: 10 * 3600, end: 12 * 3600, role: "side", title: "B" },
      ],
      0,
    );
    const { items, laneCount } = assignPlanLanes(blocks);
    expect(laneCount).toBe(2);
    const byTitle = Object.fromEntries(items.map((i) => [i.title, i.lane]));
    expect(byTitle.A).not.toBe(byTitle.B);
  });

  it("reuses a lane after the earlier task ends", () => {
    const blocks = planBlocks(
      [
        { start: 9 * 3600, end: 10 * 3600, role: "mainline", title: "A" },
        { start: 10 * 3600, end: 11 * 3600, role: "chore", title: "B" },
      ],
      0,
    );
    expect(assignPlanLanes(blocks).laneCount).toBe(1);
  });

  it("lets a non-overlapping task span empty lanes", () => {
    const blocks = planBlocks(
      [
        { start: 9 * 3600, end: 11 * 3600, role: "mainline", title: "A" },
        { start: 10 * 3600, end: 12 * 3600, role: "side", title: "B" },
        { start: 16 * 3600, end: 17 * 3600, role: "chore", title: "C" },
      ],
      0,
    );
    const { items, laneCount } = assignPlanLanes(blocks);
    expect(laneCount).toBe(2);
    const byTitle = Object.fromEntries(items.map((i) => [i.title, i]));
    expect(planLaneSpan(byTitle.A, items, laneCount)).toBe(1);
    expect(planLaneSpan(byTitle.B, items, laneCount)).toBe(1);
    expect(planLaneSpan(byTitle.C, items, laneCount)).toBe(2);
  });
});

describe("actual column split", () => {
  function memoryStorage(initial?: Record<string, string>) {
    const store = new Map<string, string>(Object.entries(initial ?? {}));
    return {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => {
        store.set(k, v);
      },
    };
  }

  it("keeps the actual column wide enough for a two-character label", () => {
    expect(ACTUAL_MIN).toBeGreaterThanOrEqual(56);
    expect(clampActualWidth(0)).toBe(ACTUAL_MIN);
    expect(clampActualWidth(9_999)).toBe(ACTUAL_MAX);
    expect(clampActualWidth(ACTUAL_DEFAULT)).toBe(ACTUAL_DEFAULT);
  });

  it("remembers a dragged gutter width", () => {
    const storage = memoryStorage();
    expect(readStoredActualWidth(storage)).toBe(ACTUAL_DEFAULT);
    writeStoredActualWidth(storage, 120);
    expect(readStoredActualWidth(storage)).toBe(120);
  });

  it("migrates a leftover overlay width to the gutter default", () => {
    const storage = memoryStorage({ [TIMELINE_ACTUAL_KEY]: "22" });
    expect(readStoredActualWidth(storage)).toBe(ACTUAL_DEFAULT);
    storage.setItem(TIMELINE_ACTUAL_KEY, "40");
    expect(readStoredActualWidth(storage)).toBe(ACTUAL_DEFAULT);
  });
});
