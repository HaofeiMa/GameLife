import { ChevronLeft, ChevronRight } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { PageHeader } from "../components/PageHeader";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card } from "../components/ui/card";
import { EmptyLine } from "../components/ui/empty-state";
import { Segmented } from "../components/ui/segmented";
import { Select } from "../components/ui/select";
import { SkeletonPanel } from "../components/ui/skeleton";
import {
  getAppReport,
  getMonthReport,
  getRhythmReport,
  getSettings,
  getToday,
  getWeek,
  saveSettings,
  type AppReportView,
  type AppSettings,
  type MonthDayCell,
  type MonthReportView,
  type RhythmReportView,
  type SlotActivityMinutes,
  type StatsRangeKind,
  type WeekDayRow,
  type WeekHourRow,
  type WeekView,
} from "../lib/api";
import { addDays } from "../lib/calendar";
import { calendarCells, monthHeatCell } from "../lib/monthGrid";
import {
  dayStackCaption,
  monthCellNote,
  monthShowsLedgerCards,
  weekHasObservation,
  weekRangeLabel,
} from "../lib/statsView";
import {
  categoryColor,
  categoryColorAt,
  type CategoryKey,
} from "../lib/theme";
import { cn } from "../lib/utils";
import { heatTone } from "../lib/weekHeat";

type Segment = "week" | "month" | "rhythm" | "app";

const SEGMENTS: { value: Segment; label: string }[] = [
  { value: "week", label: "周" },
  { value: "month", label: "月" },
  { value: "rhythm", label: "习惯" },
  { value: "app", label: "应用" },
];

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];
const HEAT_HOURS = Array.from({ length: 14 }, (_, i) => i + 8);

const WEEK_CATEGORIES: { key: keyof WeekView; cat: CategoryKey; label: string }[] = [
  { key: "core", cat: "mainline", label: "主线" },
  { key: "support", cat: "support", label: "辅助" },
  { key: "side", cat: "side", label: "支线" },
  { key: "admin", cat: "admin", label: "杂项" },
  { key: "distraction", cat: "entertainment", label: "娱乐" },
  { key: "away", cat: "away", label: "离开" },
  { key: "unobserved", cat: "unobserved", label: "未观测" },
  { key: "pendingReview", cat: "pending", label: "待复核" },
];

const MONTH_CATEGORIES: { key: keyof SlotActivityMinutes; cat: CategoryKey; label: string }[] = [
  { key: "core", cat: "mainline", label: "主线" },
  { key: "support", cat: "support", label: "辅助" },
  { key: "side", cat: "side", label: "支线" },
  { key: "admin", cat: "admin", label: "杂项" },
  { key: "distraction", cat: "entertainment", label: "娱乐" },
  { key: "away", cat: "away", label: "离开" },
  { key: "unobserved", cat: "unobserved", label: "未观测" },
];

/**
 * Which policy list an app row can be filed under. "待定" is not a list —
 * it means the app matches no rule, so the slot stays gray and the text /
 * vision AI decides from the window title and screenshot.
 */
const APP_LISTS = [
  { value: "unlisted", label: "待定（交给 AI）" },
  { value: "mainline", label: "主线" },
  { value: "side", label: "支线" },
  { value: "admin", label: "杂项" },
  { value: "entertainment", label: "娱乐" },
  { value: "reading", label: "阅读" },
] as const;

type AppListTarget = (typeof APP_LISTS)[number]["value"];

const LIST_FIELD: Record<
  Exclude<AppListTarget, "unlisted">,
  "trustedApps" | "sideProjectRules" | "adminApps" | "distractionRules" | "readingApps"
> = {
  mainline: "trustedApps",
  side: "sideProjectRules",
  admin: "adminApps",
  entertainment: "distractionRules",
  reading: "readingApps",
};

const LIST_FIELDS = Object.values(LIST_FIELD);

/* ------------------------------ helpers ------------------------------ */

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

function todayIso(): string {
  const n = new Date();
  return `${n.getFullYear()}-${pad2(n.getMonth() + 1)}-${pad2(n.getDate())}`;
}

function isoMonday(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const dt = new Date(y, m - 1, d);
  const offset = (dt.getDay() + 6) % 7;
  return addDays(day, -offset);
}

function isoSunday(day: string): string {
  return addDays(isoMonday(day), 6);
}

function shiftMonth(year: number, month: number, delta: number): { year: number; month: number } {
  const dt = new Date(year, month - 1 + delta, 1);
  return { year: dt.getFullYear(), month: dt.getMonth() + 1 };
}

function isWeekendDay(day: string): boolean {
  const [y, m, d] = day.split("-").map(Number);
  const wd = new Date(y, m - 1, d).getDay();
  return wd === 0 || wd === 6;
}

function observedMinutes(view: {
  core: number;
  support: number;
  admin: number;
  side: number;
  distraction: number;
  away: number;
  pendingReview?: number;
}): number {
  return (
    view.core +
    view.support +
    view.admin +
    view.side +
    view.distraction +
    view.away +
    (view.pendingReview ?? 0)
  );
}

function shareLabel(mins: number, observed: number): string {
  if (observed <= 0) return "无观测";
  return `${Math.round((mins / observed) * 100)}%`;
}

function minutesOfWeek(view: WeekView): number {
  return WEEK_CATEGORIES.reduce((sum, c) => sum + (view[c.key] as number), 0);
}

function activityTotal(a: SlotActivityMinutes): number {
  return a.core + a.support + a.admin + a.side + a.distraction + a.away + a.unobserved;
}

function hintLabel(dominant: string): string {
  switch (dominant) {
    case "core":
    case "core_research":
      return "主线";
    case "support":
    case "research_support":
      return "辅助";
    case "admin":
      return "杂项";
    case "side":
    case "side_project":
      return "支线";
    case "distraction":
      return "娱乐";
    case "away":
    case "break_away":
      return "离开";
    case "unobserved":
      return "未观测";
    default:
      return "未分";
  }
}

function listedLabel(listed: string): string {
  switch (listed) {
    case "mainline":
      return "主线";
    case "side":
      return "支线";
    case "admin":
      return "杂项";
    case "entertainment":
      return "娱乐";
    case "reading":
      return "阅读";
    case "never_capture":
      return "永不截屏";
    default:
      return "未列入";
  }
}

/** The report's `listedAs` key → the value the picker should show. */
function currentListOf(listedAs: string): AppListTarget {
  return APP_LISTS.some((o) => o.value === listedAs)
    ? (listedAs as AppListTarget)
    : "unlisted";
}

function wowLabel(delta: number | null): string {
  if (delta == null) return "—";
  if (delta === 0) return "持平";
  const sign = delta > 0 ? "+" : "";
  return `${sign}${delta} 分钟`;
}

function pct(rate: number): string {
  if (!Number.isFinite(rate)) return "无观测";
  return `${Math.round(rate * 100)}%`;
}

function monthAnchor(year: number, month: number): string {
  return `${year}-${pad2(month)}-01`;
}

/* ----------------------------静 elements ---------------------------- */

/**
 * The mockup's .card + .ch. `meta` is the short right-aligned note in the
 * header; `caption` is the longer explanation, which the design drops to a
 * dashed footnote at the bottom of the card rather than under the title.
 */
function PanelCard({
  title,
  meta,
  caption,
  children,
  wide,
  className,
}: {
  title: string;
  meta?: ReactNode;
  caption?: string;
  children: ReactNode;
  wide?: boolean;
  className?: string;
}) {
  return (
    <Card className={cn("flex flex-col", wide && "lg:col-span-2", className)}>
      <div className="flex items-baseline gap-[9px] px-[18px] pt-3 pb-[9px]">
        <h2 className="text-[13.5px] font-semibold tracking-[-0.005em]">
          {title}
        </h2>
        {meta != null && (
          <span className="ml-auto whitespace-nowrap text-[11px] text-muted-foreground">
            {meta}
          </span>
        )}
      </div>
      <div className="flex flex-1 flex-col gap-3 px-[18px] pt-0.5 pb-3.5">
        {children}
        {caption && (
          <p className="mt-auto border-t border-dashed border-hairline pt-2.5 text-[11px] leading-relaxed text-muted-foreground">
            {caption}
          </p>
        )}
      </div>
    </Card>
  );
}

function Legend({ items }: { items: { label: string; cat?: CategoryKey; dot?: string }[] }) {
  return (
    <div className="mb-3 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[11px] text-muted-foreground">
      {items.map((item) => (
        <span key={item.label} className="inline-flex items-center gap-1.5">
          <i
            className="size-2.5 shrink-0 rounded-full"
            style={{ background: item.dot ?? categoryColor(item.cat ?? "away") }}
          />
          {item.label}
        </span>
      ))}
    </div>
  );
}

function MiniStat({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="space-y-0.5">
      <p className="text-[11px] text-muted-foreground">{label}</p>
      <p className="text-base font-semibold tabular-nums tracking-tight">{value}</p>
    </div>
  );
}

function CategoryPanel({
  rows,
  observed,
}: {
  rows: { key: string; label: string; cat: CategoryKey; mins: number }[];
  observed: number;
}) {
  const max = Math.max(1, ...rows.map((r) => r.mins));
  return (
    <PanelCard
      title="按类别"
      meta={`跨槽求和 · 观测 ${Math.round(observed / 60)} 分钟`}
      caption="跨槽求和 activity 秒数 / 60 取整；占观测比不含未观测。条长按最大值归一，不是按 8 小时——这一段最长的那一类占满。"
    >
      <div className="space-y-3">
        {rows.map((c) => (
          <div key={c.key} className="space-y-1.5">
            <div className="flex items-center justify-between gap-2 text-xs">
              <span className="flex items-center gap-2">
                <i
                  className="size-2.5 shrink-0 rounded-full"
                  style={{ background: categoryColor(c.cat) }}
                />
                {c.label}
              </span>
              <span className="tabular-nums text-muted-foreground">
                {c.mins} 分钟 ·{" "}
                {c.key === "unobserved" ? "—" : shareLabel(c.mins, observed)}
              </span>
            </div>
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
              <div
                className="h-full rounded-full transition-[width] duration-500 ease-out"
                style={{
                  width: `${(c.mins / max) * 100}%`,
                  background: `linear-gradient(90deg, ${categoryColor(
                    c.cat,
                  )}, ${categoryColorAt(c.cat, 55)})`,
                }}
              />
            </div>
          </div>
        ))}
      </div>
    </PanelCard>
  );
}

function DailyPanel({ days }: { days: WeekDayRow[] }) {
  const max = Math.max(1, ...days.map((d) => d.core + d.side + d.chore));
  return (
    <PanelCard
      title="按天"
      meta="主线 / 支线 / 杂项堆叠"
      caption="无堆叠分钟标 —，不是假 0。周末照常记录，未达标不惩罚。"
    >
      <Legend
        items={[
          { label: "主线", cat: "mainline" },
          { label: "支线", cat: "side" },
          { label: "杂项", cat: "admin" },
        ]}
      />
      <div className="flex items-end gap-2">
        {days.map((d, i) => {
          const weekend = isWeekendDay(d.day);
          const total = d.core + d.side + d.chore;
          return (
            <div key={d.day} className="flex min-w-0 flex-1 flex-col items-center gap-1.5">
              <div
                className="flex h-36 w-full flex-col justify-end overflow-hidden rounded-md bg-muted/40"
                title={`${total} 分钟`}
              >
                {total > 0 && (
                  <>
                    {d.core > 0 && (
                      <div
                        style={{
                          height: `${(d.core / max) * 100}%`,
                          background: `linear-gradient(180deg, ${categoryColor("mainline")}, ${categoryColorAt("mainline", 70)})`,
                        }}
                      />
                    )}
                    {d.side > 0 && (
                      <div
                        style={{
                          height: `${(d.side / max) * 100}%`,
                          background: `linear-gradient(180deg, ${categoryColor("side")}, ${categoryColorAt("side", 70)})`,
                        }}
                      />
                    )}
                    {d.chore > 0 && (
                      <div
                        style={{
                          height: `${(d.chore / max) * 100}%`,
                          background: `linear-gradient(180deg, ${categoryColor("admin")}, ${categoryColorAt("admin", 70)})`,
                        }}
                      />
                    )}
                  </>
                )}
              </div>
              <span className="text-xs font-medium tabular-nums">
                {dayStackCaption(weekend, total)}
              </span>
              <span className="w-full text-center text-[10px] text-muted-foreground">
                {`周${WEEKDAYS[i] ?? ""}`}
              </span>
              <span
                className={cn(
                  "w-full truncate text-center text-[10px] tabular-nums",
                  weekend ? "text-muted-foreground/60" : "text-muted-foreground/80",
                )}
              >
                {d.day.slice(5).replace("-", "/")}
              </span>
            </div>
          );
        })}
      </div>
    </PanelCard>
  );
}

function HeatPanel({ hours }: { hours: WeekHourRow[] }) {
  const byHour = new Map(hours.map((h) => [h.hour, h]));
  return (
    <PanelCard
      title="高效时段"
      meta="工作日 8–21 点 · 主线分钟"
      caption="该小时主线秒 / 观测秒。没有观测的小时是米色，不是 0——两者含义不同。"
    >
      <div className="grid grid-cols-[repeat(14,minmax(0,1fr))] gap-1.5">
        {HEAT_HOURS.map((hour) => {
          const row = byHour.get(hour) ?? { hour, core: 0, observed: 0 };
          const tone = heatTone(row.core, row.observed);
          const mins = Math.floor(row.core / 60);
          const observed = row.observed > 0;
          return (
            <div key={hour} className="flex flex-col items-center gap-1">
              <div
                className={cn(
                  "flex h-12 w-full items-center justify-center rounded-md text-[11px] font-medium tabular-nums transition-all duration-200",
                  observed && "hover:ring-2 hover:ring-primary/40",
                )}
                title={
                  observed
                    ? `${hour}:00 主线 ${mins} 分钟 / 观测 ${Math.floor(row.observed / 60)} 分钟`
                    : `${hour}:00 无观测`
                }
                style={{
                  background: observed
                    ? `linear-gradient(160deg, ${categoryColorAt(
                        "mainline",
                        14 + tone * 42,
                      )}, ${categoryColorAt("mainline", 6 + tone * 26)})`
                    : "hsl(var(--muted))",
                }}
              >
                {observed ? mins : "·"}
              </div>
              <span className="text-[10px] tabular-nums text-muted-foreground">{hour}</span>
            </div>
          );
        })}
      </div>
    </PanelCard>
  );
}

function WeekNumbers({
  data,
  streak,
  observed,
}: {
  data: WeekView;
  streak: number | null;
  observed: number;
}) {
  const playShare = observed <= 0 ? "无观测" : pct(data.distractionObservedRatio);
  return (
    <PanelCard
      title="本周数字"
      meta="达标只计工作日"
      caption="主线小时来自 credited 秒 / 3600。环比无上周槽则为 —。"
    >
      <div className="grid grid-cols-2 gap-x-4 gap-y-5 sm:grid-cols-3 lg:grid-cols-6">
        <MiniStat label="主线" value={`${data.coreHours.toFixed(1)} 小时`} />
        <MiniStat label="较上周" value={wowLabel(data.wowCoreDeltaMinutes)} />
        <MiniStat label="娱乐占观测" value={playShare} />
        <MiniStat label="待复核率" value={pct(data.pendingOverResolved)} />
        <MiniStat label="≥6h / ≥8h" value={`${data.daysGe6h} / ${data.daysGe8h} 日`} />
        <MiniStat label="连胜" value={streak == null ? "—" : `${streak} 天`} />
      </div>
    </PanelCard>
  );
}

function MonthCalendar({
  year,
  month,
  days,
  today,
  onPickDay,
}: {
  year: number;
  month: number;
  days: MonthDayCell[];
  today: string;
  onPickDay: (day: string) => void;
}) {
  const byDay = new Map(days.map((d) => [d.day, d]));
  const cells = calendarCells(year, month);
  return (
    <PanelCard
      wide
      title="热力月历"
      caption="颜色按当天 credited 主线秒 / 28800（8h）钳制 0–1。周末同样着色。点格打开该日日报。"
    >
      <div className="grid grid-cols-7 gap-1.5">
        {WEEKDAYS.map((w) => (
          <div
            key={w}
            className="pb-1 text-center text-[10px] font-medium text-muted-foreground"
          >
            {w}
          </div>
        ))}
        {cells.map((c, i) => {
          if (!c.day) return <div key={`b-${i}`} className="aspect-square" />;
          const row = byDay.get(c.day);
          const weekend = row?.isWeekend ?? isWeekendDay(c.day);
          const future = row?.isFuture ?? false;
          const tone = monthHeatCell(row?.creditedCore ?? 0);
          const coreMin = Math.floor((row?.creditedCore ?? 0) / 60);
          const credited = row?.creditedCore ?? 0;

          return (
            <button
              key={c.day}
              type="button"
              onClick={() => onPickDay(c.day!)}
              title={`${c.day} · ${monthCellNote(future, coreMin)}`}
              className={cn(
                "flex aspect-square flex-col items-center justify-center rounded-md text-xs transition-all duration-200",
                !future && "hover:-translate-y-0.5 hover:shadow-md hover:shadow-primary/10",
                future && "cursor-default opacity-50",
                weekend && "ring-1 ring-inset ring-border",
                c.day === today && "ring-2 ring-primary",
              )}
              style={{
                background: future
                  ? undefined
                  : credited > 0
                    ? `linear-gradient(160deg, ${categoryColorAt(
                        "mainline",
                        12 + tone * 42,
                      )}, ${categoryColorAt("mainline", 6 + tone * 24)})`
                    : "hsl(var(--muted))",
              }}
            >
              <span className="font-medium tabular-nums leading-none">
                {c.day.slice(8)}
              </span>
              <span className="mt-0.5 text-[9px] leading-none text-muted-foreground">
                {monthCellNote(future, coreMin)}
              </span>
            </button>
          );
        })}
      </div>
    </PanelCard>
  );
}

function MonthExtras({
  data,
  streak,
}: {
  data: MonthReportView;
  streak: number | null;
}) {
  const emptyCoins =
    data.coinsEarned === 0 && data.coinsSpent === 0 && data.xpEarned === 0;
  return (
    <>
      <PanelCard
        title="硬币与能量"
        caption="本月 ledger 正增量合计 vs 兑换支出。能量只展示本月获得，不含未完成娱乐折算。"
      >
        {emptyCoins ? (
          <EmptyLine>本月还没有账本记录</EmptyLine>
        ) : (
          <div className="grid grid-cols-3 gap-4">
            <MiniStat label="硬币获得" value={data.coinsEarned} />
            <MiniStat label="兑换支出" value={data.coinsSpent} />
            <MiniStat label="能量获得" value={data.xpEarned} />
          </div>
        )}
      </PanelCard>
      <PanelCard
        wide
        title="月份徽章"
        caption="黄金日按 credited ≥ 8h。冻结按被保护日所在月。连胜是今日快照，不考古历史最长。"
      >
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
          <MiniStat label="黄金日" value={`${data.goldDays} 天`} />
          <MiniStat label="冻结" value={`${data.freezeCount} 次`} />
          <MiniStat label="本月完成" value={`${data.completedDays} 日`} />
          <MiniStat label="当前连胜" value={streak == null ? "—" : `${streak} 天`} />
        </div>
      </PanelCard>
    </>
  );
}

function RhythmPanel({ data }: { data: RhythmReportView }) {
  const hasStart = data.startHours.some((s) => s.hour != null);
  const runMinutes = data.distractionRunSlots * 15;
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <PanelCard
        title="开工时刻"
        caption="每个工作日第一个 credited 主线 > 0 的槽开始钟点。尚无主线的日子不画点。"
      >
        {hasStart ? (
          <div className="flex items-end gap-2">
            {data.startHours.map((s) => (
              <div key={s.day} className="flex min-w-0 flex-1 flex-col items-center gap-1.5">
                <div className="relative h-36 w-full rounded-md bg-muted/40">
                  {s.hour != null && (
                    <div
                      className="absolute left-1/2 size-3 -translate-x-1/2 translate-y-1/2 rounded-full"
                      style={{
                        bottom: `${(s.hour / 24) * 100}%`,
                        background: categoryColor("mainline"),
                      }}
                      title={`${s.day} ${s.hour} 点`}
                    />
                  )}
                </div>
                <span className="truncate text-[10px] text-muted-foreground">
                  {s.day.slice(5)}
                </span>
                <span className="text-xs font-medium tabular-nums">
                  {s.hour == null ? "—" : `${s.hour}点`}
                </span>
              </div>
            ))}
          </div>
        ) : (
          <EmptyLine>尚无主线开工记录</EmptyLine>
        )}
      </PanelCard>

      <PanelCard
        title="达标率"
        caption="范围内工作日 credited ≥6h / ≥8h 的比例。周末照常记录，但不进分母、未达标不惩罚。"
      >
        <div className="grid grid-cols-2 gap-4">
          <MiniStat label="≥6h" value={pct(data.rate6h)} />
          <MiniStat label="≥8h" value={pct(data.rate8h)} />
        </div>
        <p className="mt-4 text-[11px] text-muted-foreground">周末未达标不断连</p>
      </PanelCard>

      <PanelCard
        title="娱乐连段"
        caption="连续 ≥3 个槽 dominant 为娱乐的次数与总分钟（槽 × 15）。用来看是不是一滑就半小时。"
      >
        {data.distractionRunCount === 0 ? (
          <EmptyLine>没有连续娱乐段</EmptyLine>
        ) : (
          <div className="grid grid-cols-2 gap-4">
            <MiniStat label="次数" value={data.distractionRunCount} />
            <MiniStat label="总分钟" value={runMinutes} />
          </div>
        )}
      </PanelCard>

      <PanelCard title="深时段" caption="范围内主线占观测比最高的三个钟点（热力同一口径）。">
        {data.peakHours.length === 0 ? (
          <EmptyLine>没有足够的小时观测</EmptyLine>
        ) : (
          <div className="flex flex-wrap gap-2">
            {data.peakHours.map((h) => (
              <Badge key={h} tone="primary" className="text-xs">
                {h} 点
              </Badge>
            ))}
          </div>
        )}
      </PanelCard>
    </div>
  );
}

function AppPanel({
  data,
  busyApp,
  onAssign,
}: {
  data: AppReportView;
  busyApp: string | null;
  onAssign: (app: string, target: AppListTarget) => void;
}) {
  const newSet = new Set(data.newcomers);
  const apps = [...data.apps].sort((a, b) => {
    const an = newSet.has(a.name) ? 0 : 1;
    const bn = newSet.has(b.name) ? 0 : 1;
    if (an !== bn) return an - bn;
    return b.minutes - a.minutes;
  });
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <PanelCard
        wide
        title="应用"
        caption="范围内 app_day_stats 按 hint 秒 / 60 取整。主类别取秒最大者。右侧可直接把应用归到某个名单；选「待定」则不进任何规则，留给 AI 按标题与截图判断。改动从下一个还没开始的槽生效。"
      >
        {data.newcomers.length > 0 && (
          <p className="mb-3 rounded-lg border border-warning/30 bg-warning/10 px-3 py-2 text-xs leading-relaxed text-warning">
            有 <strong className="font-medium">{data.newcomers.length}</strong>{" "}
            个应用还没在任何名单里，标了「新」。用右侧的菜单归类；不确定就留「待定」，
            交给 AI 按标题和截图判断。
          </p>
        )}
        {apps.length === 0 ? (
          <>
            <EmptyLine>无应用明细</EmptyLine>
            <p className="mt-2 text-[11px] text-muted-foreground">
              从本版本上线后的槽才有 App 明细。
            </p>
          </>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b text-left text-xs text-muted-foreground">
                  <th className="h-9 pr-3 font-medium">名称</th>
                  <th className="h-9 pr-3 text-right font-medium">分钟</th>
                  <th className="h-9 pr-3 font-medium">主类别</th>
                  <th className="h-9 w-40 font-medium">名单</th>
                </tr>
              </thead>
              <tbody>
                {apps.map((row) => (
                  <tr
                    key={row.name}
                    className="border-b transition-colors last:border-0 hover:bg-muted/50"
                  >
                    <td className="py-2 pr-3">
                      <span className="flex items-center gap-2">
                        <span className="truncate">{row.name}</span>
                        {newSet.has(row.name) && <Badge tone="warning">新</Badge>}
                      </span>
                    </td>
                    <td className="py-2 pr-3 text-right tabular-nums">{row.minutes}</td>
                    <td className="py-2 pr-3">{hintLabel(row.dominant)}</td>
                    <td className="py-2">
                      {row.listedAs === "never_capture" ? (
                        <Badge tone="neutral">{listedLabel(row.listedAs)}</Badge>
                      ) : (
                        <Select
                          size="sm"
                          className="w-36"
                          aria-label={`${row.name} 的名单`}
                          disabled={busyApp !== null}
                          value={currentListOf(row.listedAs)}
                          onChange={(e) =>
                            onAssign(row.name, e.target.value as AppListTarget)
                          }
                        >
                          {APP_LISTS.map((opt) => (
                            <option key={opt.value} value={opt.value}>
                              {busyApp === row.name ? "保存中…" : opt.label}
                            </option>
                          ))}
                        </Select>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </PanelCard>

      <PanelCard title="浏览器 Host" caption="Top 15 host，类别同样来自 hint 秒，不是 dominant × 15。">
        {data.hosts.length === 0 ? (
          <EmptyLine>没有浏览器 host 明细</EmptyLine>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b text-left text-xs text-muted-foreground">
                  <th className="h-9 pr-3 font-medium">Host</th>
                  <th className="h-9 pr-3 text-right font-medium">分钟</th>
                  <th className="h-9 font-medium">主类别</th>
                </tr>
              </thead>
              <tbody>
                {data.hosts.map((row) => (
                  <tr
                    key={row.host}
                    className="border-b transition-colors last:border-0 hover:bg-muted/50"
                  >
                    <td className="py-2 pr-3">{row.host}</td>
                    <td className="py-2 pr-3 text-right tabular-nums">{row.minutes}</td>
                    <td className="py-2">{hintLabel(row.dominant)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </PanelCard>

      <PanelCard
        wide
        title="保护时长"
        caption="永不截屏 / 密码框，不发币，不进主线也不进娱乐。"
      >
        <div className="flex items-baseline gap-3">
          <span className="text-3xl font-semibold tabular-nums tracking-tight">
            {data.protectedMinutes}
          </span>
          <span className="text-sm text-muted-foreground">分钟未观测</span>
        </div>
      </PanelCard>
    </div>
  );
}

/* -------------------------------- page ------------------------------- */

export function Stats({ onPickDay }: { onPickDay: (day: string) => void }) {
  const [segment, setSegment] = useState<Segment>("week");
  const [rangeKind, setRangeKind] = useState<StatsRangeKind>("week");
  const [weekAnchor, setWeekAnchor] = useState(todayIso);
  const [monthYear, setMonthYear] = useState(() => new Date().getFullYear());
  const [monthNum, setMonthNum] = useState(() => new Date().getMonth() + 1);
  const [weekData, setWeekData] = useState<WeekView | null>(null);
  const [monthData, setMonthData] = useState<MonthReportView | null>(null);
  const [rhythm, setRhythm] = useState<RhythmReportView | null>(null);
  const [apps, setApps] = useState<AppReportView | null>(null);
  const [streak, setStreak] = useState<number | null>(null);
  const [todayDay, setTodayDay] = useState(todayIso);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [reload, setReload] = useState(0);
  const [busyApp, setBusyApp] = useState<string | null>(null);
  const [assignError, setAssignError] = useState<string | null>(null);

  const effectiveKind: StatsRangeKind =
    segment === "month" ? "month" : segment === "week" ? "week" : rangeKind;

  function pickSegment(id: Segment) {
    setSegment(id);
    if (id === "week") setRangeKind("week");
    if (id === "month") setRangeKind("month");
  }

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    const anchor =
      effectiveKind === "week" ? weekAnchor : monthAnchor(monthYear, monthNum);

    async function load() {
      try {
        if (segment === "week") {
          const w = await getWeek(weekAnchor);
          if (cancelled) return;
          setWeekData(w);
          try {
            const t = await getToday();
            if (cancelled) return;
            setStreak(t.streak);
            setTodayDay(t.day);
          } catch {
            /* 连胜取今日快照失败时仍展示周报 */
          }
        } else if (segment === "month") {
          const m = await getMonthReport(monthYear, monthNum);
          if (cancelled) return;
          setMonthData(m);
          try {
            const t = await getToday();
            if (cancelled) return;
            setStreak(t.streak);
            setTodayDay(t.day);
          } catch {
            /* 连胜取今日快照失败时仍展示月报 */
          }
        } else if (segment === "rhythm") {
          const r = await getRhythmReport(effectiveKind, anchor);
          if (cancelled) return;
          setRhythm(r);
        } else {
          const a = await getAppReport(effectiveKind, anchor);
          if (cancelled) return;
          setApps(a);
        }
        if (!cancelled) setError(null);
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, [segment, effectiveKind, weekAnchor, monthYear, monthNum, reload]);

  /**
   * File an app under one policy list, or under none. Reuses the existing
   * settings round trip, and passes `updatePolicy: true` so a new policy
   * version is pinned — per the versioning invariant this only affects
   * slots that have not started yet.
   */
  async function assignApp(app: string, target: AppListTarget) {
    setBusyApp(app);
    setAssignError(null);
    try {
      const current = await getSettings();
      const withoutApp = (list: string[]) =>
        list.filter((x) => x.toLowerCase() !== app.toLowerCase());
      const next: AppSettings = { ...current };
      for (const field of LIST_FIELDS) {
        next[field] = withoutApp(current[field]);
      }
      if (target !== "unlisted") {
        const field = LIST_FIELD[target];
        next[field] = [...next[field], app];
      }
      await saveSettings(next, true);
      setReload((n) => n + 1);
    } catch (e) {
      setAssignError(String(e));
    } finally {
      setBusyApp(null);
    }
  }

  const currentMonday = isoMonday(todayDay);
  const viewMonday = isoMonday(weekAnchor);
  const thisMonth = {
    year: Number(todayDay.slice(0, 4)),
    month: Number(todayDay.slice(5, 7)),
  };
  const atCurrentWeek = viewMonday >= currentMonday;
  const atCurrentMonth =
    monthYear > thisMonth.year ||
    (monthYear === thisMonth.year && monthNum >= thisMonth.month);

  function goPrev() {
    if (effectiveKind === "week") {
      setWeekAnchor(addDays(isoMonday(weekAnchor), -7));
    } else {
      const next = shiftMonth(monthYear, monthNum, -1);
      setMonthYear(next.year);
      setMonthNum(next.month);
    }
  }

  function goNext() {
    if (effectiveKind === "week") {
      if (atCurrentWeek) return;
      setWeekAnchor(addDays(isoMonday(weekAnchor), 7));
    } else {
      if (atCurrentMonth) return;
      const next = shiftMonth(monthYear, monthNum, 1);
      setMonthYear(next.year);
      setMonthNum(next.month);
    }
  }

  function goHere() {
    setWeekAnchor(todayDay);
    setMonthYear(thisMonth.year);
    setMonthNum(thisMonth.month);
  }

  const rangeText =
    segment === "week"
      ? weekRangeLabel(weekData?.byDay ?? [], isoMonday(todayDay), isoSunday(todayDay))
      : effectiveKind === "week"
        ? `${isoMonday(weekAnchor)} 至 ${isoSunday(weekAnchor)}`
        : `${monthYear}年${monthNum}月`;

  const weekEmpty = !weekData || !weekHasObservation(minutesOfWeek(weekData));
  const monthEmpty = !monthData || activityTotal(monthData.activity) === 0;

  const header = (
    <PageHeader
      title={<h1 className="text-lg font-semibold tracking-tight">统计</h1>}
      subtitle={rangeText}
      center={
        <div className="flex items-center gap-2">
          <Segmented
            aria-label="统计分段"
            size="sm"
            value={segment}
            onChange={pickSegment}
            options={SEGMENTS}
          />
          {(segment === "rhythm" || segment === "app") && (
            <Segmented
              aria-label="范围种类"
              size="sm"
              value={rangeKind}
              onChange={setRangeKind}
              options={[
                { value: "week", label: "按周" },
                { value: "month", label: "按月" },
              ]}
            />
          )}
        </div>
      }
      actions={
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="icon-sm" aria-label="上一段" onClick={goPrev}>
            <ChevronLeft className="size-4" />
          </Button>
          <Button variant="outline" size="sm" onClick={goHere}>
            {effectiveKind === "week" ? "本周" : "本月"}
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="下一段"
            onClick={goNext}
            disabled={effectiveKind === "week" ? atCurrentWeek : atCurrentMonth}
          >
            <ChevronRight className="size-4" />
          </Button>
        </div>
      }
    />
  );

  return (
    <>
      {header}
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-5xl space-y-4 px-6 py-4">
          {error && (
            <Card className="flex flex-col items-center gap-3 p-8 text-center">
              <p className="text-sm text-destructive">{error}</p>
              <Button
                variant="outline"
                size="sm"
                onClick={() => setReload((n) => n + 1)}
              >
                重试
              </Button>
            </Card>
          )}

          {!error && loading && (
            <div className="grid gap-4 lg:grid-cols-2">
              <SkeletonPanel rows={2} />
              <SkeletonPanel rows={2} />
            </div>
          )}

          {!error && !loading && segment === "week" && weekData && (
            <>
              {weekEmpty ? (
                <EmptyLine>本周还没有观测</EmptyLine>
              ) : (
                <div className="grid gap-4 lg:grid-cols-2">
                  <CategoryPanel
                    rows={WEEK_CATEGORIES.map((c) => ({
                      key: String(c.key),
                      label: c.label,
                      cat: c.cat,
                      mins: weekData[c.key] as number,
                    }))}
                    observed={observedMinutes(weekData)}
                  />
                  <DailyPanel days={weekData.byDay} />
                  <HeatPanel hours={weekData.byHour} />
                  <WeekNumbers
                    data={weekData}
                    streak={streak}
                    observed={observedMinutes(weekData)}
                  />
                </div>
              )}
            </>
          )}

          {!error && !loading && segment === "month" && monthData && (
            <div className="grid gap-4 lg:grid-cols-2">
              <MonthCalendar
                year={monthYear}
                month={monthNum}
                days={monthData.days}
                today={todayDay}
                onPickDay={onPickDay}
              />
              {monthEmpty && <EmptyLine className="lg:col-span-2">本月还没有观测</EmptyLine>}
              {monthShowsLedgerCards(!monthEmpty) && (
                <CategoryPanel
                  rows={MONTH_CATEGORIES.map((c) => ({
                    key: c.key,
                    label: c.label,
                    cat: c.cat,
                    mins: monthData.activity[c.key],
                  }))}
                  observed={observedMinutes(monthData.activity)}
                />
              )}
              {monthShowsLedgerCards(!monthEmpty) && (
                <MonthExtras data={monthData} streak={streak} />
              )}
            </div>
          )}

          {!error && !loading && segment === "rhythm" && rhythm && (
            <RhythmPanel data={rhythm} />
          )}
          {!error && !loading && segment === "app" && apps && (
            <>
              {assignError && (
                <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
                  {assignError}
                </p>
              )}
              <AppPanel data={apps} busyApp={busyApp} onAssign={assignApp} />
            </>
          )}
        </div>
      </div>
    </>
  );
}
