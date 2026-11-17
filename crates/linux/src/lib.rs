//! What every Linux adapter shares, whichever display server it talks to: how the daemon
//! starts one, one daemon per session, quitting on `SIGTERM` and stopping for a restart on
//! `SIGUSR2`, the autostart entry and the app icon it names, process facts from `/proc`,
//! recognizing the session, physical key codes, the menu's structure, how overlays look,
//! stable display IDs, drawing overlays and the scenes desktop shells draw, the tray icon,
//! warnings for the journal, the user's XDG folders, a minimal Wayland client, Caps Lock
//! as the Snappy key through XKB, removing what Snappy added for `snappy --uninstall`, and
//! the user-facing messages for it. For the adapters
//! that work through a desktop shell over the session bus: D-Bus plumbing, and which of the
//! windows the shell reports Snappy manages.
#![cfg(target_os = "linux")]

pub mod adapter;
pub mod app_icon;
pub mod clients;
pub mod dbus;
pub mod display_id;
pub mod instance;
pub mod journal;
pub mod key_codes;
pub mod login_item;
pub mod menu;
pub mod messages;
pub mod names;
pub mod overlay_image;
pub mod overlay_style;
pub mod process;
pub mod scene;
pub mod session;
pub mod tray;
pub mod uninstall;
pub mod wayland;
pub mod windows;
pub mod xdg;
pub mod xkb;

pub use instance::{Claim, InstanceLock, claim, lock_path, request_quit};
