//! The menu bar item and its menu. Main thread only.

mod menu;
mod status_item;

pub use status_item::{choose, install, rebuild, update};
