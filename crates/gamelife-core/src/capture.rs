#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureStatus {
    Scheduled,
    Captured,
    Missed,
    Skipped,
}

pub fn schedule_capture(slot_start: i64, rng_u32: u32) -> i64 {
    slot_start + 240 + (rng_u32 % 541) as i64
}

pub fn capture_on_resume(scheduled_at: i64, now: i64, already: CaptureStatus) -> CaptureStatus {
    match already {
        CaptureStatus::Captured | CaptureStatus::Skipped | CaptureStatus::Missed => already,
        CaptureStatus::Scheduled => {
            if now >= scheduled_at {
                CaptureStatus::Missed
            } else {
                CaptureStatus::Scheduled
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_is_inside_minute_4_to_13() {
        for seed in 0..200u32 {
            let t = schedule_capture(1_000, seed);
            assert!((1_000 + 240..=1_000 + 780).contains(&t));
        }
    }

    #[test]
    fn resume_after_scheduled_time_is_missed_not_reschedule() {
        let scheduled = 1_000 + 500;
        assert_eq!(
            capture_on_resume(scheduled, scheduled + 10, CaptureStatus::Scheduled),
            CaptureStatus::Missed
        );
    }
}
