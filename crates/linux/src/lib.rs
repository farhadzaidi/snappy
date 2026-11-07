//! What every Linux adapter shares, whichever display server it talks to: one daemon per
//! session, quitting on `SIGTERM`, the autostart entry, process facts from `/proc`,
//! recognizing the session, physical key codes, the menu's structure, how overlays look,
//! and stable display IDs.
#![cfg(target_os = "linux")]

pub mod clients;
pub mod display_id;
pub mod instance;
pub mod key_codes;
pub mod login_item;
pub mod menu;
pub mod overlay_style;
pub mod process;
pub mod session;

pub use instance::{Claim, InstanceLock, claim, lock_path};
