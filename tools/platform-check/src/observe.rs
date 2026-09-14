//! Re-export of the real shared observation modules, so the platform backends
//! can keep writing `crate::observe::…` here exactly as they do in the app.
//! `#[path]` is relative to this file's directory, hence three levels up.
#[path = "../../../src-tauri/src/observe/session.rs"]
pub mod session;

#[path = "../../../src-tauri/src/observe/state.rs"]
pub mod state;

// Mirrors the re-export in the real `observe/mod.rs`, so the contract below can
// name these the same way the sampler does.
pub use state::{FrontmostSnapshot, ObservationState};

/// Which backend the app would compile against on this target — the same
/// resolution `observe/mod.rs` does, so the contract below exercises the right
/// module.
#[cfg(windows)]
pub use crate::windows as imp;

#[cfg(all(unix, not(target_os = "macos")))]
pub use crate::linux as imp;
