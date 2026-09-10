use crate::r#const::{COIN_TICK_SECS, MAX_GAP_SECS};

/// credited_spans: 按时间排序的 [start,end) 估计有效 Core 段（已是 credited 对应时间）
/// 不足 900 秒时返回 None。
pub fn early_start_anchor(credited_spans: &[(i64, i64)]) -> Option<i64> {
    let tick = COIN_TICK_SECS as i64;
    let max_gap = MAX_GAP_SECS as i64;

    let mut cumulative = 0i64;
    let mut end_idx = None;

    for (i, (start, end)) in credited_spans.iter().enumerate() {
        cumulative += end - start;
        if cumulative >= tick {
            end_idx = Some(i);
            break;
        }
    }

    let end_idx = end_idx?;
    let mut anchor = credited_spans[end_idx].0;
    let mut idx = end_idx;

    while idx > 0 {
        let prev_end = credited_spans[idx - 1].1;
        let curr_start = credited_spans[idx].0;
        if curr_start - prev_end <= max_gap {
            idx -= 1;
            anchor = credited_spans[idx].0;
        } else {
            break;
        }
    }

    Some(anchor)
}

pub fn early_start_coins_for_local_secs(secs_from_midnight: i64) -> i64 {
    if secs_from_midnight < 8 * 3600 + 30 * 60 {
        8
    } else if secs_from_midnight < 9 * 3600 {
        6
    } else if secs_from_midnight < 9 * 3600 + 30 * 60 {
        4
    } else if secs_from_midnight < 10 * 3600 {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_minute_then_lunch_does_not_keep_0825_bonus() {
        let spans = [
            (8 * 3600 + 25 * 60, 8 * 3600 + 26 * 60),
            (10 * 3600 + 30 * 60, 10 * 3600 + 30 * 60 + 840),
        ];
        let anchor = early_start_anchor(&spans).unwrap();
        assert!(anchor >= 10 * 3600 + 30 * 60);
        assert_eq!(early_start_coins_for_local_secs(anchor), 0);
    }

    #[test]
    fn contiguous_fifteen_minutes_keeps_early_anchor() {
        let start = 8 * 3600 + 25 * 60;
        let spans = [(start, start + 900)];
        let anchor = early_start_anchor(&spans).unwrap();
        assert_eq!(anchor, start);
        assert_eq!(early_start_coins_for_local_secs(anchor), 8);
    }
}
