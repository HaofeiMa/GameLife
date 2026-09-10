pub fn format_estimated_minutes(secs: i64) -> String {
    let total_minutes = secs.max(0) / 60;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    format!("{}h {}m", hours, minutes)
}

#[cfg(test)]
mod tests {
    use super::format_estimated_minutes;

    #[test]
    fn hides_seconds() {
        assert_eq!(format_estimated_minutes(5 * 3600 + 23 * 60 + 17), "5h 23m");
    }

    #[test]
    fn zero() {
        assert_eq!(format_estimated_minutes(0), "0h 0m");
    }
}
