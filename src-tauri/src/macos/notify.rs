//! Optional task banners via `UNUserNotificationCenter`.
//! Windows / Linux keep the same API as a no-op.

#[derive(Clone, Debug)]
pub struct PendingTaskNotification {
    pub identifier: String,
    pub title: String,
    pub body: String,
    pub fire_at: i64,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::PendingTaskNotification;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;

    #[link(name = "UserNotifications", kind = "framework")]
    extern "C" {}

    fn now_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    unsafe fn center() -> *mut AnyObject {
        let cls = class!(UNUserNotificationCenter);
        msg_send![cls, currentNotificationCenter]
    }

    pub fn request_authorization() {
        unsafe {
            let center = center();
            if center.is_null() {
                return;
            }
            // UNAuthorizationOptionAlert
            let options: u64 = 1 << 2;
            let nil: *mut AnyObject = std::ptr::null_mut();
            let _: () = msg_send![
                center,
                requestAuthorizationWithOptions: options,
                completionHandler: nil
            ];
        }
    }

    pub fn cancel_all_task_notifications() {
        unsafe {
            let center = center();
            if center.is_null() {
                return;
            }
            let _: () = msg_send![center, removeAllPendingNotificationRequests];
        }
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
        unsafe {
            let center = center();
            if center.is_null() {
                return;
            }
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
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::PendingTaskNotification;

    pub fn request_authorization() {}
    pub fn cancel_all_task_notifications() {}
    pub fn replace_task_notifications(_items: &[PendingTaskNotification]) {}
}

pub use imp::{
    cancel_all_task_notifications, replace_task_notifications, request_authorization,
};
