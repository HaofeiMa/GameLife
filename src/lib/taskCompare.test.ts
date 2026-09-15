import { describe, expect, it } from "vitest";
import type { DayTask, TodaySlot } from "./api";
import { compareDayTasks, compareState } from "./taskCompare";

const DAY_START = 1_760_000_000;

function task(id: string, hourStart: number, hourEnd: number, role = "mainline"): DayTask {
  return {
    id,
    title: `任务 ${id}`,
    role,
    start: DAY_START + hourStart * 3600,
    end: DAY_START + hourEnd * 3600,
  };
}

function slot(hourStart: number, creditedMinutes: number): TodaySlot {
  return {
    start: DAY_START + hourStart * 3600,
    dominant: creditedMinutes > 0 ? "core" : "away",
    creditedMinutes,
    activity: {
      core: creditedMinutes * 60,
      support: 0,
      admin: 0,
      side: 0,
      distraction: 0,
      away: 0,
      unobserved: 0,
    },
    activitySummary: "",
    pending: false,
    final: true,
  };
}

describe("compareState", () => {
  it("is idle at zero and doing below plan", () => {
    expect(compareState(0, 60)).toBe("idle");
    expect(compareState(30, 60)).toBe("doing");
  });

  it("is done once the plan is met or exceeded", () => {
    expect(compareState(60, 60)).toBe("done");
    expect(compareState(90, 60)).toBe("done");
  });

  it("never reports done for a zero-length plan with real work", () => {
    expect(compareState(15, 0)).toBe("doing");
  });
});

describe("compareDayTasks", () => {
  it("returns nothing for a day with no tasks", () => {
    const out = compareDayTasks([], [slot(9, 45)]);
    expect(out.tasks).toEqual([]);
    expect(out.plannedMinutes).toBe(0);
    expect(out.actualInPlanMinutes).toBe(0);
    expect(out.unplannedMinutes).toBe(45);
  });

  it("ignores tasks that are not mainline", () => {
    const out = compareDayTasks([task("s", 9, 11, "side")], [slot(9, 45)]);
    expect(out.tasks).toEqual([]);
    expect(out.unplannedMinutes).toBe(45);
  });

  it("sums credited minutes inside the planned window", () => {
    const out = compareDayTasks([task("a", 9, 11)], [slot(9, 30), slot(10, 15)]);
    expect(out.tasks).toHaveLength(1);
    expect(out.tasks[0].plannedMinutes).toBe(120);
    expect(out.tasks[0].actualMinutes).toBe(45);
    expect(out.tasks[0].state).toBe("doing");
    expect(out.actualInPlanMinutes).toBe(45);
    expect(out.unplannedMinutes).toBe(0);
  });

  it("leaves credited minutes outside every plan unplanned", () => {
    const out = compareDayTasks([task("a", 9, 11)], [slot(9, 30), slot(20, 25)]);
    expect(out.tasks[0].actualMinutes).toBe(30);
    expect(out.actualInPlanMinutes).toBe(30);
    expect(out.unplannedMinutes).toBe(25);
  });

  it("claims a shared slot for the earlier plan only", () => {
    // 两个计划本身重叠（10:00–10:30 与 10:15–11:00），10:15 那一格两边都碰得到。
    const out = compareDayTasks(
      [task("late", 10.25, 11), task("early", 10, 10.5)],
      [slot(10.25, 15)],
    );
    const byId = new Map(out.tasks.map((t) => [t.id, t]));
    expect(byId.get("early")?.actualMinutes).toBe(15);
    expect(byId.get("late")?.actualMinutes).toBe(0);
    expect(byId.get("late")?.state).toBe("idle");
    expect(out.actualInPlanMinutes + out.unplannedMinutes).toBe(15);
  });

  it("counts a slot that runs into a plan opening mid-slot", () => {
    // 计划 09:12 开始，09:00 那一格覆盖到 09:15，算得进去。
    const out = compareDayTasks([task("a", 9.2, 10)], [slot(9, 20)]);
    expect(out.tasks[0].actualMinutes).toBe(20);
  });

  it("ignores pending and uncredited slots", () => {
    const out = compareDayTasks([task("a", 9, 11)], [slot(9, 0)]);
    expect(out.tasks[0].state).toBe("idle");
    expect(out.tasks[0].actualMinutes).toBe(0);
    expect(out.unplannedMinutes).toBe(0);
  });

  it("keeps planned and actual totals consistent across tasks", () => {
    const out = compareDayTasks(
      [task("a", 9, 11), task("b", 16, 17)],
      [slot(9, 60), slot(10, 30), slot(16, 45), slot(19, 20)],
    );
    expect(out.plannedMinutes).toBe(180);
    expect(out.tasks.map((t) => t.actualMinutes)).toEqual([90, 45]);
    expect(out.actualInPlanMinutes).toBe(135);
    expect(out.unplannedMinutes).toBe(20);
  });
});
