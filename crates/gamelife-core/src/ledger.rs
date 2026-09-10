use crate::r#const::{
    CHEST_SECS, COIN_TICK_SECS, GOLD_DAY_SECS, MAX_COIN_TICKS, MAX_XP_TICKS, XP_TICK_SECS,
};

pub struct RewardEvent {
    pub key: String,
    pub coin: i64,
    pub xp: i64,
}

const LADDER_THRESHOLDS: [(u64, &str, i64); 4] = [
    (7200, "2h", 1),
    (14400, "4h", 2),
    (CHEST_SECS, "6h", 3),
    (GOLD_DAY_SECS, "8h", 5),
];

pub fn tick_keys_for_credited(day: &str, before: i64, after: i64) -> Vec<RewardEvent> {
    let cap = GOLD_DAY_SECS as i64;
    if before >= cap {
        return vec![];
    }

    let after_c = after.min(cap);
    let before_c = before.min(cap);
    let mut events = Vec::new();

    let old_coin = (before_c / COIN_TICK_SECS as i64).min(MAX_COIN_TICKS as i64);
    let new_coin = (after_c / COIN_TICK_SECS as i64).min(MAX_COIN_TICKS as i64);
    for i in (old_coin + 1)..=new_coin {
        events.push(RewardEvent {
            key: format!("validated_coin:{day}:{i}"),
            coin: 1,
            xp: 0,
        });
    }

    let old_xp = (before_c / XP_TICK_SECS as i64).min(MAX_XP_TICKS as i64);
    let new_xp = (after_c / XP_TICK_SECS as i64).min(MAX_XP_TICKS as i64);
    for i in (old_xp + 1)..=new_xp {
        events.push(RewardEvent {
            key: format!("validated_xp:{day}:{i}"),
            coin: 0,
            xp: 1,
        });
    }

    for &(threshold, label, coin) in &LADDER_THRESHOLDS {
        let t = threshold as i64;
        if before < t && t <= after {
            events.push(RewardEvent {
                key: format!("ladder:{label}:{day}"),
                coin,
                xp: 0,
            });
        }
    }

    events
}

pub fn support_xp_key(day: &str, slot_start: i64) -> RewardEvent {
    RewardEvent {
        key: format!("xp_support:{day}:{slot_start}"),
        coin: 0,
        xp: 6,
    }
}

pub fn admin_xp_key(day: &str, slot_start: i64) -> RewardEvent {
    RewardEvent {
        key: format!("xp_admin:{day}:{slot_start}"),
        coin: 0,
        xp: 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_minute_partials_do_not_lose_xp_to_floor() {
        // 两个槽各 720s credited：逐槽 floor(10*720/900)=8 → 16；累计 1440/90=16 XP ticks
        let mut ev = tick_keys_for_credited("2026-09-10", 0, 720);
        ev.extend(tick_keys_for_credited("2026-09-10", 720, 1440));
        let xp: i64 = ev.iter().map(|e| e.xp).sum();
        assert_eq!(xp, 16);
    }

    #[test]
    fn gold_day_stops_all_currency() {
        let ev = tick_keys_for_credited("2026-09-10", 28800, 28800 + 900);
        assert!(ev.is_empty());
    }

    #[test]
    fn coin_ticks_cap_at_32() {
        let ev = tick_keys_for_credited("2026-09-10", 0, 28800);
        let coins: i64 = ev.iter().map(|e| e.coin).sum();
        assert!(coins >= 32 + 1 + 2 + 3 + 5);
        let tick_coins: i64 = ev
            .iter()
            .filter(|e| e.key.starts_with("validated_coin:"))
            .map(|e| e.coin)
            .sum();
        assert_eq!(tick_coins, 32);
    }

    #[test]
    fn support_xp_key_format() {
        let ev = support_xp_key("2026-09-10", 900);
        assert_eq!(ev.key, "xp_support:2026-09-10:900");
        assert_eq!(ev.xp, 6);
        assert_eq!(ev.coin, 0);
    }

    #[test]
    fn admin_xp_key_format() {
        let ev = admin_xp_key("2026-09-10", 1800);
        assert_eq!(ev.key, "xp_admin:2026-09-10:1800");
        assert_eq!(ev.xp, 2);
        assert_eq!(ev.coin, 0);
    }
}
