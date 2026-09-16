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

pub fn url_host(url: &str) -> Option<String> {
    let stripped = strip_url_query_fragment(url);
    let lower = stripped.to_ascii_lowercase();
    let rest = if let Some(r) = lower.strip_prefix("https://") {
        r
    } else if let Some(r) = lower.strip_prefix("http://") {
        r
    } else {
        return None;
    };
    let hostport = rest.split('/').next().unwrap_or("").trim();
    if hostport.is_empty() {
        return None;
    }
    let hostport = hostport.rsplit('@').next()?.trim();
    let host = if let Some(rest) = hostport.strip_prefix('[') {
        let end = rest.find(']')?;
        rest[..end].to_string()
    } else {
        hostport.split(':').next()?.to_string()
    };
    let host = host.trim_end_matches('.').to_string();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn is_alpha_len(s: &str, min: usize, max: usize) -> bool {
    let n = s.len();
    n >= min && n <= max && s.bytes().all(|b| b.is_ascii_lowercase())
}

fn host_matches_base(host: &str, base: &str) -> bool {
    host == base || host.ends_with(&format!(".{base}"))
}

fn host_is_scholar(host: &str) -> bool {
    let h = host.strip_prefix("www.").unwrap_or(host);
    if h == "scholar.google.com" {
        return true;
    }
    let Some(rest) = h.strip_prefix("scholar.google.") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    match parts.as_slice() {
        [a] if is_alpha_len(a, 2, 3) => true,
        [a, b] if is_alpha_len(a, 2, 3) && is_alpha_len(b, 2, 2) => true,
        _ => false,
    }
}

const RESEARCH_BASES: &[&str] = &[
    "overleaf.com",
    "arxiv.org",
    "ieee.org",
    "acm.org",
    "nature.com",
    "sciencedirect.com",
    "webofscience.com",
    "pubmed.ncbi.nlm.nih.gov",
];

pub fn host_is_research(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    host_is_scholar(&host)
        || RESEARCH_BASES
            .iter()
            .copied()
            .any(|b| host_matches_base(&host, b))
}

#[cfg(test)]
mod tests {
    use super::{host_is_research, optional_stripped_url, strip_url_query_fragment, url_host};

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

    #[test]
    fn arxiv_host_is_research() {
        assert_eq!(
            url_host("https://arxiv.org/abs/1?x=2").as_deref(),
            Some("arxiv.org")
        );
        assert!(host_is_research("arxiv.org"));
    }

    #[test]
    fn research_hosts_and_suffixes() {
        for url in [
            "https://www.overleaf.com/project/abc",
            "https://ieeexplore.ieee.org/document/1",
            "https://scholar.google.com/scholar?q=a",
            "https://scholar.google.co.uk/scholar",
            "https://dl.acm.org/doi/10.1",
            "https://www.nature.com/articles/s1",
            "https://www.sciencedirect.com/science/article/pii/x",
            "https://www.webofscience.com/wos",
            "https://pubmed.ncbi.nlm.nih.gov/123",
        ] {
            let host = url_host(url).unwrap_or_else(|| panic!("host for {url}"));
            assert!(host_is_research(&host), "{url} host={host}");
        }
    }

    #[test]
    fn github_nature_and_youtube_are_not_research() {
        assert_eq!(
            url_host("https://github.com/nature/foo").as_deref(),
            Some("github.com")
        );
        assert!(!host_is_research("github.com"));
        assert!(!host_is_research(
            &url_host("https://www.youtube.com/watch?v=1").unwrap()
        ));
        assert_eq!(url_host(""), None);
        assert_eq!(url_host("not a url"), None);
        assert!(!host_is_research("scholar.google.com.evil.example"));
    }
}
