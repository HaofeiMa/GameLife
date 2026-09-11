#[cfg(target_os = "macos")]
mod imp {
    use std::process::Command;

    pub fn frontmost_app() -> Result<(String, String), ()> {
        let script = r#"
            tell application "System Events"
                set p to first application process whose frontmost is true
                set appName to name of p
                try
                    set t to name of front window of p
                on error
                    set t to ""
                end try
                return appName & "\t" & t
            end tell
        "#;
        let out = Command::new("osascript").args(["-e", script]).output().map_err(|_| ())?;
        if !out.status.success() {
            return Err(());
        }
        let s = String::from_utf8_lossy(&out.stdout);
        let mut parts = s.trim().splitn(2, '\t');
        let app = parts.next().unwrap_or("").to_string();
        let title = parts.next().unwrap_or("").to_string();
        if app.is_empty() {
            return Err(());
        }
        Ok((app, title))
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
        None
    }

    pub fn bundle_id() -> Option<String> {
        None
    }

    pub fn capture_context() -> gamelife_core::CaptureContext {
        super::empty_capture_context()
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
}

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

pub fn metadata_observation_available() -> bool {
    accessibility_granted()
}

pub fn capture_observation_available() -> bool {
    screen_recording_granted()
}

pub fn observation_available() -> bool {
    metadata_observation_available()
}
