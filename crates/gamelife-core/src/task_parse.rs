use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveTime, Weekday};

use crate::task::{align_range, TaskList};

pub struct ParseContext<'a> {
    pub now: DateTime<FixedOffset>,
    pub lists: &'a [TaskList],
    pub current_list_id: &'a str,
    pub default_list_id: &'a str,
}

pub struct ParsedTask {
    pub title: String,
    pub list_id: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub parse_ok: bool,
}

pub fn parse_task_line(input: &str, ctx: &ParseContext<'_>) -> ParsedTask {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return fail(trimmed, ctx.current_list_id);
    }

    let (tag, mut rest) = take_hashtag(trimmed);
    let list_id = tag
        .as_deref()
        .and_then(|t| match_list(t, ctx.lists))
        .unwrap_or(ctx.current_list_id)
        .to_string();

    rest = rest.trim().to_string();
    let (day_offset, rest) = take_relative_day(&rest, ctx.now);
    let rest = rest.trim().to_string();
    let (times, rest) = take_clock_range(&rest);
    let title = strip_title(&rest);

    if let Some((start_h, start_m, end_h, end_m)) = times {
        let date = ctx.now.date_naive() + Duration::days(day_offset);
        let start_naive = date.and_time(NaiveTime::from_hms_opt(start_h, start_m, 0).unwrap());
        let end_naive = date.and_time(NaiveTime::from_hms_opt(end_h, end_m, 0).unwrap());
        let start_dt = start_naive.and_local_timezone(ctx.now.timezone()).unwrap();
        let end_dt = end_naive.and_local_timezone(ctx.now.timezone()).unwrap();
        let (start, end) = align_range(start_dt.timestamp(), end_dt.timestamp());
        let title = if title.is_empty() {
            trimmed.to_string()
        } else {
            title
        };
        return ParsedTask {
            title,
            list_id,
            start: Some(start),
            end: Some(end),
            parse_ok: true,
        };
    }

    if title.is_empty() || looks_like_garbage(trimmed) {
        return fail(trimmed, &list_id);
    }

    ParsedTask {
        title,
        list_id,
        start: None,
        end: None,
        parse_ok: false,
    }
}

fn fail(title: &str, list_id: &str) -> ParsedTask {
    ParsedTask {
        title: title.trim().to_string(),
        list_id: list_id.to_string(),
        start: None,
        end: None,
        parse_ok: false,
    }
}

fn looks_like_garbage(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric()) && !s.chars().any(|c| c.is_ascii_digit())
}

fn take_hashtag(input: &str) -> (Option<String>, String) {
    if let Some(idx) = input.rfind('#') {
        let tag = input[idx + 1..].trim();
        if tag.is_empty() {
            return (None, input.to_string());
        }
        let tag_end = tag
            .find(|c: char| c.is_whitespace() || c == ',' || c == '，')
            .unwrap_or(tag.len());
        let name = tag[..tag_end].to_string();
        let mut rest = String::new();
        rest.push_str(&input[..idx]);
        rest.push_str(&tag[tag_end..]);
        return (Some(name), rest);
    }
    (None, input.to_string())
}

fn match_list<'a>(tag: &str, lists: &'a [TaskList]) -> Option<&'a str> {
    let lower = tag.trim();
    if let Some(list) = lists.iter().find(|l| l.name == lower) {
        return Some(&list.id);
    }
    let role = crate::task::match_role_alias(lower)?;
    lists
        .iter()
        .find(|l| l.role == role)
        .map(|l| l.id.as_str())
}

fn take_relative_day(input: &str, now: DateTime<FixedOffset>) -> (i64, String) {
    const MARKERS: &[(&str, i64)] = &[("后天", 2), ("明天", 1), ("今天", 0)];
    for (word, days) in MARKERS {
        if let Some(rest) = input.strip_prefix(word) {
            return (*days, rest.to_string());
        }
    }
    for (word, weekday) in [
        ("周一", Weekday::Mon),
        ("周二", Weekday::Tue),
        ("周三", Weekday::Wed),
        ("周四", Weekday::Thu),
        ("周五", Weekday::Fri),
        ("周六", Weekday::Sat),
        ("周日", Weekday::Sun),
        ("星期一", Weekday::Mon),
        ("星期二", Weekday::Tue),
        ("星期三", Weekday::Wed),
        ("星期四", Weekday::Thu),
        ("星期五", Weekday::Fri),
        ("星期六", Weekday::Sat),
        ("星期日", Weekday::Sun),
        ("星期天", Weekday::Sun),
    ] {
        if let Some(rest) = input.strip_prefix(word) {
            let today = now.weekday();
            let mut delta = weekday.num_days_from_monday() as i64
                - today.num_days_from_monday() as i64;
            if delta < 0 {
                delta += 7;
            }
            return (delta, rest.to_string());
        }
    }
    (0, input.to_string())
}

fn take_clock_range(input: &str) -> (Option<(u32, u32, u32, u32)>, String) {
    let (period, rest) = take_period(input);
    let Some((sh, sm, after_start)) = take_clock(rest, period) else {
        return (None, input.to_string());
    };
    let after_start = after_start.trim_start();
    let Some(after_to) = strip_to(after_start) else {
        return (None, input.to_string());
    };
    let (end_period, after_to) = take_period(after_to);
    let end_period = end_period.or(period);
    let Some((eh, em, after_end)) = take_clock(after_to, end_period) else {
        return (None, input.to_string());
    };
    (Some((sh, sm, eh, em)), after_end.to_string())
}

fn take_period(input: &str) -> (Option<DayPeriod>, &str) {
    if let Some(rest) = input.strip_prefix("上午") {
        return (Some(DayPeriod::Morning), rest);
    }
    if let Some(rest) = input.strip_prefix("下午") {
        return (Some(DayPeriod::Afternoon), rest);
    }
    if let Some(rest) = input.strip_prefix("晚上") {
        return (Some(DayPeriod::Evening), rest);
    }
    (None, input)
}

#[derive(Clone, Copy)]
enum DayPeriod {
    Morning,
    Afternoon,
    Evening,
}

fn strip_to(input: &str) -> Option<&str> {
    input
        .strip_prefix("到")
        .or_else(|| input.strip_prefix("至"))
        .or_else(|| input.strip_prefix("-"))
        .or_else(|| input.strip_prefix("—"))
}

fn take_clock(input: &str, period: Option<DayPeriod>) -> Option<(u32, u32, &str)> {
    let (hour, rest) = take_chinese_or_ascii_int(input)?;
    let rest = rest.strip_prefix('点').or_else(|| rest.strip_prefix(':'))?;
    let (minute, rest) = if let Some(rest) = rest.strip_prefix('半') {
        (30, rest)
    } else if rest.starts_with(|c: char| c.is_ascii_digit()) {
        let (m, rest) = take_chinese_or_ascii_int(rest)?;
        let rest = rest.strip_prefix('分').unwrap_or(rest);
        (m, rest)
    } else {
        (0, rest)
    };
    let hour = apply_period(hour, period)?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some((hour, minute, rest))
}

fn apply_period(hour: u32, period: Option<DayPeriod>) -> Option<u32> {
    match period {
        None | Some(DayPeriod::Morning) => {
            if hour == 12 {
                Some(0)
            } else {
                Some(hour)
            }
        }
        Some(DayPeriod::Afternoon) | Some(DayPeriod::Evening) => {
            if hour < 12 {
                Some(hour + 12)
            } else {
                Some(hour)
            }
        }
    }
}

fn take_chinese_or_ascii_int(input: &str) -> Option<(u32, &str)> {
    if let Some(rest) = input.strip_prefix("十二") {
        return Some((12, rest));
    }
    if let Some(rest) = input.strip_prefix("十一") {
        return Some((11, rest));
    }
    if let Some(rest) = input.strip_prefix('十') {
        return Some((10, rest));
    }
    const DIGITS: &[(&str, u32)] = &[
        ("零", 0),
        ("一", 1),
        ("二", 2),
        ("两", 2),
        ("三", 3),
        ("四", 4),
        ("五", 5),
        ("六", 6),
        ("七", 7),
        ("八", 8),
        ("九", 9),
    ];
    for (ch, n) in DIGITS {
        if let Some(rest) = input.strip_prefix(ch) {
            return Some((*n, rest));
        }
    }
    let ascii = input
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    if ascii.is_empty() {
        return None;
    }
    let n: u32 = ascii.parse().ok()?;
    Some((n, &input[ascii.len()..]))
}

fn strip_title(rest: &str) -> String {
    rest.trim()
        .trim_start_matches(['，', ',', '。', '、', ' '])
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{preset_lists, TaskList, PRESET_CHORE_ID, PRESET_MAINLINE_ID};
    use chrono::DateTime;

    fn ctx<'a>(lists: &'a [TaskList]) -> ParseContext<'a> {
        let now = DateTime::parse_from_rfc3339("2026-09-11T18:00:00+08:00").unwrap();
        ParseContext {
            now,
            lists,
            current_list_id: PRESET_MAINLINE_ID,
            default_list_id: PRESET_MAINLINE_ID,
        }
    }

    #[test]
    fn example_chore_tomorrow_morning() {
        let lists = preset_lists();
        let p = parse_task_line("明天上午十点到十一点，RAIDS+会议讨论 #杂项", &ctx(&lists));
        assert!(p.parse_ok);
        assert_eq!(p.title, "RAIDS+会议讨论");
        assert_eq!(p.list_id, PRESET_CHORE_ID);
        let start = DateTime::parse_from_rfc3339("2026-09-12T10:00:00+08:00")
            .unwrap()
            .timestamp();
        let end = DateTime::parse_from_rfc3339("2026-09-12T11:00:00+08:00")
            .unwrap()
            .timestamp();
        assert_eq!(p.start, Some(start));
        assert_eq!(p.end, Some(end));
    }

    #[test]
    fn no_hash_uses_current_list() {
        let lists = preset_lists();
        let p = parse_task_line("写方法节", &ctx(&lists));
        assert_eq!(p.list_id, PRESET_MAINLINE_ID);
        assert_eq!(p.title, "写方法节");
    }

    #[test]
    fn garbage_stays_unscheduled() {
        let lists = preset_lists();
        let p = parse_task_line("asdfgh", &ctx(&lists));
        assert!(!p.parse_ok);
        assert_eq!(p.title, "asdfgh");
        assert_eq!(p.start, None);
    }
}
