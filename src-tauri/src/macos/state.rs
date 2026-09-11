use std::sync::Mutex;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrontmostSnapshot {
    pub app: String,
    pub title: String,
    pub bundle_id: Option<String>,
    pub document_raw: Option<String>,
    pub pid: Option<i32>,
    pub cg_window_id: Option<u32>,
}

pub struct ObservationState {
    last: Mutex<Option<FrontmostSnapshot>>,
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

    pub fn take_cg_window_id(&self) -> Option<u32> {
        let mut g = self.last.lock().expect("observation state");
        let snap = g.as_mut()?;
        snap.cg_window_id.take()
    }
}

#[cfg(test)]
mod tests {
    use super::{FrontmostSnapshot, ObservationState};

    #[test]
    fn take_clears_only_window_id() {
        let st = ObservationState::new();
        assert_eq!(st.take_cg_window_id(), None);
        st.store(FrontmostSnapshot {
            app: "Preview".into(),
            title: "a.pdf".into(),
            bundle_id: Some("com.apple.Preview".into()),
            document_raw: Some("/tmp/a.pdf".into()),
            pid: Some(9),
            cg_window_id: Some(42),
        });
        assert_eq!(st.take_cg_window_id(), Some(42));
        assert_eq!(st.take_cg_window_id(), None);
        let last = st.last().unwrap();
        assert_eq!(last.app, "Preview");
        assert_eq!(last.cg_window_id, None);
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
