//! Which kind of Linux session the app is running in.
//!
//! Only the Linux backend consults this, but it lives here rather than in
//! `linux/` so it compiles on every platform: a decision table with this many
//! edge cases is worth executing, and a Linux-target test binary cannot be run
//! on the machine this is developed on.
//!
//! The X11 backend can only see windows on an Xorg session. Under Wayland there
//! is no protocol to ask which window is focused, so the honest thing is to
//! report the session and let the UI say it cannot observe — rather than
//! sampling empty snapshots that look like an idle day.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKind {
    X11,
    Wayland,
    Unknown,
}

/// Read the session from the environment.
pub fn kind() -> SessionKind {
    kind_from(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
    )
}

/// Pure, so the rules can be tested without the environment they read.
pub fn kind_from(
    session_type: Option<&str>,
    wayland_display: bool,
    x_display: bool,
) -> SessionKind {
    match session_type.map(str::trim) {
        Some(t) if t.eq_ignore_ascii_case("wayland") => SessionKind::Wayland,
        Some(t) if t.eq_ignore_ascii_case("x11") => SessionKind::X11,
        _ => match (wayland_display, x_display) {
            // A Wayland session with XWayland sets both. XWayland only shows X
            // clients, never the compositor's own windows, so it does not make
            // the session observable — this is the case that would otherwise
            // look like a working X11 session.
            (true, _) => SessionKind::Wayland,
            (false, true) => SessionKind::X11,
            _ => SessionKind::Unknown,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{kind_from, SessionKind};

    #[test]
    fn the_session_type_variable_wins() {
        assert_eq!(kind_from(Some("x11"), true, true), SessionKind::X11);
        assert_eq!(kind_from(Some("wayland"), false, true), SessionKind::Wayland);
        assert_eq!(kind_from(Some(" X11 "), false, false), SessionKind::X11);
    }

    #[test]
    fn xwayland_still_reads_as_wayland() {
        assert_eq!(kind_from(None, true, true), SessionKind::Wayland);
    }

    #[test]
    fn an_unset_environment_is_unknown() {
        assert_eq!(kind_from(None, false, false), SessionKind::Unknown);
        assert_eq!(kind_from(None, false, true), SessionKind::X11);
    }
}
