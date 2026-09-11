use gamelife_core::{ListRole, TaskSnapshot};

use crate::vision::{USER_AGENT, VISION_TIMEOUT_SECS, VisionEndpoint};

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

pub fn call_text_task_match(
    endpoint: &VisionEndpoint,
    snapshots: &[TaskSnapshot],
    sample_summary: &str,
) -> Result<String, TextAiError> {
    if sample_summary.trim().is_empty() {
        return Err(TextAiError::EmptySummary);
    }
    if endpoint.api_key.trim().is_empty() || endpoint.base_url.trim().is_empty() {
        return Err(TextAiError::Client);
    }
    let tasks = snapshots
        .iter()
        .map(|s| format!("id={} title={} role={}", s.id, s.title, role_name(s.role)))
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = format!(
        "Match the observed windows to at most one timed task. Reply JSON {{\"task_id\": string|null, \"confidence\": number}}.\nTasks:\n{tasks}\nWindows:\n{sample_summary}"
    );
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
