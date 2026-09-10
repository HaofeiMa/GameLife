use crate::r#const::READING_BRIDGE_SECS;
use crate::policy::{
    Policy, all_side_project_rules, matches_any_rule, matches_app_name, matches_rule_fields,
};
use crate::types::{Hint, Quest, Sample};

const LOW_INPUT_IDLE_SECS: i64 = 180;
const AWAY_IDLE_SECS: i64 = 600;

pub fn hint_sample(
    sample: &Sample,
    policy: &Policy,
    quests: &[Quest],
    last_core_interaction_ts: Option<i64>,
) -> Hint {
    if sample.screen_locked || sample.paused {
        return Hint::Away;
    }

    let haystacks = sample_haystacks(sample);

    if matches_any_rule(&haystacks, &policy.distraction_rules) {
        return Hint::Distraction;
    }

    if matches_any_rule(&haystacks, &all_side_project_rules(policy)) {
        return Hint::Side;
    }

    let is_reading = matches_app_name(&sample.app, &policy.reading_apps);

    if sample.idle_seconds >= AWAY_IDLE_SECS && !is_reading {
        return Hint::Away;
    }

    if is_reading {
        if let Some(last) = last_core_interaction_ts {
            let since_core = sample.ts - last;
            if sample.idle_seconds >= LOW_INPUT_IDLE_SECS {
                if since_core <= READING_BRIDGE_SECS as i64 {
                    return Hint::CoreReading;
                }
                return Hint::UnsureReading;
            }
        }
    }

    if !matches_app_name(&sample.app, &policy.trusted_apps) {
        return Hint::Unsure;
    }

    if is_core_candidate(sample, quests) {
        return Hint::CoreCandidate;
    }

    Hint::Unsure
}

fn sample_haystacks(sample: &Sample) -> Vec<&str> {
    let mut haystacks = vec![sample.app.as_str(), sample.window_title.as_str()];
    if let Some(url) = &sample.url {
        haystacks.push(url.as_str());
    }
    if let Some(path) = &sample.path {
        haystacks.push(path.as_str());
    }
    haystacks
}

fn is_core_candidate(sample: &Sample, quests: &[Quest]) -> bool {
    if is_readme_or_settings_title(&sample.window_title) {
        return false;
    }

    let title_lower = sample.window_title.to_ascii_lowercase();
    let path_lower = sample
        .path
        .as_deref()
        .map(|p| p.to_ascii_lowercase())
        .unwrap_or_default();

    quests.iter().any(|quest| {
        quest.keywords.iter().any(|keyword| {
            let needle = keyword.to_ascii_lowercase();
            title_lower.contains(&needle) || path_lower.contains(&needle)
        })
    })
}

fn is_readme_or_settings_title(title: &str) -> bool {
    matches_rule_fields(&[title], "README")
        || matches_rule_fields(&[title], "Settings")
        || title.contains("设置")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{builtin_never_capture, builtin_side_project_rules, never_capture_removable};

    fn sample(app: &str, title: &str, idle: i64) -> Sample {
        Sample {
            ts: 1_000,
            app: app.into(),
            window_title: title.into(),
            url: None,
            path: Some("/Users/me/HDP/README.md".into()),
            idle_seconds: idle,
            screen_locked: false,
            paused: false,
        }
    }

    #[test]
    fn gamelife_is_side_and_not_removable_from_side_rules() {
        let p = Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec![],
            side_project_rules: builtin_side_project_rules(),
            reading_apps: vec![],
            never_capture_apps: builtin_never_capture(),
        };
        let h = hint_sample(&sample("GameLife", "Today", 10), &p, &[], None);
        assert_eq!(h, Hint::Side);
        assert!(!never_capture_removable("1Password", &p.never_capture_apps));
    }

    #[test]
    fn readme_in_project_path_is_not_core_candidate() {
        let p = Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
        };
        let q = [Quest {
            text: "HDP".into(),
            keywords: vec!["HDP".into()],
        }];
        let mut s = sample("Cursor", "README.md", 5);
        s.path = Some("/proj/HDP/README.md".into());
        assert_eq!(hint_sample(&s, &p, &q, None), Hint::Unsure);
    }

    #[test]
    fn reading_bridge_then_unsure_reading() {
        let p = Policy {
            trusted_apps: vec!["Preview".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec!["Preview".into()],
            never_capture_apps: vec![],
        };
        let q = [Quest {
            text: "paper".into(),
            keywords: vec!["paper".into()],
        }];
        let mut s = sample("Preview", "paper.pdf", 200);
        s.ts = 1_000 + 200;
        assert_eq!(hint_sample(&s, &p, &q, Some(1_000)), Hint::CoreReading);
        s.ts = 1_000 + 400;
        s.idle_seconds = 400;
        assert_eq!(hint_sample(&s, &p, &q, Some(1_000)), Hint::UnsureReading);
    }

    #[test]
    fn homepage_is_side_not_distraction() {
        let p = Policy {
            trusted_apps: vec!["Safari".into()],
            distraction_rules: vec!["bilibili".into()],
            side_project_rules: vec!["haofei.ma".into()],
            reading_apps: vec![],
            never_capture_apps: vec![],
        };
        let mut s = sample("Safari", "home", 5);
        s.url = Some("https://haofei.ma/".into());
        assert_eq!(hint_sample(&s, &p, &[], None), Hint::Side);
    }
}
