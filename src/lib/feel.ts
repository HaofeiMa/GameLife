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
