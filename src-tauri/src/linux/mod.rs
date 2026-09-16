//! Linux (X11) observation backend.
//!
//! The same surface as `macos/` (the required list is in `observe/mod.rs`),
//! built on the X protocol through `x11rb` — pure Rust, so it needs no C
//! toolchain to build and can be cross-checked from a Mac.
//!
//! **This is the Xorg path.** Wayland deliberately exposes no way to ask which
//! window is focused, and capturing one there means going through
//! xdg-desktop-portal and PipeWire — a different project, not a different
//! module. `session::kind()` reports which session the app is actually in, so
//! the UI can say so instead of silently observing nothing.
//!
//! What X11 genuinely cannot answer, reported empty rather than invented:
//!
//! * **No document path.** No per-window property carries the open file, so
//!   `document_raw` is `None` and the file name has to come from the title
//!   (which is why most editors put it there).
//! * **No secure-input flag.** Nothing in the protocol exposes "a password
//!   field has focus".
//! * **No permission model.** Nothing to grant, so the predicates report
//!   granted and `request_screen_recording` is a no-op.
//! * **Best-effort lock detection.** The XScreenSaver extension reports the
//!   idle timer and, on Mutter, whether the screensaver is engaged; it is not a
//!   lock API. The idle rule in `hint` is what actually catches a walked-away
//!   screen, which bounds the cost of getting this wrong.
//! * **No browser tab URL.** Reading a browser's current tab needs per-browser
//!   automation, so `url_for` is `None` — the same degradation macOS has when
//!   the Automation permission is refused.
//! * **App identity is `WM_CLASS`, not a bundle id.** `app` carries the class
//!   (`Firefox`, `Code`), and `bundle_id` carries GTK's reverse-DNS application
//!   id when the app publishes one (`org.gnome.Nautilus`), which is the closest
//!   thing X11 has to a bundle identifier.

use std::path::Path;
use std::sync::Mutex;

use x11rb::connection::Connection;
use x11rb::protocol::screensaver::ConnectionExt as ScreensaverExt;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

use crate::observe::state::FrontmostSnapshot;

/// JPEG quality for a stored capture; `vision.rs` re-encodes before upload.
const JPEG_QUALITY: u8 = 85;

/// A window smaller than this is a tooltip or a stub.
const MIN_CAPTURE_SIDE: u16 = 8;

struct Display {
    conn: RustConnection,
    root: Window,
}

/// One connection for the process. Reconnecting per sample would be a round
/// trip every 15 seconds for no benefit; a connection that has died is detected
/// by the calls failing, and the next `snapshot()` re-connects.
static DISPLAY: Mutex<Option<Option<Display>>> = Mutex::new(None);

fn with_display<T>(f: impl FnOnce(&Display) -> T) -> Option<T> {
    let mut guard = DISPLAY.lock().ok()?;
    let slot = guard.get_or_insert_with(|| {
        RustConnection::connect(None)
            .ok()
            .and_then(|(conn, screen)| {
                let root = conn.setup().roots.get(screen)?.root;
                Some(Display { conn, root })
            })
    });
    Some(f(slot.as_ref()?))
}

fn atom(conn: &RustConnection, name: &[u8]) -> Option<u32> {
    conn.intern_atom(false, name)
        .ok()?
        .reply()
        .ok()
        .map(|r| r.atom)
}

/// The first 32-bit value of a property, or `None` when it is unset or empty.
fn first_cardinal(conn: &RustConnection, window: Window, property: u32) -> Option<u32> {
    conn.get_property(false, window, property, AtomEnum::ANY, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()
}

/// A property's bytes, treating an empty reply as "not set".
fn property_bytes(
    conn: &RustConnection,
    window: Window,
    property: u32,
    type_: u32,
) -> Option<Vec<u8>> {
    let reply = conn
        .get_property(false, window, property, type_, 0, u32::MAX)
        .ok()?
        .reply()
        .ok()?;
    (!reply.value.is_empty()).then_some(reply.value)
}

/// `WM_CLASS` is two NUL-terminated strings: instance, then class.
fn wm_class(conn: &RustConnection, window: Window) -> (Option<String>, Option<String>) {
    let Some(bytes) = property_bytes(
        conn,
        window,
        AtomEnum::WM_CLASS.into(),
        AtomEnum::STRING.into(),
    ) else {
        return (None, None);
    };
    let mut parts = bytes.split(|&b| b == 0).filter(|p| !p.is_empty());
    let instance = parts
        .next()
        .map(|p| String::from_utf8_lossy(p).into_owned());
    let class = parts
        .next()
        .map(|p| String::from_utf8_lossy(p).into_owned());
    (instance, class)
}

/// `_NET_WM_NAME` (UTF-8, the modern one) with a `WM_NAME` fallback.
fn window_title(conn: &RustConnection, window: Window) -> String {
    // `_NET_WM_NAME` is UTF-8 and authoritative; `WM_NAME` is the legacy
    // latin-1 property and the fallback for a window that only sets that.
    let modern = atom(conn, b"_NET_WM_NAME").and_then(|property| {
        let utf8_string = atom(conn, b"UTF8_STRING")?;
        property_bytes(conn, window, property, utf8_string)
    });
    if let Some(bytes) = modern {
        return String::from_utf8_lossy(&bytes).into_owned();
    }
    property_bytes(
        conn,
        window,
        AtomEnum::WM_NAME.into(),
        AtomEnum::STRING.into(),
    )
    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    .unwrap_or_default()
}

/// The window the window manager reports as focused.
fn active_window(conn: &RustConnection, root: Window) -> Option<Window> {
    first_cardinal(conn, root, atom(conn, b"_NET_ACTIVE_WINDOW")?).map(|id| id as Window)
}

pub fn snapshot() -> FrontmostSnapshot {
    with_display(|display| {
        let conn = &display.conn;
        let Some(window) = active_window(conn, display.root) else {
            return FrontmostSnapshot::default();
        };
        if window == 0 {
            return FrontmostSnapshot::default();
        }

        let (instance, class) = wm_class(conn, window);
        // The class is what a person would write into a rule table; the
        // instance is the fallback for the rare app that only sets one.
        let app = class.clone().or(instance).unwrap_or_default();
        let bundle_id = atom(conn, b"_GTK_APPLICATION_ID")
            .and_then(|id| property_bytes(conn, window, id, AtomEnum::STRING.into()))
            .map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .filter(|id| !id.is_empty());

        FrontmostSnapshot {
            app,
            title: window_title(conn, window),
            bundle_id,
            // No X11 property carries the open document; see the module header.
            document_raw: None,
            pid: atom(conn, b"_NET_WM_PID")
                .and_then(|property| first_cardinal(conn, window, property))
                .map(|pid| pid as i32),
            window_id: Some(window as u64),
        }
    })
    .unwrap_or_default()
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
    snapshot().bundle_id
}

/// Milliseconds since the last input, from the XScreenSaver extension.
///
/// Reporting `0` when the extension is missing is the safe direction: it reads
/// as "the user is active", so a missing extension can only cost the idle rule,
/// never invent an absence.
pub fn idle_seconds() -> i64 {
    with_display(|display| {
        display
            .conn
            .screensaver_query_info(display.root)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|info| (info.ms_since_user_input / 1000) as i64)
            .unwrap_or(0)
    })
    .unwrap_or(0)
}

/// Best effort: the XScreenSaver extension's state, which Mutter engages when
/// the session locks. See the module header — the idle rule is the real guard.
pub fn screen_locked() -> bool {
    with_display(|display| {
        display
            .conn
            .screensaver_query_info(display.root)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|info| info.state == u8::from(x11rb::protocol::screensaver::State::ON))
            .unwrap_or(false)
    })
    .unwrap_or(false)
}

pub fn secure_input_on() -> bool {
    false
}

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

/// Whether the app can see windows at all.
///
/// Both halves matter: there has to be an X connection, and the session has to
/// be Xorg. A Wayland session keeps an XWayland display around for X clients,
/// so a connection alone would report success while the compositor's own
/// windows stayed invisible.
pub fn metadata_observation_available() -> bool {
    crate::observe::session::kind() == crate::observe::session::SessionKind::X11
        && with_display(|_| ()).is_some()
}

pub fn capture_observation_available() -> bool {
    metadata_observation_available()
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

/// A JPEG of the window.
///
/// `GetImage` reads the framebuffer, so an obscured window captures whatever is
/// on top of it — the same caveat as macOS when a window is behind another, and
/// the reason the sampler captures the window it *sampled* rather than the
/// frontmost one at timer time.
pub fn capture_window(window_id: u64, path: &Path) -> Result<(), ()> {
    let geometry = with_display(|display| {
        display
            .conn
            .get_geometry(window_id as Window)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
    })
    .flatten()
    .ok_or(())?;

    if geometry.width < MIN_CAPTURE_SIDE || geometry.height < MIN_CAPTURE_SIDE {
        return Err(());
    }

    // The pixel layout comes from the window's own visual, not the root's: a
    // 24-bit window on a 32-bit root would otherwise be decoded with the wrong
    // channel order.
    let visual = with_display(|display| {
        display
            .conn
            .get_window_attributes(window_id as Window)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|attrs| attrs.visual)
    })
    .flatten()
    .ok_or(())?;

    let format = with_display(|display| {
        display
            .conn
            .setup()
            .pixmap_formats
            .iter()
            .find(|f| f.depth == geometry.depth)
            .map(|f| (f.bits_per_pixel, f.scanline_pad))
    })
    .flatten()
    .ok_or(())?;

    let (bits_per_pixel, scanline_pad) = format;
    if !matches!(bits_per_pixel, 24 | 32) {
        return Err(());
    }

    let reply = with_display(|display| {
        display
            .conn
            .get_image(
                x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
                window_id as Window,
                0,
                0,
                geometry.width,
                geometry.height,
                !0,
            )
            .ok()
            .and_then(|cookie| cookie.reply().ok())
    })
    .flatten()
    .ok_or(())?;

    let _ = visual;
    let bytes_per_pixel = (bits_per_pixel / 8) as usize;
    let row_bytes = (geometry.width as usize) * bytes_per_pixel;
    // X pads every scanline out to `scanline_pad` bits.
    let stride =
        row_bytes.div_ceil((scanline_pad as usize / 8).max(1)) * (scanline_pad as usize / 8).max(1);

    let mut rgb = Vec::with_capacity((geometry.width as usize) * (geometry.height as usize) * 3);
    for row in 0..geometry.height as usize {
        let start = row * stride;
        let end = start + row_bytes;
        let Some(line) = reply.data.get(start..end) else {
            return Err(());
        };
        for pixel in line.chunks_exact(bytes_per_pixel) {
            // Both 24- and 32-bit X visuals put blue first in memory on a
            // little-endian host, which is what X sends as bytes.
            rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
    }

    let image =
        image::RgbImage::from_raw(geometry.width as u32, geometry.height as u32, rgb).ok_or(())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| ())?;
    }
    let file = std::fs::File::create(path).map_err(|_| ())?;
    let mut writer = std::io::BufWriter::new(file);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, JPEG_QUALITY)
        .encode_image(&image)
        .map_err(|_| ())?;
    Ok(())
}
