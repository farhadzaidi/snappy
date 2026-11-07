//! The GNOME on Wayland platform adapter.
//!
//! Wayland lets no ordinary program move other apps' windows, watch global input, or own
//! global shortcuts; on GNOME only GNOME Shell can. Snappy's GNOME Shell extension
//! (`apps/gnome-extension`) does those things when asked over the session bus and reports
//! what happens. Every decision stays here and in the engine.
//!
//! `App` owns the event loop, which turns the extension's events into engine events;
//! `GnomePlatform` is the engine thread's handle. Both share one bus connection.
#![cfg(target_os = "linux")]

mod app;
mod bus;
mod displays;
mod error;
pub mod extension;
mod keys;
mod menu;
mod messages;
mod overlay;
mod platform;
pub mod protocol;
pub mod reference;
mod shared;
mod windows;

pub use app::{App, Stopper};
pub use error::GnomeError;
pub use platform::GnomePlatform;
