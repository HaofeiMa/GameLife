import { Check, Coins, Pencil, Plus, Timer, Zap } from "lucide-react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import { PageHeader } from "../components/PageHeader";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card } from "../components/ui/card";
import { EmptyLine } from "../components/ui/empty-state";
import { Input } from "../components/ui/input";
import { Progress } from "../components/ui/progress";
import { SkeletonPanel } from "../components/ui/skeleton";
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
import { cn } from "../lib/utils";

const GOLD_DAY_MINUTES = 480;
const HISTORY_LIMIT = 30;

/** Module-level single-flight guard: a double click must not spend twice. */
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

/* --------------------------- wish glyphs ---------------------------- */

function GlyphSvg({ children, label }: { children: ReactNode; label: string }) {
  return (
    <svg
      className="size-7"
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
  return (
    <span className="text-xl font-semibold" aria-hidden="true">
      {glyph}
    </span>
  );
}

/* ---------------------------- gift card ----------------------------- */

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
      <form
        onSubmit={(e) => void handleSave(e)}
        className="flex flex-col gap-2 rounded-xl border bg-card p-4"
      >
        <Input
          value={editName}
          disabled={busy}
          placeholder="名称"
          onChange={(e) => setEditName(e.target.value)}
        />
        <Input
          type="number"
          min={1}
          value={editPrice}
          disabled={busy}
          placeholder="价格"
          onChange={(e) => setEditPrice(e.target.value)}
        />
        {isEnergy && (
          <Input
            type="number"
            min={5}
            value={editDuration}
            disabled={busy}
            placeholder="分钟"
            onChange={(e) => setEditDuration(e.target.value)}
          />
        )}
        {err && <p className="text-[11px] text-destructive">{err}</p>}
        <div className="mt-auto flex gap-2">
          <Button type="submit" size="sm" disabled={busy} className="flex-1">
            保存
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => setEditing(false)}
          >
            取消
          </Button>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={busy}
          onClick={() => void handleArchive()}
          className="text-destructive hover:bg-destructive/10 hover:text-destructive"
        >
          停用这个愿望
        </Button>
      </form>
    );
  }

  return (
    <article
      className={cn(
        "group relative flex flex-col gap-1.5 rounded-2xl p-2.5 transition-all duration-200",
        "hover:-translate-y-0.5 hover:shadow-[0_10px_22px_-14px_rgba(120,95,60,0.55)]",
        locked
          ? "bg-loot shadow-none"
          : "bg-card shadow-[0_4px_14px_-10px_rgba(120,95,60,0.6)]",
        justRedeemed && "animate-pop bg-success/5",
      )}
    >
      <button
        type="button"
        aria-label={`编辑 ${wish.name}`}
        disabled={busy}
        onClick={() => setEditing(true)}
        className={cn(
          "absolute right-2 top-2 rounded-md p-1.5 text-muted-foreground transition-opacity",
          "hover:bg-accent hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100",
          "opacity-0 disabled:pointer-events-none",
        )}
      >
        <Pencil className="size-3.5" aria-hidden />
      </button>

      <div
        className={cn(
          "flex size-8 items-center justify-center rounded-[10px] text-[16px]",
          isEnergy ? "bg-primary/12 text-primary" : "bg-gold-soft text-gold",
        )}
      >
        <WishGlyph name={wish.name} />
      </div>
      <p className="line-clamp-2 text-[13px] font-semibold leading-[1.35]">
        {wish.name}
      </p>
      <p className="flex items-center gap-1 text-[11.5px] tabular-nums text-muted-foreground">
        <i className="not-italic" aria-hidden>
          {isEnergy ? "⚡" : "◉"}
        </i>
        {wish.price}
        {wish.durationMinutes != null && (
          <>
            <span className="text-input">·</span>
            <Timer className="size-3" aria-hidden />
            {wish.durationMinutes} 分钟
          </>
        )}
      </p>
      {err && <p className="mt-2 text-[11px] text-destructive">{err}</p>}

      <button
        type="button"
        disabled={busy || locked || entertainmentBlocked || justRedeemed}
        onClick={() => void handleRedeem()}
        className={cn(
          "mt-auto w-full rounded-[10px] py-[5px] text-center text-xs font-semibold transition-colors",
          locked || entertainmentBlocked || justRedeemed
            ? "bg-pip font-medium text-[hsl(34_17%_62%)]"
            : "bg-primary text-primary-foreground hover:brightness-[1.04]",
        )}
      >
        {redeemLabel}
      </button>
    </article>
  );
}

function AddCard({ kind, onCreated }: { kind: "coin" | "xp"; onCreated: () => void }) {
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
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="flex min-h-[9.5rem] flex-col items-center justify-center gap-2 rounded-xl border border-dashed text-muted-foreground transition-all duration-200 hover:-translate-y-0.5 hover:border-primary/50 hover:text-primary"
      >
        <Plus className="size-5" aria-hidden />
        <span className="text-xs">添加</span>
      </button>
    );
  }

  return (
    <form
      onSubmit={(e) => void handleSubmit(e)}
      className="flex flex-col gap-2 rounded-xl border bg-card p-4"
    >
      <Input
        value={name}
        disabled={busy}
        placeholder="名称"
        onChange={(e) => setName(e.target.value)}
      />
      <Input
        type="number"
        min={1}
        value={price}
        disabled={busy}
        placeholder="价格"
        onChange={(e) => setPrice(e.target.value)}
      />
      {kind === "xp" && (
        <Input
          type="number"
          min={5}
          value={duration}
          disabled={busy}
          placeholder="分钟"
          onChange={(e) => setDuration(e.target.value)}
        />
      )}
      {err && <p className="text-[11px] text-destructive">{err}</p>}
      <div className="mt-auto flex gap-2">
        <Button type="submit" size="sm" disabled={busy} className="flex-1">
          添加
        </Button>
        <Button type="button" variant="outline" size="sm" onClick={() => setOpen(false)}>
          取消
        </Button>
      </div>
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
    <section className="space-y-3">
      <div className="flex items-baseline gap-[9px] px-0.5 pb-[9px]">
        <h2 className="text-[13.5px] font-semibold">{title}</h2>
        <span className="text-[11px] text-muted-foreground">{hint}</span>
        <span className="ml-auto text-[11px] tabular-nums text-muted-foreground">
          共 {kind === "coin" ? week.coinBalance : week.xpToday}
        </span>
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(148px,1fr))] gap-3">
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

function HeaderStat({
  icon,
  label,
  value,
  emphasis,
}: {
  icon: React.ReactNode;
  label: string;
  value: React.ReactNode;
  emphasis?: boolean;
}) {
  return (
    <span className="flex items-center gap-1.5">
      {icon}
      <span className="text-muted-foreground">{label}</span>
      <span
        className={cn(
          "font-medium tabular-nums",
          emphasis && "animate-pulse-soft text-primary",
        )}
      >
        {value}
      </span>
    </span>
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

  const { coin, energy } = week
    ? splitWishes(week.wishes)
    : { coin: [], energy: [] };
  const remainingMins = week ? minutesUntilShopUnlock(week.creditedTodayMinutes) : 0;
  const session = week?.activeEntertainment ?? null;
  const remainSecs = session ? session.endsAt - nowSecs : 0;
  const goldDay = (week?.creditedTodayMinutes ?? 0) >= GOLD_DAY_MINUTES;
  const have = Math.min(60, Math.max(0, week?.creditedTodayMinutes ?? 0));
  const history = (week?.redemptions ?? []).slice(0, HISTORY_LIMIT);

  const header = (
    <PageHeader
      title={<h1 className="text-lg font-semibold tracking-tight">商店</h1>}
      actions={
        week ? (
          <div className="flex items-center gap-3 text-xs">
            <HeaderStat
              icon={<Coins className="size-3.5 text-warning" aria-hidden />}
              label="硬币"
              value={week.coinBalance}
            />
            <span className="text-border">|</span>
            {session ? (
              <HeaderStat
                icon={<Timer className="size-3.5 text-primary" aria-hidden />}
                label={`${session.name} 剩余`}
                value={formatMmSs(remainSecs)}
                emphasis
              />
            ) : (
              <HeaderStat
                icon={<Zap className="size-3.5 text-primary" aria-hidden />}
                label="今日能量"
                value={week.xpToday}
              />
            )}
            <span className="text-border">|</span>
            {week.xpShopUnlocked ? (
              <span className="flex items-center gap-1.5 text-success">
                <Check className="size-3.5" aria-hidden />
                商店已开
              </span>
            ) : (
              <span className="flex items-center gap-2">
                <span className="text-muted-foreground">解锁</span>
                <span className="w-20">
                  <Progress
                    value={(have / 60) * 100}
                    aria-label="商店解锁进度"
                    barClassName="bg-gradient-to-r from-primary/70 to-primary"
                  />
                </span>
                <span className="tabular-nums text-muted-foreground">
                  {have}/60 分钟
                </span>
              </span>
            )}
            {goldDay && (
              <span className="text-warning">黄金日 · 不再获得新币</span>
            )}
          </div>
        ) : undefined
      }
    />
  );

  if (error) {
    return (
      <>
        {header}
        <div className="flex-1 overflow-y-auto">
          <div className="mx-auto max-w-5xl px-6 py-5">
            <Card className="flex flex-col items-center gap-3 p-8 text-center">
              <p className="text-sm text-destructive">{error}</p>
              <Button variant="outline" size="sm" onClick={() => refresh()}>
                重试
              </Button>
            </Card>
          </div>
        </div>
      </>
    );
  }

  if (!week) {
    return (
      <>
        {header}
        <div className="flex-1 overflow-y-auto">
          <div className="mx-auto max-w-5xl space-y-4 px-6 py-5">
            <SkeletonPanel rows={3} />
          </div>
        </div>
      </>
    );
  }


  return (
    <>
      {header}
      <div className="flex-1 overflow-y-auto">
        <div className="flex flex-1 flex-col gap-3 px-[22px] pt-3 pb-4">
          {/* The mockup's .session — the one saturated block in the app. */}
          {session && (
            <Card className="flex items-center gap-5 rounded-[18px] border-transparent bg-[linear-gradient(120deg,#5FBE8C,#43A97A_55%,#3E9A70)] px-[18px] py-3 text-white shadow-[0_16px_34px_-22px_rgba(67,169,122,0.95)]">
              <div className="min-w-0 flex-1">
                <div className="text-[11px] font-semibold tracking-[0.08em] opacity-85">
                  进行中 · 剩余
                </div>
                <div className="mt-0.5 text-base font-semibold">
                  {session.name}
                  <span className="ml-2.5 text-[11.5px] font-normal opacity-85">
                    到期 {formatClock(session.endsAt)} · 结束后仍按规则判定娱乐
                  </span>
                </div>
              </div>
              <div className="animate-pulse-soft shrink-0 text-[26px] font-bold leading-[1.1] tracking-[-0.04em] tabular-nums">
                {formatMmSs(remainSecs)}
              </div>
            </Card>
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

          <section className="space-y-3">
            <h2 className="text-sm font-medium">兑换记录</h2>
            {history.length === 0 ? (
              <EmptyLine>还没有兑换。</EmptyLine>
            ) : (
              <Card className="overflow-hidden">
                <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="border-b text-left text-xs text-muted-foreground">
                        <th className="h-10 px-4 font-medium">时间</th>
                        <th className="h-10 px-4 font-medium">商品</th>
                        <th className="h-10 px-4 font-medium">类型</th>
                        <th className="h-10 px-4 text-right font-medium">扣额</th>
                        <th className="h-10 px-4 font-medium">状态</th>
                      </tr>
                    </thead>
                    <tbody>
                      {history.map((r: RedemptionView) => (
                        <tr
                          key={r.id}
                          className={cn(
                            "border-b transition-colors last:border-0 hover:bg-muted/50",
                            r.status === "进行中" && "bg-primary/5",
                          )}
                        >
                          <td className="px-4 py-2.5 tabular-nums text-muted-foreground">
                            {formatRedemptionTs(r.ts)}
                          </td>
                          <td className="px-4 py-2.5">{r.name}</td>
                          <td className="px-4 py-2.5">{r.kind === "coin" ? "硬币" : "能量"}</td>
                          <td className="px-4 py-2.5 text-right tabular-nums">{r.spent}</td>
                          <td className="px-4 py-2.5">
                            {r.status === "进行中" ? (
                              <Badge tone="primary">进行中</Badge>
                            ) : (
                              <span className="text-muted-foreground">
                                {redemptionStatusLabel(r.kind, r.status)}
                              </span>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </Card>
            )}
          </section>
        </div>
      </div>
    </>
  );
}
