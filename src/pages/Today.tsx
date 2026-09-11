import { useCallback, useEffect, useState } from "react";
import { ConfirmEndDay } from "../components/ConfirmEndDay";
import { EntertainmentBanner } from "../components/EntertainmentBanner";
import { PermissionBanner } from "../components/PermissionBanner";
import { Progress32 } from "../components/Progress32";
import {
  endToday,
  freezeDay,
  getToday,
  setQuests,
  type TodayView,
} from "../lib/api";

const GOLD_DAY_MSG =
  "Gold Day completed. Additional work is recorded, but no more Coins or XP are earned.";

export function Today() {
  const [data, setData] = useState<TodayView | null>(null);
  const [quests, setQuestsLocal] = useState<string[]>(["", "", ""]);
  const [error, setError] = useState<string | null>(null);
  const [confirmEnd, setConfirmEnd] = useState(false);
  const [busy, setBusy] = useState(false);
  const [freezeDate, setFreezeDate] = useState("");
  const [freezeMsg, setFreezeMsg] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const t = await getToday();
      setData(t);
      setFreezeDate((prev) => {
        if (prev && t.freezeCandidates.includes(prev)) return prev;
        return t.defaultFreezeDate ?? t.freezeCandidates[0] ?? "";
      });
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

  if (!data) return <p className="muted">加载中…</p>;

  const showFreeze = data.atRisk || data.freezeCandidates.length > 0;

  return (
    <div className="page">
      <h2>今日</h2>
      <PermissionBanner />
      <EntertainmentBanner
        active={data.activeEntertainment}
        ended={data.endedEntertainment}
      />
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
        <button type="button" disabled={busy} onClick={saveQuests}>
          保存 Quest
        </button>
      </section>

      <section>
        <h3>估计有效主线</h3>
        <p className="hero">{data.creditedLabel} / 8h</p>
        <Progress32 haveSecs={data.creditedSeconds} />
        <p className="muted">
          Chest {data.chest.have}/{data.chest.need} min · Gold {data.gold.have}/
          {data.gold.need} min
        </p>
        {data.goldDay && <p className="gold-day">{GOLD_DAY_MSG}</p>}
      </section>

      <section className="stats-row">
        <div>Coins 今日 +{data.coinsToday}</div>
        <div>
          XP 今日 {data.xpToday}
          {!data.xpShopUnlocked && "（商店锁定）"}
        </div>
        <div>
          连胜 {data.streak}
          {data.atRisk && " ⚠ 今日未达 Chest 有断连风险"}
        </div>
        {data.firstCoreLabel && <div>{data.firstCoreLabel}</div>}
      </section>

      {showFreeze && (
        <section className="freeze-section">
          <h3>保护连胜（冻结）</h3>
          {data.atRisk && (
            <p className="muted">
              今日估计有效主线未满 6 小时。若昨日已 settle 且结果为 failed，可在下一工作日
              settle 前冻结该日以恢复连胜（每月 2 次，按被保护日所属月份计）。
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
                onClick={handleFreeze}
              >
                {busy ? "冻结中…" : `冻结 ${freezeDate || "…"}`}
              </button>
            </>
          ) : (
            <p className="muted">当前没有可冻结的 failed 工作日（窗口已关或额度已用完）。</p>
          )}
          {freezeMsg && <p className="muted">{freezeMsg}</p>}
        </section>
      )}

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
        onConfirm={handleEndToday}
      />
    </div>
  );
}
