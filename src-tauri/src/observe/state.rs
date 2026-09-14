//! The platform-neutral snapshot of "what the user is looking at".
//!
//! Every platform fills this in, so the sampler can stay one implementation
//! instead of three. Fields that a platform cannot answer are left empty
//! rather than faked — an empty `document_raw` is what the hint rules already
//! handle for a window that shows no file.

use std::sync::Mutex;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrontmostSnapshot {
    /// The application's display name (`Google Chrome`, `explorer.exe`).
    pub app: String,
    /// The window title.
    pub title: String,
    /// macOS bundle identifier. Windows and Linux have no equivalent, so this
    /// stays `None` and callers treat it as "unknown app identity".
    pub bundle_id: Option<String>,
    /// Raw `AXDocument`-style path, before `normalize_document_path`.
    pub document_raw: Option<String>,
    pub pid: Option<i32>,
    /// macOS `CGWindowID`, Windows `HWND`, X11 `XID` — the handle the capture
    /// path needs to grab exactly this window. Widened to `u64` so one field
    /// can carry all three.
    pub window_id: Option<u64>,
}

pub struct ObservationState {
    last: Mutex<Option<FrontmostSnapshot>>,
}

impl Default for ObservationState {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationState {
    pub fn new() -> Self {
        Self {
            last: Mutex::new(None),
        }
    }

    pub fn store(&self, snap: FrontmostSnapshot) {
        *self.last.lock().expect("observation state") = Some(snap);
    }

    pub fn last(&self) -> Option<FrontmostSnapshot> {
        self.last.lock().expect("observation state").clone()
    }

    /// The capture path takes the handle so a screenshot is tied to the window
    /// the sample was about, not to whatever is frontmost when the timer fires.
    pub fn take_window_id(&self) -> Option<u64> {
        let mut guard = self.last.lock().expect("observation state");
        let snap = guard.as_mut()?;
        snap.window_id.take()
    }
}

#[cfg(test)]
mod tests {
    use super::{FrontmostSnapshot, ObservationState};

    #[test]
    fn take_clears_only_window_id() {
        let st = ObservationState::new();
        assert_eq!(st.take_window_id(), None);
        st.store(FrontmostSnapshot {
            app: "Preview".into(),
            title: "a.pdf".into(),
            bundle_id: Some("com.apple.Preview".into()),
            document_raw: Some("/tmp/a.pdf".into()),
            pid: Some(9),
            window_id: Some(42),
        });
        assert_eq!(st.take_window_id(), Some(42));
        assert_eq!(st.take_window_id(), None);
        let last = st.last().unwrap();
        assert_eq!(last.app, "Preview");
        assert_eq!(last.window_id, None);
        assert_eq!(last.document_raw.as_deref(), Some("/tmp/a.pdf"));
    }

    #[test]
    fn last_empty_means_no_browser_snapshot() {
        let st = ObservationState::new();
        assert!(st.last().is_none());
        st.store(FrontmostSnapshot {
            app: "Safari".into(),
            ..FrontmostSnapshot::default()
        });
        assert_eq!(st.last().unwrap().app, "Safari");
    }
}
