use std::io::Read;
use std::time::Duration;

pub const BROWSER_URL_TIMEOUT: Duration = Duration::from_secs(1);

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

pub(crate) fn wait_output_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Option<Vec<u8>> {
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = child.stdout.take()?;
                let mut bytes = Vec::new();
                stdout.read_to_end(&mut bytes).ok()?;
                return if status.success() {
                    Some(bytes)
                } else {
                    None
                };
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    }
}

pub fn fetch_browser_url_for(bundle_id: Option<&str>, app: &str) -> Option<String> {
    let target = if bundle_id == Some("com.apple.Safari") || app == "Safari" {
        "Safari"
    } else if bundle_id == Some("com.google.Chrome") || app == "Google Chrome" {
        "Google Chrome"
    } else if bundle_id == Some("company.thebrowser.Browser") || app == "Arc" {
        "Arc"
    } else {
        return None;
    };
    let script = if target == "Safari" {
        r#"tell application "Safari" to return URL of current tab of front window"#.to_string()
    } else {
        format!(r#"tell application "{target}" to return URL of active tab of front window"#)
    };
    let mut child = std::process::Command::new("osascript")
        .args(["-e", &script])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let bytes = wait_output_timeout(&mut child, BROWSER_URL_TIMEOUT)?;
    let url = String::from_utf8(bytes).ok()?;
    let url = url.trim();
    if url.is_empty() {
        None
    } else {
        Some(url.to_string())
    }
}

pub fn fetch_browser_url(app: &str) -> Option<String> {
    fetch_browser_url_for(None, app)
}

#[cfg(test)]
mod tests {
    use super::{is_url_browser, url_for, wait_output_timeout};
    use std::time::Duration;

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

    #[test]
    fn wait_output_timeout_kills_child() {
        let mut child = std::process::Command::new("sleep")
            .arg("2")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        assert!(wait_output_timeout(&mut child, Duration::from_millis(50)).is_none());
        let still = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .unwrap();
        assert!(!still.success());
    }
}
