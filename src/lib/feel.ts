export type FeelNotice =
  | { type: "coins"; n: number }
  | { type: "xp"; n: number }
  | { type: "chest" }
  | { type: "goldDay" }
  | { type: "earlyStart"; coins: number }
  | { type: "streak"; n: number }
  | { type: "redeem" }
  | { type: "entertainmentOver"; name: string };

export type LedgerRow = { key: string; coin: number; xp: number; ts: number };

export function noticeText(notice: FeelNotice): string {
  switch (notice.type) {
    case "coins":
      return `+${notice.n} 硬币`;
    case "xp":
      return `+${notice.n} 能量`;
    case "chest":
      return "宝箱已达成";
    case "goldDay":
      return "黄金日";
    case "earlyStart":
      return `早开始 +${notice.coins}`;
    case "streak":
      return `连胜 ${notice.n}`;
    case "redeem":
      return "已兑换";
    case "entertainmentOver":
      return `${notice.name} 时间到`;
  }
}

function maxTs(rows: LedgerRow[]): number | null {
  if (rows.length === 0) return null;
  return Math.max(...rows.map((r) => r.ts));
}

export function coalesceFeelEvents(rows: LedgerRow[]): FeelNotice[] {
  const notices: FeelNotice[] = [];
  let totalCoins = 0;
  let totalXp = 0;

  for (const row of rows) {
    const key = row.key;
    if (key.startsWith("shop_spend:")) {
      notices.push({ type: "redeem" });
    } else if (key.startsWith("ladder:6h:")) {
      notices.push({ type: "chest" });
    } else if (key.startsWith("ladder:8h:")) {
      notices.push({ type: "goldDay" });
    } else if (key.startsWith("ladder:2h:") || key.startsWith("ladder:4h:")) {
      if (row.coin > 0) {
        totalCoins += row.coin;
      }
    } else if (key.startsWith("early_start:")) {
      notices.push({ type: "earlyStart", coins: row.coin });
    } else if (key.startsWith("streak_milestone:")) {
      const tail = key.slice(key.lastIndexOf(":") + 1);
      const n = Number(tail);
      if (!Number.isNaN(n)) {
        notices.push({ type: "streak", n });
      }
    } else if (key.startsWith("validated_coin:")) {
      totalCoins += row.coin;
    } else if (
      key.startsWith("validated_xp:") ||
      key.startsWith("xp_support:") ||
      key.startsWith("xp_admin:")
    ) {
      totalXp += row.xp;
    }
  }

  if (totalCoins > 0) {
    notices.push({ type: "coins", n: totalCoins });
  }
  if (totalXp > 0) {
    notices.push({ type: "xp", n: totalXp });
  }

  return notices;
}

export function nextFeelNotices(
  prevMaxTs: number | null,
  rows: LedgerRow[],
): { notices: FeelNotice[]; maxTs: number | null } {
  const rowsMax = maxTs(rows);

  if (prevMaxTs === null) {
    return { notices: [], maxTs: rowsMax };
  }

  const fresh = rows.filter((r) => r.ts > prevMaxTs);
  const notices = coalesceFeelEvents(fresh);
  const maxTsOut =
    rowsMax === null ? prevMaxTs : Math.max(prevMaxTs, rowsMax);

  return { notices, maxTs: maxTsOut };
}

export function sessionJustEnded(
  prevRemaining: number | null,
  active: { remainingSecs: number } | null,
): boolean {
  return prevRemaining !== null && prevRemaining > 0 && active === null;
}

/** Mirrors Rust `tray_entertainment_minutes`: <=0 → null; else max(1, floor(secs/60)). */
export function trayEntertainmentMinutes(
  remainingSecs: number,
): number | null {
  if (remainingSecs <= 0) {
    return null;
  }
  return Math.max(1, Math.floor(remainingSecs / 60));
}

/** Timed XP lock: active only while endsAt is strictly after now. */
export function entertainmentStillActive(
  endsAt: number,
  nowSecs: number,
): boolean {
  return endsAt > nowSecs;
}

/** When local remaining hits 0 but the parent still has `active`, show this instead of a blank banner. */
export function showEndedFromActive(
  remainingSecs: number,
  name: string,
): string | null {
  if (remainingSecs > 0) return null;
  return `${name} 已结束`;
}

const WISH_REJECTED: Record<string, string> = {
  empty_name: "名称不能为空",
  name_too_long: "名称最多 80 字",
  non_positive_price: "价格必须大于 0",
  entertainment_needs_duration: "XP 时时长至少 5 分钟",
};

export function wishRejectedMessage(err: unknown): string {
  const raw =
    typeof err === "string"
      ? err
      : err instanceof Error
        ? err.message
        : String(err);
  return WISH_REJECTED[raw] ?? raw;
}
