mod browser;
mod capture;
mod document;
mod input;
mod login_item;
mod notify;
mod snapshot;
mod window_id;
pub use browser::{fetch_browser_url, fetch_browser_url_for, url_for, BROWSER_URL_TIMEOUT};
pub use capture::CAPTURE_TIMEOUT;
pub use login_item::apply_login_at_startup;
pub use notify::{
    cancel_all_task_notifications, replace_task_notifications, request_authorization,
    PendingTaskNotification,
};

/// The observation surface carries a `u64` window handle so one field can hold
/// a macOS `CGWindowID`, a Windows `HWND` or an X11 `XID`. This is where macOS
/// narrows it back to a `CGWindowID`.
pub fn capture_window(window_id: u64, path: &std::path::Path) -> Result<(), ()> {
    let id = u32::try_from(window_id).map_err(|_| ())?;
    capture::capture_window(id, path)
}
pub use crate::observe::state::{FrontmostSnapshot, ObservationState};
pub use document::ax_document_raw;
pub use input::{
    accessibility_granted, idle_seconds, request_screen_recording, screen_locked,
    screen_recording_granted, secure_input_on,
};
pub use snapshot::{snapshot, AX_TIMEOUT_SECS};
pub use window_id::{pick_front_window_id, CgWindowEntry};

pub fn frontmost_app() -> Result<(String, String), ()> {
    let snap = snapshot();
    if snap.app.is_empty() {
        Err(())
    } else {
        Ok((snap.app, snap.title))
    }
}

pub fn optional_browser_url() -> Option<String> {
    let snap = snapshot();
    url_for(snap.bundle_id.as_deref(), &snap.app, || {
        fetch_browser_url_for(snap.bundle_id.as_deref(), &snap.app)
    })
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
        fetch_browser_url_for(snap.bundle_id.as_deref(), &snap.app)
    });
    gamelife_core::CaptureContext {
        app: snap.app,
        bundle_id: snap.bundle_id,
        title: snap.title,
        document_path: snap
            .document_raw
            .and_then(|s| gamelife_core::normalize_document_path(&s)),
        url: gamelife_core::optional_stripped_url(url.as_deref()),
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
