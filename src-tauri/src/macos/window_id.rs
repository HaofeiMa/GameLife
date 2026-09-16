#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CgWindowEntry {
    pub owner_pid: i32,
    pub layer: i64,
    pub window_id: u32,
    pub on_screen: bool,
}

pub fn pick_front_window_id(pid: i32, windows: &[CgWindowEntry]) -> Option<u32> {
    windows
        .iter()
        .find(|w| w.on_screen && w.layer == 0 && w.owner_pid == pid)
        .map(|w| w.window_id)
}

#[cfg(test)]
mod tests {
    use super::{pick_front_window_id, CgWindowEntry};

    fn w(pid: i32, layer: i64, id: u32, on_screen: bool) -> CgWindowEntry {
        CgWindowEntry {
            owner_pid: pid,
            layer,
            window_id: id,
            on_screen,
        }
    }

    #[test]
    fn picks_first_layer0_for_pid() {
        let windows = [w(7, 0, 42, true), w(7, 0, 43, true)];
        assert_eq!(pick_front_window_id(7, &windows), Some(42));
    }

    #[test]
    fn skips_other_pid_and_menu_layer() {
        let windows = [w(1, 0, 99, true), w(7, 25, 100, true), w(7, 0, 42, true)];
        assert_eq!(pick_front_window_id(7, &windows), Some(42));
    }

    #[test]
    fn skips_offscreen() {
        let windows = [w(7, 0, 42, false), w(7, 0, 8, true)];
        assert_eq!(pick_front_window_id(7, &windows), Some(8));
    }

    #[test]
    fn ax_window_id_is_not_used() {
        let ax_id = 9_000_001u32;
        let windows = [w(7, 0, 42, true)];
        assert_eq!(pick_front_window_id(7, &windows), Some(42));
        assert_ne!(pick_front_window_id(7, &windows), Some(ax_id));
    }

    #[test]
    fn none_when_no_match() {
        assert_eq!(pick_front_window_id(7, &[w(1, 0, 42, true)]), None);
    }
}
