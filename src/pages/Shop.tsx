import { useCallback, useEffect, useState, type FormEvent } from "react";
import { EntertainmentBanner } from "../components/EntertainmentBanner";
import {
  archiveWish,
  createWish,
  getWeek,
  redeem,
  updateWish,
  type RedemptionView,
  type WeekView,
  type WishView,
} from "../lib/api";
import {
  entertainmentStillActive,
  wishRejectedMessage,
} from "../lib/feel";
import { splitWishes } from "../lib/shopSplit";

let redeemInFlight = false;

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

function parseWishFields(
  name: string,
  price: string,
  kind: string,
  duration: string,
): { name: string; price: number; duration: number | null; err: string | null } {
  const trimmed = name.trim();
  const priceNum = Number(price);
  const durationNum = kind === "coin" ? null : Number(duration);
  if (!trimmed) {
    return { name: trimmed, price: priceNum, duration: durationNum, err: "名称不能为空" };
  }
  if (!Number.isFinite(priceNum) || priceNum <= 0) {
    return { name: trimmed, price: priceNum, duration: durationNum, err: "价格必须大于 0" };
  }
  if (kind !== "coin" && (!Number.isFinite(durationNum!) || durationNum! < 5)) {
    return {
      name: trimmed,
      price: priceNum,
      duration: durationNum,
      err: "能量兑换时长至少 5 分钟",
    };
  }
  return { name: trimmed, price: priceNum, duration: durationNum, err: null };
}

function GiftCard({
  wish,
  week,
  nowSecs,
  remainingMins,
  onChanged,
}: {
  wish: WishView;
  week: WeekView;
  nowSecs: number;
  remainingMins: number;
  onChanged: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [editName, setEditName] = useState(wish.name);
  const [editPrice, setEditPrice] = useState(String(wish.price));
  const [editDuration, setEditDuration] = useState(
    wish.durationMinutes != null ? String(wish.durationMinutes) : "30",
  );
  const isEnergy = wish.kind !== "coin";
  const locked = !week.xpShopUnlocked;
  const entertainmentBlocked =
    hasTimedXp(wish) &&
    week.activeEntertainment != null &&
    entertainmentStillActive(week.activeEntertainment.endsAt, nowSecs);

  async function handleRedeem() {
    if (redeemInFlight) return;
    redeemInFlight = true;
    setBusy(true);
    setErr(null);
    try {
      await redeem(wish.id, newId());
      onChanged();
    } catch (e) {
      setErr(wishRejectedMessage(e));
    } finally {
      redeemInFlight = false;
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
      setErr(wishRejectedMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleSave(e: FormEvent) {
    e.preventDefault();
    const parsed = parseWishFields(editName, editPrice, wish.kind, editDuration);
    if (parsed.err) {
      setErr(parsed.err);
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      await updateWish(wish.id, parsed.name, parsed.price, parsed.duration);
      setEditing(false);
      onChanged();
    } catch (e) {
      setErr(wishRejectedMessage(e));
    } finally {
      setBusy(false);
    }
  }

  let redeemLabel = "兑换";
  if (locked) redeemLabel = `还差 ${remainingMins} 分钟`;
  else if (entertainmentBlocked) redeemLabel = "进行中";
  else if (busy) redeemLabel = "…";

  const glyph = wish.name.trim().slice(0, 1) || "礼";

  if (editing) {
    return (
      <form className="gift-card gift-edit" onSubmit={(e) => void handleSave(e)}>
        <input value={editName} disabled={busy} onChange={(e) => setEditName(e.target.value)} />
        <input
          type="number"
          min={1}
          value={editPrice}
          disabled={busy}
          onChange={(e) => setEditPrice(e.target.value)}
        />
        {isEnergy && (
          <input
            type="number"
            min={5}
            value={editDuration}
            disabled={busy}
            onChange={(e) => setEditDuration(e.target.value)}
          />
        )}
        {err && <p className="error">{err}</p>}
        <button type="submit" disabled={busy}>
          保存
        </button>
        <button type="button" className="secondary" onClick={() => setEditing(false)}>
          取消
        </button>
      </form>
    );
  }

  return (
    <article className={`gift-card ${isEnergy ? "energy" : "coin"}`}>
      <span className="gift-glyph" aria-hidden="true">
        {glyph}
      </span>
      <strong>{wish.name}</strong>
      <p>
        {wish.price} {isEnergy ? "能量" : "硬币"}
        {wish.durationMinutes != null ? ` · ${wish.durationMinutes} 分钟` : ""}
      </p>
      {err && <p className="error">{err}</p>}
      <button
        type="button"
        disabled={busy || locked || entertainmentBlocked}
        onClick={() => void handleRedeem()}
      >
        {redeemLabel}
      </button>
      <div className="gift-more">
        <button type="button" className="secondary" disabled={busy} onClick={() => setEditing(true)}>
          改
        </button>
        <button type="button" className="secondary" disabled={busy} onClick={() => void handleArchive()}>
          停
        </button>
      </div>
    </article>
  );
}

function AddCard({
  kind,
  onCreated,
}: {
  kind: "coin" | "xp";
  onCreated: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [price, setPrice] = useState(kind === "coin" ? "32" : "20");
  const [duration, setDuration] = useState("30");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    const parsed = parseWishFields(name, price, kind, duration);
    if (parsed.err) {
      setErr(parsed.err);
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      await createWish(
        newId(),
        parsed.name,
        kind,
        parsed.price,
        kind === "xp" ? parsed.duration : null,
      );
      setName("");
      setOpen(false);
      onCreated();
    } catch (e) {
      setErr(wishRejectedMessage(e));
    } finally {
      setBusy(false);
    }
  }

  if (!open) {
    return (
      <button type="button" className="gift-card gift-add" onClick={() => setOpen(true)}>
        <span aria-hidden="true">+</span>
        添加
      </button>
    );
  }

  return (
    <form className="gift-card gift-edit" onSubmit={(e) => void handleSubmit(e)}>
      <input
        value={name}
        disabled={busy}
        placeholder="名称"
        onChange={(e) => setName(e.target.value)}
      />
      <input
        type="number"
        min={1}
        value={price}
        disabled={busy}
        onChange={(e) => setPrice(e.target.value)}
      />
      {kind === "xp" && (
        <input
          type="number"
          min={5}
          value={duration}
          disabled={busy}
          onChange={(e) => setDuration(e.target.value)}
        />
      )}
      {err && <p className="error">{err}</p>}
      <button type="submit" disabled={busy}>
        添加
      </button>
      <button type="button" className="secondary" onClick={() => setOpen(false)}>
        取消
      </button>
    </form>
  );
}

function GiftSection({
  title,
  hint,
  wishes,
  kind,
  week,
  nowSecs,
  remainingMins,
  onChanged,
}: {
  title: string;
  hint: string;
  wishes: WishView[];
  kind: "coin" | "xp";
  week: WeekView;
  nowSecs: number;
  remainingMins: number;
  onChanged: () => void;
}) {
  return (
    <section className="shop-section">
      <h3>{title}</h3>
      <p className="muted">{hint}</p>
      <div className="gift-grid">
        {wishes.map((w) => (
          <GiftCard
            key={w.id}
            wish={w}
            week={week}
            nowSecs={nowSecs}
            remainingMins={remainingMins}
            onChanged={onChanged}
          />
        ))}
        <AddCard kind={kind} onCreated={onChanged} />
      </div>
    </section>
  );
}

export function Shop() {
  const [week, setWeek] = useState<WeekView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [nowSecs, setNowSecs] = useState(() => Math.floor(Date.now() / 1000));

  const refresh = useCallback(() => {
    getWeek()
      .then((w) => {
        setWeek(w);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    refresh();
    const poll = window.setInterval(() => {
      if (document.visibilityState === "visible") refresh();
    }, 5_000);
    const tick = window.setInterval(() => {
      setNowSecs(Math.floor(Date.now() / 1000));
    }, 1_000);
    return () => {
      window.clearInterval(poll);
      window.clearInterval(tick);
    };
  }, [refresh]);

  if (error) {
    return (
      <div className="page">
        <p className="error">{error}</p>
        <button type="button" onClick={() => refresh()}>
          重试
        </button>
      </div>
    );
  }
  if (!week) return <p className="muted">加载中…</p>;

  const { coin, energy } = splitWishes(week.wishes);
  const remainingMins = Math.max(0, 60 - week.creditedTodayMinutes);

  return (
    <div className="page shop-page">
      <header className="shop-head">
        <div>
          <h2>商店</h2>
          <p className="muted">礼品卡墙。锁定时两栏都不能兑。</p>
        </div>
        <div className="shop-badges">
          <div>
            <span>硬币</span>
            <strong>{week.coinBalance}</strong>
          </div>
          <div>
            <span>今日能量</span>
            <strong>{week.xpToday}</strong>
          </div>
        </div>
      </header>
      {!week.xpShopUnlocked && (
        <p className="gold-day">商店锁定：估计有效主线还差 {remainingMins} 分钟（满 60 分钟解锁）。</p>
      )}
      <EntertainmentBanner
        active={week.activeEntertainment}
        ended={week.endedEntertainment}
      />
      <GiftSection
        title="硬币兑换"
        hint="无时长，兑完即得。"
        wishes={coin}
        kind="coin"
        week={week}
        nowSecs={nowSecs}
        remainingMins={remainingMins}
        onChanged={refresh}
      />
      <GiftSection
        title="能量兑换"
        hint="有时长。同时只能一段娱乐。"
        wishes={energy}
        kind="xp"
        week={week}
        nowSecs={nowSecs}
        remainingMins={remainingMins}
        onChanged={refresh}
      />
      <section className="shop-history">
        <h3>兑换记录</h3>
        {week.redemptions.length === 0 ? (
          <p className="muted">还没有兑换。</p>
        ) : (
          <table className="history-table">
            <thead>
              <tr>
                <th>时间</th>
                <th>商品</th>
                <th>类型</th>
                <th>扣额</th>
                <th>状态</th>
              </tr>
            </thead>
            <tbody>
              {week.redemptions.map((r: RedemptionView) => (
                <tr key={r.id}>
                  <td>{formatRedemptionTs(r.ts)}</td>
                  <td>{r.name}</td>
                  <td>{r.kind === "coin" ? "硬币" : "能量"}</td>
                  <td>{r.spent}</td>
                  <td>{r.status}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
