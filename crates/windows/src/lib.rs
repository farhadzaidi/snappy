//! The Windows platform adapter.
//!
//! `App` lives on the main thread and owns everything that must run there: a hidden
//! window and its message loop, WinEvent hooks, the low-level input hooks while
//! dragging and for Caps Lock as the Snappy key, hotkeys, overlays, the notification-area icon, and the desktop guard.
//! `WindowsPlatform` is the engine thread's handle: it reads and moves windows directly
//! and sends main-thread work through a dispatcher.
#![cfg(windows)]

mod app;
mod caps_lock;
mod class;
mod console;
mod desktop_guard;
mod dispatch;
mod displays;
mod error;
mod events;
mod handle;
mod hotkeys;
mod input;
mod instance;
mod keys;
mod login_item;
mod main_state;
mod managed;
mod messages;
mod overlay;
mod package;
mod platform;
/// Snappy's own view of the desktop, for the tests to check against; built only for them.
#[cfg(feature = "reference")]
pub mod reference;
mod shared;
mod tray;
pub mod uninstall;
mod wide;

pub use app::{App, Stopper};
pub use console::attach_to_parent_console;
pub use error::WindowsError;
pub use instance::{Claim, Instance, RestartRequest, claim, request_quit, request_restart};
pub use package::{package_data_folder, started_at_login};
pub use platform::WindowsPlatform;
