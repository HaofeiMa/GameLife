use crate::document::looks_like_work_path;
use crate::policy::{
    all_side_project_rules, matches_any_rule, matches_app_identity, matches_rule_fields, Policy,
};
use crate::r#const::READING_BRIDGE_SECS;
use crate::types::{Hint, Quest, Sample};
use crate::url::{host_is_research, strip_url_query_fragment, url_host};

const LOW_INPUT_IDLE_SECS: i64 = 180;

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

    if matches_app_identity(&sample.app, sample.bundle_id.as_deref(), &policy.admin_apps) {
        return Hint::Admin;
    }

    if matches_any_rule(&haystacks, &all_side_project_rules(policy)) {
        return Hint::Side;
    }

    let is_reading = matches_app_identity(
        &sample.app,
        sample.bundle_id.as_deref(),
        &policy.reading_apps,
    );

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

    if !matches_app_identity(
        &sample.app,
        sample.bundle_id.as_deref(),
        &policy.trusted_apps,
    ) {
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
    if let Some(path) = &sample.document_path {
        haystacks.push(path.as_str());
    }
    haystacks
}

fn evidence_in(hay: &str, quests: &[Quest]) -> bool {
    let lower = hay.to_ascii_lowercase();
    quests.iter().any(|quest| {
        quest.evidence.iter().any(|token| {
            let needle = token.to_ascii_lowercase();
            !needle.is_empty() && lower.contains(&needle)
        })
    })
}

fn stripped_url(sample: &Sample) -> Option<String> {
    sample.url.as_deref().map(strip_url_query_fragment)
}

pub fn is_grounded_core_sample(sample: &Sample, quests: &[Quest]) -> bool {
    if sample
        .document_path
        .as_deref()
        .is_some_and(looks_like_work_path)
    {
        return true;
    }
    if sample
        .document_path
        .as_deref()
        .is_some_and(|p| evidence_in(p, quests))
    {
        return true;
    }
    let Some(url) = stripped_url(sample) else {
        return false;
    };
    if evidence_in(&url, quests) {
        return true;
    }
    url_host(&url).is_some_and(|h| host_is_research(&h))
}

fn is_core_candidate(sample: &Sample, quests: &[Quest]) -> bool {
    let title_hit = !is_readme_or_settings_title(&sample.window_title)
        && evidence_in(&sample.window_title, quests);
    title_hit || is_grounded_core_sample(sample, quests)
}

fn is_readme_or_settings_title(title: &str) -> bool {
    matches_rule_fields(&[title], "README")
        || matches_rule_fields(&[title], "Settings")
        || title.contains("设置")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{
        builtin_never_capture, builtin_side_project_rules, default_distraction_rules,
        never_capture_removable, CategoryGuides,
    };

    fn sample(app: &str, title: &str, idle: i64) -> Sample {
        Sample {
            ts: 1_000,
            app: app.into(),
            window_title: title.into(),
            url: None,
            document_path: None,
            bundle_id: None,
            idle_seconds: idle,
            screen_locked: false,
            paused: false,
            secure_input: false,
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
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        let h = hint_sample(&sample("GameLife", "Today", 10), &p, &[], None);
        assert_eq!(h, Hint::Side);
        assert!(!never_capture_removable("1Password", &p.never_capture_apps));
    }

    #[test]
    fn readme_title_still_cores_via_hdp_path() {
        let p = Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        let q = [Quest::fixture("HDP", "HDP")];
        let mut s = sample("Cursor", "README.md", 5);
        s.document_path = Some("/proj/HDP/README.md".into());
        assert_eq!(hint_sample(&s, &p, &q, None), Hint::CoreCandidate);
        assert!(is_grounded_core_sample(&s, &q));
    }

    #[test]
    fn reading_bridge_then_unsure_reading() {
        let p = Policy {
            trusted_apps: vec!["Preview".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec!["Preview".into()],
            never_capture_apps: vec![],
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        let q = [Quest::fixture("paper", "paper")];
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
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        let mut s = sample("Safari", "home", 5);
        s.url = Some("https://haofei.ma/".into());
        assert_eq!(hint_sample(&s, &p, &[], None), Hint::Side);
    }

    fn hdp_policy() -> Policy {
        Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec![],
            side_project_rules: builtin_side_project_rules(),
            reading_apps: vec![],
            never_capture_apps: vec![],
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        }
    }

    fn hdp_quest() -> [Quest; 1] {
        [Quest::fixture("HDP", "HDP")]
    }

    #[test]
    fn hdp_document_path_is_core_not_side_despite_gamelife_screenshot_dir() {
        let mut s = sample("Cursor", "train.py — HDP", 5);
        s.document_path = Some("/Users/me/Projects/HDP/train.py".into());
        assert_eq!(
            hint_sample(&s, &hdp_policy(), &hdp_quest(), None),
            Hint::CoreCandidate
        );
    }

    #[test]
    fn gamelife_document_path_is_side() {
        let mut s = sample("Cursor", "App.tsx", 5);
        s.document_path = Some("/Users/me/Projects/GameLife/src/App.tsx".into());
        assert_eq!(
            hint_sample(&s, &hdp_policy(), &hdp_quest(), None),
            Hint::Side
        );
    }

    #[test]
    fn title_only_quest_keyword_is_core_without_invented_document_path() {
        let s = sample("Cursor", "train.py — HDP", 5);
        assert_eq!(s.document_path, None);
        assert_eq!(
            hint_sample(&s, &hdp_policy(), &hdp_quest(), None),
            Hint::CoreCandidate
        );
    }

    #[test]
    fn localized_cursor_is_trusted_via_bundle_id() {
        let mut s = sample("光标", "train.py — HDP", 5);
        s.bundle_id = Some("com.todesktop.230313mzl4w4u92".into());
        assert_eq!(
            hint_sample(&s, &hdp_policy(), &hdp_quest(), None),
            Hint::CoreCandidate
        );
    }

    #[test]
    fn idle_title_match_is_core_candidate_not_away_and_not_grounded() {
        let s = sample("Cursor", "train.py — HDP", 700);
        assert_eq!(s.document_path, None);
        assert_eq!(s.url, None);
        assert_eq!(
            hint_sample(&s, &hdp_policy(), &hdp_quest(), None),
            Hint::CoreCandidate
        );
        assert!(!is_grounded_core_sample(&s, &hdp_quest()));
    }

    #[test]
    fn document_path_is_grounded() {
        let mut s = sample("Cursor", "train.py", 10);
        s.document_path = Some("/Users/me/HDP/train.py".into());
        assert_eq!(
            hint_sample(&s, &hdp_policy(), &hdp_quest(), None),
            Hint::CoreCandidate
        );
        assert!(is_grounded_core_sample(&s, &hdp_quest()));
    }

    #[test]
    fn work_like_document_path_is_grounded_without_quest_tokens() {
        let mut s = sample("Cursor", "main.tex", 10);
        s.document_path = Some("/paper/main.tex".into());
        assert_eq!(
            hint_sample(&s, &hdp_policy(), &[], None),
            Hint::CoreCandidate
        );
        assert!(is_grounded_core_sample(&s, &[]));
    }

    #[test]
    fn title_only_filename_is_not_grounded() {
        let s = sample("Cursor", "main.tex", 10);
        assert_eq!(s.document_path, None);
        assert!(!is_grounded_core_sample(&s, &[]));
    }

    #[test]
    fn overleaf_host_is_core_and_grounded_without_keyword() {
        let p = Policy {
            trusted_apps: vec!["Safari".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        let mut s = sample("Safari", "Overleaf", 10);
        s.url = Some("https://www.overleaf.com/project/abc123".into());
        assert_eq!(hint_sample(&s, &p, &hdp_quest(), None), Hint::CoreCandidate);
        assert!(is_grounded_core_sample(&s, &hdp_quest()));
        assert!(is_grounded_core_sample(&s, &[]));
    }

    #[test]
    fn overleaf_without_trusted_is_unsure() {
        let p = Policy {
            trusted_apps: vec!["Cursor".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        let mut s = sample("Safari", "Overleaf", 10);
        s.url = Some("https://www.overleaf.com/project/abc123".into());
        assert_eq!(hint_sample(&s, &p, &hdp_quest(), None), Hint::Unsure);
    }

    #[test]
    fn youtube_beats_quest_keyword_in_title() {
        let p = Policy {
            trusted_apps: vec!["Safari".into()],
            distraction_rules: default_distraction_rules(),
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        let mut s = sample("Safari", "HDP lecture", 5);
        s.url = Some("https://www.youtube.com/watch?v=1".into());
        assert_eq!(hint_sample(&s, &p, &hdp_quest(), None), Hint::Distraction);
    }

    #[test]
    fn readme_title_still_cores_via_overleaf_url() {
        let p = Policy {
            trusted_apps: vec!["Safari".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        let mut s = sample("Safari", "README", 5);
        s.url = Some("https://overleaf.com/project/x".into());
        assert_eq!(hint_sample(&s, &p, &hdp_quest(), None), Hint::CoreCandidate);
        assert!(is_grounded_core_sample(&s, &hdp_quest()));
    }

    #[test]
    fn finish_in_title_without_hdp_evidence_is_unsure() {
        let q = [Quest {
            text: "Finish HDP tactile ablation".into(),
            evidence: vec!["HDP".into()],
            hero: true,
        }];
        let s = sample("Cursor", "Finish notes", 5);
        assert_eq!(hint_sample(&s, &hdp_policy(), &q, None), Hint::Unsure);
    }

    #[test]
    fn trusted_chrome_url_evidence_is_core() {
        let p = Policy {
            trusted_apps: vec!["Google Chrome".into()],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        let q = [Quest::fixture("overleaf", "overleaf.com")];
        let mut s = sample("Google Chrome", "Overleaf", 5);
        s.url = Some("https://overleaf.com/project/abc".into());
        assert_eq!(hint_sample(&s, &p, &q, None), Hint::CoreCandidate);
    }

    #[test]
    fn untrusted_app_with_evidence_in_title_is_not_core() {
        let s = sample("WeChat", "HDP chat", 5);
        assert_eq!(
            hint_sample(&s, &hdp_policy(), &hdp_quest(), None),
            Hint::Unsure
        );
    }

    #[test]
    fn admin_app_beats_trusted_but_loses_to_distraction() {
        let p = Policy {
            trusted_apps: vec!["Mail".into()],
            distraction_rules: vec!["youtube.com".into()],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
            admin_apps: vec!["Mail".into()],
            category_guides: CategoryGuides::default(),
        };
        assert_eq!(
            hint_sample(&sample("Mail", "Inbox", 5), &p, &[], None),
            Hint::Admin
        );
        let mut yt = sample("Google Chrome", "YouTube", 5);
        yt.url = Some("https://www.youtube.com/watch?v=1".into());
        assert_eq!(hint_sample(&yt, &p, &[], None), Hint::Distraction);
    }

    #[test]
    fn builtin_gamelife_is_side_when_not_in_admin_list() {
        let p = Policy {
            trusted_apps: vec![],
            distraction_rules: vec![],
            side_project_rules: vec![],
            reading_apps: vec![],
            never_capture_apps: vec![],
            admin_apps: vec![],
            category_guides: CategoryGuides::default(),
        };
        assert_eq!(
            hint_sample(&sample("GameLife", "Today", 5), &p, &[], None),
            Hint::Side
        );
    }
}
