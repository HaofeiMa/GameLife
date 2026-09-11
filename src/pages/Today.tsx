import { useCallback, useEffect, useState } from "react";
import { ConfirmEndDay } from "../components/ConfirmEndDay";
import { PermissionBanner } from "../components/PermissionBanner";
import { Progress32 } from "../components/Progress32";
import {
  continuePreviousWorkday,
  endToday,
  freezeDay,
  getToday,
  setQuests,
  type TodayView,
} from "../lib/api";
import { liveMatchLabel } from "../lib/questLive";

const GOLD_DAY_MSG =
  "Gold Day completed. Additional work is recorded, but no more Coins or XP are earned.";

interface QuestDraftLocal {
  text: string;
  evidenceText: string;
  hero: boolean;
}

const EMPTY_QUEST: QuestDraftLocal = {
  text: "",
  evidenceText: "",
  hero: false,
};

function parseEvidenceText(evidenceText: string): string[] {
  return evidenceText
    .split(/[,，\n]/)
    .map((token) => token.trim())
    .filter((token) => token.length > 0);
}

function questsFromView(t: TodayView): QuestDraftLocal[] {
  const loaded = t.quests.map((q) => ({
    text: q.text,
    evidenceText: q.evidence.join(", "),
    hero: q.hero,
  }));
  while (loaded.length < 3) {
    loaded.push({ ...EMPTY_QUEST });
  }
  return loaded.slice(0, 3);
}

export function Today() {
  const [data, setData] = useState<TodayView | null>(null);
  const [quests, setQuestsLocal] = useState<QuestDraftLocal[]>([
    { ...EMPTY_QUEST },
    { ...EMPTY_QUEST },
    { ...EMPTY_QUEST },
  ]);
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
      setQuestsLocal(questsFromView(t));
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

  function updateQuest(index: number, patch: Partial<QuestDraftLocal>) {
    setQuestsLocal((prev) => {
      const next = prev.map((q, i) => (i === index ? { ...q, ...patch } : q));
      if (patch.hero) {
        return next.map((q, i) => ({ ...q, hero: i === index }));
      }
      return next;
    });
  }

  async function saveQuests() {
    setBusy(true);
    try {
      const payload = quests
        .filter((q) => q.text.trim().length > 0)
        .map((q) => ({
          text: q.text.trim(),
          evidence: parseEvidenceText(q.evidenceText),
          hero: q.hero,
        }));
      await setQuests(payload);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleContinuePrevious() {
    setBusy(true);
    try {
      await continuePreviousWorkday();
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
  const liveLabel = liveMatchLabel(data.live, data.quests);

  return (
    <div className="page">
      <h2>今日</h2>
      <PermissionBanner />
      {error && <p className="error">{error}</p>}
      <section>
        <h3>Quest（1–3 条）</h3>
        <p className="live-match">{liveLabel}</p>
        {data.previousWorkday && (
          <button
            type="button"
            className="continue-previous"
            disabled={busy}
            onClick={handleContinuePrevious}
          >
            沿用 {data.previousWorkday}
          </button>
        )}
        {quests.map((q, i) => {
          const tokens = parseEvidenceText(q.evidenceText);
          const cardClass = q.hero ? "quest-card quest-hero" : "quest-card quest-secondary";
          return (
            <div key={i} className={cardClass}>
              <label>
                文案
                <input
                  value={q.text}
                  disabled={busy}
                  placeholder={`Quest ${i + 1}`}
                  onChange={(e) => updateQuest(i, { text: e.target.value })}
                />
              </label>
              <label>
                证据（逗号或换行分隔）
                <textarea
                  value={q.evidenceText}
                  disabled={busy}
                  rows={2}
                  onChange={(e) =>
                    updateQuest(i, { evidenceText: e.target.value })
                  }
                />
              </label>
              {tokens.length > 0 && (
                <div className="quest-evidence-chips">
                  {tokens.map((token) => (
                    <span key={token} className="quest-evidence-chip">
                      {token}
                    </span>
                  ))}
                </div>
              )}
              <label className="quest-hero-toggle">
                <input
                  type="radio"
                  name="quest-hero"
                  checked={q.hero}
                  disabled={busy || q.text.trim().length === 0}
                  onChange={() => updateQuest(i, { hero: true })}
                />
                设为今日主线
              </label>
            </div>
          );
        })}
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
