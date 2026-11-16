//! The whole platform: what it supports, and every trait an adapter implements.

use snappy_core::keys::Os;
use snappy_core::workspace::ParkingPolicy;

use crate::caps_lock::CapsLock;
use crate::desktop_guard::DesktopGuard;
use crate::hotkeys::HotKeys;
use crate::input::Input;
use crate::live_control::{LiveControl, Remedy};
use crate::login_item::LoginItem;
use crate::overlay::Overlay;
use crate::permissions::Permissions;
use crate::tray::Tray;
use crate::windows::WindowSystem;

/// What a platform supports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capabilities {
    pub platform: Os,
    pub parking: ParkingPolicy,
    /// macOS asks for Accessibility; Windows and Linux need nothing.
    pub needs_permission: bool,
}

pub trait Platform:
    WindowSystem + Input + HotKeys + CapsLock + Overlay + Permissions + DesktopGuard + Tray + LoginItem
{
    fn capabilities(&self) -> Capabilities;
    /// Whether live control can run now. Adapters where this changes while Snappy runs
    /// report `LiveControlChanged`.
    fn live_control(&mut self) -> LiveControl;
    /// Carry out a remedy `live_control` offered. Errors are sentences for the user.
    fn apply_remedy(&mut self, _remedy: Remedy) -> Result<(), String> {
        Err("Snappy can't do that on this system.".to_owned())
    }
    /// Short notes about the desktop that affect Snappy, such as another tool that also
    /// snaps windows. Read at start and on `NotesChanged`, and shown in the settings window.
    fn notes(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// Monotonic seconds.
    fn now(&self) -> f64;
}
