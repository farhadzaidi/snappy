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
mod coords;
mod delegate;
mod desktop_guard;
mod displays;
mod eligibility;
mod events;
mod ffi;
mod hotkeys;
mod keycodes;
mod login_item;
mod main_thread;
mod messages;
mod monitors;
mod observers;
mod occupancy;
mod overlay;
mod permissions;
mod platform;
mod registry;
mod signals;
mod system_shortcuts;
mod thresholds;
mod tray;
mod windows;

pub use app::{App, request_termination};
pub use platform::MacPlatform;

/// Read by the live-test harness, so its expectations use Snappy's own view of displays
/// and keys.
pub mod reference {
    pub use crate::displays::read as displays;
    pub use crate::keycodes::virtual_key_code;
    pub use crate::occupancy::ServerWindow;
    pub use crate::windows::server::on_screen_windows;
}
