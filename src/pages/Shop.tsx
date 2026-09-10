import { useCallback, useEffect, useState } from "react";
import { getWeek, redeem, type WeekView, type WishView } from "../lib/api";

function newRedemptionId(): string {
  return crypto.randomUUID();
}

function WishButton({
  wish,
  week,
  onRedeemed,
}: {
  wish: WishView;
  week: WeekView;
  onRedeemed: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const isXp = wish.kind !== "coin";
  const locked = isXp && !week.xpShopUnlocked;

  async function handleRedeem() {
    setBusy(true);
    setErr(null);
    try {
      await redeem(wish.id, newRedemptionId());
      onRedeemed();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  const priceLabel = isXp ? `${wish.price} XP` : `${wish.price} Coins`;

  return (
    <div className="wish-row">
      <div>
        <strong>{wish.name}</strong>
        <span className="muted"> · {priceLabel}</span>
        {wish.durationMinutes != null && (
          <span className="muted"> · {wish.durationMinutes} min</span>
        )}
      </div>
      <button type="button" disabled={busy || locked} onClick={handleRedeem}>
        {locked ? "锁定（未满 60m）" : busy ? "兑换中…" : "兑换"}
      </button>
      {err && <p className="error">{err}</p>}
    </div>
  );
}

export function Shop() {
  const [week, setWeek] = useState<WeekView | null>(null);

  const refresh = useCallback(() => {
    getWeek().then(setWeek).catch(console.error);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  if (!week) return <p className="muted">加载中…</p>;

  return (
    <div className="page">
      <h2>商店</h2>
      <p>Coins 余额 {week.coinBalance} · XP 今日 {week.xpToday}</p>
      {!week.xpShopUnlocked && (
        <p className="muted">估计有效主线未满 60 分钟，XP 愿望锁定。</p>
      )}
      {week.wishes.length === 0 && (
        <p className="muted">暂无愿望（可在数据库 wishes 表添加）。</p>
      )}
      {week.wishes.map((w) => (
        <WishButton key={w.id} wish={w} week={week} onRedeemed={refresh} />
      ))}
    </div>
  );
}
