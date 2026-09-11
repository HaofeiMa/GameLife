#[cfg(target_os = "macos")]
mod imp {
    use std::process::Command;

    pub fn idle_seconds() -> i64 {
        extern "C" {
            fn CGEventSourceSecondsSinceLastEventType(state_id: u32, event_type: u32) -> f64;
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
    pub fn idle_seconds() -> i64 {
        0
    }

    pub fn screen_locked() -> bool {
        false
    }

    pub fn secure_input_on() -> bool {
        false
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

pub fn idle_seconds() -> i64 {
    imp::idle_seconds()
}

pub fn screen_locked() -> bool {
    imp::screen_locked()
}

pub fn secure_input_on() -> bool {
    imp::secure_input_on()
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
