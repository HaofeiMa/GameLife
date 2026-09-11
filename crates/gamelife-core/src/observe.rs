use crate::r#const::{MAX_GAP_SECS, SAMPLE_INTERVAL_SECS};
use crate::types::{Hint, Sample};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpanKind {
    Observed(Hint),
    Unobserved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: i64,
    pub end: i64,
    pub kind: SpanKind,
    pub sample_index: Option<usize>,
}

pub fn spans_for_slot(
    samples: &[Sample],
    hints: &[Hint],
    slot_start: i64,
    slot_end: i64,
) -> Vec<Span> {
    assert_eq!(samples.len(), hints.len());

    let max_gap = MAX_GAP_SECS as i64;
    let sample_interval = SAMPLE_INTERVAL_SECS as i64;

    let mut indexed: Vec<(i64, Hint, usize)> = samples
        .iter()
        .enumerate()
        .zip(hints.iter())
        .map(|((i, s), h)| (s.ts, *h, i))
        .collect();
    indexed.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.cmp(&b.2)));

    let mut spans = Vec::new();
    let mut cursor = slot_start;

    if indexed.is_empty() {
        if slot_end > slot_start {
            spans.push(Span {
                start: slot_start,
                end: slot_end,
                kind: SpanKind::Unobserved,
                sample_index: None,
            });
        }
        return spans;
    }

    let first_ts = indexed[0].0;
    let first_index = indexed[0].2;
    if first_ts > slot_start {
        if first_ts - slot_start > max_gap {
            push_span(&mut spans, slot_start, first_ts, SpanKind::Unobserved, None);
        } else {
            push_span(
                &mut spans,
                slot_start,
                first_ts,
                SpanKind::Observed(indexed[0].1),
                Some(first_index),
            );
        }
        cursor = first_ts;
    }

    for i in 0..indexed.len() {
        let (t0, hint0, sample_index) = indexed[i];
        let segment_end = if i + 1 < indexed.len() {
            indexed[i + 1].0
        } else {
            slot_end
        };

        if segment_end <= t0 {
            continue;
        }

        let gap = segment_end - t0;
        if gap <= max_gap {
            push_span(
                &mut spans,
                t0,
                segment_end,
                SpanKind::Observed(hint0),
                Some(sample_index),
            );
        } else {
            let observed_end = t0 + sample_interval;
            if observed_end > segment_end {
                push_span(
                    &mut spans,
                    t0,
                    segment_end,
                    SpanKind::Observed(hint0),
                    Some(sample_index),
                );
            } else {
                push_span(
                    &mut spans,
                    t0,
                    observed_end,
                    SpanKind::Observed(hint0),
                    Some(sample_index),
                );
                push_span(
                    &mut spans,
                    observed_end,
                    segment_end,
                    SpanKind::Unobserved,
                    None,
                );
            }
        }
        cursor = segment_end;
    }

    if cursor < slot_end {
        push_span(&mut spans, cursor, slot_end, SpanKind::Unobserved, None);
    }

    spans
}

fn push_span(
    spans: &mut Vec<Span>,
    start: i64,
    end: i64,
    kind: SpanKind,
    sample_index: Option<usize>,
) {
    if end > start {
        spans.push(Span {
            start,
            end,
            kind,
            sample_index,
        });
    }
}

pub fn observed_seconds(spans: &[Span]) -> i64 {
    spans
        .iter()
        .filter_map(|s| match s.kind {
            SpanKind::Unobserved => None,
            SpanKind::Observed(_) => Some(s.end - s.start),
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Sample;

    #[test]
    fn four_and_a_half_minute_gap_is_unobserved_not_core() {
        let samples = vec![
            Sample {
                ts: 100,
                app: "Cursor".into(),
                window_title: "t".into(),
                url: None,
                document_path: None,
                bundle_id: None,
                idle_seconds: 1,
                screen_locked: false,
                paused: false,
                secure_input: false,
            },
            Sample {
                ts: 100 + 270,
                app: "Cursor".into(),
                window_title: "t".into(),
                url: None,
                document_path: None,
                bundle_id: None,
                idle_seconds: 1,
                screen_locked: false,
                paused: false,
                secure_input: false,
            },
        ];
        let hints = vec![Hint::CoreCandidate, Hint::CoreCandidate];
        let spans = spans_for_slot(&samples, &hints, 0, 900);
        let unobs: i64 = spans
            .iter()
            .filter_map(|s| match s.kind {
                SpanKind::Unobserved => Some(s.end - s.start),
                _ => None,
            })
            .sum();
        assert!(unobs >= 240, "gap middle must be unobserved, got {unobs}");
    }

    fn sample_at(ts: i64, app: &str, title: &str) -> Sample {
        Sample {
            ts,
            app: app.into(),
            window_title: title.into(),
            url: None,
            document_path: None,
            bundle_id: None,
            idle_seconds: 1,
            screen_locked: false,
            paused: false,
            secure_input: false,
        }
    }

    #[test]
    fn leading_fill_span_points_at_first_sample() {
        let samples = vec![sample_at(10, "Cursor", "t")];
        let hints = vec![Hint::Unsure];
        let spans = spans_for_slot(&samples, &hints, 0, 40);
        let lead = spans.iter().find(|s| s.start == 0).unwrap();
        assert_eq!(lead.sample_index, Some(0));
    }
}
