use std::path::Path;
use std::time::Duration;

pub const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);

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

    let content = shareable_content()?;
    let window = find_window(&content, window_id)?;
    let filter = unsafe {
        SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
    };
    let config = stream_config(&filter, &window);
    let bytes = capture_jpeg(&filter, &config)?;
    persist_jpeg(path, &bytes)
}

#[cfg(target_os = "macos")]
fn shareable_content(
) -> Result<objc2::rc::Retained<objc2_screen_capture_kit::SCShareableContent>, ()> {
    use std::sync::mpsc;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2_foundation::NSError;
    use objc2_screen_capture_kit::SCShareableContent;

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
    rx.recv_timeout(CAPTURE_TIMEOUT).map_err(|_| ())?.ok_or(())
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
    filter: &objc2_screen_capture_kit::SCContentFilter,
    config: &objc2_screen_capture_kit::SCStreamConfiguration,
) -> Result<Vec<u8>, ()> {
    use std::ptr::NonNull;
    use std::sync::mpsc;

    use block2::RcBlock;
    use objc2_core_foundation::CFRetained;
    use objc2_core_graphics::CGImage;
    use objc2_foundation::NSError;
    use objc2_screen_capture_kit::SCScreenshotManager;

    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |image: *mut CGImage, _err: *mut NSError| {
        let result = NonNull::new(image).ok_or(()).and_then(|ptr| {
            let img = unsafe { CFRetained::retain(ptr) };
            jpeg_bytes(&img)
        });
        let _ = tx.send(result);
    });
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            filter,
            config,
            Some(&block),
        );
    }
    rx.recv_timeout(CAPTURE_TIMEOUT).map_err(|_| ())?
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
    use super::capture_window;
    #[allow(unused_imports)]
    use std::path::Path;

    #[test]
    fn missing_file_after_error_is_ok_to_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.jpg");
        let _ = capture_window(0, &path);
        assert!(!path.exists() || std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) == 0);
    }
}
