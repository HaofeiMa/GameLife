use crate::GOLD_DAY_SECS;

/// Fraction of days whose credited seconds meet `threshold`.
/// Denominator is the slice length; an empty slice is 0.0, never NaN.
pub fn hit_rate(credited_secs: &[i64], threshold: i64) -> f64 {
    if credited_secs.is_empty() {
        return 0.0;
    }
    let hits = credited_secs.iter().filter(|&&secs| secs >= threshold).count();
    hits as f64 / credited_secs.len() as f64
}

/// This ISO week's credited core minutes minus last week's.
/// Callers that have no last-week slots return `None` without calling this;
/// the helper itself always yields the signed delta.
pub fn wow_delta(this_week_core: i64, last_week_core: i64) -> Option<i64> {
    Some(this_week_core - last_week_core)
}

/// Consecutive distraction slots. A run is ≥3 consecutive `true`s.
/// Returns `(run_count, run_slots)` counting only slots inside qualifying runs.
pub fn distraction_runs(slot_is_distraction: &[bool]) -> (usize, i64) {
    let mut run_count = 0usize;
    let mut run_slots = 0i64;
    let mut current = 0i64;
    for &is_d in slot_is_distraction {
        if is_d {
            current += 1;
        } else if current > 0 {
            if current >= 3 {
                run_count += 1;
                run_slots += current;
            }
            current = 0;
        }
    }
    if current >= 3 {
        run_count += 1;
        run_slots += current;
    }
    (run_count, run_slots)
}

/// Local hour of the first credited-core slot, as seconds from `day_start`.
/// Callers must pass only slots with `credited_core > 0`, already ordered.
pub fn first_core_hour(slot_starts: &[i64], day_start: i64) -> Option<i32> {
    slot_starts
        .first()
        .map(|slot_start| ((slot_start - day_start) / 3600) as i32)
}

/// Heat cell in 0..=1 for a day's credited core versus 8h (28800s).
pub fn month_heat_cell(credited_core: i64) -> f64 {
    let gold = i64::try_from(GOLD_DAY_SECS).unwrap_or(28800) as f64;
    (credited_core as f64 / gold).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_slot_minutes_do_not_become_fifteen_core_in_hit_rate_denom() {
        assert_eq!(hit_rate(&[480, 0], 3600), 0.0);
        assert!((hit_rate(&[6 * 3600, 8 * 3600], 6 * 3600) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn distraction_run_needs_three_slots() {
        assert_eq!(distraction_runs(&[true, true, false]), (0, 0));
        assert_eq!(
            distraction_runs(&[true, true, true, false, true, true, true]),
            (2, 6)
        );
    }

    #[test]
    fn month_heat_caps_at_one() {
        assert_eq!(month_heat_cell(0), 0.0);
        assert_eq!(month_heat_cell(28800), 1.0);
        assert_eq!(month_heat_cell(40000), 1.0);
    }

    #[test]
    fn hit_rate_empty_is_zero() {
        assert_eq!(hit_rate(&[], 3600), 0.0);
    }

    #[test]
    fn first_core_hour_uses_second_offset() {
        assert_eq!(first_core_hour(&[], 0), None);
        assert_eq!(first_core_hour(&[8 * 3600], 0), Some(8));
        assert_eq!(first_core_hour(&[10 * 3600, 11 * 3600], 0), Some(10));
    }
}
