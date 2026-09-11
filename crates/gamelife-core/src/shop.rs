use crate::r#const::XP_SHOP_UNLOCK_SECS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WishKind {
    Xp { duration_minutes: Option<i64> },
    Coin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wish {
    pub id: String,
    pub kind: WishKind,
    pub price: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WishError {
    EmptyName,
    NameTooLong,
    NonPositivePrice,
    EntertainmentNeedsDuration,
    CoinMustNotHaveDuration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedeemError {
    ShopLocked,
    Insufficient,
    EntertainmentNeedsDuration,
    EntertainmentInProgress,
    WishArchived,
    WishMissing,
    FinalSlotImmutable,
}

pub fn validate_wish(name: &str, kind: &WishKind, price: i64) -> Result<(), WishError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(WishError::EmptyName);
    }
    if name.chars().count() > 80 {
        return Err(WishError::NameTooLong);
    }
    if price <= 0 {
        return Err(WishError::NonPositivePrice);
    }
    match kind {
        WishKind::Xp { duration_minutes } => match duration_minutes {
            Some(d) if *d >= 5 => Ok(()),
            _ => Err(WishError::EntertainmentNeedsDuration),
        },
        WishKind::Coin => Ok(()),
    }
}

pub fn has_entertainment_timer(kind: &WishKind) -> bool {
    matches!(kind, WishKind::Xp { duration_minutes: Some(d) } if *d >= 5)
}

pub fn can_start_entertainment(now: i64, active_ends_at: Option<i64>) -> bool {
    match active_ends_at {
        Some(ends) if now < ends => false,
        _ => true,
    }
}

pub fn entertainment_remaining_secs(now: i64, ends_at: i64) -> i64 {
    ends_at - now
}

pub fn tray_entertainment_minutes(remaining_secs: i64) -> Option<u32> {
    if remaining_secs <= 0 {
        None
    } else {
        Some(u32::try_from((remaining_secs / 60).max(1)).unwrap_or(1))
    }
}

pub fn xp_shop_unlocked(credited_today: i64) -> bool {
    credited_today >= XP_SHOP_UNLOCK_SECS as i64
}

pub fn validate_redeem(
    credited_today: i64,
    coin_balance: i64,
    xp_today: i64,
    wish: &Wish,
) -> Result<(), RedeemError> {
    match &wish.kind {
        WishKind::Xp { duration_minutes: Some(d) } if *d < 5 => {
            return Err(RedeemError::EntertainmentNeedsDuration);
        }
        WishKind::Xp { .. } => {
            if !xp_shop_unlocked(credited_today) {
                return Err(RedeemError::ShopLocked);
            }
            if xp_today < wish.price {
                return Err(RedeemError::Insufficient);
            }
        }
        WishKind::Coin => {
            if coin_balance < wish.price {
                return Err(RedeemError::Insufficient);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xp_shop_locked_before_60_minutes() {
        let w = Wish {
            id: "a".into(),
            kind: WishKind::Xp {
                duration_minutes: None,
            },
            price: 10,
        };
        assert_eq!(
            validate_redeem(3599, 100, 50, &w),
            Err(RedeemError::ShopLocked)
        );
        assert!(validate_redeem(3600, 100, 50, &w).is_ok());
    }

    #[test]
    fn entertainment_requires_duration() {
        let w = Wish {
            id: "b".into(),
            kind: WishKind::Xp {
                duration_minutes: Some(3),
            },
            price: 10,
        };
        assert_eq!(
            validate_redeem(3600, 0, 50, &w),
            Err(RedeemError::EntertainmentNeedsDuration)
        );
    }

    #[test]
    fn validate_wish_rejects_empty_and_zero_price() {
        let xp = WishKind::Xp {
            duration_minutes: Some(30),
        };
        assert_eq!(
            validate_wish("  ", &xp, 10),
            Err(WishError::EmptyName)
        );
        assert_eq!(
            validate_wish("视频", &xp, 0),
            Err(WishError::NonPositivePrice)
        );
        let long = "x".repeat(81);
        assert_eq!(
            validate_wish(&long, &xp, 10),
            Err(WishError::NameTooLong)
        );
    }

    #[test]
    fn validate_wish_xp_needs_duration_coin_ok() {
        assert_eq!(
            validate_wish(
                "视频",
                &WishKind::Xp {
                    duration_minutes: None
                },
                10
            ),
            Err(WishError::EntertainmentNeedsDuration)
        );
        assert_eq!(
            validate_wish(
                "视频",
                &WishKind::Xp {
                    duration_minutes: Some(3)
                },
                10
            ),
            Err(WishError::EntertainmentNeedsDuration)
        );
        assert!(validate_wish(
            "视频",
            &WishKind::Xp {
                duration_minutes: Some(30)
            },
            10
        )
        .is_ok());
        assert!(validate_wish("咖啡", &WishKind::Coin, 5).is_ok());
    }

    #[test]
    fn entertainment_lock_and_tray_minutes() {
        assert!(!can_start_entertainment(100, Some(101)));
        assert!(can_start_entertainment(100, Some(100)));
        assert!(can_start_entertainment(100, None));
        assert_eq!(entertainment_remaining_secs(100, 160), 60);
        assert_eq!(tray_entertainment_minutes(0), None);
        assert_eq!(tray_entertainment_minutes(59), Some(1));
        assert_eq!(tray_entertainment_minutes(120), Some(2));
        assert!(has_entertainment_timer(&WishKind::Xp {
            duration_minutes: Some(30)
        }));
        assert!(!has_entertainment_timer(&WishKind::Xp {
            duration_minutes: None
        }));
        assert!(!has_entertainment_timer(&WishKind::Coin));
    }
}
