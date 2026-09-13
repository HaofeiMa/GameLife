import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ConfirmEndDay } from "../components/ConfirmEndDay";
import { EntertainmentBanner } from "../components/EntertainmentBanner";
import { PermissionBanner } from "../components/PermissionBanner";
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
  dominantLabel,
  planMarksFromSnapshots,
  roleClass,
  weekdayLabel,
} from "../lib/calendar";

const GOLD_DAY_MSG =
  "黄金日已达成。继续记录，但不再获得硬币或能量。";

const REVIEW_CATEGORIES = [
  { id: "core_research", label: "主线" },
  { id: "research_support", label: "辅助" },
  { id: "side_project", label: "支线" },
  { id: "admin", label: "杂项" },
  { id: "distraction", label: "娱乐" },
  { id: "break_away", label: "离开" },
];

const CAT_ROWS: { key: keyof SlotActivityMinutes; label: string; color: string }[] = [
  { key: "core", label: "主线", color: "var(--gl-mainline)" },
  { key: "support", label: "辅助", color: "#86efac" },
  { key: "side", label: "支线", color: "var(--gl-side)" },
  { key: "admin", label: "杂项", color: "var(--gl-chore)" },
  { key: "distraction", label: "娱乐", color: "var(--gl-play)" },
  { key: "away", label: "离开", color: "#d1d5db" },
  { key: "unobserved", label: "未观测", color: "#9ca3af" },
];

const HOURS = Array.from({ length: 24 }, (_, h) => h);
const SLOT_PX = 14;
const SLOT_COUNT = 96;
const GOAL_MINUTES = 480;
const CAL_DAY_KEY = "gl-cal-day";

function hm(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function activityTotal(a: SlotActivityMinutes): number {
  return a.core + a.support + a.admin + a.side + a.distraction + a.away + a.unobserved;
}

function pendingMinutesOf(slots: TodaySlot[]): number {
  return slots.filter((s) => s.pending).reduce((n, s) => n + activityTotal(s.activity), 0);
}

function readStoredCalDay(): string | null {
  try {
    const stored = window.localStorage.getItem(CAL_DAY_KEY);
    if (stored) {
      window.localStorage.removeItem(CAL_DAY_KEY);
      return stored;
    }
  } catch {
    /* ignore quota / private mode */
  }
  return null;
}

function StreakRing({ streak, atRisk }: { streak: number; atRisk: boolean }) {
  const r = 15;
  const c = 2 * Math.PI * r;
  const frac = streak <= 0 ? 0 : Math.min(1, 0.12 + (streak % 30) / 30);
  return (
    <svg className="streak-ring" width="52" height="52" viewBox="0 0 40 40" aria-label={`连胜 ${streak}`}>
      <circle cx="20" cy="20" r={r} fill="none" stroke="#e5e7eb" strokeWidth="4" />
      <circle
        cx="20"
        cy="20"
        r={r}
        fill="none"
        stroke={atRisk ? "#f59e0b" : "#22c55e"}
        strokeWidth="4"
        strokeDasharray={`${c * frac} ${c}`}
        strokeLinecap="round"
        transform="rotate(-90 20 20)"
      />
      <text x="20" y="24" textAnchor="middle" fontSize="13" fontWeight="700" fill="#111">
        {streak}
      </text>
    </svg>
  );
}

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
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal slot-review-modal">
        <h3>
          {hm(slot.start)} · {dominantLabel(slot.dominant)}
        </h3>
        <p className="muted">
          计入 {slot.creditedMinutes} 分钟
          {slot.activitySummary && slot.activitySummary !== "—"
            ? ` · ${slot.activitySummary}`
            : ""}
        </p>
        {err && <p className="error">{err}</p>}
        {slot.pending && (
          <div className="slot-actions">
            {REVIEW_CATEGORIES.filter((c) => !(hideAway && c.id === "break_away")).map(
              (c) => (
                <button
                  key={c.id}
                  type="button"
                  disabled={busy}
                  onClick={() => void classify(c.id)}
                >
                  {c.label}
                </button>
              ),
            )}
          </div>
        )}
        {slot.final && (
          <div className="slot-actions">
            <input
              value={note}
              disabled={busy}
              placeholder="误分类说明"
              onChange={(e) => setNote(e.target.value)}
            />
            <button type="button" disabled={busy || !note.trim()} onClick={() => void report()}>
              报告误判
            </button>
          </div>
        )}
        {!slot.pending && !slot.final && (
          <p className="muted">正在识别这一槽，结束后可复核。</p>
        )}
        <div className="modal-actions">
          <button type="button" onClick={onClose}>
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}

export function Today() {
  const [data, setData] = useState<TodayView | null>(null);
  const [dayView, setDayView] = useState<DayView | null>(null);
  const [calDay, setCalDay] = useState<string | null>(readStoredCalDay);
  const [error, setError] = useState<string | null>(null);
  const [confirmEnd, setConfirmEnd] = useState(false);
  const [busy, setBusy] = useState(false);
  const [freezeDate, setFreezeDate] = useState("");
  const [freezeMsg, setFreezeMsg] = useState<string | null>(null);
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
    void refresh();
    const id = setInterval(() => void refresh(), 30_000);
    return () => clearInterval(id);
  }, [refresh]);

  useEffect(() => {
    const id = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 15_000);
    return () => clearInterval(id);
  }, []);

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

  if (error && !data) {
    return (
      <div className="page">
        <p className="error">{error}</p>
        <button type="button" onClick={() => void refresh()}>
          重试
        </button>
      </div>
    );
  }
  if (!data || !dayView || !calDay) return <p className="muted">加载中…</p>;

  const dayStart = dayView.dayStart;
  const marks = planMarksFromSnapshots(
    (dayView.planMarks ?? []).map((m) => ({
      start: m.start,
      end: m.end,
      title: m.title,
    })),
    dayStart,
  );
  const showNow = calDay === data.day;
  const nowOffset = Math.min(SLOT_COUNT * SLOT_PX, Math.max(0, ((now - dayStart) / 900) * SLOT_PX));
  const isTodayCal = calDay === data.day;
  const activity = isTodayCal ? data.activity : dayView.activity;
  const appTop: AppTopRow[] = isTodayCal ? data.appTop : dayView.appTop;
  const pendingCount = isTodayCal ? data.pendingCount : dayView.pendingCount;
  const pendingMinutes = pendingMinutesOf(dayView.slots);
  const catMax = Math.max(
    1,
    ...CAT_ROWS.map((c) => activity[c.key]),
    pendingMinutes,
  );
  const creditedMin = Math.floor(data.creditedSeconds / 60);
  const fillPct = data.goldDay ? 100 : Math.min(100, (creditedMin / GOAL_MINUTES) * 100);
  const emptyDay = dayView.slots.length === 0;

  function scrollToFirstPending() {
    const el = timelineRef.current?.querySelector(".cal-actual.pending");
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
  }

  return (
    <div className="page today-shell">
      <PermissionBanner />
      <EntertainmentBanner
        active={data.activeEntertainment}
        ended={data.endedEntertainment}
      />
      {error && <p className="error">{error}</p>}

      <div className="today-badges today-badges-row">
        <div className="badge-card">
          <span className="badge-label">硬币</span>
          <strong>
            今日 +{data.coinsToday}
            <span className="badge-sub"> · 共 {data.coinBalance}</span>
          </strong>
        </div>
        <div className="badge-card">
          <span className="badge-label">能量</span>
          <strong>
            今日 {data.xpToday}
            {!data.xpShopUnlocked && <span className="badge-sub">（商店锁定）</span>}
          </strong>
        </div>
        <div className="badge-card badge-streak">
          <StreakRing streak={data.streak} atRisk={data.atRisk} />
          <div>
            <span className="badge-label">连胜</span>
            {data.atRisk && <p className="badge-warn">未达宝箱有断连风险</p>}
          </div>
        </div>
      </div>

      <section className="today-report-card">
        <div className="today-progress-head">
          <strong className="hero today-hero">{data.creditedLabel} / 8h</strong>
          <span className="muted">
            宝箱 {data.chest.have}/{data.chest.need} · 黄金日 {data.gold.have}/{data.gold.need}
          </span>
        </div>
        <div
          className="today-progress-track"
          role="progressbar"
          aria-valuenow={creditedMin}
          aria-valuemin={0}
          aria-valuemax={GOAL_MINUTES}
        >
          <div className="today-progress-fill" style={{ width: `${fillPct}%` }} />
        </div>
        {data.goldDay && <p className="gold-day">{GOLD_DAY_MSG}</p>}
        {data.firstCoreLabel && <p className="muted">{data.firstCoreLabel}</p>}
      </section>

      <section className="today-report-card">
        <h3>类别</h3>
        <p className="muted">跨槽求和 activity 分钟，不是 dominant × 15。</p>
        {emptyDay && <p className="muted">这一天没有监测记录</p>}
        <ul className="today-cat-list">
          {CAT_ROWS.map((c) => {
            const mins = activity[c.key];
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
                    style={{ width: `${(mins / catMax) * 100}%`, background: c.color }}
                  />
                </div>
              </li>
            );
          })}
          <li>
            <div className="week-cat-meta">
              <span>
                <i className="week-swatch" style={{ background: "#fbbf24" }} />
                待复核
              </span>
              <strong>{pendingMinutes} 分钟</strong>
            </div>
            <div className="week-bar-track">
              <div
                className="week-bar-fill"
                style={{
                  width: `${(pendingMinutes / catMax) * 100}%`,
                  background: "#fbbf24",
                }}
              />
            </div>
          </li>
        </ul>
        {pendingCount > 0 && (
          <button type="button" className="linkish today-pending-link" onClick={scrollToFirstPending}>
            {pendingCount} 条待复核，点按滚到时间轴
          </button>
        )}
      </section>

      <section className="today-cal today-report-card">
        <header className="today-cal-head">
          <button
            type="button"
            aria-label="前一天"
            onClick={() => setCalDay(addDays(calDay, -1))}
          >
            ‹
          </button>
          <div>
            <h2>{weekdayLabel(calDay)}</h2>
            {!isTodayCal && (
              <button type="button" className="linkish" onClick={() => setCalDay(data.day)}>
                回到今天
              </button>
            )}
          </div>
          <button
            type="button"
            aria-label="后一天"
            onClick={() => setCalDay(addDays(calDay, 1))}
          >
            ›
          </button>
        </header>
        <div className="today-cal-body" ref={timelineRef}>
          <div className="today-cal-grid-wrap">
            <div
              className="today-cal-grid today-cal-grid-single"
              style={{ gridTemplateRows: `repeat(${SLOT_COUNT}, ${SLOT_PX}px)` }}
            >
              {HOURS.map((h) => (
                <div
                  key={h}
                  className="cal-hour"
                  style={{ gridColumn: 1, gridRow: `${h * 4 + 1} / span 4` }}
                >
                  {String(h).padStart(2, "0")}
                </div>
              ))}
              {Array.from({ length: SLOT_COUNT }, (_, i) => {
                const start = dayStart + i * 900;
                const slot = slotsByStart.get(start);
                const live =
                  showNow && now >= start && now < start + 900 && !(slot?.final);
                const label = live
                  ? "正在识别"
                  : slot
                    ? dominantLabel(slot.dominant)
                    : "";
                const className = [
                  "cal-block",
                  "cal-actual",
                  slot ? roleClass(slot.dominant) : "cal-empty",
                  live ? "identifying" : "",
                  slot?.pending ? "pending" : "",
                ]
                  .filter(Boolean)
                  .join(" ");
                if (slot) {
                  return (
                    <button
                      key={start}
                      type="button"
                      className={className}
                      style={{ gridColumn: 2, gridRow: `${i + 1} / span 1` }}
                      onClick={() => setSelected(slot)}
                    >
                      {label}
                    </button>
                  );
                }
                return (
                  <div
                    key={start}
                    className={className}
                    style={{ gridColumn: 2, gridRow: `${i + 1} / span 1` }}
                  >
                    {label}
                  </div>
                );
              })}
            </div>
            {marks.map((m, i) => {
              const start = Math.max(m.start, dayStart);
              const end = Math.min(m.end, dayStart + 86400);
              const top = ((start - dayStart) / 900) * SLOT_PX;
              const height = Math.max(6, ((end - start) / 900) * SLOT_PX);
              return (
                <div
                  key={`m-${i}`}
                  className="cal-plan-mark"
                  title={m.title}
                  style={{ top, height }}
                />
              );
            })}
            {showNow && (
              <div className="cal-now" style={{ top: nowOffset }} aria-hidden="true" />
            )}
          </div>
        </div>
      </section>

      <section className="today-report-card">
        <h3>当日应用</h3>
        {appTop.length === 0 ? (
          <p className="muted">今天还没有应用明细</p>
        ) : (
          <ul className="today-app-list">
            {appTop.slice(0, 5).map((row) => (
              <li key={row.name}>
                <button
                  type="button"
                  className={`today-app-row ${roleClass(row.dominant)}`}
                  onClick={() => setOpenApp(openApp === row.name ? null : row.name)}
                >
                  <span>{row.name}</span>
                  <strong>{row.minutes} 分钟</strong>
                </button>
                {openApp === row.name && (
                  <p className="muted">
                    {row.name} 当日 {row.minutes} 分钟 · {dominantLabel(
                      row.dominant === "core"
                        ? "core_research"
                        : row.dominant === "support"
                          ? "research_support"
                          : row.dominant === "side"
                            ? "side_project"
                            : row.dominant === "admin"
                              ? "admin"
                              : row.dominant,
                    )}
                  </p>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      <details className="freeze-section">
        <summary>保护连胜</summary>
        {data.atRisk && (
          <p className="muted">
            今日估计有效主线未满 6 小时。可冻结一个 failed 工作日以恢复连胜（每月 2 次）。
          </p>
        )}
        {data.freezeCandidates.length > 0 ? (
          <>
            <label>
              被保护日
              <select
                value={freezeDate}
                disabled={busy}
                onChange={(e) => setFreezeDate(e.target.value)}
              >
                {data.freezeCandidates.map((d) => (
                  <option key={d} value={d}>
                    {d}（failed）
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              disabled={busy || !freezeDate}
              onClick={() => void handleFreeze()}
            >
              {busy ? "冻结中…" : `冻结 ${freezeDate || "…"}`}
            </button>
          </>
        ) : (
          <p className="muted">当前没有可冻结的 failed 工作日。</p>
        )}
        {freezeMsg && <p className="muted">{freezeMsg}</p>}
      </details>

      <button
        type="button"
        className="danger"
        disabled={busy}
        onClick={() => setConfirmEnd(true)}
      >
        结束今天
      </button>

      <ConfirmEndDay
        open={confirmEnd}
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
    </div>
  );
}
