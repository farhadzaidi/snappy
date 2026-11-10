//! The KDE Plasma (KWin on Wayland) platform adapter.
//!
//! Wayland lets no ordinary program move other apps' windows or watch global input; on KDE
//! Plasma only KWin can. Snappy's KWin script (`apps/kwin-script`), loaded into KWin over
//! D-Bus at every start, does those things when asked and reports what happens. Shortcuts go
//! through KDE's shortcut service, and the modifier keys come from KWin's keystate protocol
//! while a drag runs. Every decision stays here and in the engine.
//!
//! `App` owns the event loop, which turns what KWin reports into engine events;
//! `KWinPlatform` is the engine thread's handle. Both share one bus connection.
#![cfg(target_os = "linux")]

mod app;
mod bus;
mod displays;
mod error;
mod keys;
mod keystate;
mod messages;
mod order;
mod overlay;
mod platform;
pub mod protocol;
pub mod reference;
pub mod script;
mod shared;
mod shortcuts;
mod windows;

pub use app::{App, Stopper};
pub use error::KWinError;
pub use platform::KWinPlatform;
