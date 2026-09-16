use chrono::{Datelike, NaiveDate};

use crate::SLOT_SECS;

pub fn slot_start(ts: i64) -> i64 {
    let slot_secs = i64::try_from(SLOT_SECS).expect("SLOT_SECS fits in i64");
    ts - ts.rem_euclid(slot_secs)
}

pub fn slot_end_exclusive(slot_start: i64) -> i64 {
    let slot_secs = i64::try_from(SLOT_SECS).expect("SLOT_SECS fits in i64");
    slot_start + slot_secs
}

pub fn is_weekday(date: NaiveDate) -> bool {
    use chrono::Weekday;
    matches!(
        date.weekday(),
        Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri
    )
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{is_weekday, slot_start};

    #[test]
    fn slot_aligns_down() {
        assert_eq!(
            slot_start(1_700_000_100),
            1_700_000_100 - (1_700_000_100 % 900)
        );
    }

    #[test]
    fn friday_is_weekday_sunday_is_not() {
        assert!(is_weekday(NaiveDate::from_ymd_opt(2026, 9, 11).unwrap())); // Fri
        assert!(!is_weekday(NaiveDate::from_ymd_opt(2026, 9, 12).unwrap())); // Sat
    }
}
