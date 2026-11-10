//! The Windows platform adapter.
//!
//! `App` lives on the main thread and owns everything that must run there: a hidden
//! window and its message loop, WinEvent hooks, the low-level input hooks while
//! dragging, hotkeys, overlays, the notification-area icon, and the desktop guard.
//! `WindowsPlatform` is the engine thread's handle: it reads and moves windows directly
//! and sends main-thread work through a dispatcher.
#![cfg(windows)]

mod app;
mod class;
mod desktop_guard;
mod dispatch;
mod displays;
mod eligibility;
mod error;
mod events;
mod frames;
mod hotkeys;
mod identity;
mod input_hooks;
mod instance;
mod keys;
mod login_item;
mod main_state;
mod managed;
mod messages;
mod overlay;
mod platform;
mod process;
pub mod reference;
mod shared;
mod tray;
mod wide;

pub use app::{App, Stopper};
pub use error::WindowsError;
pub use instance::{Claim, Instance, claim};
pub use platform::WindowsPlatform;
