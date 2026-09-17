//! Login-item registration via `SMAppService.mainApp`.
//! `tauri dev` / `cargo test` live outside a `.app` bundle and must no-op.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginItemStatus {
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginItemOp {
    None,
    Register,
    Unregister,
}

/// Decide the OS call from the Settings toggle and the current SMAppService
/// status. Already-enabled stays put. Pending approval is not re-registered
/// and does not nag on launch.
pub fn login_item_op(want: bool, status: LoginItemStatus) -> LoginItemOp {
    match (want, status) {
        (true, LoginItemStatus::Enabled | LoginItemStatus::RequiresApproval) => LoginItemOp::None,
        (true, _) => LoginItemOp::Register,
        (false, LoginItemStatus::NotRegistered | LoginItemStatus::NotFound) => LoginItemOp::None,
        (false, _) => LoginItemOp::Unregister,
    }
}

pub fn should_open_login_items(after_register: LoginItemStatus) -> bool {
    after_register == LoginItemStatus::RequiresApproval
}

pub(crate) fn status_from_code(code: isize) -> LoginItemStatus {
    match code {
        1 => LoginItemStatus::Enabled,
        2 => LoginItemStatus::RequiresApproval,
        3 => LoginItemStatus::NotFound,
        _ => LoginItemStatus::NotRegistered,
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{
        login_item_op, should_open_login_items, status_from_code, LoginItemOp, LoginItemStatus,
    };
    use crate::macos::notify::bundle_supports_user_notifications;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2::msg_send;
    use objc2_foundation::NSBundle;
    use std::ffi::CStr;

    #[link(name = "ServiceManagement", kind = "framework")]
    extern "C" {}

    fn bundled() -> bool {
        let bundle = NSBundle::mainBundle();
        let identifier = bundle.bundleIdentifier().map(|s| s.to_string());
        let url = bundle
            .bundleURL()
            .absoluteString()
            .map(|s| s.to_string())
            .unwrap_or_default();
        bundle_supports_user_notifications(identifier.as_deref(), &url)
    }

    fn service_class() -> Option<&'static AnyClass> {
        AnyClass::get(CStr::from_bytes_with_nul(b"SMAppService\0").ok()?)
    }

    fn with_service(f: impl FnOnce(*mut AnyObject) + std::panic::UnwindSafe) {
        if !bundled() {
            return;
        }
        let Some(cls) = service_class() else {
            eprintln!("login item: SMAppService unavailable");
            return;
        };
        if let Err(exc) = objc2::exception::catch(|| unsafe {
            let service: *mut AnyObject = msg_send![cls, mainApp];
            if !service.is_null() {
                f(service);
            }
        }) {
            eprintln!("login item: SMAppService threw: {exc:?}");
        }
    }

    fn read_status(service: *mut AnyObject) -> LoginItemStatus {
        let code: isize = unsafe { msg_send![service, status] };
        status_from_code(code)
    }

    fn register(service: *mut AnyObject) {
        unsafe {
            let mut error: *mut AnyObject = std::ptr::null_mut();
            let _ok: Bool = msg_send![service, registerAndReturnError: &mut error];
        }
    }

    fn unregister(service: *mut AnyObject) {
        unsafe {
            let mut error: *mut AnyObject = std::ptr::null_mut();
            let _ok: Bool = msg_send![service, unregisterAndReturnError: &mut error];
        }
    }

    fn open_login_items() {
        let Some(cls) = service_class() else {
            return;
        };
        if let Err(exc) = objc2::exception::catch(|| unsafe {
            let _: () = msg_send![cls, openSystemSettingsLoginItems];
        }) {
            eprintln!("login item: openSystemSettingsLoginItems threw: {exc:?}");
        }
    }

    pub fn apply(enabled: bool) {
        with_service(|service| {
            match login_item_op(enabled, read_status(service)) {
                LoginItemOp::None => {}
                LoginItemOp::Register => {
                    register(service);
                    if should_open_login_items(read_status(service)) {
                        open_login_items();
                    }
                }
                LoginItemOp::Unregister => unregister(service),
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn apply(_enabled: bool) {}
}

pub fn apply_login_at_startup(enabled: bool) {
    imp::apply(enabled);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_item_op_matches_intent_without_nagging() {
        use LoginItemOp as Op;
        use LoginItemStatus as St;
        let cases = [
            (true, St::NotRegistered, Op::Register),
            (true, St::NotFound, Op::Register),
            (true, St::Enabled, Op::None),
            (true, St::RequiresApproval, Op::None),
            (false, St::Enabled, Op::Unregister),
            (false, St::RequiresApproval, Op::Unregister),
            (false, St::NotRegistered, Op::None),
            (false, St::NotFound, Op::None),
        ];
        for (want, status, op) in cases {
            assert_eq!(
                login_item_op(want, status),
                op,
                "want={want} status={status:?}"
            );
        }
    }

    #[test]
    fn pending_approval_after_register_opens_login_items() {
        assert!(should_open_login_items(LoginItemStatus::RequiresApproval));
        assert!(!should_open_login_items(LoginItemStatus::Enabled));
        assert!(!should_open_login_items(LoginItemStatus::NotRegistered));
        assert!(!should_open_login_items(LoginItemStatus::NotFound));
    }

    #[test]
    fn status_from_code_matches_smappservice_enum() {
        assert_eq!(status_from_code(0), LoginItemStatus::NotRegistered);
        assert_eq!(status_from_code(1), LoginItemStatus::Enabled);
        assert_eq!(status_from_code(2), LoginItemStatus::RequiresApproval);
        assert_eq!(status_from_code(3), LoginItemStatus::NotFound);
        assert_eq!(status_from_code(-1), LoginItemStatus::NotRegistered);
    }

    #[test]
    fn unpackaged_process_does_not_abort_on_login_item_api() {
        apply_login_at_startup(true);
        apply_login_at_startup(false);
    }
}
