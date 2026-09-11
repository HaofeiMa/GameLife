import { useEffect, useState } from "react";
import { getWeek, type WeekDayRow, type WeekHourRow, type WeekView } from "../lib/api";
import { heatTone } from "../lib/weekHeat";

const CATEGORIES: {
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

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];
const HEAT_HOURS = Array.from({ length: 14 }, (_, i) => i + 8);

function minutesOf(view: WeekView): number {
  return CATEGORIES.reduce((sum, c) => sum + (view[c.key] as number), 0);
}

function weekdayLabel(day: string, index: number): string {
  const md = day.slice(5).replace("-", "/");
  return `周${WEEKDAYS[index] ?? ""} ${md}`;
}

function CategoryPanel({ data }: { data: WeekView }) {
  const max = Math.max(1, ...CATEGORIES.map((c) => data[c.key] as number));
  return (
    <section className="week-card">
      <h3>按类别</h3>
      <p className="muted">跨槽求和 activity 秒数，显示整数分钟</p>
      <ul className="week-cat-list">
        {CATEGORIES.map((c) => {
          const mins = data[c.key] as number;
          return (
            <li key={c.key}>
              <div className="week-cat-meta">
                <span>
                  <i className="week-swatch" style={{ background: c.color }} />
                  {c.label}
                </span>
                <strong>{mins} 分钟</strong>
              </div>
              <div className="week-bar-track">
                <div
                  className="week-bar-fill"
                  style={{ width: `${(mins / max) * 100}%`, background: c.color }}
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
    ...days.map((d) => d.core + d.side + d.chore),
  );
  return (
    <section className="week-card">
      <h3>按天</h3>
      <p className="muted">工作日主线 / 支线 / 杂项堆叠。周末不采样。</p>
      <div className="week-legend">
        <span><i className="week-swatch" style={{ background: "var(--gl-mainline)" }} />主线</span>
        <span><i className="week-swatch" style={{ background: "var(--gl-side)" }} />支线</span>
        <span><i className="week-swatch" style={{ background: "var(--gl-chore)" }} />杂项</span>
      </div>
      <div className="week-day-chart">
        {days.map((d, i) => {
          const total = d.core + d.side + d.chore;
          const weekend = i >= 5;
          return (
            <div key={d.day} className={`week-day-col ${weekend ? "weekend" : ""}`}>
              <div className="week-day-stack" title={`${total} 分钟`}>
                {total === 0 ? (
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
              <strong>{total}m</strong>
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
    <section className="week-card">
      <h3>高效时段</h3>
      <p className="muted">本周工作日每小时主线秒 / 观测秒，颜色越深主线占比越高。</p>
      <div className="week-legend">
        <span className="heat-legend-scale">
          <i />低
          <i />
          <i />
          <i />高
        </span>
      </div>
      <div className="week-heat">
        {HEAT_HOURS.map((hour) => {
          const row = byHour.get(hour) ?? { hour, core: 0, observed: 0 };
          const tone = heatTone(row.core, row.observed);
          const mins = Math.floor(row.core / 60);
          return (
            <div key={hour} className="week-heat-cell">
              <div
                className="week-heat-swatch"
                style={{
                  background: `rgba(34, 197, 94, ${0.12 + tone * 0.88})`,
                }}
                title={`${hour}:00 主线 ${mins} 分钟 / 观测 ${Math.floor(row.observed / 60)} 分钟`}
              >
                {row.observed > 0 ? `${mins}` : "·"}
              </div>
              <span>{hour}</span>
            </div>
          );
        })}
      </div>
    </section>
  );
}

export function Week() {
  const [data, setData] = useState<WeekView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getWeek()
      .then((w) => {
        setData(w);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, []);

  if (error) {
    return (
      <div className="page">
        <p className="error">{error}</p>
        <button
          type="button"
          onClick={() =>
            getWeek()
              .then((w) => {
                setData(w);
                setError(null);
              })
              .catch((e) => setError(String(e)))
          }
        >
          重试
        </button>
      </div>
    );
  }
  if (!data) return <p className="muted">加载中…</p>;

  const empty = minutesOf(data) === 0;
  const range =
    data.byDay.length >= 2
      ? `${data.byDay[0].day} 至 ${data.byDay[data.byDay.length - 1].day}`
      : "";

  return (
    <div className="page week-page">
      <header className="week-head">
        <div>
          <h2>本周</h2>
          <p className="muted">{range}</p>
        </div>
        <p className="hero">主线 {data.coreLabel}</p>
      </header>
      {empty ? (
        <p className="week-empty">本周还没有观测</p>
      ) : (
        <div className="week-grid">
          <CategoryPanel data={data} />
          <DailyPanel days={data.byDay} />
          <HeatPanel hours={data.byHour} />
        </div>
      )}
    </div>
  );
}
