use super::{ax_document_raw, pick_front_window_id, CgWindowEntry, FrontmostSnapshot};

pub const AX_TIMEOUT_SECS: f32 = 0.4;

pub fn snapshot() -> FrontmostSnapshot {
    #[cfg(target_os = "macos")]
    {
        macos_snapshot()
    }
    #[cfg(not(target_os = "macos"))]
    {
        FrontmostSnapshot::default()
    }
}

#[cfg(target_os = "macos")]
fn macos_snapshot() -> FrontmostSnapshot {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSWorkspace;

    autoreleasepool(|_| {
        let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
            return FrontmostSnapshot::default();
        };
        let name = app
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let bundle_id = app
            .bundleIdentifier()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        let pid = app.processIdentifier();
        let pid_i32 = if pid > 0 { Some(pid as i32) } else { None };

        let (title, document_raw) = match pid_i32 {
            Some(pid) => ax_title_and_document(pid),
            None => (String::new(), None),
        };
        let cg_window_id = pid_i32.and_then(|pid| {
            let entries = cg_window_entries();
            pick_front_window_id(pid, &entries)
        });

        FrontmostSnapshot {
            app: name,
            title,
            bundle_id,
            document_raw,
            pid: pid_i32,
            cg_window_id,
        }
    })
}

#[cfg(target_os = "macos")]
fn ax_title_and_document(pid: i32) -> (String, Option<String>) {
    let Some(app_el) = ax_app_element(pid) else {
        return (String::new(), None);
    };
    unsafe {
        AXUIElementSetMessagingTimeout(app_el.as_ptr(), AX_TIMEOUT_SECS);
    }
    let win = ax_copy_attr(app_el.as_ptr(), "AXFocusedWindow")
        .or_else(|| ax_copy_attr(app_el.as_ptr(), "AXMainWindow"));
    let Some(win) = win else {
        return (String::new(), None);
    };
    unsafe {
        AXUIElementSetMessagingTimeout(win.as_ptr(), AX_TIMEOUT_SECS);
    }
    let title = ax_copy_string(win.as_ptr(), "AXTitle").unwrap_or_default();
    let document = ax_copy_string(win.as_ptr(), "AXDocument");
    let url = ax_copy_string(win.as_ptr(), "AXURL");
    let document_raw = ax_document_raw(document.as_deref(), url.as_deref());
    (title, document_raw)
}

#[cfg(target_os = "macos")]
fn ax_app_element(pid: i32) -> Option<CfOwned> {
    CfOwned::from_create(unsafe { AXUIElementCreateApplication(pid) })
}

#[cfg(target_os = "macos")]
fn ax_copy_attr(element: *mut std::ffi::c_void, name: &str) -> Option<CfOwned> {
    let attr = cfstring(name)?;
    let mut value: *mut std::ffi::c_void = std::ptr::null_mut();
    let err = unsafe { AXUIElementCopyAttributeValue(element, attr.as_ptr(), &mut value) };
    if err != 0 {
        None
    } else {
        CfOwned::from_create(value)
    }
}

#[cfg(target_os = "macos")]
fn ax_copy_string(element: *mut std::ffi::c_void, name: &str) -> Option<String> {
    let value = ax_copy_attr(element, name)?;
    cf_to_string(value.as_ptr())
}

#[cfg(target_os = "macos")]
fn cg_window_entries() -> Vec<CgWindowEntry> {
    const ON_SCREEN_ONLY: u32 = 1 << 0;
    const EXCLUDE_DESKTOP: u32 = 1 << 4;
    let arr = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0) };
    let Some(arr) = CfOwned::from_create(arr) else {
        return Vec::new();
    };
    let count = unsafe { CFArrayGetCount(arr.as_ptr()) };
    let mut out = Vec::with_capacity(count.max(0) as usize);
    for i in 0..count {
        let dict = unsafe { CFArrayGetValueAtIndex(arr.as_ptr(), i) };
        if dict.is_null() {
            continue;
        }
        let Some(owner_pid) = cfdict_i64(dict, unsafe { kCGWindowOwnerPID }).map(|v| v as i32)
        else {
            continue;
        };
        let layer = cfdict_i64(dict, unsafe { kCGWindowLayer }).unwrap_or(0);
        let Some(window_id) = cfdict_i64(dict, unsafe { kCGWindowNumber }).map(|v| v as u32) else {
            continue;
        };
        let on_screen = cfdict_bool(dict, unsafe { kCGWindowIsOnscreen }).unwrap_or(true);
        out.push(CgWindowEntry {
            owner_pid,
            layer,
            window_id,
            on_screen,
        });
    }
    out
}

#[cfg(target_os = "macos")]
fn cfstring(name: &str) -> Option<CfOwned> {
    let cstr = std::ffi::CString::new(name).ok()?;
    CfOwned::from_create(unsafe {
        CFStringCreateWithCString(std::ptr::null(), cstr.as_ptr(), K_CF_STRING_ENCODING_UTF8)
    })
}

#[cfg(target_os = "macos")]
fn cf_to_string(cf: *mut std::ffi::c_void) -> Option<String> {
    if cf.is_null() {
        return None;
    }
    unsafe {
        let tid = CFGetTypeID(cf);
        let s = if tid == CFStringGetTypeID() {
            cf
        } else if tid == CFURLGetTypeID() {
            let s = CFURLGetString(cf);
            if s.is_null() {
                return None;
            }
            s
        } else {
            return None;
        };
        cfstring_to_string(s)
    }
}

#[cfg(target_os = "macos")]
fn cfstring_to_string(s: *mut std::ffi::c_void) -> Option<String> {
    unsafe {
        let len = CFStringGetLength(s);
        let max = CFStringGetMaximumSizeForEncoding(len, K_CF_STRING_ENCODING_UTF8);
        if max < 0 {
            return None;
        }
        let mut buf = vec![0u8; (max as usize).saturating_add(1)];
        let ok = CFStringGetCString(
            s,
            buf.as_mut_ptr().cast(),
            buf.len() as isize,
            K_CF_STRING_ENCODING_UTF8,
        );
        if ok == 0 {
            return None;
        }
        let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8(buf[..nul].to_vec()).ok()
    }
}

#[cfg(target_os = "macos")]
fn cfdict_i64(dict: *const std::ffi::c_void, key: *const std::ffi::c_void) -> Option<i64> {
    let value = unsafe { CFDictionaryGetValue(dict, key) };
    cf_number_i64(value)
}

#[cfg(target_os = "macos")]
fn cfdict_bool(dict: *const std::ffi::c_void, key: *const std::ffi::c_void) -> Option<bool> {
    let value = unsafe { CFDictionaryGetValue(dict, key) };
    if value.is_null() {
        return None;
    }
    unsafe {
        if CFGetTypeID(value) != CFBooleanGetTypeID() {
            return None;
        }
        Some(CFBooleanGetValue(value) != 0)
    }
}

#[cfg(target_os = "macos")]
fn cf_number_i64(cf: *const std::ffi::c_void) -> Option<i64> {
    if cf.is_null() {
        return None;
    }
    unsafe {
        if CFGetTypeID(cf) != CFNumberGetTypeID() {
            return None;
        }
        let mut v: i64 = 0;
        if CFNumberGetValue(cf, K_CF_NUMBER_SINT64_TYPE, (&mut v as *mut i64).cast()) != 0 {
            return Some(v);
        }
        let mut v32: i32 = 0;
        if CFNumberGetValue(cf, K_CF_NUMBER_SINT32_TYPE, (&mut v32 as *mut i32).cast()) != 0 {
            return Some(v32 as i64);
        }
        None
    }
}

#[cfg(target_os = "macos")]
struct CfOwned(*mut std::ffi::c_void);

#[cfg(target_os = "macos")]
impl CfOwned {
    fn from_create(p: *mut std::ffi::c_void) -> Option<Self> {
        if p.is_null() {
            None
        } else {
            Some(Self(p))
        }
    }

    fn as_ptr(&self) -> *mut std::ffi::c_void {
        self.0
    }
}

#[cfg(target_os = "macos")]
impl Drop for CfOwned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

#[cfg(target_os = "macos")]
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
#[cfg(target_os = "macos")]
const K_CF_NUMBER_SINT32_TYPE: isize = 3;
#[cfg(target_os = "macos")]
const K_CF_NUMBER_SINT64_TYPE: isize = 4;

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> *mut std::ffi::c_void;
    fn AXUIElementSetMessagingTimeout(element: *mut std::ffi::c_void, timeout_in_seconds: f32);
    fn AXUIElementCopyAttributeValue(
        element: *mut std::ffi::c_void,
        attribute: *const std::ffi::c_void,
        value: *mut *mut std::ffi::c_void,
    ) -> i32;

    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32)
        -> *mut std::ffi::c_void;
    static kCGWindowOwnerPID: *const std::ffi::c_void;
    static kCGWindowLayer: *const std::ffi::c_void;
    static kCGWindowNumber: *const std::ffi::c_void;
    static kCGWindowIsOnscreen: *const std::ffi::c_void;

    fn CFRelease(cf: *const std::ffi::c_void);
    fn CFGetTypeID(cf: *const std::ffi::c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFURLGetTypeID() -> usize;
    fn CFStringCreateWithCString(
        alloc: *const std::ffi::c_void,
        c_str: *const std::ffi::c_char,
        encoding: u32,
    ) -> *mut std::ffi::c_void;
    fn CFStringGetLength(s: *const std::ffi::c_void) -> isize;
    fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    fn CFStringGetCString(
        s: *const std::ffi::c_void,
        buffer: *mut std::ffi::c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;
    fn CFArrayGetCount(arr: *const std::ffi::c_void) -> isize;
    fn CFArrayGetValueAtIndex(arr: *const std::ffi::c_void, idx: isize) -> *const std::ffi::c_void;
    fn CFDictionaryGetValue(
        dict: *const std::ffi::c_void,
        key: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn CFNumberGetValue(
        number: *const std::ffi::c_void,
        the_type: isize,
        value_ptr: *mut std::ffi::c_void,
    ) -> u8;
    fn CFBooleanGetValue(boolean: *const std::ffi::c_void) -> u8;
    fn CFURLGetString(url: *const std::ffi::c_void) -> *mut std::ffi::c_void;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macos::{pick_front_window_id, CgWindowEntry};

    #[test]
    fn snapshot_maps_window_list_through_picker() {
        let pid = 11;
        let entries = [CgWindowEntry {
            owner_pid: pid,
            layer: 0,
            window_id: 42,
            on_screen: true,
        }];
        assert_eq!(pick_front_window_id(pid, &entries), Some(42));
    }

    #[test]
    #[ignore]
    fn snapshot_does_not_panic() {
        let _ = snapshot();
    }
}
