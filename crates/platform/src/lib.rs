//! The interfaces every operating-system adapter implements, the events adapters report,
//! the tray glyph the trays that take pixels draw (`glyph`), how every overlay looks
//! (`overlay_style`), and how the daemon is started (`launch`). The engine calls the interfaces from its own thread; calls may block only up
//! to the platform's messaging timeout.

mod caps_lock;
mod desktop_guard;
mod events;
pub mod glyph;
mod hotkeys;
mod input;
pub mod launch;
mod live_control;
mod login_item;
mod overlay;
pub mod overlay_style;
mod permissions;
mod platform;
mod tray;
mod windows;

pub use snappy_core::window::WindowId;

pub use caps_lock::{CapsLock, CapsLockInput, CapsLockRefusal, CapsLockUndo};
pub use desktop_guard::DesktopGuard;
pub use events::PlatformEvent;
pub use hotkeys::HotKeys;
pub use input::Input;
pub use live_control::{LiveControl, Remedy};
pub use login_item::LoginItem;
pub use overlay::{Caption, MarkStyle, MarkedWindow, Overlay, OverlayMarks, OverlayScene, OverlayZone};
pub use permissions::Permissions;
pub use platform::{Capabilities, Platform};
pub use tray::{Tray, TrayCommand, TrayDisplay, TrayItem, TrayMenu};
pub use windows::{Display, RecoveryKey, TabGroup, WindowInfo, WindowState, WindowSystem};
