//! Per-OS conventions: where the app keeps its data, and how to hand a URL to
//! the system browser. Everything that reaches into Apple frameworks or
//! ScreenCaptureKit stays in `macos/`; this is the small surface the rest of
//! the app needs on every platform.
//!
//! Only `HOST` is resolved at compile time — which env vars the running OS
//! actually sets. The conventions themselves are pure functions over those
//! values, so the Windows and Linux rules stay unit-testable from a Mac; a
//! `cfg`-fanned `data_dir()` would leave two thirds of this module unverified
//! until someone happened to build on that platform.

use std::ffi::OsStr;
use std::path::PathBuf;

const APP_DIR: &str = "GameLife";

/// Which platform's convention to apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

#[cfg(target_os = "macos")]
pub const HOST: Os = Os::MacOs;
#[cfg(target_os = "windows")]
pub const HOST: Os = Os::Windows;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const HOST: Os = Os::Linux;

/// An env var counts only when it is set *and* non-empty. XDG says an empty
/// `XDG_DATA_HOME` is the same as unset, and Windows can hand back "" for a
/// variable that exists but was never given a value.
fn nonempty(value: Option<&OsStr>) -> Option<&OsStr> {
    value.filter(|v| !v.is_empty())
}

/// The app's data root for a platform convention.
///
/// macOS keeps `~/Library/Application Support/GameLife` — this must not move,
/// or every existing install loses its database, config and secrets.
pub fn data_dir_for(
    os: Os,
    home: Option<&OsStr>,
    appdata: Option<&OsStr>,
    xdg_data_home: Option<&OsStr>,
) -> Option<PathBuf> {
    match os {
        Os::MacOs => {
            let home = nonempty(home)?;
            Some(PathBuf::from(home).join("Library/Application Support").join(APP_DIR))
        }
        Os::Windows => {
            // %APPDATA% is the roaming profile; fall back to the well-known
            // location under the profile directory if it is somehow unset.
            let base = nonempty(appdata)
                .map(PathBuf::from)
                .or_else(|| nonempty(home).map(|h| PathBuf::from(h).join("AppData/Roaming")))?;
            Some(base.join(APP_DIR))
        }
        Os::Linux => {
            // XDG: $XDG_DATA_HOME when set, but only if absolute — a relative
            // value is invalid and must be ignored rather than resolved against
            // the cwd.
            let base = nonempty(xdg_data_home)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| nonempty(home).map(|h| PathBuf::from(h).join(".local/share")))?;
            Some(base.join(APP_DIR))
        }
    }
}

/// The data root on the running platform. The name is historical — on every
/// platform this is `…/GameLife`, not necessarily an "Application Support" dir.
pub fn app_support_dir() -> Option<PathBuf> {
    data_dir_for(
        HOST,
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("APPDATA").as_deref(),
        std::env::var_os("XDG_DATA_HOME").as_deref(),
    )
}

pub fn screenshots_dir() -> Option<PathBuf> {
    app_support_dir().map(|d| d.join("screenshots"))
}

/// The program and argv that hands `url` to the default browser.
///
/// Split out from `open_url` so the Windows and Linux shapes are testable
/// without actually launching anything.
pub fn opener_for(os: Os, url: &str) -> (&'static str, Vec<String>) {
    match os {
        Os::MacOs => ("open", vec![url.to_string()]),
        // `start` reads its first quoted argument as the window title, so the
        // empty title is load-bearing: without it the URL is taken as the title
        // and no browser opens.
        Os::Windows => (
            "cmd",
            vec!["/C".into(), "start".into(), String::new(), url.to_string()],
        ),
        Os::Linux => ("xdg-open", vec![url.to_string()]),
    }
}

pub fn open_url(url: &str) -> Result<(), String> {
    let (program, args) = opener_for(HOST, url);
    let status = std::process::Command::new(program)
        .args(&args)
        .status()
        .map_err(|e| format!("{program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn os(s: &str) -> Option<&OsStr> {
        Some(OsStr::new(s))
    }

    /// `PathBuf::join` uses the *host* separator, so on this Mac a Windows
    /// path comes back as `...\\Roaming/GameLife`. The shape is what the
    /// convention decides; the separator is the host's business.
    fn norm(p: PathBuf) -> String {
        p.to_string_lossy().replace('\\', "/")
    }

    /// The one path that must never move: every existing install's database,
    /// config and secrets live under it. Runs on the host so a change to the
    /// macOS branch fails here, not after someone loses their history.
    #[cfg(target_os = "macos")]
    #[test]
    fn host_dir_is_still_the_application_support_path() {
        let dir = app_support_dir().expect("HOME should be set");
        assert!(
            dir.ends_with("Library/Application Support/GameLife"),
            "{dir:?} is not the historical data root"
        );
    }

    #[test]
    fn macos_keeps_the_historical_application_support_path() {
        assert_eq!(
            norm(data_dir_for(Os::MacOs, os("/Users/x"), None, None).unwrap()),
            "/Users/x/Library/Application Support/GameLife"
        );
    }

    #[test]
    fn macos_without_home_has_no_data_dir() {
        assert_eq!(data_dir_for(Os::MacOs, None, None, None), None);
        assert_eq!(data_dir_for(Os::MacOs, os(""), None, None), None);
    }

    #[test]
    fn windows_prefers_appdata() {
        assert_eq!(
            norm(data_dir_for(Os::Windows, os(r"C:\Users\x"), os(r"C:\Users\x\AppData\Roaming"), None)
                .unwrap()),
            "C:/Users/x/AppData/Roaming/GameLife"
        );
    }

    #[test]
    fn windows_falls_back_to_the_profile_directory() {
        assert_eq!(
            norm(data_dir_for(Os::Windows, os(r"C:\Users\x"), None, None).unwrap()),
            "C:/Users/x/AppData/Roaming/GameLife"
        );
        assert_eq!(data_dir_for(Os::Windows, None, None, None), None);
    }

    #[test]
    fn linux_prefers_xdg_data_home_when_absolute() {
        assert_eq!(
            norm(data_dir_for(Os::Linux, os("/home/x"), None, os("/data")).unwrap()),
            "/data/GameLife"
        );
    }

    #[test]
    fn linux_ignores_a_relative_or_empty_xdg_data_home() {
        for xdg in [os("relative/path"), os("")] {
            assert_eq!(
                norm(data_dir_for(Os::Linux, os("/home/x"), None, xdg).unwrap()),
                "/home/x/.local/share/GameLife"
            );
        }
    }

    #[test]
    fn linux_falls_back_to_local_share() {
        assert_eq!(
            norm(data_dir_for(Os::Linux, os("/home/x"), None, None).unwrap()),
            "/home/x/.local/share/GameLife"
        );
        assert_eq!(data_dir_for(Os::Linux, None, None, None), None);
    }

    #[test]
    fn windows_start_gets_an_empty_title_so_the_url_is_not_swallowed() {
        let (program, args) = opener_for(Os::Windows, "https://example.com/auth");
        assert_eq!(program, "cmd");
        assert_eq!(args, vec!["/C", "start", "", "https://example.com/auth"]);
    }

    #[test]
    fn opener_shapes_per_platform() {
        assert_eq!(opener_for(Os::MacOs, "https://x").0, "open");
        assert_eq!(opener_for(Os::Linux, "https://x"), ("xdg-open", vec!["https://x".to_string()]));
    }
}
