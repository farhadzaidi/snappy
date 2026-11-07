//! The Linux X11 platform adapter.
//!
//! `App` owns the event connection: its `run` loop reads X events and forwards them to
//! the engine, and an overlay thread draws on the same connection and drives the tray.
//! `X11Platform` is the engine thread's handle, with its own connection for window
//! requests. Nothing here needs a particular thread.
#![cfg(target_os = "linux")]

mod app;
mod displays;
mod eligibility;
mod error;
mod events;
mod frames;
mod hotkeys;
mod input;
mod keys;
mod messages;
mod overlay;
mod platform;
pub mod reference;
mod shared;
mod tray;
mod windows;
mod x;

pub use app::{App, Stopper};
pub use error::X11Error;
pub use platform::X11Platform;
