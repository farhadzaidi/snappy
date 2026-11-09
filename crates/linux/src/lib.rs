//! What every Linux adapter shares, whichever display server it talks to: one daemon per
//! session, quitting on `SIGTERM`, the autostart entry, process facts from `/proc`,
//! recognizing the session, physical key codes, the menu's structure, how overlays look,
//! stable display IDs, drawing overlays and the scenes desktop shells draw, the tray icon,
//! warnings for the journal, and a minimal Wayland client.
#![cfg(target_os = "linux")]

pub mod clients;
pub mod display_id;
pub mod instance;
pub mod journal;
pub mod key_codes;
pub mod login_item;
pub mod menu;
pub mod overlay_image;
pub mod overlay_style;
pub mod process;
pub mod scene;
pub mod session;
pub mod tray;
pub mod wayland;

pub use instance::{Claim, InstanceLock, claim, lock_path};
