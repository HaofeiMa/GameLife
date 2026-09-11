fn nonempty(s: Option<&str>) -> Option<&str> {
    let t = s?.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("missing value") {
        None
    } else {
        Some(t)
    }
}

pub fn ax_document_raw(ax_document: Option<&str>, ax_url: Option<&str>) -> Option<String> {
    if let Some(doc) = nonempty(ax_document) {
        return Some(doc.to_string());
    }
    let url = nonempty(ax_url)?;
    if url.to_ascii_lowercase().starts_with("file:") {
        Some(url.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::ax_document_raw;
    use gamelife_core::normalize_document_path;

    #[test]
    fn ax_document_wins_over_url() {
        let raw = ax_document_raw(
            Some("/Users/me/HDP/train.py"),
            Some("file:///Users/me/other.py"),
        );
        assert_eq!(
            raw.as_deref().and_then(normalize_document_path).as_deref(),
            Some("/Users/me/HDP/train.py")
        );
    }

    #[test]
    fn file_url_used_when_document_missing() {
        let raw = ax_document_raw(None, Some("file:///Users/me/My%20Project/a.py"));
        assert_eq!(
            raw.as_deref().and_then(normalize_document_path).as_deref(),
            Some("/Users/me/My Project/a.py")
        );
    }

    #[test]
    fn https_url_rejected() {
        assert_eq!(
            ax_document_raw(None, Some("https://arxiv.org/abs/123")),
            None
        );
    }

    #[test]
    fn window_title_never_passed_in_becomes_none() {
        assert_eq!(ax_document_raw(None, None), None);
        assert_eq!(
            ax_document_raw(None, Some("train.py — HDP")),
            None
        );
        assert_eq!(normalize_document_path("train.py — HDP"), None);
    }

    #[test]
    fn empty_and_missing_value_rejected() {
        assert_eq!(ax_document_raw(Some(""), Some("missing value")), None);
        assert_eq!(ax_document_raw(Some("missing value"), None), None);
    }
}
