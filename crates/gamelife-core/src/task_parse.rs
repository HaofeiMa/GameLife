use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveTime, Weekday};
use serde::Serialize;

use crate::task::{align_range, TaskList, UNSCHEDULED_DROP_SECS};

pub struct ParseContext<'a> {
    pub now: DateTime<FixedOffset>,
    pub lists: &'a [TaskList],
    pub current_list_id: &'a str,
    pub default_list_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseSpan {
    pub start: usize,
    pub end: usize,
}

pub struct ParsedTask {
    pub title: String,
    pub list_id: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub parse_ok: bool,
    pub spans: Vec<ParseSpan>,
}

pub fn parse_task_line(input: &str, ctx: &ParseContext<'_>) -> ParsedTask {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return fail(trimmed, ctx.current_list_id);
    }

    let tag_hit = find_hashtag(input);
    let list_id = tag_hit
        .as_ref()
        .and_then(|(name, _, _)| match_list(name, ctx.lists))
        .unwrap_or(ctx.current_list_id)
        .to_string();
    let tag_bytes = tag_hit.as_ref().map(|(_, a, b)| (*a, *b));

    let hits = scan_tokens(input, tag_bytes);
    let spans = hits
        .iter()
        .map(|h| ParseSpan {
            start: char_index(input, h.b0),
            end: char_index(input, h.b1),
        })
        .collect();

    let title = title_from(input, &hits, tag_bytes);
    let scheduled = build_schedule(&hits, ctx.now);

    if let Some((start, end)) = scheduled {
        let (start, end) = align_range(start, end);
        return ParsedTask {
            title: if title.is_empty() {
                trimmed.to_string()
            } else {
                title
            },
            list_id,
            start: Some(start),
            end: Some(end),
            parse_ok: true,
            spans,
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
        spans,
    }
}

fn fail(title: &str, list_id: &str) -> ParsedTask {
    ParsedTask {
        title: title.trim().to_string(),
        list_id: list_id.to_string(),
        start: None,
        end: None,
        parse_ok: false,
        spans: Vec::new(),
    }
}

fn looks_like_garbage(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric()) && !s.chars().any(|c| c.is_ascii_digit())
}

fn char_index(s: &str, byte: usize) -> usize {
    s.get(..byte).map(|p| p.chars().count()).unwrap_or(0)
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    while let Some(c) = s.get(i..).and_then(|r| r.chars().next()) {
        if c != ' ' && c != '\u{3000}' && c != '\t' {
            break;
        }
        i += c.len_utf8();
    }
    i
}

fn eat(s: &str, i: usize, lit: &str) -> Option<usize> {
    s.get(i..)?.starts_with(lit).then_some(i + lit.len())
}

fn eat_unit(s: &str, i: usize, unit: char) -> Option<usize> {
    let j = skip_ws(s, i);
    let c = s.get(j..)?.chars().next()?;
    (c == unit).then_some(j + c.len_utf8())
}

fn ascii_digit(c: char) -> Option<u32> {
    if c.is_ascii_digit() {
        c.to_digit(10)
    } else if ('０'..='９').contains(&c) {
        Some(c as u32 - '０' as u32)
    } else {
        None
    }
}

fn cn_ones(c: char) -> Option<u32> {
    match c {
        '零' | '〇' => Some(0),
        '一' => Some(1),
        '二' | '两' => Some(2),
        '三' => Some(3),
        '四' => Some(4),
        '五' => Some(5),
        '六' => Some(6),
        '七' => Some(7),
        '八' => Some(8),
        '九' => Some(9),
        _ => None,
    }
}

fn take_ones(s: &str, i: usize) -> Option<(u32, usize)> {
    let c = s.get(i..)?.chars().next()?;
    Some((cn_ones(c)?, i + c.len_utf8()))
}

fn take_uint(s: &str, i: usize) -> Option<(u32, usize)> {
    let c = s.get(i..)?.chars().next()?;
    if ascii_digit(c).is_some() {
        let mut n = 0u32;
        let mut j = i;
        loop {
            let k = skip_ws(s, j);
            let Some(ch) = s.get(k..).and_then(|r| r.chars().next()) else {
                break;
            };
            let Some(d) = ascii_digit(ch) else {
                break;
            };
            n = n.saturating_mul(10).saturating_add(d);
            j = k + ch.len_utf8();
        }
        return Some((n, j));
    }
    take_cn_uint(s, i)
}

fn take_cn_uint(s: &str, i: usize) -> Option<(u32, usize)> {
    if let Some(j) = eat(s, i, "二十").or_else(|| eat(s, i, "廿")) {
        let k = skip_ws(s, j);
        if let Some((d, e)) = take_ones(s, k) {
            return Some((20 + d, e));
        }
        return Some((20, j));
    }
    if let Some(j) = eat(s, i, "三十").or_else(|| eat(s, i, "卅")) {
        let k = skip_ws(s, j);
        if let Some((d, e)) = take_ones(s, k) {
            return Some((30 + d, e));
        }
        return Some((30, j));
    }
    if let Some(j) = eat(s, i, "十") {
        let k = skip_ws(s, j);
        if let Some((d, e)) = take_ones(s, k) {
            return Some((10 + d, e));
        }
        return Some((10, j));
    }
    take_ones(s, i)
}

fn find_hashtag(input: &str) -> Option<(String, usize, usize)> {
    let idx = input.rfind('#')?;
    let after = idx + 1;
    let tail = input.get(after..)?;
    if tail.trim().is_empty() {
        return None;
    }
    let rel = tail
        .find(|c: char| c.is_whitespace() || c == ',' || c == '，')
        .unwrap_or(tail.len());
    let name = tail[..rel].trim();
    if name.is_empty() {
        None
    } else {
        Some((name.to_string(), idx, after + rel))
    }
}

fn match_list<'a>(tag: &str, lists: &'a [TaskList]) -> Option<&'a str> {
    let lower = tag.trim();
    if let Some(list) = lists.iter().find(|l| l.name == lower) {
        return Some(&list.id);
    }
    let role = crate::task::match_role_alias(lower)?;
    lists.iter().find(|l| l.role == role).map(|l| l.id.as_str())
}

#[derive(Clone, Copy)]
enum Period {
    Dawn,
    Morning,
    Noon,
    Afternoon,
    Dusk,
    Evening,
}

#[derive(Clone, Copy)]
enum DateTok {
    Rel { days: i64, keep_past: bool },
    Weekday(Weekday),
    Md { month: u32, day: u32 },
    Ymd { year: i32, month: u32, day: u32 },
}

#[derive(Clone, Copy)]
enum Kind {
    Date(DateTok),
    Period(Period),
    Clock { hour: u32, minute: u32 },
    Sep,
}

struct Hit {
    kind: Kind,
    b0: usize,
    b1: usize,
}

fn scan_tokens(input: &str, tag: Option<(usize, usize)>) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut i = 0;
    while i < input.len() {
        if let Some((a, b)) = tag {
            if i >= a && i < b {
                i = b;
                continue;
            }
        }
        if let Some(hit) = try_token(input, i) {
            i = hit.b1;
            hits.push(hit);
            continue;
        }
        let n = input
            .get(i..)
            .and_then(|r| r.chars().next())
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        i += n;
    }
    hits
}

fn try_token(s: &str, i: usize) -> Option<Hit> {
    try_rel(s, i)
        .or_else(|| try_weekday(s, i))
        .or_else(|| try_date(s, i))
        .or_else(|| try_period(s, i))
        .or_else(|| try_clock(s, i))
        .or_else(|| try_sep(s, i))
}

fn hit(kind: Kind, b0: usize, b1: usize) -> Hit {
    Hit { kind, b0, b1 }
}

fn try_rel(s: &str, i: usize) -> Option<Hit> {
    for (w, days) in [("后天", 2), ("明天", 1), ("今天", 0)] {
        if let Some(j) = eat(s, i, w) {
            return Some(hit(
                Kind::Date(DateTok::Rel {
                    days,
                    keep_past: true,
                }),
                i,
                j,
            ));
        }
    }
    None
}

fn try_weekday(s: &str, i: usize) -> Option<Hit> {
    for (w, wd) in [
        ("星期一", Weekday::Mon),
        ("星期二", Weekday::Tue),
        ("星期三", Weekday::Wed),
        ("星期四", Weekday::Thu),
        ("星期五", Weekday::Fri),
        ("星期六", Weekday::Sat),
        ("星期日", Weekday::Sun),
        ("星期天", Weekday::Sun),
        ("周一", Weekday::Mon),
        ("周二", Weekday::Tue),
        ("周三", Weekday::Wed),
        ("周四", Weekday::Thu),
        ("周五", Weekday::Fri),
        ("周六", Weekday::Sat),
        ("周日", Weekday::Sun),
        ("周天", Weekday::Sun),
    ] {
        if let Some(j) = eat(s, i, w) {
            return Some(hit(Kind::Date(DateTok::Weekday(wd)), i, j));
        }
    }
    None
}

fn try_period(s: &str, i: usize) -> Option<Hit> {
    for (w, p) in [
        ("早上", Period::Dawn),
        ("上午", Period::Morning),
        ("中午", Period::Noon),
        ("下午", Period::Afternoon),
        ("傍晚", Period::Dusk),
        ("晚上", Period::Evening),
    ] {
        if let Some(j) = eat(s, i, w) {
            return Some(hit(Kind::Period(p), i, j));
        }
    }
    None
}

fn try_clock(s: &str, i: usize) -> Option<Hit> {
    let (hour, j) = take_uint(s, i)?;
    let j = skip_ws(s, j);
    let j = if let Some(j) = eat_unit(s, j, '点') {
        j
    } else if let Some(j) = eat(s, j, ":").or_else(|| eat(s, j, "：")) {
        j
    } else {
        return None;
    };
    let j = skip_ws(s, j);
    let (minute, j) = if let Some(k) = eat(s, j, "半") {
        (30, k)
    } else if take_uint(s, j).is_some() {
        let (m, k) = take_uint(s, j)?;
        let k = eat_unit(s, k, '分').unwrap_or(k);
        (m, k)
    } else {
        (0, j)
    };
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(hit(Kind::Clock { hour, minute }, i, j))
}

fn try_date(s: &str, i: usize) -> Option<Hit> {
    if let Some((year, j)) = take_uint(s, i) {
        if year >= 1900 && year <= 2100 {
            if let Some(j) = eat_unit(s, j, '年') {
                let (month, k) = take_uint(s, skip_ws(s, j))?;
                let k = eat_unit(s, k, '月')?;
                let (day, n) = take_uint(s, skip_ws(s, k))?;
                let n = eat_unit(s, n, '日')
                    .or_else(|| eat_unit(s, n, '号'))
                    .unwrap_or(n);
                if valid_ymd(year as i32, month, day) {
                    return Some(hit(
                        Kind::Date(DateTok::Ymd {
                            year: year as i32,
                            month,
                            day,
                        }),
                        i,
                        n,
                    ));
                }
            }
        }
    }
    let (month, j) = take_uint(s, i)?;
    let jw = skip_ws(s, j);
    if let Some(j) = eat_unit(s, jw, '月') {
        let k = skip_ws(s, j);
        if let Some((day, n)) = take_uint(s, k) {
            let n2 = eat_unit(s, n, '日')
                .or_else(|| eat_unit(s, n, '号'))
                .unwrap_or(n);
            if valid_md(month, day) {
                return Some(hit(Kind::Date(DateTok::Md { month, day }), i, n2));
            }
        }
        if month >= 1 && month <= 12 {
            return Some(hit(Kind::Date(DateTok::Md { month, day: 1 }), i, j));
        }
        return None;
    }
    let sep = s.get(jw..)?.chars().next()?;
    if sep != '/' && sep != '-' {
        return None;
    }
    let k = jw + sep.len_utf8();
    let (day, n) = take_uint(s, skip_ws(s, k))?;
    if valid_md(month, day) {
        Some(hit(Kind::Date(DateTok::Md { month, day }), i, n))
    } else {
        None
    }
}

fn valid_md(month: u32, day: u32) -> bool {
    (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && NaiveDate::from_ymd_opt(2024, month, day).is_some()
}

fn valid_ymd(year: i32, month: u32, day: u32) -> bool {
    NaiveDate::from_ymd_opt(year, month, day).is_some()
}

fn try_sep(s: &str, i: usize) -> Option<Hit> {
    let j = if let Some(j) = eat(s, i, "到")
        .or_else(|| eat(s, i, "至"))
        .or_else(|| eat(s, i, "—"))
        .or_else(|| eat(s, i, "–"))
        .or_else(|| eat(s, i, "~"))
        .or_else(|| eat(s, i, "～"))
        .or_else(|| eat(s, i, "-"))
    {
        j
    } else {
        return None;
    };
    let k = skip_ws(s, j);
    if try_clock(s, k).is_some() || try_period(s, k).is_some() {
        Some(hit(Kind::Sep, i, j))
    } else {
        None
    }
}

fn period_hour(p: Period) -> u32 {
    match p {
        Period::Dawn => 7,
        Period::Morning => 9,
        Period::Noon => 12,
        Period::Afternoon => 13,
        Period::Dusk => 17,
        Period::Evening => 20,
    }
}

fn apply_hour(hour: u32, period: Option<Period>) -> u32 {
    match period {
        None => hour,
        Some(Period::Dawn) | Some(Period::Morning) => {
            if hour == 12 {
                0
            } else {
                hour
            }
        }
        Some(Period::Noon) => {
            if hour < 12 {
                hour + 12
            } else {
                hour
            }
        }
        Some(Period::Afternoon) | Some(Period::Dusk) | Some(Period::Evening) => {
            if hour < 12 {
                hour + 12
            } else {
                hour
            }
        }
    }
}

fn local_at(
    now: DateTime<FixedOffset>,
    date: NaiveDate,
    hour: u32,
    minute: u32,
) -> Option<DateTime<FixedOffset>> {
    let time = NaiveTime::from_hms_opt(hour, minute, 0)?;
    date.and_time(time)
        .and_local_timezone(now.timezone())
        .single()
}

fn next_md(today: NaiveDate, month: u32, day: u32) -> Option<NaiveDate> {
    let y = today.year();
    let cand = NaiveDate::from_ymd_opt(y, month, day)?;
    if cand >= today {
        Some(cand)
    } else {
        NaiveDate::from_ymd_opt(y + 1, month, day)
    }
}

fn next_weekday(today: NaiveDate, wd: Weekday) -> NaiveDate {
    let mut delta =
        wd.num_days_from_monday() as i64 - today.weekday().num_days_from_monday() as i64;
    if delta < 0 {
        delta += 7;
    }
    today + Duration::days(delta)
}

fn nearest_12h(now: DateTime<FixedOffset>, hour: u32, minute: u32) -> Option<DateTime<FixedOffset>> {
    let today = now.date_naive();
    let mut cands: Vec<DateTime<FixedOffset>> = Vec::new();
    let push = |cands: &mut Vec<_>, date, h, m| {
        if let Some(dt) = local_at(now, date, h, m) {
            cands.push(dt);
        }
    };
    if hour == 0 || hour >= 13 {
        push(&mut cands, today, hour, minute);
        push(&mut cands, today + Duration::days(1), hour, minute);
    } else if hour == 12 {
        push(&mut cands, today, 12, minute);
        push(&mut cands, today + Duration::days(1), 0, minute);
        push(&mut cands, today + Duration::days(1), 12, minute);
    } else {
        push(&mut cands, today, hour, minute);
        push(&mut cands, today, hour + 12, minute);
        push(&mut cands, today + Duration::days(1), hour, minute);
    }
    cands.into_iter().find(|dt| *dt >= now)
}

fn build_schedule(hits: &[Hit], now: DateTime<FixedOffset>) -> Option<(i64, i64)> {
    let mut date: Option<DateTok> = None;
    let mut start_period: Option<Period> = None;
    let mut end_period: Option<Period> = None;
    let mut start_clock: Option<(u32, u32)> = None;
    let mut end_clock: Option<(u32, u32)> = None;
    let mut seen_sep = false;
    for h in hits {
        match h.kind {
            Kind::Date(d) => date = Some(d),
            Kind::Period(p) => {
                if seen_sep {
                    end_period = Some(p);
                } else {
                    start_period = Some(p);
                }
            }
            Kind::Clock { hour, minute } => {
                if seen_sep || start_clock.is_some() {
                    end_clock = Some((hour, minute));
                } else {
                    start_clock = Some((hour, minute));
                }
            }
            Kind::Sep => seen_sep = true,
        }
    }
    if start_clock.is_none() {
        if let Some(p) = start_period {
            start_clock = Some((period_hour(p), 0));
        } else if date.is_some() {
            start_clock = Some((9, 0));
            start_period = Some(Period::Morning);
        }
    }
    let (sh, sm) = start_clock?;
    let start_dt = resolve_start(now, date, start_period, sh, sm, end_clock.is_some())?;
    let end_dt = if let Some((eh, em)) = end_clock {
        let ep = end_period.or(start_period);
        let eh = apply_hour(eh, ep);
        let dummy = NaiveDate::from_ymd_opt(2026, 1, 1)?;
        let s0 = dummy.and_time(NaiveTime::from_hms_opt(apply_hour(sh, start_period), sm, 0)?);
        let mut e0 = dummy.and_time(NaiveTime::from_hms_opt(eh, em, 0)?);
        if e0 <= s0 {
            e0 += Duration::days(1);
        }
        start_dt + (e0 - s0)
    } else {
        start_dt + Duration::seconds(UNSCHEDULED_DROP_SECS)
    };
    Some((start_dt.timestamp(), end_dt.timestamp()))
}

fn resolve_start(
    now: DateTime<FixedOffset>,
    date: Option<DateTok>,
    period: Option<Period>,
    hour: u32,
    minute: u32,
    is_range: bool,
) -> Option<DateTime<FixedOffset>> {
    let today = now.date_naive();
    match date {
        None => {
            if is_range || period.is_some() {
                let h = apply_hour(hour, period);
                let start = local_at(now, today, h, minute)?;
                if start >= now {
                    Some(start)
                } else {
                    local_at(now, today + Duration::days(1), h, minute)
                }
            } else {
                nearest_12h(now, hour, minute)
            }
        }
        Some(DateTok::Rel { days, keep_past }) => {
            let date = today + Duration::days(days);
            let dt = local_at(now, date, apply_hour(hour, period), minute)?;
            if dt < now && !keep_past {
                local_at(now, date + Duration::days(1), apply_hour(hour, period), minute)
            } else {
                Some(dt)
            }
        }
        Some(DateTok::Weekday(wd)) => {
            let date = next_weekday(today, wd);
            let dt = local_at(now, date, apply_hour(hour, period), minute)?;
            if dt < now {
                local_at(now, date + Duration::days(7), apply_hour(hour, period), minute)
            } else {
                Some(dt)
            }
        }
        Some(DateTok::Md { month, day }) => {
            let date = next_md(today, month, day)?;
            let dt = local_at(now, date, apply_hour(hour, period), minute)?;
            if dt < now {
                local_at(
                    now,
                    NaiveDate::from_ymd_opt(date.year() + 1, month, day)?,
                    apply_hour(hour, period),
                    minute,
                )
            } else {
                Some(dt)
            }
        }
        Some(DateTok::Ymd { year, month, day }) => {
            local_at(
                now,
                NaiveDate::from_ymd_opt(year, month, day)?,
                apply_hour(hour, period),
                minute,
            )
        }
    }
}

fn title_from(input: &str, hits: &[Hit], tag: Option<(usize, usize)>) -> String {
    let mut chars: Vec<char> = input.chars().collect();
    let mask = |chars: &mut [char], b0: usize, b1: usize| {
        let a = char_index(input, b0);
        let b = char_index(input, b1);
        for ch in chars.iter_mut().take(b).skip(a) {
            *ch = ' ';
        }
    };
    for h in hits {
        mask(&mut chars, h.b0, h.b1);
    }
    if let Some((a, b)) = tag {
        mask(&mut chars, a, b);
    }
    strip_title(&chars.into_iter().collect::<String>())
}

fn strip_title(rest: &str) -> String {
    rest.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| c == '，' || c == ',' || c == '。' || c == '、' || c.is_whitespace())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{
        preset_lists, TaskList, PRESET_CHORE_ID, PRESET_LONGTERM_ID, PRESET_MAINLINE_ID,
    };
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

    #[test]
    fn hashtag_longterm_planning_alias() {
        let lists = preset_lists();
        let p = parse_task_line("写开题 #长期规划", &ctx(&lists));
        assert_eq!(p.list_id, PRESET_LONGTERM_ID);
        assert!(!p.parse_ok);
        assert_eq!(p.title, "写开题");
    }

    fn ctx_at<'a>(lists: &'a [TaskList], iso: &str) -> ParseContext<'a> {
        let now = DateTime::parse_from_rfc3339(iso).unwrap();
        ParseContext {
            now,
            lists,
            current_list_id: PRESET_MAINLINE_ID,
            default_list_id: PRESET_MAINLINE_ID,
        }
    }

    fn ts(iso: &str) -> i64 {
        DateTime::parse_from_rfc3339(iso).unwrap().timestamp()
    }

    fn slice_chars(s: &str, start: usize, end: usize) -> String {
        s.chars().skip(start).take(end.saturating_sub(start)).collect()
    }

    fn covered(input: &str, p: &ParsedTask) -> String {
        p.spans
            .iter()
            .map(|sp| slice_chars(input, sp.start, sp.end))
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn nine_oclock_at_afternoon_picks_2100() {
        let lists = preset_lists();
        let ctx = ctx_at(&lists, "2026-09-11T15:10:00+08:00");
        let p = parse_task_line("9点 写论文", &ctx);
        assert!(p.parse_ok);
        assert_eq!(p.title, "写论文");
        assert_eq!(p.start, Some(ts("2026-09-11T21:00:00+08:00")));
        assert_eq!(p.end, Some(ts("2026-09-11T21:30:00+08:00")));
    }

    #[test]
    fn spaced_chinese_clock() {
        let lists = preset_lists();
        let ctx = ctx_at(&lists, "2026-09-11T15:10:00+08:00");
        let p = parse_task_line("九 点 写论文", &ctx);
        assert!(p.parse_ok);
        assert_eq!(p.start, Some(ts("2026-09-11T21:00:00+08:00")));
        assert_eq!(p.title, "写论文");
    }

    #[test]
    fn mixed_clock_formats_and_half() {
        let lists = preset_lists();
        let ctx = ctx_at(&lists, "2026-09-11T08:00:00+08:00");
        for line in ["9:30 开会", "9：30 开会", "九点半 开会", "9点30分 开会"] {
            let p = parse_task_line(line, &ctx);
            assert!(p.parse_ok, "{line}");
            assert_eq!(p.start, Some(ts("2026-09-11T09:30:00+08:00")), "{line}");
            assert_eq!(p.title, "开会", "{line}");
        }
    }

    #[test]
    fn period_words_default_hours() {
        let lists = preset_lists();
        let ctx = ctx_at(&lists, "2026-09-11T06:00:00+08:00");
        let cases = [
            ("早上 写", "2026-09-11T07:00:00+08:00"),
            ("上午 写", "2026-09-11T09:00:00+08:00"),
            ("中午 写", "2026-09-11T12:00:00+08:00"),
            ("下午 写", "2026-09-11T13:00:00+08:00"),
            ("傍晚 写", "2026-09-11T17:00:00+08:00"),
            ("晚上 写", "2026-09-11T20:00:00+08:00"),
        ];
        for (line, iso) in cases {
            let p = parse_task_line(line, &ctx);
            assert!(p.parse_ok, "{line}");
            assert_eq!(p.start, Some(ts(iso)), "{line}");
            assert_eq!(p.title, "写", "{line}");
        }
    }

    #[test]
    fn date_formats_pick_next_valid_march() {
        let lists = preset_lists();
        let ctx = ctx_at(&lists, "2026-09-11T15:10:00+08:00");
        for line in ["3月6日 写", "三月六日 写", "3/6 写", "03/06 写", "3-6 写", "03-06 写"] {
            let p = parse_task_line(line, &ctx);
            assert!(p.parse_ok, "{line}");
            assert_eq!(p.start, Some(ts("2027-03-06T09:00:00+08:00")), "{line}");
            assert_eq!(p.title, "写", "{line}");
        }
        let p = parse_task_line("3月 写", &ctx);
        assert_eq!(p.start, Some(ts("2027-03-01T09:00:00+08:00")));
    }

    #[test]
    fn weekday_is_next_monday() {
        let lists = preset_lists();
        let ctx = ctx_at(&lists, "2026-09-11T15:10:00+08:00");
        let p = parse_task_line("周一 写", &ctx);
        assert_eq!(p.start, Some(ts("2026-09-14T09:00:00+08:00")));
        assert_eq!(p.title, "写");
    }

    #[test]
    fn datetime_can_follow_the_title() {
        let lists = preset_lists();
        let ctx = ctx_at(&lists, "2026-09-11T08:00:00+08:00");
        let p = parse_task_line("写方法节 明天十点", &ctx);
        assert!(p.parse_ok);
        assert_eq!(p.title, "写方法节");
        assert_eq!(p.start, Some(ts("2026-09-12T10:00:00+08:00")));
    }

    #[test]
    fn afternoon_clock_with_period() {
        let lists = preset_lists();
        let ctx = ctx_at(&lists, "2026-09-11T08:00:00+08:00");
        let p = parse_task_line("下午2点 开会", &ctx);
        assert_eq!(p.start, Some(ts("2026-09-11T14:00:00+08:00")));
    }

    #[test]
    fn highlights_recognized_datetime_spans() {
        let lists = preset_lists();
        let input = "明天上午十点到十二点，写方法节";
        let p = parse_task_line(input, &ctx(&lists));
        assert!(p.parse_ok);
        let text = covered(input, &p);
        assert!(text.contains("明天"), "{text}");
        assert!(text.contains("上午"), "{text}");
        assert!(text.contains("十点"), "{text}");
        assert!(text.contains("十二点"), "{text}");
    }

    #[test]
    fn spaced_month_day() {
        let lists = preset_lists();
        let ctx = ctx_at(&lists, "2026-09-11T15:10:00+08:00");
        let p = parse_task_line("3 月 6 日 写", &ctx);
        assert_eq!(p.start, Some(ts("2027-03-06T09:00:00+08:00")));
        assert_eq!(p.title, "写");
    }
}
