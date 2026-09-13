use chrono::{Datelike, NaiveDate};

use crate::time::is_weekday;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayOutcome {
    Completed,
    Protected,
    Failed,
}

pub const FREEZE_PER_MONTH: usize = 2;

const MILESTONES: [u32; 4] = [3, 5, 10, 20];

pub fn recompute_streak(days_newest_first: &[(NaiveDate, DayOutcome)]) -> u32 {
    let mut streak = 0u32;
    for &(date, outcome) in days_newest_first {
        if !is_weekday(date) {
            continue;
        }
        match outcome {
            DayOutcome::Completed | DayOutcome::Protected => streak += 1,
            DayOutcome::Failed => break,
        }
    }
    streak
}

/// Non-weekdays still record; a miss must not become `Failed`.
pub fn settle_outcome(credited: i64, chest_secs: i64, weekday: bool) -> Option<DayOutcome> {
    if credited >= chest_secs {
        Some(DayOutcome::Completed)
    } else if weekday {
        Some(DayOutcome::Failed)
    } else {
        None
    }
}

pub fn streak_at_risk(
    weekday: bool,
    settled: bool,
    streak: u32,
    credited: i64,
    chest_secs: i64,
) -> bool {
    weekday && !settled && streak > 0 && credited < chest_secs
}

pub fn freeze_month_key(protected: NaiveDate) -> String {
    format!("{:04}-{:02}", protected.year(), protected.month())
}

pub fn freeze_quota_used(protected_dates: &[NaiveDate], month_key: &str) -> usize {
    protected_dates
        .iter()
        .copied()
        .filter(|d| freeze_month_key(*d) == month_key)
        .count()
}

pub fn can_use_freeze(protected_dates: &[NaiveDate], protected: NaiveDate) -> bool {
    if protected_dates.contains(&protected) {
        return false;
    }
    let month_key = freeze_month_key(protected);
    freeze_quota_used(protected_dates, &month_key) < FREEZE_PER_MONTH
}

pub fn new_milestones(old_len: u32, new_len: u32) -> Vec<u32> {
    MILESTONES
        .iter()
        .copied()
        .filter(|&m| m > old_len && m <= new_len)
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{
        DayOutcome, can_use_freeze, freeze_month_key, freeze_quota_used, new_milestones,
        recompute_streak, settle_outcome, streak_at_risk,
    };

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn freeze_monday_then_tuesday_complete_is_plus_two() {
        // Fri completed, Mon failed then protected, Tue completed
        let newest_first = vec![
            (d(2026, 9, 8), DayOutcome::Completed), // Tue
            (d(2026, 9, 7), DayOutcome::Protected), // Mon
            (d(2026, 9, 4), DayOutcome::Completed), // Fri
        ];
        assert_eq!(recompute_streak(&newest_first), 3);
    }

    #[test]
    fn april_click_counts_against_march_protected_date() {
        let used = vec![d(2026, 3, 31)];
        assert_eq!(freeze_month_key(d(2026, 3, 31)), "2026-03");
        assert!(can_use_freeze(&used, d(2026, 3, 30)));
        assert_eq!(freeze_quota_used(&used, "2026-03"), 1);
    }

    #[test]
    fn monday_only_failed_gives_zero_streak() {
        let newest_first = vec![(d(2026, 9, 7), DayOutcome::Failed)]; // Mon
        assert_eq!(recompute_streak(&newest_first), 0);
    }

    #[test]
    fn failed_then_protected_equals_freeze_before_plus_one() {
        let fri_completed = vec![(d(2026, 9, 4), DayOutcome::Completed)];
        let before_freeze = vec![
            (d(2026, 9, 7), DayOutcome::Failed),
            (d(2026, 9, 4), DayOutcome::Completed),
        ];
        let after_freeze = vec![
            (d(2026, 9, 7), DayOutcome::Protected),
            (d(2026, 9, 4), DayOutcome::Completed),
        ];
        let streak_before = recompute_streak(&before_freeze);
        let streak_after = recompute_streak(&after_freeze);
        assert_eq!(recompute_streak(&fri_completed), 1);
        assert_eq!(streak_before, 0);
        assert_eq!(streak_after, streak_before + 1 + recompute_streak(&fri_completed));
        assert_eq!(streak_after, 2);
    }

    #[test]
    fn new_milestones_returns_crossed_thresholds() {
        assert_eq!(new_milestones(2, 5), vec![3, 5]);
        assert_eq!(new_milestones(5, 5), Vec::<u32>::new());
        assert_eq!(new_milestones(10, 25), vec![20]);
    }

    #[test]
    fn recompute_streak_skips_weekends() {
        let newest_first = vec![
            (d(2026, 9, 12), DayOutcome::Completed), // Sat
            (d(2026, 9, 11), DayOutcome::Completed), // Fri
        ];
        assert_eq!(recompute_streak(&newest_first), 1);
    }

    #[test]
    fn weekend_miss_is_not_a_failed_outcome() {
        use crate::r#const::CHEST_SECS;
        let chest = i64::try_from(CHEST_SECS).unwrap();
        assert_eq!(settle_outcome(0, chest, false), None);
        assert_eq!(settle_outcome(chest, chest, false), Some(DayOutcome::Completed));
        assert_eq!(settle_outcome(0, chest, true), Some(DayOutcome::Failed));
        assert_eq!(settle_outcome(chest, chest, true), Some(DayOutcome::Completed));
    }

    #[test]
    fn weekend_does_not_put_streak_at_risk() {
        use crate::r#const::CHEST_SECS;
        let chest = i64::try_from(CHEST_SECS).unwrap();
        assert!(!streak_at_risk(false, false, 3, 0, chest));
        assert!(streak_at_risk(true, false, 3, 0, chest));
        assert!(!streak_at_risk(true, true, 3, 0, chest));
        assert!(!streak_at_risk(true, false, 0, 0, chest));
    }
}
