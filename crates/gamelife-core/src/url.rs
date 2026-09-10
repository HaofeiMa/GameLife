pub fn strip_url_query_fragment(url: &str) -> String {
    let query_pos = url.find('?');
    let hash_pos = url.find('#');

    match (query_pos, hash_pos) {
        (None, None) => url.to_string(),
        (Some(pos), None) | (None, Some(pos)) => url[..pos].to_string(),
        (Some(q), Some(h)) => url[..q.min(h)].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::strip_url_query_fragment;

    #[test]
    fn strip_query_and_hash() {
        assert_eq!(
            strip_url_query_fragment("https://arxiv.org/abs/1?foo=1#bar"),
            "https://arxiv.org/abs/1"
        );
    }
}
