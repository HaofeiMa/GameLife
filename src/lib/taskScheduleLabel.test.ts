import { describe, expect, it } from "vitest";
import {
  listScheduleChip,
  repeatShortLabel,
  scheduleOverdue,
  scheduleSummary,
} from "./taskScheduleLabel";

function unix(y: number, m: number, d: number, h: number, min: number): number {
  return Math.floor(new Date(y, m - 1, d, h, min, 0).getTime() / 1000);
}

const NOW = unix(2026, 9, 16, 11, 0);

describe("scheduleSummary", () => {
  it("returns 未排期 when start or end is missing", () => {
    expect(scheduleSummary(null, null, NOW)).toBe("未排期");
    expect(scheduleSummary(NOW, null, NOW)).toBe("未排期");
  });

  it("names yesterday on the same calendar day", () => {
    const start = unix(2026, 9, 15, 16, 0);
    const end = unix(2026, 9, 15, 17, 30);
    expect(scheduleSummary(start, end, NOW)).toBe(
      "昨天，9月15日，下午 16:00 – 下午 17:30",
    );
  });

  it("names today and uses 上午 / 中午", () => {
    const start = unix(2026, 9, 16, 10, 0);
    const end = unix(2026, 9, 16, 12, 0);
    expect(scheduleSummary(start, end, NOW)).toBe(
      "今天，9月16日，上午 10:00 – 中午 12:00",
    );
  });

  it("splits a range that crosses midnight", () => {
    const start = unix(2026, 9, 15, 22, 0);
    const end = unix(2026, 9, 16, 2, 0);
    expect(scheduleSummary(start, end, NOW)).toBe(
      "昨天，9月15日 晚上 22:00 – 今天，9月16日 上午 2:00",
    );
  });

  it("uses month-day only when not yesterday/today/tomorrow", () => {
    const start = unix(2026, 9, 10, 9, 0);
    const end = unix(2026, 9, 10, 10, 0);
    expect(scheduleSummary(start, end, NOW)).toBe(
      "9月10日，上午 9:00 – 上午 10:00",
    );
  });
});

describe("scheduleOverdue", () => {
  it("is overdue when end is past and the task is not done", () => {
    expect(scheduleOverdue(unix(2026, 9, 15, 17, 30), NOW, false)).toBe(true);
  });

  it("is not overdue when done or unscheduled", () => {
    expect(scheduleOverdue(unix(2026, 9, 15, 17, 30), NOW, true)).toBe(false);
    expect(scheduleOverdue(null, NOW, false)).toBe(false);
    expect(scheduleOverdue(unix(2026, 9, 16, 18, 0), NOW, false)).toBe(false);
  });
});

describe("listScheduleChip", () => {
  it("shows today's start clock in 24h with 中午 / 下午 / 晚上", () => {
    expect(
      listScheduleChip(unix(2026, 9, 16, 19, 0), unix(2026, 9, 16, 20, 0), NOW, false),
    ).toEqual({ label: "晚上 19:00", tone: "upcoming" });
    expect(
      listScheduleChip(unix(2026, 9, 16, 12, 0), unix(2026, 9, 16, 13, 0), NOW, false),
    ).toEqual({ label: "中午 12:00", tone: "upcoming" });
    expect(
      listScheduleChip(unix(2026, 9, 16, 11, 0), unix(2026, 9, 16, 12, 0), NOW, false),
    ).toEqual({ label: "中午 11:00", tone: "upcoming" });
    expect(
      listScheduleChip(unix(2026, 9, 16, 14, 0), unix(2026, 9, 16, 15, 0), NOW, false),
    ).toEqual({ label: "下午 14:00", tone: "upcoming" });
    expect(
      listScheduleChip(unix(2026, 9, 16, 13, 0), unix(2026, 9, 16, 14, 0), NOW, false),
    ).toEqual({ label: "下午 13:00", tone: "upcoming" });
  });

  it("marks a past time today overdue", () => {
    expect(
      listScheduleChip(unix(2026, 9, 16, 9, 0), unix(2026, 9, 16, 10, 0), NOW, false),
    ).toEqual({ label: "上午 9:00", tone: "overdue" });
  });

  it("names yesterday, tomorrow and the day after", () => {
    expect(
      listScheduleChip(unix(2026, 9, 15, 10, 0), unix(2026, 9, 15, 11, 0), NOW, false),
    ).toEqual({ label: "昨天", tone: "overdue" });
    expect(
      listScheduleChip(unix(2026, 9, 17, 10, 0), unix(2026, 9, 17, 11, 0), NOW, false),
    ).toEqual({ label: "明天", tone: "upcoming" });
    expect(
      listScheduleChip(unix(2026, 9, 18, 10, 0), unix(2026, 9, 18, 11, 0), NOW, false),
    ).toEqual({ label: "后天", tone: "upcoming" });
  });

  it("uses weekday this week and 下周 next week", () => {
    expect(
      listScheduleChip(unix(2026, 9, 14, 10, 0), unix(2026, 9, 14, 11, 0), NOW, false),
    ).toEqual({ label: "周一", tone: "overdue" });
    expect(
      listScheduleChip(unix(2026, 9, 19, 10, 0), unix(2026, 9, 19, 11, 0), NOW, false),
    ).toEqual({ label: "周六", tone: "upcoming" });
    expect(
      listScheduleChip(unix(2026, 9, 22, 10, 0), unix(2026, 9, 22, 11, 0), NOW, false),
    ).toEqual({ label: "下周二", tone: "upcoming" });
  });

  it("falls back to a calendar date further out", () => {
    expect(
      listScheduleChip(unix(2026, 9, 30, 10, 0), unix(2026, 9, 30, 11, 0), NOW, false),
    ).toEqual({ label: "9月30日", tone: "upcoming" });
    expect(
      listScheduleChip(unix(2027, 1, 3, 10, 0), unix(2027, 1, 3, 11, 0), NOW, false),
    ).toEqual({ label: "2027年1月3日", tone: "upcoming" });
  });

  it("mutes unscheduled and completed chips", () => {
    expect(listScheduleChip(null, null, NOW, false)).toEqual({
      label: "未排期",
      tone: "muted",
    });
    expect(
      listScheduleChip(unix(2026, 9, 15, 10, 0), unix(2026, 9, 15, 11, 0), NOW, true),
    ).toEqual({ label: "昨天", tone: "muted" });
  });
});

describe("repeatShortLabel", () => {
  it("maps engine repeat keys", () => {
    expect(repeatShortLabel("daily")).toBe("每天");
    expect(repeatShortLabel("weekly")).toBe("每周");
    expect(repeatShortLabel("monthly")).toBe("每月");
    expect(repeatShortLabel("none")).toBeNull();
    expect(repeatShortLabel("")).toBeNull();
  });
});
