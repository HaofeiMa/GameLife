mod browser;
mod capture;
mod document;
mod input;
mod snapshot;
mod state;
mod window_id;
pub use browser::url_for;
pub use capture::{capture_window, CAPTURE_TIMEOUT};
pub use document::ax_document_raw;
pub use input::{
    accessibility_granted, idle_seconds, request_screen_recording, screen_locked,
    screen_recording_granted, secure_input_on,
};
pub use snapshot::{snapshot, AX_TIMEOUT_SECS};
pub use state::{FrontmostSnapshot, ObservationState};
pub use window_id::{pick_front_window_id, CgWindowEntry};

#[cfg(target_os = "macos")]
mod browser_os {
    use std::process::Command;

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
}

#[cfg(not(target_os = "macos"))]
mod browser_os {
    pub fn optional_browser_url() -> Option<String> {
        None
    }
}

pub fn frontmost_app() -> Result<(String, String), ()> {
    let snap = snapshot();
    if snap.app.is_empty() {
        Err(())
    } else {
        Ok((snap.app, snap.title))
    }
}

pub fn optional_browser_url() -> Option<String> {
    browser_os::optional_browser_url()
}

fn optional_browser_url_os(app: &str) -> Option<String> {
    let _ = app;
    optional_browser_url()
}

pub fn document_path() -> Option<String> {
    snapshot()
        .document_raw
        .as_deref()
        .and_then(gamelife_core::normalize_document_path)
}

pub fn bundle_id() -> Option<String> {
    snapshot().bundle_id
}

pub fn capture_context() -> gamelife_core::CaptureContext {
    let snap = snapshot();
    let url = url_for(snap.bundle_id.as_deref(), &snap.app, || {
        optional_browser_url_os(&snap.app)
    });
    gamelife_core::CaptureContext {
        app: snap.app,
        bundle_id: snap.bundle_id,
        title: snap.title,
        document_path: snap
            .document_raw
            .and_then(|s| gamelife_core::normalize_document_path(&s)),
        url,
        secure_input: secure_input_on(),
    }
}

pub fn capture_frontmost_window(_path: &std::path::Path) -> Result<(), ()> {
    Err(())
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
