import { useCallback, useEffect, useState } from "react";
import {
  getToday,
  reportMisclassification,
  reviewSlot,
  type TodaySlot,
  type TodayView,
} from "../lib/api";
import { PermissionBanner } from "../components/PermissionBanner";
import { formatEstimatedMinutes } from "../lib/format";

const REVIEW_CATEGORIES = [
  { id: "core_research", label: "Core" },
  { id: "research_support", label: "Support" },
  { id: "admin", label: "Admin" },
  { id: "side_project", label: "Side" },
  { id: "distraction", label: "Distraction" },
  { id: "break_away", label: "Away" },
];

function slotTimeLabel(start: number): string {
  const d = new Date(start * 1000);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function dominantLabel(d: string): string {
  return d.replace(/_/g, " ");
}

function SlotRow({
  slot,
  day,
  onChange,
}: {
  slot: TodaySlot;
  day: string;
  onChange: () => void;
}) {
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function classify(category: string) {
    setBusy(true);
    setErr(null);
    try {
      await reviewSlot(day, slot.start, category);
      onChange();
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
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="slot-row">
      <div className="slot-head">
        <span>{slotTimeLabel(slot.start)}</span>
        <span>{dominantLabel(slot.dominant)}</span>
        <span>{slot.creditedMinutes}m</span>
      </div>
      {err && <p className="error">{err}</p>}
      {slot.pending && (
        <div className="slot-actions">
          {REVIEW_CATEGORIES.map((c) => (
            <button key={c.id} type="button" disabled={busy} onClick={() => classify(c.id)}>
              {c.label}
            </button>
          ))}
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
          <button type="button" disabled={busy || !note.trim()} onClick={report}>
            Report misclassification
          </button>
        </div>
      )}
    </div>
  );
}

export function Timeline() {
  const [data, setData] = useState<TodayView | null>(null);
  const refresh = useCallback(async () => {
    const t = await getToday();
    setData(t);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  if (!data) return <p className="muted">加载中…</p>;

  return (
    <div className="page">
      <PermissionBanner />
      <h2>时间轴</h2>
      <p className="muted">估计有效主线合计 {formatEstimatedMinutes(data.creditedSeconds)}</p>
      {data.slots.length === 0 && <p className="muted">暂无槽记录</p>}
      {data.slots.map((s) => (
        <SlotRow key={s.start} slot={s} day={data.day} onChange={refresh} />
      ))}
    </div>
  );
}
