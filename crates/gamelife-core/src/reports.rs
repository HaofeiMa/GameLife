use crate::types::ActivitySeconds;
use crate::{GOLD_DAY_SECS, SLOT_SECS};

/// One slot's contribution to a day's 24 hour cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HourContribution {
    pub hour: i32,
    pub pending: bool,
    pub activity: ActivitySeconds,
}

/// Dominant category per local hour. Empty string means no slots in that hour.
pub fn month_day_hours(slots: &[HourContribution]) -> Vec<String> {
    let slot_secs = i64::try_from(SLOT_SECS).unwrap_or(900);
    let mut pending = [0i64; 24];
    let mut activity = std::array::from_fn::<_, 24, _>(|_| ActivitySeconds::default());
    let mut seen = [false; 24];
    for slot in slots {
        if !(0..24).contains(&slot.hour) {
            continue;
        }
        let i = slot.hour as usize;
        seen[i] = true;
        if slot.pending {
            pending[i] = pending[i].saturating_add(slot_secs);
        } else {
            activity[i].add_assign(&slot.activity);
        }
    }
    (0..24)
        .map(|i| {
            if !seen[i] {
                return String::new();
            }
            hour_dominant(&activity[i], pending[i])
        })
        .collect()
}

fn hour_dominant(activity: &ActivitySeconds, pending: i64) -> String {
    let mut best = ("", 0i64);
    for (name, secs) in [
        ("core", activity.core),
        ("support", activity.support),
        ("admin", activity.admin),
        ("side", activity.side),
        ("distraction", activity.distraction),
        ("away", activity.away),
        ("unobserved", activity.unobserved),
        ("pending", pending),
    ] {
        if secs > best.1 {
            best = (name, secs);
        }
    }
    if best.1 <= 0 {
        String::new()
    } else {
        best.0.to_string()
    }
}

/// One run of adjacent 15-minute slots of the same judged category, in hours from midnight.
#[derive(Clone, Debug, PartialEq)]
pub struct ObservationBand {
    pub start_hour: f64,
    pub end_hour: f64,
    pub category: String,
}

/// Merge consecutive same-category slots. `offset_secs` is `slot_start - day_start`.
/// A gap of more than one slot, or a category change, starts a new band.
pub fn merge_observation_bands(mut slots: Vec<(i64, String)>) -> Vec<ObservationBand> {
    slots.sort_by_key(|(offset, _)| *offset);
    let slot_secs = i64::try_from(SLOT_SECS).unwrap_or(900);
    let day_secs = 24 * 3600;
    let mut bands: Vec<(i64, i64, String)> = Vec::new();
    for (offset, category) in slots {
        if offset < 0 || offset >= day_secs {
            continue;
        }
        let end = (offset + slot_secs).min(day_secs);
        if let Some(last) = bands.last_mut() {
            if last.1 == offset && last.2 == category {
                last.1 = end;
                continue;
            }
        }
        bands.push((offset, end, category));
    }
    bands
        .into_iter()
        .map(|(start, end, category)| ObservationBand {
            start_hour: start as f64 / 3600.0,
            end_hour: end as f64 / 3600.0,
            category,
        })
        .collect()
}

/// Fraction of days whose credited seconds meet `threshold`.
/// Denominator is the slice length; an empty slice is 0.0, never NaN.
pub fn hit_rate(credited_secs: &[i64], threshold: i64) -> f64 {
    if credited_secs.is_empty() {
        return 0.0;
    }
    let hits = credited_secs
        .iter()
        .filter(|&&secs| secs >= threshold)
        .count();
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
    use crate::types::ActivitySeconds;

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

    #[test]
    fn adjacent_same_category_slots_merge_into_one_band() {
        let bands = merge_observation_bands(vec![
            (8 * 3600, "core".into()),
            (8 * 3600 + 900, "core".into()),
        ]);
        assert_eq!(
            bands,
            vec![ObservationBand {
                start_hour: 8.0,
                end_hour: 8.5,
                category: "core".into(),
            }]
        );
    }

    #[test]
    fn different_categories_do_not_merge() {
        let bands = merge_observation_bands(vec![
            (8 * 3600, "core".into()),
            (8 * 3600 + 900, "distraction".into()),
        ]);
        assert_eq!(bands.len(), 2);
        assert_eq!(bands[0].category, "core");
        assert_eq!(bands[1].category, "distraction");
        assert_eq!(bands[1].start_hour, 8.25);
        assert_eq!(bands[1].end_hour, 8.5);
    }

    #[test]
    fn a_missing_slot_breaks_the_band() {
        let bands = merge_observation_bands(vec![
            (8 * 3600, "core".into()),
            (8 * 3600 + 1800, "core".into()),
        ]);
        assert_eq!(bands.len(), 2);
        assert_eq!(bands[0].end_hour, 8.25);
        assert_eq!(bands[1].start_hour, 8.5);
    }

    #[test]
    fn last_slot_of_the_day_reaches_24() {
        let bands = merge_observation_bands(vec![(23 * 3600 + 45 * 60, "away".into())]);
        assert_eq!(bands[0].start_hour, 23.75);
        assert_eq!(bands[0].end_hour, 24.0);
    }

    #[test]
    fn empty_slots_yield_no_bands() {
        assert!(merge_observation_bands(vec![]).is_empty());
    }

    #[test]
    fn hour_core_beats_entertainment_in_the_same_hour() {
        let hours = month_day_hours(&[HourContribution {
            hour: 8,
            pending: false,
            activity: ActivitySeconds {
                core: 2400,
                distraction: 1200,
                ..Default::default()
            },
        }]);
        assert_eq!(hours.len(), 24);
        assert_eq!(hours[8], "core");
        assert_eq!(hours[9], "");
    }

    #[test]
    fn empty_hours_are_blank_not_unobserved() {
        let hours = month_day_hours(&[]);
        assert_eq!(hours, vec![""; 24]);
    }

    #[test]
    fn pending_dominates_when_it_has_more_seconds() {
        let hours = month_day_hours(&[
            HourContribution {
                hour: 10,
                pending: true,
                activity: ActivitySeconds {
                    core: 900,
                    ..Default::default()
                },
            },
            HourContribution {
                hour: 10,
                pending: true,
                activity: Default::default(),
            },
            HourContribution {
                hour: 10,
                pending: true,
                activity: Default::default(),
            },
            HourContribution {
                hour: 10,
                pending: false,
                activity: ActivitySeconds {
                    core: 900,
                    ..Default::default()
                },
            },
        ]);
        assert_eq!(hours[10], "pending");
    }
}
