use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const CAPTURE_TIMEOUT: Duration = Duration::from_millis(2000);

fn remaining_until(deadline: Instant) -> Result<Duration, ()> {
    let rem = deadline.saturating_duration_since(Instant::now());
    if rem.is_zero() {
        Err(())
    } else {
        Ok(rem)
    }
}

fn recv_until<T>(rx: &mpsc::Receiver<T>, deadline: Instant) -> Result<T, ()> {
    let remaining = remaining_until(deadline)?;
    rx.recv_timeout(remaining).map_err(|_| ())
}

fn drop_or_leak_on_timeout<T>(hold: T, timed_out: bool) {
    if timed_out {
        std::mem::forget(hold);
    }
}

pub fn capture_window(window_id: u32, path: &Path) -> Result<(), ()> {
    #[cfg(target_os = "macos")]
    {
        macos_capture_window(window_id, path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window_id, path);
        Err(())
    }
}

#[cfg(target_os = "macos")]
fn macos_capture_window(window_id: u32, path: &Path) -> Result<(), ()> {
    use std::ffi::CStr;

    use objc2::available;
    use objc2::runtime::AnyClass;
    use objc2::AnyThread;
    use objc2_screen_capture_kit::SCContentFilter;

    if window_id == 0 {
        return Err(());
    }
    if !available!(macos = 14.0) {
        return Err(());
    }
    let name = CStr::from_bytes_with_nul(b"SCScreenshotManager\0").map_err(|_| ())?;
    if AnyClass::get(name).is_none() {
        return Err(());
    }

    let deadline = Instant::now() + CAPTURE_TIMEOUT;
    let content = shareable_content(deadline)?;
    let window = find_window(&content, window_id)?;
    let filter = unsafe {
        SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
    };
    let config = stream_config(&filter, &window);
    let bytes = capture_jpeg(filter, config, deadline)?;
    persist_jpeg(path, &bytes)
}

#[cfg(target_os = "macos")]
fn shareable_content(
    deadline: Instant,
) -> Result<objc2::rc::Retained<objc2_screen_capture_kit::SCShareableContent>, ()> {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2_foundation::NSError;
    use objc2_screen_capture_kit::SCShareableContent;

    remaining_until(deadline)?;
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(
        move |content: *mut SCShareableContent, _err: *mut NSError| {
            let retained = unsafe { Retained::retain(content) };
            let _ = tx.send(retained);
        },
    );
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            true,
            true,
            &block,
        );
    }
    match recv_until(&rx, deadline) {
        Ok(Some(content)) => Ok(content),
        Ok(None) => Err(()),
        Err(()) => {
            drop_or_leak_on_timeout(block, true);
            Err(())
        }
    }
}

#[cfg(target_os = "macos")]
fn find_window(
    content: &objc2_screen_capture_kit::SCShareableContent,
    window_id: u32,
) -> Result<objc2::rc::Retained<objc2_screen_capture_kit::SCWindow>, ()> {
    let windows = unsafe { content.windows() };
    for i in 0..windows.len() {
        let window = windows.objectAtIndex(i);
        if unsafe { window.windowID() } == window_id {
            return Ok(window);
        }
    }
    Err(())
}

#[cfg(target_os = "macos")]
fn stream_config(
    filter: &objc2_screen_capture_kit::SCContentFilter,
    window: &objc2_screen_capture_kit::SCWindow,
) -> objc2::rc::Retained<objc2_screen_capture_kit::SCStreamConfiguration> {
    use objc2_screen_capture_kit::SCStreamConfiguration;

    let config = unsafe { SCStreamConfiguration::new() };
    let frame = unsafe { window.frame() };
    let scale = {
        let s = unsafe { filter.pointPixelScale() } as f64;
        if s.is_finite() && s > 0.0 {
            s
        } else {
            1.0
        }
    };
    let width = (frame.size.width * scale).round().max(1.0) as usize;
    let height = (frame.size.height * scale).round().max(1.0) as usize;
    unsafe {
        config.setWidth(width);
        config.setHeight(height);
        config.setShowsCursor(false);
    }
    config
}

#[cfg(target_os = "macos")]
fn capture_jpeg(
    filter: objc2::rc::Retained<objc2_screen_capture_kit::SCContentFilter>,
    config: objc2::rc::Retained<objc2_screen_capture_kit::SCStreamConfiguration>,
    deadline: Instant,
) -> Result<Vec<u8>, ()> {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_core_foundation::CFRetained;
    use objc2_core_graphics::CGImage;
    use objc2_foundation::NSError;
    use objc2_screen_capture_kit::SCScreenshotManager;

    remaining_until(deadline)?;
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |image: *mut CGImage, _err: *mut NSError| {
        let retained = NonNull::new(image).map(|ptr| unsafe { CFRetained::retain(ptr) });
        let _ = tx.send(retained);
    });
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &config,
            Some(&block),
        );
    }
    match recv_until(&rx, deadline) {
        Ok(Some(img)) => jpeg_bytes(&img),
        Ok(None) => Err(()),
        Err(()) => {
            drop_or_leak_on_timeout((block, filter, config), true);
            Err(())
        }
    }
}

#[cfg(target_os = "macos")]
fn jpeg_bytes(image: &objc2_core_graphics::CGImage) -> Result<Vec<u8>, ()> {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey};
    use objc2_foundation::NSDictionary;

    let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), image);
    let props: Retained<NSDictionary<NSBitmapImageRepPropertyKey, AnyObject>> = NSDictionary::new();
    let data =
        unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::JPEG, &props) }
            .ok_or(())?;
    let bytes = data.to_vec();
    if bytes.is_empty() {
        Err(())
    } else {
        Ok(bytes)
    }
}

#[cfg(target_os = "macos")]
fn persist_jpeg(path: &Path, bytes: &[u8]) -> Result<(), ()> {
    if bytes.is_empty() {
        return Err(());
    }
    if std::fs::write(path, bytes).is_err() {
        let _ = std::fs::remove_file(path);
        return Err(());
    }
    let empty = std::fs::metadata(path)
        .map(|m| m.len() == 0)
        .unwrap_or(true);
    if empty || image::open(path).is_err() {
        let _ = std::fs::remove_file(path);
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        capture_window, drop_or_leak_on_timeout, recv_until, remaining_until, CAPTURE_TIMEOUT,
    };
    #[allow(unused_imports)]
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn missing_file_after_error_is_ok_to_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.jpg");
        let _ = capture_window(0, &path);
        assert!(!path.exists() || std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) == 0);
    }

    #[test]
    fn capture_timeout_budget_is_2000ms() {
        assert_eq!(CAPTURE_TIMEOUT, Duration::from_millis(2000));
    }

    #[test]
    fn remaining_until_is_err_when_deadline_has_passed() {
        let deadline = Instant::now();
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(remaining_until(deadline), Err(()));
    }

    #[test]
    fn recv_until_returns_err_immediately_when_no_time_left() {
        let (_tx, rx) = mpsc::channel::<i32>();
        let deadline = Instant::now();
        std::thread::sleep(Duration::from_millis(2));
        let start = Instant::now();
        assert!(recv_until(&rx, deadline).is_err());
        assert!(start.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn second_recv_uses_remaining_deadline_not_full_timeout() {
        let deadline = Instant::now() + Duration::from_millis(80);
        let (tx, rx) = mpsc::channel();
        std::thread::sleep(Duration::from_millis(40));
        tx.send(1).unwrap();
        assert_eq!(recv_until(&rx, deadline), Ok(1));

        let (_tx2, rx2) = mpsc::channel::<i32>();
        let start = Instant::now();
        assert!(recv_until(&rx2, deadline).is_err());
        assert!(
            start.elapsed() < Duration::from_millis(80),
            "second wait must use leftover budget, got {:?}",
            start.elapsed()
        );
        assert!(start.elapsed() < CAPTURE_TIMEOUT / 4);
    }

    struct DropFlag<'a>(&'a AtomicBool);

    impl Drop for DropFlag<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn leak_on_timeout_does_not_drop_hold() {
        let dropped = AtomicBool::new(false);
        drop_or_leak_on_timeout(DropFlag(&dropped), true);
        assert!(
            !dropped.load(Ordering::SeqCst),
            "timeout must leak hold so a late SCK callback cannot UAF"
        );
    }

    #[test]
    fn drop_on_success_runs_drop() {
        let dropped = AtomicBool::new(false);
        drop_or_leak_on_timeout(DropFlag(&dropped), false);
        assert!(dropped.load(Ordering::SeqCst));
    }
}
