//! The platform seam for window observation.
//!
//! The sampler reaches the desktop only through `observe::imp`, which resolves
//! to `macos/`, `windows/` or `linux/` at compile time, so nothing else in the
//! app has to name a platform. Each platform module exposes the same surface:
//!
//! ```text
//! snapshot() / capture_window() / frontmost_app()
//! idle_seconds() / screen_locked() / secure_input_on()
//! document_path() / bundle_id() / capture_context()
//! url_for() / fetch_browser_url_for()
//! metadata_observation_available() / capture_observation_available()
//! accessibility_granted() / screen_recording_granted() / request_screen_recording()
//! current_process_label() / current_process_path()
//! ```
//!
//! What a platform cannot answer it reports honestly rather than guessing —
//! see each module for what that means there.

pub mod session;
pub mod state;

#[cfg(target_os = "macos")]
pub use crate::macos as imp;

#[cfg(target_os = "windows")]
pub use crate::windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use crate::linux as imp;

pub use state::{FrontmostSnapshot, ObservationState};

/// Whether this platform — and, on Linux, this session — can observe windows
/// at all. This is a *capability* answer, not a runtime one: a missing macOS
/// permission is reported by `get_permission_status` and is a different
/// problem from a platform that has no backend.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ObservationStatus {
    pub supported: bool,
    /// Named so the UI can say which backend it is talking about.
    pub backend: String,
}

pub fn status() -> ObservationStatus {
    #[cfg(target_os = "macos")]
    {
        ObservationStatus {
            supported: true,
            backend: "macOS".into(),
        }
    }
    #[cfg(target_os = "windows")]
    {
        ObservationStatus {
            supported: true,
            backend: "Windows".into(),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Wayland deliberately has no way to ask which window is focused, so a
        // Wayland session is a platform that cannot observe — not a broken one.
        match session::kind() {
            session::SessionKind::X11 => ObservationStatus {
                supported: true,
                backend: "X11".into(),
            },
            session::SessionKind::Wayland => ObservationStatus {
                supported: false,
                backend: "Wayland".into(),
            },
            session::SessionKind::Unknown => ObservationStatus {
                supported: false,
                backend: "no X display".into(),
            },
        }
    }
}
