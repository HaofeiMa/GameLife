import { useCallback, useEffect, useState } from "react";
import { EntertainmentBanner } from "../components/EntertainmentBanner";
import {
  archiveWish,
  createWish,
  getWeek,
  redeem,
  type RedemptionView,
  type WeekView,
  type WishView,
} from "../lib/api";

function newId(): string {
  return crypto.randomUUID();
}

function formatRedemptionTs(ts: number): string {
  return new Date(ts * 1000).toLocaleString("zh-CN", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function hasTimedXp(wish: WishView): boolean {
  return wish.kind !== "coin" && wish.durationMinutes != null && wish.durationMinutes >= 5;
}

function WishRow({
  wish,
  week,
  onChanged,
}: {
  wish: WishView;
  week: WeekView;
  onChanged: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const isXp = wish.kind !== "coin";
  const locked = isXp && !week.xpShopUnlocked;
  const entertainmentBlocked =
    hasTimedXp(wish) && week.activeEntertainment != null;

  async function handleRedeem() {
    setBusy(true);
    setErr(null);
    try {
      await redeem(wish.id, newId());
      onChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleArchive() {
    setBusy(true);
    setErr(null);
    try {
      await archiveWish(wish.id);
      onChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  let redeemLabel = "兑换";
  if (locked) {
    redeemLabel = "锁定（未满 60m）";
  } else if (entertainmentBlocked) {
    redeemLabel = "先结束当前的";
  } else if (busy) {
    redeemLabel = "兑换中…";
  }

  const priceLabel = isXp ? `${wish.price} XP` : `${wish.price} Coins`;

  return (
    <div className="wish-row">
      <div className="wish-info">
        <strong>{wish.name}</strong>
        <span className="muted"> · {priceLabel}</span>
        {wish.durationMinutes != null && (
          <span className="muted"> · {wish.durationMinutes} min</span>
        )}
        {err && <p className="error">{err}</p>}
      </div>
      <div className="wish-actions">
        <button
          type="button"
          disabled={busy || locked || entertainmentBlocked}
          onClick={handleRedeem}
        >
          {redeemLabel}
        </button>
        <button
          type="button"
          className="secondary"
          disabled={busy}
          onClick={handleArchive}
        >
          停用
        </button>
      </div>
    </div>
  );
}

function AddWishForm({ onCreated }: { onCreated: () => void }) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"coin" | "xp">("xp");
  const [price, setPrice] = useState("10");
  const [duration, setDuration] = useState("30");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setErr(null);
    const trimmed = name.trim();
    const priceNum = Number(price);
    const durationNum = kind === "xp" ? Number(duration) : null;
    if (!trimmed) {
      setErr("名称不能为空");
      setBusy(false);
      return;
    }
    if (!Number.isFinite(priceNum) || priceNum <= 0) {
      setErr("价格必须大于 0");
      setBusy(false);
      return;
    }
    if (kind === "xp" && (!Number.isFinite(durationNum!) || durationNum! < 5)) {
      setErr("XP 时时长至少 5 分钟");
      setBusy(false);
      return;
    }
    try {
      await createWish(
        newId(),
        trimmed,
        kind,
        priceNum,
        kind === "xp" ? durationNum : null,
      );
      setName("");
      setPrice("10");
      setDuration("30");
      onCreated();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className="wish-form" onSubmit={handleSubmit}>
      <h3>添加愿望</h3>
      <label>
        名称
        <input
          value={name}
          disabled={busy}
          placeholder="例如：30 分钟视频"
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      <label>
        类型
        <select
          value={kind}
          disabled={busy}
          onChange={(e) => setKind(e.target.value as "coin" | "xp")}
        >
          <option value="xp">XP（娱乐时长）</option>
          <option value="coin">Coin</option>
        </select>
      </label>
      <label>
        价格
        <input
          type="number"
          min={1}
          value={price}
          disabled={busy}
          onChange={(e) => setPrice(e.target.value)}
        />
      </label>
      {kind === "xp" && (
        <label>
          时长（分钟，≥5）
          <input
            type="number"
            min={5}
            value={duration}
            disabled={busy}
            onChange={(e) => setDuration(e.target.value)}
          />
        </label>
      )}
      {err && <p className="error">{err}</p>}
      <button type="submit" disabled={busy}>
        {busy ? "添加中…" : "添加愿望"}
      </button>
    </form>
  );
}

function RedemptionHistory({ rows }: { rows: RedemptionView[] }) {
  if (rows.length === 0) return null;
  return (
    <section>
      <h3>兑换记录</h3>
      <ul className="redemption-list">
        {rows.map((r) => (
          <li key={r.id}>
            <span className="muted">{formatRedemptionTs(r.ts)}</span>
            <span>{r.name}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function Shop() {
  const [week, setWeek] = useState<WeekView | null>(null);

  const refresh = useCallback(() => {
    getWeek().then(setWeek).catch(console.error);
  }, []);

  useEffect(() => {
    refresh();
    const id = window.setInterval(refresh, 30_000);
    return () => window.clearInterval(id);
  }, [refresh]);

  if (!week) return <p className="muted">加载中…</p>;

  return (
    <div className="page">
      <h2>商店</h2>
      <p>Coins 余额 {week.coinBalance} · XP 今日 {week.xpToday}</p>
      {!week.xpShopUnlocked && (
        <p className="muted">估计有效主线未满 60 分钟，XP 愿望锁定。</p>
      )}
      <EntertainmentBanner
        active={week.activeEntertainment}
        ended={week.endedEntertainment}
      />
      <AddWishForm onCreated={refresh} />
      <section>
        <h3>愿望</h3>
        {week.wishes.length === 0 && (
          <p className="muted">还没有愿望，添加一个你真正想兑的奖励。</p>
        )}
        {week.wishes.map((w) => (
          <WishRow key={w.id} wish={w} week={week} onChanged={refresh} />
        ))}
      </section>
      <RedemptionHistory rows={week.redemptions} />
    </div>
  );
}
