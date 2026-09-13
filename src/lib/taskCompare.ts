import type { TickTickTask, TodaySlot } from "./api";

/** Every slot is a 15-minute cell, and `slot.start` is its opening second. */
const SLOT_SECONDS = 900;

export type TaskCompareState = "done" | "doing" | "idle";

export interface TaskComparison {
  id: string;
  title: string;
  start: number;
  end: number;
  /** How long the task was scheduled for. */
  plannedMinutes: number;
  /** Credited mainline minutes that landed inside the planned window. */
  actualMinutes: number;
  state: TaskCompareState;
}

export interface DayComparison {
  tasks: TaskComparison[];
  plannedMinutes: number;
  /** Credited minutes inside one of the plans. */
  actualInPlanMinutes: number;
  /** Credited minutes with no plan covering them — good news, not an error. */
  unplannedMinutes: number;
}

/**
 * 「计划 vs 实际」的对照 —— 今日页的主卡。
 *
 * 「实际」用时间窗口重叠近似：一条主线任务的计划时段内，credited 核心分钟之和
 * （`creditedMinutes` 本身就是计入 8 小时目标的主线分钟）。
 *
 * 判定时 AI 确实做过任务匹配，但那次匹配没有落库（`JudgeOutput` 不存 matched
 * task id），所以这里退一步问的是「你计划做主线的那段时间里，你到底推进了多少
 * 主线」——诚实、不需要后端改动，也没有 TickTick 时自然降级。
 *
 * 每个槽只会归给第一条与它重叠的计划，这样 `actualInPlanMinutes +
 * unplannedMinutes` 恰好等于当天的 credited 总数，不会重复计数。
 */
export function compareDayTasks(
  tasks: TickTickTask[],
  slots: TodaySlot[],
): DayComparison {
  const plans = tasks
    .filter((t) => t.role === "mainline" && t.end > t.start)
    .slice()
    .sort((a, b) => a.start - b.start);

  const actual = new Map<string, number>();
  let actualInPlan = 0;
  let unplanned = 0;

  for (const slot of slots) {
    const credited = Math.max(0, slot.creditedMinutes);
    if (credited === 0) continue;
    const slotEnd = slot.start + SLOT_SECONDS;
    const hit = plans.find((t) => slot.start < t.end && slotEnd > t.start);
    if (hit) {
      actual.set(hit.id, (actual.get(hit.id) ?? 0) + credited);
      actualInPlan += credited;
    } else {
      unplanned += credited;
    }
  }

  const rows = plans.map((t) => {
    const plannedMinutes = Math.round((t.end - t.start) / 60);
    const actualMinutes = actual.get(t.id) ?? 0;
    return {
      id: t.id,
      title: t.title,
      start: t.start,
      end: t.end,
      plannedMinutes,
      actualMinutes,
      state: compareState(actualMinutes, plannedMinutes),
    };
  });

  return {
    tasks: rows,
    plannedMinutes: rows.reduce((n, r) => n + r.plannedMinutes, 0),
    actualInPlanMinutes: actualInPlan,
    unplannedMinutes: unplanned,
  };
}

export function compareState(
  actualMinutes: number,
  plannedMinutes: number,
): TaskCompareState {
  if (actualMinutes <= 0) return "idle";
  if (plannedMinutes > 0 && actualMinutes >= plannedMinutes) return "done";
  return "doing";
}

export const COMPARE_STATE_LABEL: Record<TaskCompareState, string> = {
  idle: "今天没动",
  doing: "在推进",
  done: "计划已满",
};
