pub fn strip_url_query_fragment(url: &str) -> String {
    let query_pos = url.find('?');
    let hash_pos = url.find('#');

    match (query_pos, hash_pos) {
        (None, None) => url.to_string(),
        (Some(pos), None) | (None, Some(pos)) => url[..pos].to_string(),
        (Some(q), Some(h)) => url[..q.min(h)].to_string(),
    }
}

pub fn optional_stripped_url(url: Option<&str>) -> Option<String> {
    let stripped = strip_url_query_fragment(url?);
    if stripped.is_empty() {
        None
    } else {
        Some(stripped)
    }
}

#[cfg(test)]
mod tests {
    use super::{optional_stripped_url, strip_url_query_fragment};

    #[test]
    fn strip_query_and_hash() {
        assert_eq!(
            strip_url_query_fragment("https://arxiv.org/abs/1?foo=1#bar"),
            "https://arxiv.org/abs/1"
        );
    }

    #[test]
    fn optional_stripped_url_strips_query_and_fragment() {
        assert_eq!(
            optional_stripped_url(Some("https://arxiv.org/abs/1?foo=1#bar")).as_deref(),
            Some("https://arxiv.org/abs/1")
        );
    }

    #[test]
    fn optional_stripped_url_empty_or_only_query_is_none() {
        assert_eq!(optional_stripped_url(None), None);
        assert_eq!(optional_stripped_url(Some("")), None);
        assert_eq!(optional_stripped_url(Some("?foo=1")), None);
        assert_eq!(optional_stripped_url(Some("#frag")), None);
    }
}
