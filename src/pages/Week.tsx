import { useEffect, useState } from "react";
import {
  getAppReport,
  getMonthReport,
  getRhythmReport,
  getToday,
  getWeek,
  type AppReportView,
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
import { heatTone } from "../lib/weekHeat";

type Segment = "week" | "month" | "rhythm" | "app";

const SEGMENTS: { id: Segment; label: string }[] = [
  { id: "week", label: "周" },
  { id: "month", label: "月" },
  { id: "rhythm", label: "节奏" },
  { id: "app", label: "应用" },
];

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];
const HEAT_HOURS = Array.from({ length: 14 }, (_, i) => i + 8);

const WEEK_CATEGORIES: {
  key: keyof WeekView;
  label: string;
  color: string;
}[] = [
  { key: "core", label: "主线", color: "var(--gl-mainline)" },
  { key: "support", label: "辅助", color: "#86efac" },
  { key: "side", label: "支线", color: "var(--gl-side)" },
  { key: "admin", label: "杂项", color: "var(--gl-chore)" },
  { key: "distraction", label: "娱乐", color: "var(--gl-play)" },
  { key: "away", label: "离开", color: "#d1d5db" },
  { key: "unobserved", label: "未观测", color: "#9ca3af" },
  { key: "pendingReview", label: "待复核", color: "#fbbf24" },
];

const MONTH_CATEGORIES: {
  key: keyof SlotActivityMinutes;
  label: string;
  color: string;
}[] = [
  { key: "core", label: "主线", color: "var(--gl-mainline)" },
  { key: "support", label: "辅助", color: "#86efac" },
  { key: "side", label: "支线", color: "var(--gl-side)" },
  { key: "admin", label: "杂项", color: "var(--gl-chore)" },
  { key: "distraction", label: "娱乐", color: "var(--gl-play)" },
  { key: "away", label: "离开", color: "#d1d5db" },
  { key: "unobserved", label: "未观测", color: "#9ca3af" },
];

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

function weekdayLabel(day: string, index: number): string {
  const md = day.slice(5).replace("-", "/");
  return `周${WEEKDAYS[index] ?? ""} ${md}`;
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

function CategoryPanel({
  rows,
  observed,
}: {
  rows: { key: string; label: string; color: string; mins: number }[];
  observed: number;
}) {
  const max = Math.max(1, ...rows.map((r) => r.mins));
  return (
    <section className="week-card">
      <h3>按类别</h3>
      <p className="muted">跨槽求和 activity 秒数 / 60 取整。占观测比不含未观测。</p>
      <div className="week-legend">
        {rows.map((c) => (
          <span key={c.key}>
            <i className="week-swatch" style={{ background: c.color }} />
            {c.label}
          </span>
        ))}
      </div>
      <ul className="week-cat-list">
        {rows.map((c) => {
          const ratio = c.key === "unobserved" ? "—" : shareLabel(c.mins, observed);
          return (
            <li key={c.key}>
              <div className="week-cat-meta">
                <span>
                  <i className="week-swatch" style={{ background: c.color }} />
                  {c.label}
                </span>
                <strong>
                  {c.mins} 分钟 · {ratio}
                </strong>
              </div>
              <div className="week-bar-track">
                <div
                  className="week-bar-fill"
                  style={{ width: `${(c.mins / max) * 100}%`, background: c.color }}
                />
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function DailyPanel({ days }: { days: WeekDayRow[] }) {
  const max = Math.max(
    1,
    ...days.filter((d) => !isWeekendDay(d.day)).map((d) => d.core + d.side + d.chore),
  );
  return (
    <section className="week-card">
      <h3>按天</h3>
      <p className="muted">工作日主线 / 支线 / 杂项堆叠。娱乐不进主线柱。周末显示未采样，不是 0 主线。</p>
      <div className="week-legend">
        <span><i className="week-swatch" style={{ background: "var(--gl-mainline)" }} />主线</span>
        <span><i className="week-swatch" style={{ background: "var(--gl-side)" }} />支线</span>
        <span><i className="week-swatch" style={{ background: "var(--gl-chore)" }} />杂项</span>
        <span>
          <i className="week-swatch week-play-line-swatch" />
          娱乐（不计入堆叠）
        </span>
      </div>
      <div className="week-day-chart">
        {days.map((d, i) => {
          const weekend = isWeekendDay(d.day);
          const total = d.core + d.side + d.chore;
          return (
            <div key={d.day} className={`week-day-col ${weekend ? "weekend" : ""}`}>
              <div className="week-day-stack" title={weekend ? "未采样" : `${total} 分钟`}>
                {weekend || total === 0 ? (
                  <div className="week-day-empty" />
                ) : (
                  <>
                    {d.core > 0 && (
                      <div
                        className="week-stack-seg"
                        style={{ flex: d.core, background: "var(--gl-mainline)" }}
                      />
                    )}
                    {d.side > 0 && (
                      <div
                        className="week-stack-seg"
                        style={{ flex: d.side, background: "var(--gl-side)" }}
                      />
                    )}
                    {d.chore > 0 && (
                      <div
                        className="week-stack-seg"
                        style={{ flex: d.chore, background: "var(--gl-chore)" }}
                      />
                    )}
                    <div style={{ flex: Math.max(0, max - total) }} />
                  </>
                )}
              </div>
              <strong>{weekend ? "未采样" : `${total}m`}</strong>
              <span>{weekdayLabel(d.day, i)}</span>
            </div>
          );
        })}
      </div>
    </section>
  );
}

function HeatPanel({ hours }: { hours: WeekHourRow[] }) {
  const byHour = new Map(hours.map((h) => [h.hour, h]));
  return (
    <section className="week-card week-card-wide">
      <h3>高效时段</h3>
      <p className="muted">工作日 8–21 点：该小时主线秒 / 观测秒。无观测为灰，不是 0%。</p>
      <div className="week-legend">
        <span className="heat-legend-scale">
          <i />低
          <i />
          <i />
          <i />高
        </span>
        <span>无观测</span>
      </div>
      <div className="week-heat">
        {HEAT_HOURS.map((hour) => {
          const row = byHour.get(hour) ?? { hour, core: 0, observed: 0 };
          const tone = heatTone(row.core, row.observed);
          const mins = Math.floor(row.core / 60);
          const observed = row.observed > 0;
          return (
            <div key={hour} className="week-heat-cell">
              <div
                className={`week-heat-swatch ${observed ? "" : "empty"}`.trim()}
                style={{
                  background: observed
                    ? `rgba(34, 197, 94, ${0.12 + tone * 0.88})`
                    : "#e5e7eb",
                }}
                title={
                  observed
                    ? `${hour}:00 主线 ${mins} 分钟 / 观测 ${Math.floor(row.observed / 60)} 分钟`
                    : `${hour}:00 无观测`
                }
              >
                {observed ? `${mins}` : "·"}
              </div>
              <span>{hour}</span>
            </div>
          );
        })}
      </div>
    </section>
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
    <section className="week-card">
      <h3>本周数字</h3>
      <p className="muted">主线小时来自 credited 秒 / 3600。环比无上周槽则为 —。达标只计工作日。</p>
      <div className="week-legend">
        <span>主线</span>
        <span>环比</span>
        <span>娱乐</span>
        <span>复核</span>
        <span>≥6h / ≥8h</span>
        <span>连胜</span>
      </div>
      <div className="stats-nums">
        <div>
          <span className="muted">主线</span>
          <strong>{data.coreHours.toFixed(1)} 小时</strong>
        </div>
        <div>
          <span className="muted">较上周</span>
          <strong>{wowLabel(data.wowCoreDeltaMinutes)}</strong>
        </div>
        <div>
          <span className="muted">娱乐占观测</span>
          <strong>{playShare}</strong>
        </div>
        <div>
          <span className="muted">待复核率</span>
          <strong>{pct(data.pendingOverResolved)}</strong>
        </div>
        <div>
          <span className="muted">≥6h / ≥8h</span>
          <strong>
            {data.daysGe6h} 日 / {data.daysGe8h} 日
          </strong>
        </div>
        <div>
          <span className="muted">连胜</span>
          <strong>{streak == null ? "—" : `${streak} 天`}</strong>
        </div>
      </div>
    </section>
  );
}

function MonthCalendar({
  year,
  month,
  days,
  onPickDay,
}: {
  year: number;
  month: number;
  days: MonthDayCell[];
  onPickDay: (day: string) => void;
}) {
  const byDay = new Map(days.map((d) => [d.day, d]));
  const cells = calendarCells(year, month);
  return (
    <section className="week-card week-card-wide">
      <h3>热力月历</h3>
      <p className="muted">颜色按当天 credited 主线秒 / 28800（8h）钳制 0–1。周末未采样，不是 0 主线。点格打开该日日报。</p>
      <div className="week-legend">
        <span className="heat-legend-scale">
          <i />低
          <i />
          <i />
          <i />高
        </span>
        <span>周末未采样</span>
        <span>未来日</span>
      </div>
      <div className="month-cal">
        {WEEKDAYS.map((w) => (
          <div key={w} className="month-cal-head">
            {w}
          </div>
        ))}
        {cells.map((c, i) => {
          if (!c.day) {
            return <div key={`b-${i}`} className="month-cell blank" />;
          }
          const row = byDay.get(c.day);
          const weekend = row?.isWeekend ?? isWeekendDay(c.day);
          const future = row?.isFuture ?? false;
          const tone = monthHeatCell(row?.creditedCore ?? 0);
          const coreMin = Math.floor((row?.creditedCore ?? 0) / 60);
          let note = "无观测";
          if (weekend) note = "未采样";
          else if (future) note = "未来";
          else if (coreMin > 0) note = `${coreMin}m`;
          const credited = row?.creditedCore ?? 0;
          const style =
            weekend || future
              ? undefined
              : credited > 0
                ? { background: `rgba(34, 197, 94, ${0.08 + tone * 0.88})` }
                : { background: "#e5e7eb" };
          return (
            <button
              key={c.day}
              type="button"
              className={`month-cell ${weekend ? "weekend" : ""} ${future ? "future" : ""}`.trim()}
              style={style}
              onClick={() => onPickDay(c.day!)}
            >
              <span>{c.day.slice(8)}</span>
              <em>{note}</em>
            </button>
          );
        })}
      </div>
    </section>
  );
}

function MonthExtras({
  data,
  streak,
}: {
  data: MonthReportView;
  streak: number | null;
}) {
  const emptyCoins = data.coinsEarned === 0 && data.coinsSpent === 0 && data.xpEarned === 0;
  return (
    <>
      <section className="week-card">
        <h3>硬币与能量</h3>
        <p className="muted">本月 ledger 正增量合计 vs 兑换支出。能量只展示本月获得，不含未完成娱乐折算。</p>
        <div className="week-legend">
          <span>硬币获得</span>
          <span>硬币兑换</span>
          <span>能量获得</span>
        </div>
        {emptyCoins ? (
          <p className="week-empty-inline">本月还没有账本记录</p>
        ) : (
          <div className="stats-nums">
            <div>
              <span className="muted">硬币获得</span>
              <strong>{data.coinsEarned}</strong>
            </div>
            <div>
              <span className="muted">兑换支出</span>
              <strong>{data.coinsSpent}</strong>
            </div>
            <div>
              <span className="muted">能量获得</span>
              <strong>{data.xpEarned}</strong>
            </div>
          </div>
        )}
      </section>
      <section className="week-card">
        <h3>月份徽章</h3>
        <p className="muted">黄金日按 credited ≥ 8h。冻结按被保护日所在月。连胜是今日快照，不考古历史最长。</p>
        <div className="week-legend">
          <span>黄金日</span>
          <span>冻结</span>
          <span>completed 日</span>
          <span>连胜</span>
        </div>
        <div className="stats-nums">
          <div>
            <span className="muted">黄金日</span>
            <strong>{data.goldDays} 天</strong>
          </div>
          <div>
            <span className="muted">冻结</span>
            <strong>{data.freezeCount} 次</strong>
          </div>
          <div>
            <span className="muted">本月完成</span>
            <strong>{data.completedDays} 日</strong>
          </div>
          <div>
            <span className="muted">当前连胜</span>
            <strong>{streak == null ? "—" : `${streak} 天`}</strong>
          </div>
        </div>
      </section>
    </>
  );
}

function RhythmPanel({ data }: { data: RhythmReportView }) {
  const hasStart = data.startHours.some((s) => s.hour != null);
  const runMinutes = data.distractionRunSlots * 15;
  return (
    <div className="week-grid">
      <section className="week-card">
        <h3>开工时刻</h3>
        <p className="muted">每个工作日第一个 credited 主线 &gt; 0 的槽开始钟点。尚无主线的日子不画点。</p>
        <div className="week-legend">
          <span>点的高度 = 钟点 / 24</span>
        </div>
        {hasStart ? (
          <div className="rhythm-start-chart">
            {data.startHours.map((s) => (
              <div key={s.day} className="rhythm-start-col-wrap">
                <div className="rhythm-start-col">
                  {s.hour != null && (
                    <div
                      className="rhythm-start-dot"
                      style={{ bottom: `${(s.hour / 24) * 100}%` }}
                      title={`${s.day} ${s.hour} 点`}
                    />
                  )}
                </div>
                <span>{s.day.slice(5)}</span>
                <strong>{s.hour == null ? "—" : `${s.hour}点`}</strong>
              </div>
            ))}
          </div>
        ) : (
          <p className="week-empty-inline">尚无主线开工记录</p>
        )}
      </section>
      <section className="week-card">
        <h3>达标率</h3>
        <p className="muted">范围内工作日 credited ≥6h / ≥8h 的比例。周末不采样，不进分母。</p>
        <div className="week-legend">
          <span>≥6h</span>
          <span>≥8h</span>
        </div>
        <div className="stats-nums">
          <div>
            <span className="muted">≥6h</span>
            <strong>{pct(data.rate6h)}</strong>
          </div>
          <div>
            <span className="muted">≥8h</span>
            <strong>{pct(data.rate8h)}</strong>
          </div>
        </div>
        <p className="muted">周末不采样</p>
      </section>
      <section className="week-card">
        <h3>娱乐连段</h3>
        <p className="muted">连续 ≥3 个槽 dominant 为娱乐的次数与总分钟（槽 × 15）。用来看是不是一滑就半小时。</p>
        <div className="week-legend">
          <span>次数</span>
          <span>分钟</span>
        </div>
        {data.distractionRunCount === 0 ? (
          <p className="week-empty-inline">没有连续娱乐段</p>
        ) : (
          <div className="stats-nums">
            <div>
              <span className="muted">次数</span>
              <strong>{data.distractionRunCount}</strong>
            </div>
            <div>
              <span className="muted">总分钟</span>
              <strong>{runMinutes}</strong>
            </div>
          </div>
        )}
      </section>
      <section className="week-card">
        <h3>深时段</h3>
        <p className="muted">范围内主线占观测比最高的三个钟点（热力同一口径）。</p>
        <div className="week-legend">
          <span>高峰钟点</span>
        </div>
        {data.peakHours.length === 0 ? (
          <p className="week-empty-inline">没有足够的小时观测</p>
        ) : (
          <ol className="stats-peak-list">
            {data.peakHours.map((h) => (
              <li key={h}>{h} 点</li>
            ))}
          </ol>
        )}
      </section>
    </div>
  );
}

function AppPanel({ data }: { data: AppReportView }) {
  const newSet = new Set(data.newcomers);
  const apps = [...data.apps].sort((a, b) => {
    const an = newSet.has(a.name) ? 0 : 1;
    const bn = newSet.has(b.name) ? 0 : 1;
    if (an !== bn) return an - bn;
    return b.minutes - a.minutes;
  });
  return (
    <div className="week-grid">
      <section className="week-card">
        <h3>应用</h3>
        <p className="muted">范围内 app_day_stats 按 hint 秒 / 60 取整。主类别取秒最大者。排序默认总分钟降序，新面孔置顶。</p>
        {data.newcomers.length > 0 && (
          <p className="app-newcomers">
            新面孔（考虑加进名单）：{data.newcomers.join("、")}
          </p>
        )}
        {apps.length === 0 ? (
          <p className="week-empty-inline">无应用明细</p>
        ) : (
          <table className="stats-table">
            <thead>
              <tr>
                <th>名称</th>
                <th>分钟</th>
                <th>主类别</th>
                <th>名单</th>
              </tr>
            </thead>
            <tbody>
              {apps.map((row) => (
                <tr key={row.name} className={newSet.has(row.name) ? "newcomer" : ""}>
                  <td>{row.name}</td>
                  <td>{row.minutes}</td>
                  <td>{hintLabel(row.dominant)}</td>
                  <td>{listedLabel(row.listedAs)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {apps.length === 0 && (
          <p className="muted">从本版本上线后的槽才有 App 明细。</p>
        )}
      </section>
      <section className="week-card">
        <h3>浏览器 Host</h3>
        <p className="muted">Top 15 host，类别同样来自 hint 秒，不是 dominant × 15。</p>
        <div className="week-legend">
          <span>host</span>
          <span>分钟</span>
          <span>主类别</span>
        </div>
        {data.hosts.length === 0 ? (
          <p className="week-empty-inline">没有浏览器 host 明细</p>
        ) : (
          <table className="stats-table">
            <thead>
              <tr>
                <th>Host</th>
                <th>分钟</th>
                <th>主类别</th>
              </tr>
            </thead>
            <tbody>
              {data.hosts.map((row) => (
                <tr key={row.host}>
                  <td>{row.host}</td>
                  <td>{row.minutes}</td>
                  <td>{hintLabel(row.dominant)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
      <section className="week-card">
        <h3>保护时长</h3>
        <p className="muted">永不截屏 / 密码框，不发币，不进主线也不进娱乐。</p>
        <div className="week-legend">
          <span>保护分钟</span>
        </div>
        <div className="app-protected">
          <strong>{data.protectedMinutes} 分钟</strong>
          <span className="muted">永不截屏 / 密码框，不发币</span>
        </div>
      </section>
    </div>
  );
}

export function Week({ onPickDay }: { onPickDay: (day: string) => void }) {
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
          const w = await getWeek();
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

  const currentMonday = isoMonday(todayDay);
  const viewMonday = isoMonday(weekAnchor);
  const weekIsCurrent = viewMonday === currentMonday;
  const thisMonth = {
    year: Number(todayDay.slice(0, 4)),
    month: Number(todayDay.slice(5, 7)),
  };
  const atCurrentWeek = viewMonday >= currentMonday;
  const atCurrentMonth =
    monthYear > thisMonth.year || (monthYear === thisMonth.year && monthNum >= thisMonth.month);

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
    effectiveKind === "week"
      ? `${isoMonday(weekAnchor)} 至 ${isoSunday(weekAnchor)}`
      : `${monthYear}年${monthNum}月`;

  if (error) {
    return (
      <div className="page">
        <p className="error">{error}</p>
        <button type="button" onClick={() => setReload((n) => n + 1)}>
          重试
        </button>
      </div>
    );
  }

  const weekEmpty =
    !weekData || !weekIsCurrent || minutesOfWeek(weekData) === 0;
  const monthEmpty = !monthData || activityTotal(monthData.activity) === 0;

  return (
    <div className="page week-page">
      <header className="stats-head">
        <div className="stats-seg" role="tablist" aria-label="统计分段">
          {SEGMENTS.map((s) => (
            <button
              key={s.id}
              type="button"
              role="tab"
              aria-selected={segment === s.id}
              className={segment === s.id ? "active" : ""}
              onClick={() => pickSegment(s.id)}
            >
              {s.label}
            </button>
          ))}
        </div>
        {(segment === "rhythm" || segment === "app") && (
          <div className="stats-kind" aria-label="范围种类">
            <button
              type="button"
              className={rangeKind === "week" ? "active" : ""}
              onClick={() => setRangeKind("week")}
            >
              按周
            </button>
            <button
              type="button"
              className={rangeKind === "month" ? "active" : ""}
              onClick={() => setRangeKind("month")}
            >
              按月
            </button>
          </div>
        )}
        <div className="stats-range">
          <button type="button" aria-label="上一段" onClick={goPrev}>
            ‹
          </button>
          <button type="button" className="stats-range-here" onClick={goHere}>
            {effectiveKind === "week" ? "本周" : "本月"}
          </button>
          <button
            type="button"
            aria-label="下一段"
            onClick={goNext}
            disabled={effectiveKind === "week" ? atCurrentWeek : atCurrentMonth}
          >
            ›
          </button>
        </div>
      </header>
      <p className="muted stats-range-label">{rangeText}</p>

      {loading && <p className="muted">加载中…</p>}

      {!loading && segment === "week" && weekData && (
        <>
          {weekEmpty ? (
            <p className="week-empty">本周还没有观测</p>
          ) : (
            <div className="week-grid">
              <CategoryPanel
                rows={WEEK_CATEGORIES.map((c) => ({
                  key: String(c.key),
                  label: c.label,
                  color: c.color,
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

      {!loading && segment === "month" && monthData && (
        <>
          {monthEmpty && <p className="week-empty">本月还没有观测</p>}
          <div className="week-grid">
            <MonthCalendar
              year={monthYear}
              month={monthNum}
              days={monthData.days}
              onPickDay={onPickDay}
            />
            {monthEmpty ? null : (
              <CategoryPanel
                rows={MONTH_CATEGORIES.map((c) => ({
                  key: c.key,
                  label: c.label,
                  color: c.color,
                  mins: monthData.activity[c.key],
                }))}
                observed={observedMinutes(monthData.activity)}
              />
            )}
            <MonthExtras data={monthData} streak={streak} />
          </div>
        </>
      )}

      {!loading && segment === "rhythm" && rhythm && <RhythmPanel data={rhythm} />}
      {!loading && segment === "app" && apps && <AppPanel data={apps} />}
    </div>
  );
}
