//! Compile-checks the per-platform observation backends against the OS they are
//! written for.
//!
//! The real app cannot be checked this way: `rusqlite` is built with `bundled`,
//! so it needs a C toolchain for the target. The backends are therefore written
//! to depend on nothing but `std`, `gamelife-core` and the platform crate, and
//! are pulled in here by path so this crate can `cargo check --target …` them
//! from a Mac.
//!
//! Run it with the rustup toolchain (see rust-toolchain.toml), which is not the
//! `cargo` on PATH:
//!
//! ```text
//! TC="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin"
//! PATH="$TC/bin:$PATH" CARGO_TARGET_DIR=/tmp/pcheck-target cargo check \
//!   --target x86_64-pc-windows-msvc
//! ```
//!
//! The target dir must be its own: sharing the app's would mix two compilers in
//! one directory and start recompiling the app.

pub mod observe;

#[cfg(windows)]
#[path = "../../../src-tauri/src/windows/mod.rs"]
pub mod windows;

#[cfg(all(unix, not(target_os = "macos")))]
#[path = "../../../src-tauri/src/linux/mod.rs"]
pub mod linux;

/// The exact surface the sampler and the commands call, in the same shapes.
///
/// The app crate cannot be checked for these targets (`rusqlite` is `bundled`,
/// so it needs a C toolchain for the target), which means nothing else verifies
/// that a backend actually satisfies what the app asks of it. This function is
/// that contract: if a platform module is missing a name, or has the wrong
/// signature, this stops compiling.
#[allow(dead_code)]
pub fn contract() {
    #[cfg(any(windows, all(unix, not(target_os = "macos"))))]
    {
        fn uses<T>(_: T) {}
        use crate::observe::imp;

        // The sampler gets the state type from `observe`, the primitives from
        // `imp`; mirror that rather than inventing a different arrangement.
        let state = crate::observe::ObservationState::new();
        let snap = imp::snapshot();
        state.store(snap.clone());

        uses(imp::frontmost_app());
        uses(imp::idle_seconds());
        uses(imp::screen_locked());
        uses(imp::secure_input_on());
        uses(imp::document_path());
        uses(imp::bundle_id());
        uses(imp::metadata_observation_available());
        uses(imp::capture_observation_available());
        uses(imp::accessibility_granted());
        uses(imp::screen_recording_granted());
        uses(imp::request_screen_recording());
        uses(imp::current_process_label());
        uses(imp::current_process_path());
        uses(imp::capture_context());

        // `take_window_id` feeds `capture_window`, exactly as the sampler does.
        if let Some(id) = state.take_window_id() {
            let _ = imp::capture_window(id, std::path::Path::new("/tmp/x.jpg"));
        }
        if let Some(last) = state.last() {
            let _ = imp::url_for(last.bundle_id.as_deref(), &last.app, || {
                imp::fetch_browser_url_for(last.bundle_id.as_deref(), &last.app)
            });
        }

        // The sampler builds its own `CaptureContext` from the snapshot rather
        // than calling the backend's, so check the fields it reads.
        let _ = (
            snap.app.len(),
            snap.title.len(),
            snap.bundle_id.is_some(),
            snap.document_raw.is_some(),
            snap.window_id.is_some(),
        );
    }
}
