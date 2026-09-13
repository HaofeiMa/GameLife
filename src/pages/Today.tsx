import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Coins,
  Shield,
  Target,
  Zap,
} from "lucide-react";
import type { ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ConfirmEndDay } from "../components/ConfirmEndDay";
import { EntertainmentBanner } from "../components/EntertainmentBanner";
import { PageHeader } from "../components/PageHeader";
import { PermissionBanner } from "../components/PermissionBanner";
import { StreakRing } from "../components/StreakRing";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card } from "../components/ui/card";
import { Dialog } from "../components/ui/dialog";
import { EmptyLine } from "../components/ui/empty-state";
import { Input } from "../components/ui/input";
import { Progress } from "../components/ui/progress";
import { Select } from "../components/ui/select";
import { SkeletonPanel } from "../components/ui/skeleton";
import {
  endToday,
  freezeDay,
  getDayView,
  getToday,
  reportMisclassification,
  reviewSlot,
  type AppTopRow,
  type DayView,
  type SlotActivityMinutes,
  type TodaySlot,
  type TodayView,
} from "../lib/api";
import {
  addDays,
  consumeStoredCalDay,
  dominantLabel,
  peekStoredCalDay,
  pendingActivityMinutes,
  planMarksFromSnapshots,
  resolvedActivityMinutes,
  weekdayLabel,
} from "../lib/calendar";
import { ticktickRoleLabel } from "../lib/ticktickBoard";
import { compareDayTasks, COMPARE_STATE_LABEL, type DayComparison } from "../lib/taskCompare";
import {
  categoryColor,
  categoryColorAt,
  categoryOf,
  type CategoryKey,
} from "../lib/theme";
import { cn } from "../lib/utils";

const GOLD_DAY_MSG = "黄金日已达成。继续记录，但不再获得硬币或能量。";

const REVIEW_CATEGORIES = [
  { id: "core_research", label: "主线" },
  { id: "research_support", label: "辅助" },
  { id: "side_project", label: "支线" },
  { id: "admin", label: "杂项" },
  { id: "distraction", label: "娱乐" },
  { id: "break_away", label: "离开" },
];

const CAT_ROWS: { key: keyof SlotActivityMinutes; cat: CategoryKey; label: string }[] = [
  { key: "core", cat: "mainline", label: "主线" },
  { key: "support", cat: "support", label: "辅助" },
  { key: "side", cat: "side", label: "支线" },
  { key: "admin", cat: "admin", label: "杂项" },
  { key: "distraction", cat: "entertainment", label: "娱乐" },
  { key: "away", cat: "away", label: "离开" },
  { key: "unobserved", cat: "unobserved", label: "未观测" },
];

const HOURS = Array.from({ length: 24 }, (_, h) => h);
const SLOT_H = 18;
const HOUR_H = SLOT_H * 4;
const SLOT_COUNT = 96;
const GOAL_MINUTES = 480;

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

/** 2026-09-10 → 2026-09-10 周五（未达标） */
function freezeDayLabel(day: string): string {
  return `${day} ${weekdaySuffix(day)}（未达标）`;
}

function weekdaySuffix(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const wd = new Date(y, m - 1, d).getDay();
  return `周${"日一二三四五六"[wd]}`;
}

/** 105 → 「1h 45m」；45 → 「45m」；0 → 「0m」 */
function minutesLabel(mins: number): string {
  const m = Math.max(0, Math.round(mins));
  const h = Math.floor(m / 60);
  const rest = m % 60;
  if (h === 0) return `${rest}m`;
  return rest === 0 ? `${h}h` : `${h}h ${rest}m`;
}

function hm(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

/* ------------------------------------------------------------------ */
/* Slot review                                                         */
/* ------------------------------------------------------------------ */

function SlotReview({
  slot,
  day,
  onClose,
  onChange,
}: {
  slot: TodaySlot;
  day: string;
  onClose: () => void;
  onChange: () => void;
}) {
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const hideAway = slot.dominant === "unobserved";

  async function classify(category: string) {
    setBusy(true);
    setErr(null);
    try {
      await reviewSlot(day, slot.start, category);
      onChange();
      onClose();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function report() {
    if (!note.trim()) return;
    setBusy(true);
    setErr(null);
    try {
      await reportMisclassification(day, slot.start, note.trim());
      setNote("");
      onChange();
      onClose();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog
      open
      onClose={onClose}
      title={`${hm(slot.start)} · ${dominantLabel(slot.dominant)}`}
      description={
        <>
          计入 {slot.creditedMinutes} 分钟
          {slot.activitySummary && slot.activitySummary !== "—"
            ? ` · ${slot.activitySummary}`
            : ""}
        </>
      }
      className="max-w-lg"
      footer={
        <Button variant="outline" onClick={onClose}>
          关闭
        </Button>
      }
    >
      <div className="space-y-4">
        {err && (
          <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {err}
          </p>
        )}

        {slot.pending && (
          <div className="space-y-2">
            <p className="text-xs text-muted-foreground">把这一槽归到哪一类？</p>
            <div className="flex flex-wrap gap-2">
              {REVIEW_CATEGORIES.filter(
                (c) => !(hideAway && c.id === "break_away"),
              ).map((c) => (
                <Button
                  key={c.id}
                  variant="outline"
                  size="sm"
                  disabled={busy}
                  onClick={() => void classify(c.id)}
                >
                  {c.label}
                </Button>
              ))}
            </div>
          </div>
        )}

        {slot.final && (
          <div className="space-y-2">
            <p className="text-xs text-muted-foreground">
              已终结的槽不改经济结果，只留下误判记录。
            </p>
            <Input
              value={note}
              disabled={busy}
              placeholder="误分类说明"
              onChange={(e) => setNote(e.target.value)}
            />
            <Button
              variant="outline"
              size="sm"
              disabled={busy || !note.trim()}
              onClick={() => void report()}
            >
              报告误判
            </Button>
          </div>
        )}

        {!slot.pending && !slot.final && (
          <p className="text-xs text-muted-foreground">
            正在识别这一槽，结束后可复核。
          </p>
        )}
      </div>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Timeline                                                            */
/* ------------------------------------------------------------------ */

function Timeline({
  dayView,
  slotsByStart,
  planMarks,
  calDay,
  today,
  now,
  showNow,
  onPick,
  onPrevDay,
  onNextDay,
  onBackToToday,
  scrollRef,
}: {
  dayView: DayView;
  slotsByStart: Map<number, TodaySlot>;
  planMarks: { start: number; end: number; title: string }[];
  calDay: string;
  today: string;
  now: number;
  showNow: boolean;
  onPick: (slot: TodaySlot) => void;
  onPrevDay: () => void;
  onNextDay: () => void;
  onBackToToday: () => void;
  scrollRef: React.RefObject<HTMLDivElement | null>;
}) {
  const dayStart = dayView.dayStart;
  const totalHeight = HOURS.length * HOUR_H;
  const nowTop = ((now - dayStart) / 900) * SLOT_H;
  const scrolledForRef = useRef<string | null>(null);

  // Open the timeline on the current hour instead of 00:00 — the day being
  // looked at is almost always "now". Runs once per selected day.
  useEffect(() => {
    if (!showNow || scrolledForRef.current === calDay) return;
    const el = scrollRef.current;
    if (!el || el.clientHeight === 0) return;
    scrolledForRef.current = calDay;
    const target = ((now - dayStart) / 900) * SLOT_H - el.clientHeight / 2;
    el.scrollTop = Math.max(0, target);
  }, [calDay, showNow, now, dayStart, scrollRef]);

  return (
    <Card className="flex min-w-0 flex-col">
      <div className="flex items-center justify-between gap-2 border-b px-5 py-3">
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="前一天"
          onClick={onPrevDay}
        >
          <ChevronLeft className="size-4" />
        </Button>
        <div className="flex min-w-0 flex-col items-center">
          <span className="truncate text-sm font-medium">{weekdayLabel(calDay)}</span>
          {calDay !== today && (
            <button
              type="button"
              onClick={onBackToToday}
              className="text-[11px] text-primary hover:underline"
            >
              回到今天
            </button>
          )}
        </div>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="后一天"
          onClick={onNextDay}
        >
          <ChevronRight className="size-4" />
        </Button>
      </div>
      <div
        ref={scrollRef}
        className="max-h-[540px] overflow-y-auto overscroll-contain"
      >
        <div className="relative" style={{ height: totalHeight }}>
          {HOURS.map((h) => (
            <div
              key={h}
              className="absolute left-0 flex w-11 justify-end pr-2"
              style={{ top: h * HOUR_H + 2 }}
            >
              <span className="text-[10px] tabular-nums text-muted-foreground">
                {pad2(h)}:00
              </span>
            </div>
          ))}
          {HOURS.slice(1).map((h) => (
            <div
              key={`line-${h}`}
              className="absolute left-11 right-3 border-t border-border/60"
              style={{ top: h * HOUR_H }}
            />
          ))}

          <div className="absolute inset-y-0 left-11 right-3">
            {Array.from({ length: SLOT_COUNT }, (_, i) => {
              const start = dayStart + i * 900;
              const slot = slotsByStart.get(start);
              const top = i * SLOT_H;

              if (!slot) {
                return (
                  <div
                    key={start}
                    className="absolute inset-x-0 rounded-[3px] bg-muted/40"
                    style={{ top: top + 1, height: SLOT_H - 2 }}
                  />
                );
              }

              const live = showNow && now >= start && now < start + 900 && !slot.final;
              const cat: CategoryKey = slot.pending
                ? "pending"
                : categoryOf(slot.dominant);
              const label = live
                ? "正在识别…"
                : dominantLabel(slot.dominant);

              return (
                <button
                  key={start}
                  type="button"
                  data-pending={slot.pending ? "true" : undefined}
                  onClick={() => onPick(slot)}
                  title={`${hm(start)} · ${dominantLabel(slot.dominant)} · 计入 ${slot.creditedMinutes} 分钟`}
                  className={cn(
                    "absolute inset-x-0 flex items-center overflow-hidden rounded-[3px] px-1.5 text-left text-[10px] leading-none transition-colors",
                    "hover:brightness-[0.97] dark:hover:brightness-110",
                    live && "animate-pulse-soft",
                  )}
                  style={{
                    top: top + 1,
                    height: SLOT_H - 2,
                    background: `linear-gradient(90deg, ${categoryColorAt(
                      cat,
                      slot.pending ? 32 : 28,
                    )}, ${categoryColorAt(cat, 12)})`,
                    borderLeft: `2px solid ${categoryColor(cat)}`,
                  }}
                >
                  <span className="truncate text-foreground/80">{label}</span>
                </button>
              );
            })}

            {planMarks.map((mark, i) => {
              const start = Math.max(mark.start, dayStart);
              const end = Math.min(mark.end, dayStart + 86400);
              if (end <= start) return null;
              return (
                <div
                  key={`mark-${i}`}
                  title={mark.title}
                  className="pointer-events-none absolute right-0.5 w-1 rounded-full bg-primary/50"
                  style={{
                    top: ((start - dayStart) / 900) * SLOT_H,
                    height: Math.max(6, ((end - start) / 900) * SLOT_H),
                  }}
                />
              );
            })}

            {showNow && (
              <div
                className="pointer-events-none absolute inset-x-0 flex items-center"
                style={{ top: nowTop }}
                aria-hidden
              >
                <span className="-ml-[3px] size-1.5 shrink-0 rounded-full bg-destructive shadow-sm shadow-destructive/50" />
                <span className="h-px flex-1 bg-gradient-to-r from-destructive to-destructive/30" />
              </div>
            )}
          </div>
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* 「计划 vs 实际」                                                     */
/* ------------------------------------------------------------------ */

const COMPARE_TAG: Record<string, string> = {
  idle: "bg-muted text-muted-foreground",
  doing: "bg-warning/15 text-warning",
  done: "bg-success/15 text-success",
};

/**
 * Today's hero. Each row is one self-assigned mainline task: what you planned
 * to spend on it, and what actually got credited inside that window.
 *
 * Without TickTick (or with no mainline task today) it falls back to naming
 * the apps the credited mainline minutes actually came from, so the card
 * never empties out.
 */
function TaskCompareCard({
  compare,
  appTop,
  emptyDay,
}: {
  compare: DayComparison;
  appTop: AppTopRow[];
  emptyDay: boolean;
}) {
  const rows = compare.tasks;
  const moved = rows.filter((r) => r.state !== "idle").length;
  const coreApps = appTop.filter((a) => a.dominant === "core");

  return (
    <Card className="flex flex-col">
      <div className="flex items-baseline gap-3 border-b px-5 py-3">
        <h2 className="text-sm font-medium">今天的主线</h2>
        {rows.length > 0 && (
          <span className="ml-auto text-xs text-muted-foreground">
            {moved} / {rows.length} 条有推进
          </span>
        )}
      </div>
      <div className="space-y-3 px-5 py-4">
        {emptyDay && <EmptyLine>这一天没有监测记录</EmptyLine>}

        {!emptyDay && rows.length === 0 && (
          <>
            <p className="text-xs text-muted-foreground">
              今天没有排主线的 TickTick 任务。下面是实际推进了主线的应用。
            </p>
            {coreApps.length === 0 ? (
              <EmptyLine>还没有计入主线的应用</EmptyLine>
            ) : (
              <ul className="space-y-2">
                {coreApps.slice(0, 5).map((a) => (
                  <li key={a.name} className="flex items-center gap-3 text-sm">
                    <span className="truncate">{a.name}</span>
                    <span className="ml-auto shrink-0 tabular-nums text-muted-foreground">
                      {minutesLabel(a.minutes)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </>
        )}

        {rows.map((r) => {
          const pct =
            r.plannedMinutes > 0
              ? Math.min(100, (r.actualMinutes / r.plannedMinutes) * 100)
              : 0;
          const idle = r.state === "idle";
          return (
            <div key={r.id} className="flex items-center gap-3">
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-3">
                  <span
                    className={cn(
                      "truncate text-sm font-medium",
                      idle && "text-muted-foreground",
                    )}
                  >
                    {r.title}
                  </span>
                  <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground">
                    计划 {hm(r.start)}–{hm(r.end)}
                  </span>
                  <span
                    className={cn(
                      "ml-auto shrink-0 text-sm font-semibold tabular-nums",
                      idle && "font-normal text-muted-foreground",
                    )}
                  >
                    {minutesLabel(r.actualMinutes)}
                  </span>
                </div>
                <Progress
                  value={pct}
                  className="mt-1.5 h-1.5"
                  aria-label={`${r.title} 推进进度`}
                  barClassName={idle ? "bg-transparent" : undefined}
                />
              </div>
              <span
                className={cn(
                  "shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium",
                  COMPARE_TAG[r.state],
                )}
              >
                {COMPARE_STATE_LABEL[r.state]}
              </span>
            </div>
          );
        })}

        {rows.length > 0 && compare.unplannedMinutes > 0 && (
          <p className="border-t border-dashed pt-3 text-xs text-muted-foreground">
            计划之外还推进了{" "}
            <strong className="font-semibold text-mainline">
              {minutesLabel(compare.unplannedMinutes)}
            </strong>
            主线 · 这段时间没有对应的 TickTick 任务
          </p>
        )}
      </div>
    </Card>
  );
}

/** The whole day as one strip: 96 cells, coloured by what owned each slot. */
function DayRibbon({
  dayStart,
  slotsByStart,
}: {
  dayStart: number;
  slotsByStart: Map<number, TodaySlot>;
}) {
  return (
    <div
      className="flex h-6 gap-px overflow-hidden rounded-lg bg-muted"
      role="img"
      aria-label="今天的时间分布"
    >
      {Array.from({ length: SLOT_COUNT }, (_, i) => {
        const slot = slotsByStart.get(dayStart + i * 900);
        if (!slot) return <i key={i} className="h-full flex-1" />;
        const cat: CategoryKey = slot.pending
          ? "pending"
          : categoryOf(slot.dominant);
        return (
          <i
            key={i}
            className="h-full flex-1"
            style={{ background: categoryColor(cat) }}
          />
        );
      })}
    </div>
  );
}

function LootTile({
  label,
  value,
  sub,
  icon,
}: {
  label: string;
  value: ReactNode;
  sub?: string;
  icon?: ReactNode;
}) {
  return (
    <div className="rounded-lg bg-muted/60 px-2.5 py-2">
      <p className="flex items-center gap-1 text-[11px] text-muted-foreground">
        {icon}
        {label}
      </p>
      <p className="mt-1 text-lg font-semibold leading-none tabular-nums tracking-tight">
        {value}
        {sub && (
          <span className="ml-1 text-[11px] font-normal text-muted-foreground">
            {sub}
          </span>
        )}
      </p>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Page                                                                */
/* ------------------------------------------------------------------ */

export function Today() {
  const [data, setData] = useState<TodayView | null>(null);
  const [dayView, setDayView] = useState<DayView | null>(null);
  const [calDay, setCalDay] = useState<string | null>(() =>
    peekStoredCalDay(window.localStorage),
  );
  const [error, setError] = useState<string | null>(null);
  const [confirmEnd, setConfirmEnd] = useState(false);
  const [busy, setBusy] = useState(false);
  const [freezeDate, setFreezeDate] = useState("");
  const [freezeMsg, setFreezeMsg] = useState<string | null>(null);
  const [freezeOpen, setFreezeOpen] = useState(false);
  const [selected, setSelected] = useState<TodaySlot | null>(null);
  const [openApp, setOpenApp] = useState<string | null>(null);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  const timelineRef = useRef<HTMLDivElement | null>(null);

  const refresh = useCallback(async () => {
    try {
      const t = await getToday();
      setData(t);
      const day = calDay ?? t.day;
      if (!calDay) setCalDay(t.day);
      const view = await getDayView(day);
      setDayView(view);
      setFreezeDate((prev) => {
        if (prev && t.freezeCandidates.includes(prev)) return prev;
        return t.defaultFreezeDate ?? t.freezeCandidates[0] ?? "";
      });
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [calDay]);

  useEffect(() => {
    consumeStoredCalDay(window.localStorage);
  }, []);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), 30_000);
    return () => clearInterval(id);
  }, [refresh]);

  useEffect(() => {
    const id = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 15_000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    setSelected(null);
    setOpenApp(null);
  }, [calDay]);

  async function handleEndToday() {
    setBusy(true);
    try {
      await endToday();
      setConfirmEnd(false);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleFreeze() {
    if (!freezeDate) return;
    setBusy(true);
    setFreezeMsg(null);
    try {
      await freezeDay(freezeDate);
      setFreezeMsg(`已冻结 ${freezeDate}，连胜已恢复。`);
      await refresh();
    } catch (e) {
      setFreezeMsg(String(e));
    } finally {
      setBusy(false);
    }
  }

  const slotsByStart = useMemo(() => {
    const m = new Map<number, TodaySlot>();
    for (const s of dayView?.slots ?? []) m.set(s.start, s);
    return m;
  }, [dayView]);

  const header = (
    <PageHeader
      title={<h1 className="text-lg font-semibold tracking-tight">今日</h1>}
      subtitle={data?.day}
      actions={
        <Button
          variant="outline"
          size="sm"
          disabled={busy || !data}
          onClick={() => setConfirmEnd(true)}
          className="text-destructive hover:bg-destructive/10 hover:text-destructive"
        >
          结束今天
        </Button>
      }
    />
  );

  if (error && !data) {
    return (
      <>
        {header}
        <div className="flex-1 overflow-y-auto">
          <div className="mx-auto max-w-5xl space-y-4 px-6 py-4">
            <Card className="flex flex-col items-center gap-3 p-8 text-center">
              <p className="text-sm text-destructive">{error}</p>
              <Button variant="outline" size="sm" onClick={() => void refresh()}>
                重试
              </Button>
            </Card>
          </div>
        </div>
      </>
    );
  }

  if (!data || !dayView || !calDay) {
    return (
      <>
        {header}
        <div className="flex-1 overflow-y-auto">
          <div className="mx-auto max-w-5xl space-y-4 px-6 py-4">
            <div className="grid grid-cols-3 gap-3">
              {[0, 1, 2].map((i) => (
                <div key={i} className="h-[76px] animate-shimmer rounded-xl bg-muted" />
              ))}
            </div>
            <SkeletonPanel rows={3} />
          </div>
        </div>
      </>
    );
  }

  const marks = planMarksFromSnapshots(dayView.planMarks ?? [], dayView.dayStart);
  const showNow = calDay === data.day;
  const isTodayCal = calDay === data.day;
  const activity = resolvedActivityMinutes(dayView.slots);
  const appTop: AppTopRow[] = isTodayCal ? data.appTop : dayView.appTop;
  const pendingCount = isTodayCal ? data.pendingCount : dayView.pendingCount;
  const pendingMinutes = pendingActivityMinutes(dayView.slots);
  const catMax = Math.max(1, ...CAT_ROWS.map((c) => activity[c.key]), pendingMinutes);
  const creditedMin = Math.floor(data.creditedSeconds / 60);
  const fillPct = data.goldDay
    ? 100
    : Math.min(100, (creditedMin / GOAL_MINUTES) * 100);
  const emptyDay = dayView.slots.length === 0;
  const ticktickTasks = dayView.ticktickTasks ?? [];
  const compare = compareDayTasks(ticktickTasks, dayView.slots);
  const observedMinutes = Object.values(activity).reduce((a, b) => a + b, 0);
  const corePct =
    observedMinutes > 0 ? Math.round((activity.core / observedMinutes) * 100) : 0;

  function scrollToFirstPending() {
    const el = timelineRef.current?.querySelector('[data-pending="true"]');
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
  }

  return (
    <>
      {header}
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-5xl space-y-4 px-6 py-4">
          <PermissionBanner />
          <EntertainmentBanner
            active={data.activeEntertainment}
            ended={data.endedEntertainment}
          />
          {error && (
            <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
              {error}
            </p>
          )}

          <TaskCompareCard compare={compare} appTop={appTop} emptyDay={emptyDay} />

          <div className="grid gap-4 lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)]">
            <Card className="flex flex-col">
              <div className="flex items-baseline gap-3 border-b px-5 py-3">
                <h2 className="text-sm font-medium">时间去哪了</h2>
                <span className="ml-auto text-xs tabular-nums text-muted-foreground">
                  观测 {minutesLabel(observedMinutes)}
                </span>
              </div>
              <div className="flex flex-1 flex-col justify-center gap-3 px-5 py-4">
                {emptyDay ? (
                  <EmptyLine>这一天没有监测记录</EmptyLine>
                ) : (
                  <>
                    <DayRibbon dayStart={dayView.dayStart} slotsByStart={slotsByStart} />
                    <div className="flex justify-between text-[10px] tabular-nums text-muted-foreground">
                      {[0, 4, 8, 12, 16, 20, 24].map((h) => (
                        <span key={h}>{pad2(h)}</span>
                      ))}
                    </div>
                    <p className="text-xs text-muted-foreground">
                      推进主线{" "}
                      <strong className="font-semibold text-mainline">
                        {minutesLabel(activity.core)}
                      </strong>
                      {observedMinutes > 0 && (
                        <span className="ml-1.5">占观测 {corePct}%</span>
                      )}
                    </p>
                    <dl className="grid grid-cols-2 gap-x-3 gap-y-1.5">
                      {CAT_ROWS.slice(0, 6).map((row) => (
                        <div
                          key={row.key}
                          className="flex items-center gap-2 text-[11px]"
                        >
                          <i
                            className="size-2 shrink-0 rounded-full"
                            style={{ background: categoryColor(row.cat) }}
                          />
                          <dt className="text-muted-foreground">{row.label}</dt>
                          <dd className="ml-auto tabular-nums text-muted-foreground">
                            {minutesLabel(activity[row.key])}
                          </dd>
                        </div>
                      ))}
                    </dl>
                  </>
                )}
              </div>
            </Card>

            <Card className="flex flex-col">
              <div className="flex items-baseline gap-3 border-b px-5 py-3">
                <h2 className="text-sm font-medium">攒到了</h2>
                <span className="ml-auto text-xs tabular-nums text-muted-foreground">
                  {calDay}
                </span>
              </div>
              <div className="flex flex-1 flex-col gap-3 px-5 py-4">
                <div className="grid grid-cols-3 gap-2.5">
                  <LootTile
                    label="硬币"
                    icon={<Coins className="size-3" aria-hidden />}
                    value={`+${data.coinsToday}`}
                    sub={`共 ${data.coinBalance}`}
                  />
                  <LootTile
                    label="能量"
                    icon={<Zap className="size-3" aria-hidden />}
                    value={data.xpToday}
                    sub={data.xpShopUnlocked ? "商店已解锁" : "未解锁"}
                  />
                  <LootTile
                    label="连胜"
                    icon={<Target className="size-3" aria-hidden />}
                    value={data.streak}
                    sub="天"
                  />
                </div>

                <div className="flex items-baseline justify-between gap-3 text-xs text-muted-foreground">
                  <span className="tabular-nums">
                    宝箱 {data.chest.have}/{data.chest.need}
                  </span>
                  <span className="tabular-nums">
                    黄金日 {data.gold.have}/{data.gold.need}
                  </span>
                  <span className="tabular-nums">
                    主线 {data.creditedLabel} / 8h
                  </span>
                </div>
                <Progress
                  value={fillPct}
                  aria-label="今日主线进度"
                  className="h-2"
                  barClassName="bg-gradient-to-r from-primary/70 via-primary to-primary/80 shadow-sm shadow-primary/40"
                />

                <div className="mt-auto flex items-center gap-3">
                  <StreakRing streak={data.streak} atRisk={data.atRisk} size={40} />
                  <div className="min-w-0 space-y-0.5">
                    <p className="text-xs text-muted-foreground">
                      {data.atRisk ? "未达宝箱有断连风险" : "已连续达成宝箱"}
                    </p>
                    {data.goldDay && (
                      <p className="text-xs text-warning" title={GOLD_DAY_MSG}>
                        已达成黄金日
                      </p>
                    )}
                    {!data.goldDay && data.firstCoreLabel && (
                      <p className="truncate text-xs text-muted-foreground">
                        {data.firstCoreLabel}
                      </p>
                    )}
                  </div>
                </div>
              </div>
            </Card>
          </div>

          <div className="grid gap-4 lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)]">
            <Card className="flex flex-col">
              <div className="border-b px-5 py-3">
                <h2 className="text-sm font-medium">类别</h2>
                <p className="mt-0.5 text-[11px] leading-relaxed text-muted-foreground">
                  跨槽求和各类分钟；待复核单独计，不含进其他类别。
                </p>
              </div>
              <div className="space-y-3 px-5 py-4">
                {emptyDay && <EmptyLine>这一天没有监测记录</EmptyLine>}
                {CAT_ROWS.map((row) => {
                  const mins = activity[row.key];
                  return (
                    <div key={row.key} className="space-y-1.5">
                      <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="flex items-center gap-2">
                          <i
                            className="size-2.5 shrink-0 rounded-full"
                            style={{ background: categoryColor(row.cat) }}
                          />
                          {row.label}
                        </span>
                        <span className="tabular-nums text-muted-foreground">
                          {mins} 分钟
                        </span>
                      </div>
                      <Progress
                        value={(mins / catMax) * 100}
                        color={`linear-gradient(90deg, ${categoryColor(
                          row.cat,
                        )}, ${categoryColorAt(row.cat, 55)})`}
                        aria-label={row.label}
                      />
                    </div>
                  );
                })}
                <div className="space-y-1.5">
                  <div className="flex items-center justify-between gap-2 text-xs">
                    <span className="flex items-center gap-2">
                      <i
                        className="size-2.5 shrink-0 rounded-full"
                        style={{ background: categoryColor("pending") }}
                      />
                      待复核
                    </span>
                    <span className="tabular-nums text-muted-foreground">
                      {pendingMinutes} 分钟
                    </span>
                  </div>
                  <Progress
                    value={(pendingMinutes / catMax) * 100}
                    color={`linear-gradient(90deg, ${categoryColor(
                      "pending",
                    )}, ${categoryColorAt("pending", 55)})`}
                    aria-label="待复核"
                  />
                </div>
                {pendingCount > 0 && (
                  <button
                    type="button"
                    onClick={scrollToFirstPending}
                    className="text-xs text-primary hover:underline"
                  >
                    {pendingCount} 条待复核，点按滚到时间轴
                  </button>
                )}
              </div>
            </Card>

            <Timeline
              dayView={dayView}
              slotsByStart={slotsByStart}
              planMarks={marks}
              calDay={calDay}
              today={data.day}
              now={now}
              showNow={showNow}
              onPick={setSelected}
              onPrevDay={() => setCalDay(addDays(calDay, -1))}
              onNextDay={() => setCalDay(addDays(calDay, 1))}
              onBackToToday={() => setCalDay(data.day)}
              scrollRef={timelineRef}
            />
          </div>

          <div className="grid gap-4 lg:grid-cols-2">
            <Card className="flex flex-col">
              <div className="border-b px-5 py-3">
                <h2 className="text-sm font-medium">当天 TickTick</h2>
              </div>
              <div className="px-5 py-4">
                {ticktickTasks.length === 0 ? (
                  <EmptyLine>
                    当天没有带时段的 TickTick 任务。可在设置里点同步任务。
                  </EmptyLine>
                ) : (
                  <ul className="space-y-2">
                    {ticktickTasks.map((task) => (
                      <li
                        key={task.id}
                        className="flex items-center justify-between gap-3 text-sm"
                      >
                        <div className="min-w-0">
                          <p className="truncate">{task.title}</p>
                          <p className="text-[11px] tabular-nums text-muted-foreground">
                            {hm(task.start)}–{hm(task.end)}
                          </p>
                        </div>
                        <Badge tone="outline">{ticktickRoleLabel(task.role)}</Badge>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </Card>

            <Card className="flex flex-col">
              <div className="border-b px-5 py-3">
                <h2 className="text-sm font-medium">当日应用</h2>
              </div>
              <div className="px-5 py-4">
                {appTop.length === 0 ? (
                  <EmptyLine>今天还没有应用明细</EmptyLine>
                ) : (
                  <ul className="space-y-1">
                    {appTop.slice(0, 5).map((row) => {
                      const cat = categoryOf(row.dominant);
                      const open = openApp === row.name;
                      return (
                        <li key={row.name}>
                          <button
                            type="button"
                            onClick={() => setOpenApp(open ? null : row.name)}
                            className="flex w-full items-center justify-between gap-3 rounded-lg px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent"
                          >
                            <span className="flex min-w-0 items-center gap-2">
                              <i
                                className="size-2.5 shrink-0 rounded-full"
                                style={{ background: categoryColor(cat) }}
                              />
                              <span className="truncate">{row.name}</span>
                            </span>
                            <span className="shrink-0 tabular-nums text-muted-foreground">
                              {row.minutes} 分钟
                            </span>
                          </button>
                          {open && (
                            <p className="px-2 pb-1 text-[11px] text-muted-foreground">
                              {row.name} 当日 {row.minutes} 分钟 ·{" "}
                              {dominantLabel(
                                row.dominant === "core"
                                  ? "core_research"
                                  : row.dominant === "support"
                                    ? "research_support"
                                    : row.dominant === "side"
                                      ? "side_project"
                                      : row.dominant,
                              )}
                            </p>
                          )}
                        </li>
                      );
                    })}
                  </ul>
                )}
              </div>
            </Card>
          </div>

          <Card>
            <button
              type="button"
              onClick={() => setFreezeOpen((open) => !open)}
              aria-expanded={freezeOpen}
              className="flex w-full items-center justify-between gap-3 px-5 py-3 text-left"
            >
              <span className="flex min-w-0 items-center gap-2">
                <Shield className="size-4 shrink-0 text-muted-foreground" aria-hidden />
                <span className="text-sm font-medium">保护连胜</span>
                <span className="truncate text-xs text-muted-foreground">
                  每月 2 次机会，把没达标的一天冻结掉，连胜不断
                </span>
              </span>
              <ChevronDown
                className={cn(
                  "size-4 shrink-0 text-muted-foreground transition-transform",
                  freezeOpen && "rotate-180",
                )}
                aria-hidden
              />
            </button>
            {freezeOpen && (
              <div className="space-y-4 border-t px-5 py-4">
                <p className="text-xs leading-relaxed text-muted-foreground">
                  每天主线满 6 小时算一次「达标」，连续达标累积连胜，漏一天就断。
                  每月有 <strong className="font-medium text-foreground">2 次</strong>机会把一个没达标的工作日
                  <strong className="font-medium text-foreground">冻结</strong>掉——这一天不再算断连，
                  连胜从冻结前的天数接着算。
                </p>

                {data.freezeCandidates.length > 0 ? (
                  <div className="space-y-1.5">
                    <span className="text-xs font-medium">要冻结哪一天</span>
                    <div className="flex flex-wrap items-center gap-2">
                      <Select
                        className="w-60"
                        value={freezeDate}
                        disabled={busy}
                        onChange={(e) => setFreezeDate(e.target.value)}
                      >
                        {data.freezeCandidates.map((d) => (
                          <option key={d} value={d}>
                            {freezeDayLabel(d)}
                          </option>
                        ))}
                      </Select>
                      <Button
                        variant="outline"
                        disabled={busy || !freezeDate}
                        onClick={() => void handleFreeze()}
                      >
                        {busy ? "冻结中…" : "冻结这一天"}
                      </Button>
                    </div>
                    {data.atRisk && (
                      <p className="text-xs text-warning">
                        今天还没到 6 小时，再不冻结就会断连。
                      </p>
                    )}
                  </div>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    当前没有可冻结的未达标工作日。
                  </p>
                )}
                {freezeMsg && (
                  <p className="text-xs text-muted-foreground">{freezeMsg}</p>
                )}
              </div>
            )}
          </Card>
        </div>
      </div>

      <ConfirmEndDay
        open={confirmEnd}
        busy={busy}
        onCancel={() => setConfirmEnd(false)}
        onConfirm={() => void handleEndToday()}
      />
      {selected && (
        <SlotReview
          slot={selected}
          day={calDay}
          onClose={() => setSelected(null)}
          onChange={() => void refresh()}
        />
      )}
    </>
  );
}
