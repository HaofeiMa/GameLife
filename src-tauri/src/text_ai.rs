use gamelife_core::{
    nonempty_guides, truncate_guide, CategoryGuides, ListRole, Policy, TaskSnapshot,
};

use crate::vision::{VisionEndpoint, USER_AGENT, VISION_TIMEOUT_SECS};

pub struct SampleLine {
    pub app: String,
    pub title: String,
    pub url: Option<String>,
    pub document_path: Option<String>,
    pub idle_seconds: i64,
    pub protected: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TextAiError {
    Transport,
    Client,
    Parse,
    EmptySummary,
}

fn role_name(role: ListRole) -> &'static str {
    match role {
        ListRole::Mainline => "mainline",
        ListRole::Side => "side",
        ListRole::Longterm => "longterm",
        ListRole::Chore => "chore",
        ListRole::Custom => "custom",
    }
}

pub fn sample_summary_lines(lines: &[SampleLine]) -> String {
    lines
        .iter()
        .filter(|line| !line.protected)
        .map(|line| {
            format!(
                "app={} title={} url={} document_path={} idle={}",
                line.app,
                line.title,
                line.url.as_deref().unwrap_or(""),
                line.document_path.as_deref().unwrap_or(""),
                line.idle_seconds
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn join_capped(names: &[String]) -> String {
    names
        .iter()
        .take(20)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn policy_names_blurb(policy: &Policy) -> String {
    format!(
        "主线应用: {}\n支线: {}\n杂项: {}\n娱乐: {}",
        join_capped(&policy.trusted_apps),
        join_capped(&policy.side_project_rules),
        join_capped(&policy.admin_apps),
        join_capped(&policy.distraction_rules),
    )
}

fn guides_section(guides: &CategoryGuides) -> String {
    let body = nonempty_guides(guides)
        .into_iter()
        .map(|(key, text)| format!("{key}: {}", truncate_guide(&text)))
        .collect::<Vec<_>>()
        .join("\n");
    if body.is_empty() {
        String::new()
    } else {
        format!("{body}\n")
    }
}

pub fn build_text_ai_prompt(
    snapshots: &[TaskSnapshot],
    guides: &CategoryGuides,
    policy: &Policy,
    sample_summary: &str,
) -> String {
    let names = policy_names_blurb(policy);
    let guides = guides_section(guides);
    if snapshots.is_empty() {
        format!(
            "Classify the observed windows into one activity category. Reply JSON {{\"category\": string|null, \"confidence\": number}}.\n{guides}{names}\nWindows:\n{sample_summary}"
        )
    } else {
        let tasks = snapshots
            .iter()
            .map(|s| format!("id={} title={} role={}", s.id, s.title, role_name(s.role)))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Match the observed windows to at most one timed task. Reply JSON {{\"task_id\": string|null, \"confidence\": number}}.\nTasks:\n{tasks}\nWindows:\n{sample_summary}\n{guides}{names}"
        )
    }
}

pub fn call_text_json(endpoint: &VisionEndpoint, prompt: &str) -> Result<String, TextAiError> {
    if prompt.trim().is_empty() {
        return Err(TextAiError::EmptySummary);
    }
    if endpoint.api_key.trim().is_empty() || endpoint.base_url.trim().is_empty() {
        return Err(TextAiError::Client);
    }
    let url = format!(
        "{}/chat/completions",
        endpoint.base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": endpoint.model,
        "response_format": { "type": "json_object" },
        "messages": [{ "role": "user", "content": prompt }]
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(VISION_TIMEOUT_SECS))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|_| TextAiError::Transport)?;
    let resp = client
        .post(&url)
        .header("x-opencode-session", "gamelife")
        .bearer_auth(&endpoint.api_key)
        .json(&body)
        .send()
        .map_err(|_| TextAiError::Transport)?;
    let status = resp.status();
    if status.is_server_error() {
        return Err(TextAiError::Transport);
    }
    if !status.is_success() {
        return Err(TextAiError::Client);
    }
    let json: serde_json::Value = resp.json().map_err(|_| TextAiError::Parse)?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or(TextAiError::Parse)?;
    Ok(content.to_string())
}

pub fn call_text_task_match(
    endpoint: &VisionEndpoint,
    prompt: &str,
) -> Result<String, TextAiError> {
    call_text_json(endpoint, prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gamelife_core::{default_v01, CategoryGuides};

    #[test]
    fn prompt_omits_empty_guides_and_protected_lines_already_filtered() {
        let p = default_v01();
        let prompt = build_text_ai_prompt(
            &[],
            &CategoryGuides::default(),
            &p,
            "app=Cursor title=x url= document_path= idle=1",
        );
        assert!(!prompt.contains("主线："));
        assert!(prompt.contains("category"));
        assert!(!prompt.contains("task_id"));
    }

    #[test]
    fn prompt_with_snapshot_asks_for_task_id() {
        let snaps = [TaskSnapshot {
            id: "tt-1".into(),
            title: "HDP".into(),
            role: ListRole::Mainline,
        }];
        let mut g = CategoryGuides::default();
        g.mainline = "写论文".into();
        let prompt = build_text_ai_prompt(
            &snaps,
            &g,
            &default_v01(),
            "app=Cursor title=HDP url= document_path=/p/HDP/a.py idle=1",
        );
        assert!(prompt.contains("task_id"));
        assert!(prompt.contains("写论文"));
        assert!(prompt.contains("tt-1"));
    }

    #[test]
    fn policy_names_blurb_caps_each_list_at_20() {
        let mut p = default_v01();
        p.trusted_apps = (0..25).map(|i| format!("App{i}")).collect();
        let b = policy_names_blurb(&p);
        assert!(b.contains("主线应用:"));
        assert!(b.contains("\n支线:"));
        assert!(b.contains("\n杂项:"));
        assert!(b.contains("\n娱乐:"));
        assert!(b.contains("App19"));
        assert!(!b.contains("App20"));
    }

    #[test]
    fn omits_never_capture_sample_lines() {
        let body = sample_summary_lines(&[SampleLine {
            app: "1Password".into(),
            title: "secret".into(),
            url: None,
            document_path: None,
            idle_seconds: 0,
            protected: true,
        }]);
        assert!(!body.contains("secret"));
    }

    #[test]
    fn empty_summary_means_skip() {
        let body = sample_summary_lines(&[SampleLine {
            app: "1Password".into(),
            title: "x".into(),
            url: None,
            document_path: None,
            idle_seconds: 0,
            protected: true,
        }]);
        assert!(body.is_empty());
    }

    #[test]
    fn drops_protected_lines_but_keeps_others() {
        let body = sample_summary_lines(&[
            SampleLine {
                app: "1Password".into(),
                title: "secret".into(),
                url: None,
                document_path: None,
                idle_seconds: 0,
                protected: true,
            },
            SampleLine {
                app: "Cursor".into(),
                title: "ok".into(),
                url: None,
                document_path: None,
                idle_seconds: 1,
                protected: false,
            },
        ]);
        assert!(!body.contains("secret"));
        assert!(body.contains("Cursor"));
        assert!(body.contains("ok"));
    }
}
