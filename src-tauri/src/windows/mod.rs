//! Windows observation backend.
//!
//! The same surface as `macos/` (the required list is in `observe/mod.rs`),
//! built on Win32: the foreground window and its title, the owning process's
//! image name, the keyboard/mouse idle timer, the workstation lock state, and a
//! GDI capture of the window.
//!
//! What Windows genuinely cannot answer, and what is reported instead — every
//! one of these is an empty answer rather than a fabricated one:
//!
//! * **No bundle id.** Nothing identifies an app the way `NSWorkspace` does, so
//!   `bundle_id` stays `None` and `app` carries the executable's file name
//!   (`chrome.exe`). The rule tables in `hint` match on macOS display names, so
//!   they need Windows entries before judgement gets sharp here — that is a
//!   product change, not a backend one.
//! * **No document path.** macOS reads `AXDocument`; the Windows equivalent is
//!   UI Automation's `ValuePattern`, which is not wired up yet. The file name
//!   therefore has to come from the window title.
//! * **No secure-input flag.** Nothing exposes "a password field has focus".
//! * **No permission model.** There is nothing to grant, so the predicates
//!   report granted and `request_screen_recording` is a no-op; the UI's
//!   permission banner stays hidden, which is the correct thing on Windows.
//! * **No browser tab URL.** Reading a browser's current tab needs per-browser
//!   automation, so this is `None` — the same degradation macOS has when the
//!   Automation permission is refused, and sampling is unaffected.

use std::path::Path;

use crate::observe::state::FrontmostSnapshot;

/// A window smaller than this is a tooltip or a stub, not something worth
/// capturing.
const MIN_CAPTURE_SIDE: i32 = 8;

/// `PrintWindow`'s "ask the app to render itself, GPU content included" flag.
/// Not exported by `windows-sys`, so it is spelled out here.
const PW_RENDERFULLCONTENT: u32 = 0x0000_0002;

/// JPEG quality for a stored capture. `vision.rs` re-encodes before upload, so
/// this only has to be good enough to look at.
const JPEG_QUALITY: u8 = 85;

/// The foreground window, or an empty snapshot when there is none (the desktop
/// itself is focused, a secure desktop is up, or the call fails).
pub fn snapshot() -> FrontmostSnapshot {
    let Some(hwnd) = ffi::foreground_window() else {
        return FrontmostSnapshot::default();
    };
    if !ffi::is_visible(hwnd) {
        return FrontmostSnapshot::default();
    }

    let pid = ffi::window_pid(hwnd);
    FrontmostSnapshot {
        app: pid
            .and_then(ffi::process_image_name)
            .map(|image| image_display_name(&image))
            .unwrap_or_default(),
        title: ffi::window_title(hwnd),
        // See the module header: neither of these has a Windows answer yet.
        bundle_id: None,
        document_raw: None,
        pid: pid.map(|p| p as i32),
        window_id: Some(hwnd as u64),
    }
}

/// `chrome.exe` → `chrome`. The extension is noise in a UI that shows the app
/// name, and the raw `Path` API handles the separators for us.
fn image_display_name(image: &str) -> String {
    Path::new(image)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| image.to_string())
}

pub fn frontmost_app() -> Result<(String, String), ()> {
    let snap = snapshot();
    if snap.app.is_empty() {
        Err(())
    } else {
        Ok((snap.app, snap.title))
    }
}

pub fn document_path() -> Option<String> {
    None
}

pub fn bundle_id() -> Option<String> {
    None
}

pub fn idle_seconds() -> i64 {
    ffi::idle_seconds()
}

pub fn screen_locked() -> bool {
    ffi::screen_locked()
}

pub fn secure_input_on() -> bool {
    false
}

/// macOS's "is this app a browser whose current tab we can read?" — not a thing
/// here, so the fetch closure is never called.
pub fn url_for(
    _bundle_id: Option<&str>,
    _app: &str,
    _fetch: impl FnOnce() -> Option<String>,
) -> Option<String> {
    None
}

pub fn fetch_browser_url_for(_bundle_id: Option<&str>, _app: &str) -> Option<String> {
    None
}

pub fn capture_window(window_id: u64, path: &Path) -> Result<(), ()> {
    ffi::capture_window(window_id as usize, path)
}

pub fn capture_context() -> gamelife_core::CaptureContext {
    let snap = snapshot();
    gamelife_core::CaptureContext {
        app: snap.app,
        bundle_id: snap.bundle_id,
        title: snap.title,
        document_path: None,
        url: None,
        secure_input: secure_input_on(),
    }
}

pub fn metadata_observation_available() -> bool {
    true
}

pub fn capture_observation_available() -> bool {
    true
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

pub fn current_process_label() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "GameLife".into())
}

pub fn current_process_path() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

mod ffi {
    use std::path::Path;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HBITMAP, HDC, HGDIOBJ, SRCCOPY,
    };
    use windows_sys::Win32::System::StationsAndDesktops::{
        CloseDesktop, OpenInputDesktop, DESKTOP_READOBJECTS, DESKTOP_SWITCHDESKTOP,
    };
    // `PrintWindow` really does live under Storage::Xps in windows-sys; it is
    // the only way to capture a hardware-composited surface (a browser or
    // Electron window BitBlt'd from its own DC comes back black).
    use windows_sys::Win32::Storage::Xps::PrintWindow;
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible,
    };

    use super::{MIN_CAPTURE_SIDE, PW_RENDERFULLCONTENT};

    /// Long enough for the extended-length path limit Windows allows.
    const PATH_UNITS: usize = 32_768;
    const TITLE_UNITS: usize = 512;

    pub(super) fn foreground_window() -> Option<HWND> {
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.is_null()).then_some(hwnd)
    }

    pub(super) fn is_visible(hwnd: HWND) -> bool {
        unsafe { IsWindowVisible(hwnd) != 0 }
    }

    /// UTF-16 up to the first NUL.
    fn wide_string(units: &[u16]) -> String {
        let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        String::from_utf16_lossy(&units[..end])
    }

    pub(super) fn window_title(hwnd: HWND) -> String {
        let mut buf = vec![0u16; TITLE_UNITS];
        let written = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), TITLE_UNITS as i32) };
        if written <= 0 {
            return String::new();
        }
        wide_string(&buf[..written as usize])
    }

    pub(super) fn window_pid(hwnd: HWND) -> Option<u32> {
        let mut pid = 0u32;
        let thread = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        (thread != 0 && pid != 0).then_some(pid)
    }

    /// The full path of the process's image, e.g. `C:\…\chrome.exe`.
    pub(super) fn process_image_name(pid: u32) -> Option<String> {
        let handle: HANDLE = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            // Without elevation this fails for some system processes; that is a
            // missing app name, not an error worth surfacing.
            return None;
        }
        let mut buf = vec![0u16; PATH_UNITS];
        let mut size = PATH_UNITS as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) };
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            return None;
        }
        let path = wide_string(&buf[..size as usize]);
        Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }

    /// Milliseconds since the last keyboard or mouse input, in seconds.
    ///
    /// `GetLastInputInfo` reports a 32-bit tick count, so the subtraction has to
    /// wrap: the counter rolls over after ~49.7 days of uptime and a plain `-`
    /// would read as a huge negative idle time.
    pub(super) fn idle_seconds() -> i64 {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if unsafe { GetLastInputInfo(&mut info) } == 0 {
            return 0;
        }
        let now = unsafe { GetTickCount() };
        let elapsed_ms = now.wrapping_sub(info.dwTime);
        (elapsed_ms / 1000) as i64
    }

    /// Whether the workstation is locked.
    ///
    /// The input desktop can only be opened while it is the one being displayed,
    /// so a failure here means a locked (or otherwise switched-away) desktop.
    /// It is the standard check that needs no window and no session callback; it
    /// cannot distinguish "locked" from "another desktop took over", which for
    /// this app are the same thing: nobody is looking at the window.
    pub(super) fn screen_locked() -> bool {
        let desktop =
            unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS | DESKTOP_SWITCHDESKTOP) };
        if desktop.is_null() {
            return true;
        }
        unsafe { CloseDesktop(desktop) };
        false
    }

    /// A JPEG of the window, via GDI.
    ///
    /// `PrintWindow` with `PW_RENDERFULLCONTENT` is preferred because it asks the
    /// window to render itself, which is the only thing that works for the
    /// hardware-composited surfaces browsers and Electron apps put up. Plain
    /// `BitBlt` is the fallback for the windows that refuse to answer it.
    pub(super) fn capture_window(hwnd: usize, path: &Path) -> Result<(), ()> {
        let hwnd = hwnd as HWND;
        if hwnd.is_null() || !is_visible(hwnd) {
            return Err(());
        }

        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            return Err(());
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width < MIN_CAPTURE_SIDE || height < MIN_CAPTURE_SIDE {
            return Err(());
        }

        let window_dc: HDC = unsafe { GetDC(hwnd) };
        if window_dc.is_null() {
            return Err(());
        }
        let mem_dc: HDC = unsafe { CreateCompatibleDC(window_dc) };
        if mem_dc.is_null() {
            unsafe { ReleaseDC(hwnd, window_dc) };
            return Err(());
        }
        let bitmap: HBITMAP = unsafe { CreateCompatibleBitmap(window_dc, width, height) };
        if bitmap.is_null() {
            unsafe {
                DeleteDC(mem_dc);
                ReleaseDC(hwnd, window_dc);
            }
            return Err(());
        }
        let previous: HGDIOBJ = unsafe { SelectObject(mem_dc, bitmap as HGDIOBJ) };

        // The window may paint nothing if it is minimised or fully occluded;
        // either way the buffer stays zeroed, which reads as a black capture
        // rather than a wrong one.
        let printed = unsafe { PrintWindow(hwnd, mem_dc, PW_RENDERFULLCONTENT) };
        if printed == 0 {
            unsafe {
                BitBlt(mem_dc, 0, 0, width, height, window_dc, 0, 0, SRCCOPY);
            }
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Negative height asks for a top-down buffer, matching every
                // other image API in this codebase.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [Default::default(); 1],
        };

        let pixel_count = (width as usize) * (height as usize);
        let mut bgra = vec![0u8; pixel_count * 4];
        let lines = unsafe {
            GetDIBits(
                mem_dc,
                bitmap,
                0,
                height as u32,
                bgra.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            )
        };

        unsafe {
            SelectObject(mem_dc, previous);
            DeleteObject(bitmap as HGDIOBJ);
            DeleteDC(mem_dc);
            ReleaseDC(hwnd, window_dc);
        }

        if lines == 0 {
            return Err(());
        }

        write_jpeg(&bgra, width as u32, height as u32, path)
    }

    /// GDI hands back BGRA; the encoder wants RGBA.
    fn write_jpeg(bgra: &[u8], width: u32, height: u32, path: &Path) -> Result<(), ()> {
        use image::codecs::jpeg::JpegEncoder;

        let mut rgba = Vec::with_capacity(bgra.len());
        for pixel in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
        let image = image::RgbaImage::from_raw(width, height, rgba).ok_or(())?;
        let rgb = image::DynamicImage::ImageRgba8(image).to_rgb8();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| ())?;
        }
        let file = std::fs::File::create(path).map_err(|_| ())?;
        let mut writer = std::io::BufWriter::new(file);
        JpegEncoder::new_with_quality(&mut writer, super::JPEG_QUALITY)
            .encode_image(&rgb)
            .map_err(|_| ())?;
        Ok(())
    }
}
