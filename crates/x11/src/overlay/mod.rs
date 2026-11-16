//! Snappy's overlays on X11: click-through windows drawn on the event connection by a
//! thread of their own, which also drives the tray.

mod surface;
mod thread;

pub use surface::negotiate;
pub use thread::{UiCommand, spawn};
