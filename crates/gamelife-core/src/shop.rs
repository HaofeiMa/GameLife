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
pub enum RedeemError {
    ShopLocked,
    Insufficient,
    EntertainmentNeedsDuration,
    FinalSlotImmutable,
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
}
