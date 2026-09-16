use crate::policy::matches_app_identity;
use crate::r#const::SAMPLE_INTERVAL_SECS;
use crate::types::{Hint, Sample};
use crate::url::url_host;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppHintSecs {
    pub samples: i64,
    pub idle_seconds: i64,
    pub core: i64,
    pub support: i64,
    pub admin: i64,
    pub side: i64,
    pub distraction: i64,
    pub away: i64,
    pub unobserved: i64,
    pub protected: i64,
}

pub fn hint_bucket(hint: Hint, protected: bool) -> &'static str {
    if protected {
        return "protected";
    }
    match hint {
        Hint::Admin => "admin",
        Hint::Side => "side",
        Hint::Distraction => "distraction",
        Hint::Away => "away",
        Hint::CoreCandidate | Hint::CoreReading => "core",
        Hint::Unsure | Hint::UnsureReading => "",
    }
}

pub fn accumulate_sample(app: &mut AppHintSecs, hint: Hint, protected: bool, idle: i64, secs: i64) {
    app.samples += 1;
    app.idle_seconds += idle;
    match hint_bucket(hint, protected) {
        "protected" => app.protected += secs,
        "admin" => app.admin += secs,
        "side" => app.side += secs,
        "distraction" => app.distraction += secs,
        "away" => app.away += secs,
        "core" => app.core += secs,
        _ => {}
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DayStatDelta {
    pub app: String,
    pub bundle_id: String,
    pub host: Option<String>,
    pub samples: i64,
    pub idle_seconds: i64,
    pub core: i64,
    pub support: i64,
    pub admin: i64,
    pub side: i64,
    pub distraction: i64,
    pub away: i64,
    pub unobserved: i64,
    pub protected: i64,
}

/// macOS lock screen (`loginwindow`). It is absence, not an app the user ran.
pub fn is_lock_screen_app(app: &str, bundle_id: Option<&str>) -> bool {
    if bundle_id.is_some_and(|id| id.eq_ignore_ascii_case("com.apple.loginwindow")) {
        return true;
    }
    app.eq_ignore_ascii_case("loginwindow")
}

pub fn deltas_from_slot(
    day_samples: &[Sample],
    hints: &[Hint],
    never: &[String],
) -> Vec<DayStatDelta> {
    let secs = SAMPLE_INTERVAL_SECS as i64;
    let mut grouped: BTreeMap<(String, String, Option<String>), DayStatDelta> = BTreeMap::new();
    for (sample, hint) in day_samples.iter().zip(hints.iter()) {
        if is_lock_screen_app(&sample.app, sample.bundle_id.as_deref()) {
            continue;
        }
        let protected = sample.secure_input
            || matches_app_identity(&sample.app, sample.bundle_id.as_deref(), never);
        let bundle_id = sample.bundle_id.clone().unwrap_or_default();
        let host = sample
            .url
            .as_deref()
            .and_then(url_host)
            .filter(|h| !h.is_empty());
        let key = (sample.app.clone(), bundle_id.clone(), host.clone());
        let delta = grouped.entry(key).or_insert_with(|| DayStatDelta {
            app: sample.app.clone(),
            bundle_id,
            host,
            ..DayStatDelta::default()
        });
        let mut acc = AppHintSecs::default();
        accumulate_sample(&mut acc, *hint, protected, sample.idle_seconds, secs);
        delta.samples += acc.samples;
        delta.idle_seconds += acc.idle_seconds;
        delta.core += acc.core;
        delta.support += acc.support;
        delta.admin += acc.admin;
        delta.side += acc.side;
        delta.distraction += acc.distraction;
        delta.away += acc.away;
        delta.unobserved += acc.unobserved;
        delta.protected += acc.protected;
    }
    grouped.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Sample;

    fn sample(app: &str) -> Sample {
        Sample {
            ts: 1_000,
            app: app.into(),
            window_title: "win".into(),
            url: None,
            document_path: None,
            bundle_id: None,
            idle_seconds: 0,
            screen_locked: false,
            paused: false,
            secure_input: false,
        }
    }

    #[test]
    fn protected_sample_only_adds_protected_seconds() {
        let mut a = AppHintSecs::default();
        accumulate_sample(&mut a, Hint::Unsure, true, 0, 15);
        assert_eq!(a.protected, 15);
        assert_eq!(a.core, 0);
        assert_eq!(a.samples, 1);
    }

    #[test]
    fn never_capture_sample_is_protected_not_distraction() {
        let s = sample("1Password");
        let deltas = deltas_from_slot(&[s], &[Hint::Distraction], &["1Password".into()]);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].protected, 15);
        assert_eq!(deltas[0].distraction, 0);
        assert_eq!(deltas[0].core, 0);
        assert_eq!(deltas[0].samples, 1);
    }

    #[test]
    fn url_host_fills_host_and_core_seconds() {
        let mut s = sample("Safari");
        s.url = Some("https://arxiv.org/abs/1".into());
        s.bundle_id = Some("com.apple.Safari".into());
        let deltas = deltas_from_slot(&[s], &[Hint::CoreCandidate], &[]);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].app, "Safari");
        assert_eq!(deltas[0].bundle_id, "com.apple.Safari");
        assert_eq!(deltas[0].host.as_deref(), Some("arxiv.org"));
        assert_eq!(deltas[0].core, 15);
        assert_eq!(deltas[0].protected, 0);
        assert_eq!(deltas[0].samples, 1);
    }

    #[test]
    fn lock_screen_sample_is_not_an_app_delta() {
        let mut lock = sample("loginwindow");
        lock.bundle_id = Some("com.apple.loginwindow".into());
        lock.screen_locked = true;
        let cursor = sample("Cursor");
        let deltas = deltas_from_slot(&[lock, cursor], &[Hint::Away, Hint::CoreCandidate], &[]);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].app, "Cursor");
        assert_eq!(deltas[0].core, 15);
        assert_eq!(deltas[0].away, 0);
    }
}
