#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerSlice {
    pub key: String,
    pub coin: i64,
    pub xp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeelNotice {
    Coins(i64),
    Xp(i64),
    Chest,
    GoldDay,
    EarlyStart { coins: i64 },
    Streak { n: u32 },
    Redeem,
}

pub fn coalesce_feel_events(rows: &[LedgerSlice]) -> Vec<FeelNotice> {
    let mut notices = Vec::new();
    let mut total_coins = 0i64;
    let mut total_xp = 0i64;

    for row in rows {
        let key = &row.key;
        if key.starts_with("shop_spend:") {
            notices.push(FeelNotice::Redeem);
        } else if key.starts_with("ladder:6h:") {
            notices.push(FeelNotice::Chest);
        } else if key.starts_with("ladder:8h:") {
            notices.push(FeelNotice::GoldDay);
        } else if key.starts_with("ladder:2h:") || key.starts_with("ladder:4h:") {
            if row.coin > 0 {
                total_coins += row.coin;
            }
        } else if key.starts_with("early_start:") {
            notices.push(FeelNotice::EarlyStart { coins: row.coin });
        } else if key.starts_with("streak_milestone:") {
            if let Some(n) = key.rsplit(':').next().and_then(|s| s.parse::<u32>().ok()) {
                notices.push(FeelNotice::Streak { n });
            }
        } else if key.starts_with("validated_coin:") {
            total_coins += row.coin;
        } else if key.starts_with("validated_xp:")
            || key.starts_with("xp_support:")
            || key.starts_with("xp_admin:")
        {
            total_xp += row.xp;
        }
    }

    if total_coins > 0 {
        notices.push(FeelNotice::Coins(total_coins));
    }
    if total_xp > 0 {
        notices.push(FeelNotice::Xp(total_xp));
    }

    notices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(key: &str, coin: i64, xp: i64) -> LedgerSlice {
        LedgerSlice {
            key: key.into(),
            coin,
            xp,
        }
    }

    #[test]
    fn merges_ticks_and_keeps_named() {
        let rows = [
            s("validated_xp:2026-09-10:1", 0, 1),
            s("validated_xp:2026-09-10:2", 0, 1),
            s("validated_coin:2026-09-10:1", 1, 0),
            s("ladder:6h:2026-09-10", 3, 0),
            s("xp_support:2026-09-10:1", 0, 6),
        ];
        assert_eq!(
            coalesce_feel_events(&rows),
            vec![FeelNotice::Chest, FeelNotice::Coins(1), FeelNotice::Xp(8),]
        );
    }

    #[test]
    fn shop_spend_is_redeem_not_negative_xp() {
        let rows = [
            s("shop_spend:r1", 0, -20),
            s("validated_xp:2026-09-10:3", 0, 1),
        ];
        assert_eq!(
            coalesce_feel_events(&rows),
            vec![FeelNotice::Redeem, FeelNotice::Xp(1)]
        );
    }

    #[test]
    fn early_start_and_streak_keys() {
        let rows = [
            s("early_start:2026-09-10", 8, 0),
            s("streak_milestone:5", 0, 0),
            s("ladder:8h:2026-09-10", 5, 0),
        ];
        assert_eq!(
            coalesce_feel_events(&rows),
            vec![
                FeelNotice::EarlyStart { coins: 8 },
                FeelNotice::Streak { n: 5 },
                FeelNotice::GoldDay,
            ]
        );
    }
}
