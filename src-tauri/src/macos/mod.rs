mod document;
pub use document::ax_document_raw;

const FRONTMOST_META_SCRIPT: &str = r#"
tell application "System Events"
    set p to first application process whose frontmost is true
    set appName to name of p
    try
        set bid to bundle identifier of p
    on error
        set bid to ""
    end try
    try
        set t to name of front window of p
    on error
        set t to ""
    end try
    set docPath to ""
    try
        set w to front window of p
        try
            set rawDoc to value of attribute "AXDocument" of w
            if rawDoc is not missing value then
                set docPath to rawDoc as string
            end if
        end try
        if docPath is "" then
            try
                set rawUrl to value of attribute "AXURL" of w
                if rawUrl is not missing value then
                    set urlStr to rawUrl as string
                    if urlStr starts with "file:" then
                        set docPath to urlStr
                    end if
                end if
            end try
        end if
    end try
    return appName & tab & t & tab & bid & tab & docPath
end tell
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
struct FrontmostMeta {
    app: String,
    title: String,
    bundle_id: Option<String>,
    document_raw: Option<String>,
}

fn nonempty_field(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("missing value") {
        None
    } else {
        Some(t.to_string())
    }
}

fn parse_frontmost_meta_line(s: &str) -> Option<FrontmostMeta> {
    let s = s.trim_end_matches(['\n', '\r']);
    let mut parts = s.splitn(4, '\t');
    let app = parts.next().unwrap_or("").trim();
    if app.is_empty() {
        return None;
    }
    let title = parts.next().unwrap_or("").trim_end_matches(['\n', '\r']).to_string();
    let bundle_id = nonempty_field(parts.next().unwrap_or(""));
    let document_raw = nonempty_field(parts.next().unwrap_or(""));
    Some(FrontmostMeta {
        app: app.to_string(),
        title,
        bundle_id,
        document_raw,
    })
}

#[cfg(target_os = "macos")]
mod imp {
    use std::process::Command;

    use super::{parse_frontmost_meta_line, FrontmostMeta, FRONTMOST_META_SCRIPT};

    fn read_frontmost_meta() -> Option<FrontmostMeta> {
        let out = Command::new("osascript")
            .args(["-e", FRONTMOST_META_SCRIPT])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_frontmost_meta_line(&String::from_utf8_lossy(&out.stdout))
    }

    pub fn frontmost_app() -> Result<(String, String), ()> {
        let meta = read_frontmost_meta().ok_or(())?;
        if meta.app.is_empty() {
            return Err(());
        }
        Ok((meta.app, meta.title))
    }

    pub fn idle_seconds() -> i64 {
        extern "C" {
            fn CGEventSourceSecondsSinceLastEventType(
                state_id: u32,
                event_type: u32,
            ) -> f64;
        }
        const K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE: u32 = 0;
        const K_CG_ANY_INPUT_EVENT_TYPE: u32 = 0xFFFFFFFF;
        let secs = unsafe {
            CGEventSourceSecondsSinceLastEventType(
                K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE,
                K_CG_ANY_INPUT_EVENT_TYPE,
            )
        };
        if secs.is_finite() && secs >= 0.0 {
            secs as i64
        } else {
            0
        }
    }

    pub fn screen_locked() -> bool {
        Command::new("/usr/sbin/ioreg")
            .args(["-n", "Root", "-d1"])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout).contains("\"CGSSessionScreenIsLocked\"=Yes")
            })
            .unwrap_or(false)
    }

    pub fn secure_input_on() -> bool {
        extern "C" {
            fn IsSecureEventInputEnabled() -> bool;
        }
        unsafe { IsSecureEventInputEnabled() }
    }

    pub fn frontmost_window_id() -> Result<u32, ()> {
        let script = r#"
            tell application "System Events"
                set p to first application process whose frontmost is true
                return id of front window of p
            end tell
        "#;
        let out = Command::new("osascript").args(["-e", script]).output().map_err(|_| ())?;
        if !out.status.success() {
            return Err(());
        }
        let binding = String::from_utf8_lossy(&out.stdout);
        let s = binding.trim();
        s.parse().map_err(|_| ())
    }

    pub fn capture_frontmost_window(path: &std::path::Path) -> Result<(), ()> {
        let wid = frontmost_window_id()?;
        let status = Command::new("/usr/sbin/screencapture")
            .args([
                "-x",
                "-t",
                "jpg",
                "-l",
                &wid.to_string(),
                &path.to_string_lossy(),
            ])
            .status()
            .map_err(|_| ())?;
        if status.success() {
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn optional_browser_url() -> Option<String> {
        let script = r#"
            tell application "System Events"
                set p to first application process whose frontmost is true
                set appName to name of p
                if appName is not "Google Chrome" and appName is not "Safari" and appName is not "Arc" then return ""
                try
                    if appName is "Safari" then
                        tell application "Safari" to return URL of current tab of front window
                    else
                        tell application appName to return URL of active tab of front window
                    end if
                on error
                    return ""
                end try
            end tell
        "#;
        let out = Command::new("osascript").args(["-e", script]).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if url.is_empty() {
            None
        } else {
            Some(url)
        }
    }

    pub fn document_path() -> Option<String> {
        read_frontmost_meta()?
            .document_raw
            .as_deref()
            .and_then(gamelife_core::normalize_document_path)
    }

    pub fn bundle_id() -> Option<String> {
        read_frontmost_meta()?.bundle_id
    }

    pub fn capture_context() -> gamelife_core::CaptureContext {
        let meta = read_frontmost_meta();
        let url = optional_browser_url();
        let secure_input = secure_input_on();
        match meta {
            Some(meta) => gamelife_core::CaptureContext {
                app: meta.app,
                bundle_id: meta.bundle_id,
                title: meta.title,
                document_path: meta
                    .document_raw
                    .as_deref()
                    .and_then(gamelife_core::normalize_document_path),
                url,
                secure_input,
            },
            None => gamelife_core::CaptureContext {
                app: String::new(),
                bundle_id: None,
                title: String::new(),
                document_path: None,
                url,
                secure_input,
            },
        }
    }

    pub fn accessibility_granted() -> bool {
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }
        unsafe { AXIsProcessTrusted() }
    }

    pub fn screen_recording_granted() -> bool {
        extern "C" {
            fn CGPreflightScreenCaptureAccess() -> bool;
        }
        unsafe { CGPreflightScreenCaptureAccess() }
    }

    pub fn request_screen_recording() -> bool {
        extern "C" {
            fn CGRequestScreenCaptureAccess() -> bool;
        }
        unsafe { CGRequestScreenCaptureAccess() }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn frontmost_app() -> Result<(String, String), ()> {
        Err(())
    }

    pub fn idle_seconds() -> i64 {
        0
    }

    pub fn screen_locked() -> bool {
        false
    }

    pub fn secure_input_on() -> bool {
        false
    }

    pub fn optional_browser_url() -> Option<String> {
        None
    }

    pub fn document_path() -> Option<String> {
        None
    }

    pub fn bundle_id() -> Option<String> {
        None
    }

    pub fn capture_context() -> gamelife_core::CaptureContext {
        super::empty_capture_context()
    }

    pub fn capture_frontmost_window(_path: &std::path::Path) -> Result<(), ()> {
        Err(())
    }

    pub fn accessibility_granted() -> bool {
        true
    }

    pub fn screen_recording_granted() -> bool {
        true
    }

    pub fn request_screen_recording() -> bool {
        true
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn empty_capture_context() -> gamelife_core::CaptureContext {
    gamelife_core::CaptureContext {
        app: String::new(),
        bundle_id: None,
        title: String::new(),
        document_path: None,
        url: None,
        secure_input: false,
    }
}

pub fn frontmost_app() -> Result<(String, String), ()> {
    imp::frontmost_app()
}

pub fn idle_seconds() -> i64 {
    imp::idle_seconds()
}

pub fn screen_locked() -> bool {
    imp::screen_locked()
}

pub fn secure_input_on() -> bool {
    imp::secure_input_on()
}

pub fn optional_browser_url() -> Option<String> {
    imp::optional_browser_url()
}

pub fn document_path() -> Option<String> {
    imp::document_path()
}

pub fn bundle_id() -> Option<String> {
    imp::bundle_id()
}

pub fn capture_context() -> gamelife_core::CaptureContext {
    imp::capture_context()
}

pub fn capture_frontmost_window(path: &std::path::Path) -> Result<(), ()> {
    imp::capture_frontmost_window(path)
}

pub fn accessibility_granted() -> bool {
    imp::accessibility_granted()
}

pub fn screen_recording_granted() -> bool {
    imp::screen_recording_granted()
}

pub fn request_screen_recording() -> bool {
    imp::request_screen_recording()
}

pub fn current_process_label() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "GameLife".into())
}

pub fn current_process_path() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

pub fn metadata_observation_available() -> bool {
    accessibility_granted()
}

pub fn capture_observation_available() -> bool {
    screen_recording_granted()
}

pub fn observation_available() -> bool {
    metadata_observation_available()
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn current_process_identity_is_available() {
        assert!(!current_process_label().is_empty());
        assert!(
            !current_process_path().is_empty(),
            "current_exe path should resolve in tests"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmost_script_reads_axdocument_and_bundle_not_title() {
        assert!(FRONTMOST_META_SCRIPT.contains("AXDocument"));
        assert!(FRONTMOST_META_SCRIPT.contains("bundle identifier"));
        assert!(FRONTMOST_META_SCRIPT.contains("file:"));
        assert!(!FRONTMOST_META_SCRIPT.contains("train.py"));
    }

    #[test]
    fn parse_does_not_invent_document_path_from_window_title() {
        let meta = parse_frontmost_meta_line(
            "Cursor\ttrain.py — HDP\tcom.todesktop.230313mzl4w4u92\t",
        )
        .unwrap();
        assert_eq!(meta.app, "Cursor");
        assert_eq!(meta.title, "train.py — HDP");
        assert_eq!(
            meta.bundle_id.as_deref(),
            Some("com.todesktop.230313mzl4w4u92")
        );
        assert_eq!(meta.document_raw, None);
        assert_eq!(
            meta.document_raw
                .as_deref()
                .and_then(gamelife_core::normalize_document_path),
            None
        );
        assert_eq!(
            gamelife_core::normalize_document_path(&meta.title),
            None
        );
    }

    #[test]
    fn parse_keeps_real_document_and_file_url() {
        let posix = parse_frontmost_meta_line(
            "Cursor\ttrain.py — HDP\tcom.todesktop.230313mzl4w4u92\t/Users/me/HDP/train.py",
        )
        .unwrap();
        assert_eq!(
            posix
                .document_raw
                .as_deref()
                .and_then(gamelife_core::normalize_document_path)
                .as_deref(),
            Some("/Users/me/HDP/train.py")
        );
        let file_url = parse_frontmost_meta_line(
            "Preview\tdoc.pdf\tcom.apple.Preview\tfile:///Users/me/My%20Project/a.py",
        )
        .unwrap();
        assert_eq!(
            file_url
                .document_raw
                .as_deref()
                .and_then(gamelife_core::normalize_document_path)
                .as_deref(),
            Some("/Users/me/My Project/a.py")
        );
    }

    #[test]
    fn non_macos_capture_context_is_empty() {
        if cfg!(target_os = "macos") {
            return;
        }
        let ctx = empty_capture_context();
        assert!(ctx.app.is_empty());
        assert!(ctx.document_path.is_none());
        assert!(ctx.bundle_id.is_none());
    }
}
