import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { ConfirmEndDay } from "../components/ConfirmEndDay";
import { EntertainmentBanner } from "../components/EntertainmentBanner";
import { PermissionBanner } from "../components/PermissionBanner";
import { Progress32 } from "../components/Progress32";
import {
  createList,
  endToday,
  freezeDay,
  getDayView,
  getToday,
  parseTaskLine,
  reportMisclassification,
  reviewSlot,
  toggleTaskDone,
  upsertTask,
  type DayView,
  type TaskListView,
  type TaskView,
  type TodaySlot,
  type TodayView,
} from "../lib/api";
import {
  addDays,
  dayStartUnix,
  dominantLabel,
  planBlocks,
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

const HOURS = Array.from({ length: 24 }, (_, h) => h);
const SLOT_PX = 14;

function hm(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function isLeftListTask(task: TaskView, dayStart: number, dayEnd: number): boolean {
  if (task.start != null && task.end != null) {
    return task.start < dayEnd && task.end > dayStart;
  }
  return !task.done;
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
  const [calDay, setCalDay] = useState<string | null>(null);
  const [currentListId, setCurrentListId] = useState("list-mainline");
  const [line, setLine] = useState("");
  const [newListName, setNewListName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [confirmEnd, setConfirmEnd] = useState(false);
  const [busy, setBusy] = useState(false);
  const [freezeDate, setFreezeDate] = useState("");
  const [freezeMsg, setFreezeMsg] = useState<string | null>(null);
  const [selected, setSelected] = useState<TodaySlot | null>(null);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

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

  async function handleAdd(e: FormEvent) {
    e.preventDefault();
    const raw = line.trim();
    if (!raw || busy) return;
    setBusy(true);
    try {
      const parsed = await parseTaskLine(raw, currentListId);
      await upsertTask({
        id: `task-${Date.now()}`,
        listId: parsed.listId,
        title: parsed.title,
        done: false,
        start: parsed.start,
        end: parsed.end,
        range: null,
      });
      setLine("");
      await refresh();
    } catch (err) {
      const msg = String(err);
      setError(
        msg.includes("too_many_judgment_tasks")
          ? "当天待判定任务已满 20 条，请先勾完或改期。"
          : msg,
      );
    } finally {
      setBusy(false);
    }
  }

  async function handleCreateList(e: FormEvent) {
    e.preventDefault();
    const name = newListName.trim();
    if (!name) return;
    setBusy(true);
    try {
      const created = await createList(name);
      setCurrentListId(created.id);
      setNewListName("");
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleToggle(task: TaskView) {
    try {
      await toggleTaskDone(task.id, !task.done);
      await refresh();
    } catch (err) {
      setError(String(err));
    }
  }

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

  const listsById = useMemo(() => {
    const m = new Map<string, TaskListView>();
    for (const l of data?.lists ?? []) m.set(l.id, l);
    return m;
  }, [data]);

  const leftTasks = useMemo(() => {
    if (!data) return [];
    const start = dayStartUnix(data.day);
    const end = dayStartUnix(addDays(data.day, 1));
    return data.tasks.filter((t) => isLeftListTask(t, start, end));
  }, [data]);

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

  const showFreeze = data.atRisk || data.freezeCandidates.length > 0;
  const dayStart = dayView.dayStart;
  const plans = planBlocks(
    dayView.tasks
      .filter((t) => t.start != null && t.end != null)
      .map((t) => ({
        start: t.start as number,
        end: t.end as number,
        role: listsById.get(t.listId)?.role ?? "side",
        title: t.title,
      })),
    dayStart,
  );
  const showNow = calDay === data.day;
  const nowOffset = Math.min(96 * SLOT_PX, Math.max(0, ((now - dayStart) / 900) * SLOT_PX));
  const isTodayCal = calDay === data.day;

  return (
    <div className="page today-shell">
      <PermissionBanner />
      <EntertainmentBanner
        active={data.activeEntertainment}
        ended={data.endedEntertainment}
      />
      {error && <p className="error">{error}</p>}
      <div className="today-split">
        <section className="today-left">
          <header className="today-left-head">
            <h2>今日</h2>
            <div className="today-badges">
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
            <p className="hero today-hero">{data.creditedLabel} / 8h</p>
            <Progress32 haveSecs={data.creditedSeconds} />
            <p className="muted">
              宝箱 {data.chest.have}/{data.chest.need} 分钟 · 黄金日 {data.gold.have}/
              {data.gold.need} 分钟
            </p>
            {data.goldDay && <p className="gold-day">{GOLD_DAY_MSG}</p>}
            {data.firstCoreLabel && <p className="muted">{data.firstCoreLabel}</p>}
          </header>

          <form className="nl-form" onSubmit={(e) => void handleAdd(e)}>
            <input
              value={line}
              disabled={busy}
              placeholder="明天上午十点到十一点，标题 #杂项"
              onChange={(e) => setLine(e.target.value)}
            />
            <button type="submit" disabled={busy || !line.trim()}>
              添加
            </button>
          </form>
          <p className="muted nl-hint">
            无 # 时加入「{listsById.get(currentListId)?.name ?? "主线任务"}」
          </p>

          {data.lists.map((list) => {
            const items = leftTasks.filter((t) => t.listId === list.id);
            return (
              <div key={list.id} className="task-list-group">
                <button
                  type="button"
                  className={`task-list-head ${roleClass(list.role)} ${
                    currentListId === list.id ? "current" : ""
                  }`}
                  onClick={() => setCurrentListId(list.id)}
                >
                  {list.name}
                  <span>{items.length}</span>
                </button>
                <ul className="task-list">
                  {items.length === 0 && <li className="muted">暂无任务</li>}
                  {items.map((task) => (
                    <li key={task.id} className={task.done ? "done" : ""}>
                      <label>
                        <input
                          type="checkbox"
                          checked={task.done}
                          onChange={() => void handleToggle(task)}
                        />
                        <span>{task.title}</span>
                      </label>
                      {task.start != null && task.end != null && (
                        <span className="task-when">
                          {hm(task.start)}–{hm(task.end)}
                        </span>
                      )}
                    </li>
                  ))}
                </ul>
              </div>
            );
          })}

          <form className="new-list-form" onSubmit={(e) => void handleCreateList(e)}>
            <input
              value={newListName}
              disabled={busy}
              placeholder="新建列表"
              onChange={(e) => setNewListName(e.target.value)}
            />
            <button type="submit" disabled={busy || !newListName.trim()}>
              创建
            </button>
          </form>

          {showFreeze && (
            <div className="freeze-section">
              <h3>保护连胜（冻结）</h3>
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
            </div>
          )}

          <button
            type="button"
            className="danger"
            disabled={busy}
            onClick={() => setConfirmEnd(true)}
          >
            结束今天
          </button>
        </section>

        <section className="today-cal">
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
          <div className="today-cal-cols">
            <span />
            <span>计划</span>
            <span>实际</span>
          </div>
          <div className="today-cal-scroll">
            <div className="today-cal-grid" style={{ gridTemplateRows: `repeat(96, ${SLOT_PX}px)` }}>
              {HOURS.map((h) => (
                <div
                  key={h}
                  className="cal-hour"
                  style={{ gridColumn: 1, gridRow: `${h * 4 + 1} / span 4` }}
                >
                  {String(h).padStart(2, "0")}
                </div>
              ))}
              {plans.map((b, i) => (
                <div
                  key={`p-${i}`}
                  className={`cal-block ${roleClass(b.role)}`}
                  style={{
                    gridColumn: 2,
                    gridRow: `${b.rowStart + 1} / span ${b.rowSpan}`,
                  }}
                  title={b.title}
                >
                  {b.title}
                </div>
              ))}
              {dayView.slots.map((slot) => {
                const row = Math.max(0, Math.min(95, Math.floor((slot.start - dayStart) / 900)));
                const live =
                  showNow && now >= slot.start && now < slot.start + 900 && !slot.final;
                return (
                  <button
                    key={slot.start}
                    type="button"
                    className={`cal-block cal-actual ${roleClass(slot.dominant)} ${
                      live ? "identifying" : ""
                    } ${slot.pending ? "pending" : ""}`}
                    style={{ gridColumn: 3, gridRow: `${row + 1} / span 1` }}
                    onClick={() => setSelected(slot)}
                  >
                    {live ? "正在识别" : dominantLabel(slot.dominant)}
                  </button>
                );
              })}
              {showNow && (
                <div className="cal-now" style={{ top: nowOffset }} aria-hidden="true" />
              )}
            </div>
          </div>
        </section>
      </div>
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
