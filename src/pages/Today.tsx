import { useCallback, useEffect, useState } from "react";
import { ConfirmEndDay } from "../components/ConfirmEndDay";
import { Progress32 } from "../components/Progress32";
import { endToday, getToday, setQuests, type TodayView } from "../lib/api";

const GOLD_DAY_MSG =
  "Gold Day completed. Additional work is recorded, but no more Coins or XP are earned.";

export function Today() {
  const [data, setData] = useState<TodayView | null>(null);
  const [quests, setQuestsLocal] = useState<string[]>(["", "", ""]);
  const [error, setError] = useState<string | null>(null);
  const [confirmEnd, setConfirmEnd] = useState(false);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const t = await getToday();
      setData(t);
      const padded = [...t.quests];
      while (padded.length < 3) padded.push("");
      setQuestsLocal(padded.slice(0, 3));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 30_000);
    return () => clearInterval(id);
  }, [refresh]);

  async function saveQuests() {
    setBusy(true);
    try {
      await setQuests(quests.filter((q) => q.trim().length > 0));
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
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

  if (!data) return <p className="muted">加载中…</p>;

  return (
    <div className="page">
      <h2>今日</h2>
      {error && <p className="error">{error}</p>}
      <section>
        <h3>Quest（1–3 条）</h3>
        {quests.map((q, i) => (
          <input
            key={i}
            value={q}
            disabled={busy}
            placeholder={`Quest ${i + 1}`}
            onChange={(e) => {
              const next = [...quests];
              next[i] = e.target.value;
              setQuestsLocal(next);
            }}
          />
        ))}
        <button type="button" disabled={busy} onClick={saveQuests}>保存 Quest</button>
      </section>

      <section>
        <h3>估计有效主线</h3>
        <p className="hero">{data.creditedLabel} / 8h</p>
        <Progress32 haveSecs={data.creditedSeconds} />
        <p className="muted">
          Chest {data.chest.have}/{data.chest.need} min · Gold {data.gold.have}/{data.gold.need} min
        </p>
        {data.goldDay && <p className="gold-day">{GOLD_DAY_MSG}</p>}
      </section>

      <section className="stats-row">
        <div>Coins 今日 +{data.coinsToday}</div>
        <div>XP 今日 {data.xpToday}{!data.xpShopUnlocked && "（商店锁定）"}</div>
        <div>连胜 {data.streak}{data.atRisk && " ⚠"}</div>
        {data.firstCoreLabel && <div>{data.firstCoreLabel}</div>}
      </section>

      <button type="button" className="danger" disabled={busy} onClick={() => setConfirmEnd(true)}>
        结束今天
      </button>
      <ConfirmEndDay
        open={confirmEnd}
        onCancel={() => setConfirmEnd(false)}
        onConfirm={handleEndToday}
      />
    </div>
  );
}
