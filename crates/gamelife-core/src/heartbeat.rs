pub fn heartbeat_unobserved(
    last_heartbeat: i64,
    now: i64,
    same_local_day: bool,
    end_of_heartbeat_local_day: i64,
) -> Vec<(i64, i64)> {
    if same_local_day {
        if now > last_heartbeat {
            return vec![(last_heartbeat, now)];
        }
        return vec![];
    }

    if end_of_heartbeat_local_day > last_heartbeat {
        return vec![(last_heartbeat, end_of_heartbeat_local_day)];
    }

    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_day_quit_is_unobserved_range() {
        let g = heartbeat_unobserved(10_000, 10_000 + 85 * 60, true, 0);
        assert_eq!(g, vec![(10_000, 10_000 + 85 * 60)]);
    }

    #[test]
    fn cross_day_does_not_fill_this_morning() {
        let end_yesterday = 86_400;
        let g = heartbeat_unobserved(86_400 - 100, 86_400 + 9 * 3600, false, end_yesterday);
        assert_eq!(g, vec![(86_400 - 100, end_yesterday)]);
    }
}
