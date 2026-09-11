const BUNDLES: &[&str] = &[
    "com.google.Chrome",
    "com.apple.Safari",
    "company.thebrowser.Browser",
];
const NAMES: &[&str] = &["Google Chrome", "Safari", "Arc"];

pub fn is_url_browser(bundle_id: Option<&str>, app: &str) -> bool {
    if let Some(b) = bundle_id {
        if BUNDLES.contains(&b) {
            return true;
        }
    }
    NAMES.contains(&app)
}

pub fn url_for(
    bundle_id: Option<&str>,
    app: &str,
    fetch: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if is_url_browser(bundle_id, app) {
        fetch()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{is_url_browser, url_for};

    #[test]
    fn allowlist_by_bundle_or_name() {
        assert!(is_url_browser(Some("com.google.Chrome"), "Chrome"));
        assert!(is_url_browser(None, "Google Chrome"));
        assert!(is_url_browser(Some("com.apple.Safari"), ""));
        assert!(is_url_browser(Some("company.thebrowser.Browser"), "Arc"));
        assert!(is_url_browser(None, "Arc"));
    }

    #[test]
    fn rejects_cursor_wechat_and_empty() {
        assert!(!is_url_browser(
            Some("com.todesktop.230313mzl4w4u92"),
            "Cursor"
        ));
        assert!(!is_url_browser(None, "WeChat"));
        assert!(!is_url_browser(None, ""));
        assert!(!is_url_browser(Some("com.microsoft.edgemac"), "Microsoft Edge"));
    }

    #[test]
    fn url_for_skips_fetch_when_not_browser() {
        let called = std::cell::Cell::new(false);
        let url = url_for(Some("com.todesktop.230313mzl4w4u92"), "Cursor", || {
            called.set(true);
            Some("https://example.com".into())
        });
        assert_eq!(url, None);
        assert!(!called.get());
    }

    #[test]
    fn url_for_calls_fetch_for_safari() {
        let called = std::cell::Cell::new(false);
        let url = url_for(Some("com.apple.Safari"), "Safari", || {
            called.set(true);
            Some("https://arxiv.org/abs/1?x=2".into())
        });
        assert_eq!(url.as_deref(), Some("https://arxiv.org/abs/1?x=2"));
        assert!(called.get());
    }
}
