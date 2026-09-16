use crate::types::Quest;
use serde::Deserialize;

pub const MAX_QUESTS: usize = 3;
pub const MAX_EVIDENCE: usize = 8;
pub const MIN_EVIDENCE_CHARS: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct QuestDraft {
    pub text: String,
    pub evidence: Vec<String>,
    pub hero: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuestListError {
    TooMany,
    EmptyText,
}

pub fn normalize_evidence(tokens: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for token in tokens {
        let trimmed = token.trim();
        if trimmed.len() < MIN_EVIDENCE_CHARS {
            continue;
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(trimmed.to_string());
            if out.len() >= MAX_EVIDENCE {
                break;
            }
        }
    }
    out
}

pub fn normalize_quest_list(drafts: Vec<QuestDraft>) -> Result<Vec<Quest>, QuestListError> {
    if drafts.len() > MAX_QUESTS {
        return Err(QuestListError::TooMany);
    }

    let mut quests = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let text = draft.text.trim();
        if text.is_empty() {
            return Err(QuestListError::EmptyText);
        }
        quests.push(Quest {
            text: text.to_string(),
            evidence: normalize_evidence(&draft.evidence),
            hero: draft.hero,
        });
    }

    if let Some(idx) = quests.iter().position(|q| q.hero) {
        for (i, quest) in quests.iter_mut().enumerate() {
            quest.hero = i == idx;
        }
    } else if let Some(first) = quests.first_mut() {
        first.hero = true;
    }

    Ok(quests)
}

pub fn quest_list_has_evidence(quests: &[Quest]) -> bool {
    quests.iter().any(|q| !q.evidence.is_empty())
}

pub fn matched_quest_index(
    title: &str,
    document_path: Option<&str>,
    url: Option<&str>,
    quests: &[Quest],
) -> Option<usize> {
    let title_lower = title.to_ascii_lowercase();
    let path_lower = document_path
        .filter(|p| !p.is_empty())
        .map(|p| p.to_ascii_lowercase());
    let url_lower = url
        .filter(|u| !u.is_empty())
        .map(|u| u.to_ascii_lowercase());

    for (idx, quest) in quests.iter().enumerate() {
        for token in &quest.evidence {
            let needle = token.to_ascii_lowercase();
            if title_lower.contains(&needle) {
                return Some(idx);
            }
            if path_lower
                .as_ref()
                .is_some_and(|path| path.contains(&needle))
            {
                return Some(idx);
            }
            if url_lower.as_ref().is_some_and(|u| u.contains(&needle)) {
                return Some(idx);
            }
        }
    }
    None
}

#[derive(Deserialize)]
struct QuestVersionJson {
    text: String,
    evidence: Option<Vec<String>>,
    keywords: Option<Vec<String>>,
    hero: Option<bool>,
}

pub fn parse_quest_versions_json(json: &str) -> Result<Vec<Quest>, String> {
    let parsed: Vec<QuestVersionJson> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let drafts = parsed
        .into_iter()
        .map(|q| QuestDraft {
            text: q.text,
            evidence: q.evidence.or(q.keywords).unwrap_or_default(),
            hero: q.hero.unwrap_or(false),
        })
        .collect();
    normalize_quest_list(drafts).map_err(|e| match e {
        QuestListError::TooMany => "too many quests".to_string(),
        QuestListError::EmptyText => "empty quest text".to_string(),
    })
}

pub fn vision_quest_label(quest: &Quest) -> String {
    if quest.hero {
        format!("[main] {}", quest.text)
    } else {
        quest.text.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(text: &str, evidence: &[&str], hero: bool) -> QuestDraft {
        QuestDraft {
            text: text.into(),
            evidence: evidence.iter().map(|s| (*s).to_string()).collect(),
            hero,
        }
    }

    #[test]
    fn normalize_evidence_trims_drops_short_and_dedupes() {
        let raw = vec![
            "  HDP  ".into(),
            "a".into(),
            "".into(),
            "hdp".into(),
            "RL".into(),
        ];
        assert_eq!(
            normalize_evidence(&raw),
            vec!["HDP".to_string(), "RL".to_string()]
        );
    }

    #[test]
    fn normalize_evidence_caps_at_eight() {
        let raw: Vec<String> = (0..12).map(|i| format!("t{i}")).collect();
        assert_eq!(normalize_evidence(&raw).len(), 8);
    }

    #[test]
    fn normalize_quest_list_empty_ok_four_fails() {
        assert_eq!(normalize_quest_list(vec![]).unwrap(), vec![]);
        let four = vec![
            draft("a", &["ab"], false),
            draft("b", &["ab"], false),
            draft("c", &["ab"], false),
            draft("d", &["ab"], false),
        ];
        assert_eq!(normalize_quest_list(four), Err(QuestListError::TooMany));
    }

    #[test]
    fn empty_text_fails() {
        assert_eq!(
            normalize_quest_list(vec![draft("  ", &["HDP"], true)]),
            Err(QuestListError::EmptyText)
        );
    }

    #[test]
    fn hero_defaults_and_first_marked_wins() {
        let qs = normalize_quest_list(vec![
            draft("A", &["aa"], false),
            draft("B", &["bb"], true),
            draft("C", &["cc"], true),
        ])
        .unwrap();
        assert!(qs[0].hero == false && qs[1].hero && !qs[2].hero);

        let qs = normalize_quest_list(vec![draft("A", &["aa"], false), draft("B", &["bb"], false)])
            .unwrap();
        assert!(qs[0].hero && !qs[1].hero);
    }

    #[test]
    fn has_evidence_false_when_only_titles() {
        let qs = vec![Quest {
            text: "Finish paper".into(),
            evidence: vec![],
            hero: true,
        }];
        assert!(!quest_list_has_evidence(&qs));
    }

    #[test]
    fn matched_index_title_path_url_not_app() {
        let qs = vec![
            Quest::fixture("paper", "main.tex"),
            Quest::fixture("web", "overleaf.com"),
        ];
        assert_eq!(
            matched_quest_index("main.tex — HDP", None, None, &qs),
            Some(0)
        );
        assert_eq!(
            matched_quest_index("x", Some("/proj/main.tex"), None, &qs),
            Some(0)
        );
        assert_eq!(
            matched_quest_index("x", None, Some("https://overleaf.com/project/1"), &qs),
            Some(1)
        );
        assert_eq!(
            matched_quest_index("train.py", None, None, &[Quest::fixture("c", "Cursor")]),
            None
        );
    }

    #[test]
    fn parse_old_keywords_json() {
        let qs = parse_quest_versions_json(r#"[{"text":"HDP","keywords":["HDP"]}]"#).unwrap();
        assert_eq!(qs[0].evidence, vec!["HDP".to_string()]);
        assert!(qs[0].hero);
    }

    #[test]
    fn vision_label_prefixes_hero() {
        let mut q = Quest::fixture("HDP", "HDP");
        assert_eq!(vision_quest_label(&q), "[main] HDP");
        q.hero = false;
        assert_eq!(vision_quest_label(&q), "HDP");
    }
}
