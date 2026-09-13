import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
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
  rejectedCode,
  wishRejectedMessage,
} from "../lib/feel";
import { minutesUntilShopUnlock } from "../lib/shopUnlock";
import { splitWishes } from "../lib/shopSplit";

const GOLD_DAY_MINUTES = 480;
const HISTORY_LIMIT = 30;

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

function formatMmSs(secs: number): string {
  const total = Math.max(0, Math.floor(secs));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

function formatClock(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function hasTimedXp(wish: WishView): boolean {
  return wish.kind !== "coin" && wish.durationMinutes != null && wish.durationMinutes >= 5;
}

function redemptionStatusLabel(kind: string, status: string): string {
  if (kind === "coin") return "硬币已兑";
  return status;
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
      err: "能量兑换至少 5 分钟",
    };
  }
  return { name: trimmed, price: priceNum, duration: durationNum, err: null };
}

function GlyphSvg({ children, label }: { children: ReactNode; label: string }) {
  return (
    <svg
      className="gift-glyph-svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <title>{label}</title>
      {children}
    </svg>
  );
}

function WishGlyph({ name }: { name: string }) {
  if (/茶/.test(name)) {
    return (
      <GlyphSvg label="茶">
        <path d="M4 10h12v5a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4v-5Z" />
        <path d="M16 11h2.5a2.5 2.5 0 0 1 0 5H16" />
        <path d="M8 4c.4 1 .4 2 0 3M12 4c.4 1 .4 2 0 3" />
      </GlyphSvg>
    );
  }
  if (/外卖|餐/.test(name)) {
    return (
      <GlyphSvg label="餐">
        <ellipse cx="12" cy="15" rx="7" ry="3" />
        <path d="M5 15c0-4 3-7 7-7s7 3 7 7" />
        <path d="M8 8V5M16 8V5" />
      </GlyphSvg>
    );
  }
  if (/书/.test(name)) {
    return (
      <GlyphSvg label="书">
        <path d="M4 5h7v14H4z" />
        <path d="M13 5h7v14h-7z" />
        <path d="M11 5v14M13 5v14" />
      </GlyphSvg>
    );
  }
  if (/游戏/.test(name)) {
    return (
      <GlyphSvg label="手柄">
        <rect x="3" y="9" width="18" height="9" rx="4" />
        <path d="M8 13h4M10 11v4" />
        <circle cx="16.5" cy="13" r="0.8" fill="currentColor" stroke="none" />
        <circle cx="18.2" cy="15" r="0.8" fill="currentColor" stroke="none" />
      </GlyphSvg>
    );
  }
  if (/站|视频|播放/.test(name)) {
    return (
      <GlyphSvg label="播放">
        <rect x="3" y="6" width="18" height="12" rx="3" />
        <path d="M10 9.5v5l5-2.5-5-2.5Z" fill="currentColor" stroke="none" />
      </GlyphSvg>
    );
  }
  const glyph = name.trim().slice(0, 1) || "礼";
  return <span aria-hidden="true">{glyph}</span>;
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
  const [justRedeemed, setJustRedeemed] = useState(false);
  const confirmTimer = useRef<number | null>(null);
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

  useEffect(() => {
    return () => {
      if (confirmTimer.current != null) window.clearTimeout(confirmTimer.current);
    };
  }, []);

  async function handleRedeem() {
    if (redeemInFlight) return;
    redeemInFlight = true;
    setBusy(true);
    setErr(null);
    try {
      await redeem(wish.id, newId());
      setJustRedeemed(true);
      if (confirmTimer.current != null) window.clearTimeout(confirmTimer.current);
      confirmTimer.current = window.setTimeout(() => setJustRedeemed(false), 1600);
      onChanged();
    } catch (e) {
      if (rejectedCode(e) === "already_applied") {
        onChanged();
      } else {
        setErr(wishRejectedMessage(e));
      }
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
  if (justRedeemed) redeemLabel = "已兑";
  else if (locked) redeemLabel = `差 ${remainingMins} 分钟`;
  else if (entertainmentBlocked) redeemLabel = "进行中";
  else if (busy) redeemLabel = "…";

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
    <article className={`gift-card ${isEnergy ? "energy" : "coin"}${justRedeemed ? " just-redeemed" : ""}`}>
      <span className="gift-glyph" aria-hidden="true">
        <WishGlyph name={wish.name} />
      </span>
      <strong>{wish.name}</strong>
      <p className="gift-price">
        {wish.price} {isEnergy ? "能量" : "硬币"}
      </p>
      {wish.durationMinutes != null && <p className="gift-duration">{wish.durationMinutes} 分钟</p>}
      {err && <p className="error">{err}</p>}
      <button
        type="button"
        disabled={busy || locked || entertainmentBlocked || justRedeemed}
        onClick={() => void handleRedeem()}
      >
        {redeemLabel}
      </button>
      <div className="gift-more">
        <button type="button" className="secondary" disabled={busy} onClick={() => setEditing(true)}>
          改
        </button>
        <button type="button" className="secondary" disabled={busy} onClick={() => void handleArchive()}>
          停用
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

function UnlockMeter({
  creditedTodayMinutes,
  unlocked,
  goldDay,
}: {
  creditedTodayMinutes: number;
  unlocked: boolean;
  goldDay: boolean;
}) {
  const have = Math.min(60, Math.max(0, creditedTodayMinutes));
  const remaining = minutesUntilShopUnlock(creditedTodayMinutes);
  const pct = (have / 60) * 100;
  return (
    <div className="shop-hero-col">
      <span className="shop-hero-label">解锁</span>
      {unlocked ? (
        <strong className="shop-hero-value shop-hero-unlocked">
          <span className="shop-unlock-check" aria-hidden="true">✓</span>
          商店已开
        </strong>
      ) : (
        <>
          <strong className="shop-hero-value">
            {have}
            <span className="shop-hero-unit"> / 60 分钟</span>
          </strong>
          <div className="shop-unlock-track" role="progressbar" aria-valuemin={0} aria-valuemax={60} aria-valuenow={have}>
            <div className="shop-unlock-fill" style={{ width: `${pct}%` }} />
          </div>
          <p className="shop-hero-note">差 {remaining} 分钟</p>
        </>
      )}
      {goldDay && <p className="shop-hero-gold">不再获得新币，余额可继续花</p>}
    </div>
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
  const remainingMins = minutesUntilShopUnlock(week.creditedTodayMinutes);
  const session = week.activeEntertainment;
  const remainSecs = session ? session.endsAt - nowSecs : 0;
  const goldDay = week.creditedTodayMinutes >= GOLD_DAY_MINUTES;
  const history = week.redemptions.slice(0, HISTORY_LIMIT);

  return (
    <div className="page shop-page">
      <section className="shop-hero">
        <div className="shop-hero-col">
          <span className="shop-hero-label">硬币</span>
          <strong className="shop-hero-value">{week.coinBalance}</strong>
          <p className="shop-hero-note">可兑换愿望</p>
        </div>
        <div className="shop-hero-col">
          {session ? (
            <>
              <span className="shop-hero-label">进行中剩余</span>
              <strong className="shop-hero-value shop-live-remain">{formatMmSs(remainSecs)}</strong>
              <p className="shop-hero-note">{session.name}</p>
            </>
          ) : (
            <>
              <span className="shop-hero-label">今日能量</span>
              <strong className="shop-hero-value">{week.xpToday}</strong>
            </>
          )}
        </div>
        <UnlockMeter
          creditedTodayMinutes={week.creditedTodayMinutes}
          unlocked={week.xpShopUnlocked}
          goldDay={goldDay}
        />
      </section>

      {session && (
        <section className="shop-live-card">
          <p className="shop-live-kicker">进行中</p>
          <h3 className="shop-live-name">{session.name}</h3>
          <p className="shop-live-remain">{formatMmSs(remainSecs)}</p>
          <p className="shop-live-ends">到期 {formatClock(session.endsAt)}</p>
          <p className="shop-live-rule">结束后仍按规则判定娱乐</p>
        </section>
      )}

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
        {history.length === 0 ? (
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
              {history.map((r: RedemptionView) => (
                <tr
                  key={r.id}
                  className={r.status === "进行中" ? "history-live" : undefined}
                >
                  <td>{formatRedemptionTs(r.ts)}</td>
                  <td>{r.name}</td>
                  <td>{r.kind === "coin" ? "硬币" : "能量"}</td>
                  <td>{r.spent}</td>
                  <td>{redemptionStatusLabel(r.kind, r.status)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
