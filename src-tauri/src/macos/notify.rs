//! Optional task banners via `UNUserNotificationCenter`.
//! Windows / Linux keep the same API as a no-op.

#[derive(Clone, Debug)]
pub struct PendingTaskNotification {
    pub identifier: String,
    pub title: String,
    pub body: String,
    pub fire_at: i64,
}

/// Apple aborts with `bundleProxyForCurrentProcess is nil` unless this process
/// is a real `.app` (identifier + bundle URL). `tauri dev` / `cargo test` live
/// in `target/debug/` and must no-op.
pub(crate) fn bundle_supports_user_notifications(
    identifier: Option<&str>,
    bundle_url: &str,
) -> bool {
    if identifier
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return false;
    }
    let path = bundle_url.trim_end_matches('/');
    path.ends_with(".app") || path.contains(".app/")
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{bundle_supports_user_notifications, PendingTaskNotification};
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::{NSBundle, NSString};

    #[link(name = "UserNotifications", kind = "framework")]
    extern "C" {}

    fn now_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    fn user_notifications_supported() -> bool {
        let bundle = NSBundle::mainBundle();
        let identifier = bundle.bundleIdentifier().map(|s| s.to_string());
        let url = bundle
            .bundleURL()
            .absoluteString()
            .map(|s| s.to_string())
            .unwrap_or_default();
        bundle_supports_user_notifications(identifier.as_deref(), &url)
    }

    fn with_center(f: impl FnOnce(*mut AnyObject) + std::panic::UnwindSafe) {
        if !user_notifications_supported() {
            return;
        }
        if let Err(exc) = objc2::exception::catch(|| unsafe {
            let cls = class!(UNUserNotificationCenter);
            let center: *mut AnyObject = msg_send![cls, currentNotificationCenter];
            if !center.is_null() {
                f(center);
            }
        }) {
            eprintln!("task notify: UserNotifications threw: {exc:?}");
        }
    }

    pub fn request_authorization() {
        with_center(|center| unsafe {
            // UNAuthorizationOptionAlert
            let options: u64 = 1 << 2;
            let nil: *mut AnyObject = std::ptr::null_mut();
            let _: () = msg_send![
                center,
                requestAuthorizationWithOptions: options,
                completionHandler: nil
            ];
        });
    }

    pub fn cancel_all_task_notifications() {
        with_center(|center| unsafe {
            let _: () = msg_send![center, removeAllPendingNotificationRequests];
        });
    }

    pub fn replace_task_notifications(items: &[PendingTaskNotification]) {
        cancel_all_task_notifications();
        let now = now_secs();
        for item in items {
            if let Err(e) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| schedule(item, now)))
            {
                eprintln!("task notify schedule panicked: {e:?}");
            }
        }
    }

    fn schedule(item: &PendingTaskNotification, now: i64) {
        let interval = (item.fire_at - now).max(1) as f64;
        with_center(|center| unsafe {
            let content: *mut AnyObject = msg_send![class!(UNMutableNotificationContent), new];
            if content.is_null() {
                return;
            }
            let title = NSString::from_str(&item.title);
            let body = NSString::from_str(&item.body);
            let _: () = msg_send![content, setTitle: &*title];
            let _: () = msg_send![content, setBody: &*body];
            let trigger: *mut AnyObject = msg_send![
                class!(UNTimeIntervalNotificationTrigger),
                triggerWithTimeInterval: interval,
                repeats: false
            ];
            let ident = NSString::from_str(&item.identifier);
            let req: *mut AnyObject = msg_send![
                class!(UNNotificationRequest),
                requestWithIdentifier: &*ident,
                content: content,
                trigger: trigger
            ];
            if req.is_null() {
                return;
            }
            let nil: *mut AnyObject = std::ptr::null_mut();
            let _: () = msg_send![
                center,
                addNotificationRequest: req,
                withCompletionHandler: nil
            ];
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::PendingTaskNotification;

    pub fn request_authorization() {}
    pub fn cancel_all_task_notifications() {}
    pub fn replace_task_notifications(_items: &[PendingTaskNotification]) {}
}

pub use imp::{cancel_all_task_notifications, replace_task_notifications, request_authorization};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpackaged_debug_binary_cannot_use_user_notifications() {
        // Crash report: mainBundle.bundleURL file:///…/GameLife/target/debug/
        assert!(!bundle_supports_user_notifications(
            None,
            "file:///Volumes/MobileSSD/MyProjects/GameLife/target/debug/"
        ));
        assert!(!bundle_supports_user_notifications(
            Some(""),
            "file:///Volumes/MobileSSD/MyProjects/GameLife/target/debug/"
        ));
        assert!(!bundle_supports_user_notifications(
            Some("ma.haofei.gamelife"),
            "file:///Volumes/MobileSSD/MyProjects/GameLife/target/debug/"
        ));
    }

    #[test]
    fn packaged_app_can_use_user_notifications() {
        assert!(bundle_supports_user_notifications(
            Some("ma.haofei.gamelife"),
            "file:///Applications/GameLife.app/"
        ));
        assert!(bundle_supports_user_notifications(
            Some("ma.haofei.gamelife"),
            "/Applications/GameLife.app"
        ));
    }

    #[test]
    fn unpackaged_process_does_not_abort_on_notification_apis() {
        request_authorization();
        cancel_all_task_notifications();
        replace_task_notifications(&[]);
        replace_task_notifications(&[PendingTaskNotification {
            identifier: "gamelife-task-test-0".into(),
            title: "test".into(),
            body: "准时".into(),
            fire_at: 1,
        }]);
    }
}
