//! The macOS platform adapter.
//!
//! `App` lives on the main thread and owns everything AppKit needs there: the run loop,
//! the status item, overlays, event monitors, hotkeys, and observers. `MacPlatform` is
//! the engine thread's handle; it makes Accessibility calls directly and dispatches
//! AppKit work to the main thread.
#![cfg(target_os = "macos")]

mod app;
mod apps;
mod ax;
mod caps_lock;
mod coords;
mod delegate;
mod desktop_guard;
mod displays;
mod error;
mod events;
mod ffi;
mod hotkeys;
mod input;
mod instance;
mod keys;
mod login_item;
#[cfg(feature = "mac-app-store")]
mod login_launch;
mod main_thread;
mod messages;
mod observers;
mod overlay;
mod permissions;
mod platform;
/// Snappy's own view of the desktop, for the tests to check against; built only for them.
#[cfg(feature = "reference")]
pub mod reference;
mod system_shortcuts;
mod thresholds;
mod tray;
mod windows;

pub use app::{App, request_termination};
pub use error::MacError;
pub use instance::{Claim, InstanceLock, claim};
pub use platform::MacPlatform;
