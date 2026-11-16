//! What adapters report to the engine.

use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::Mods;
use snappy_core::window::WindowId;

use crate::caps_lock::CapsLockInput;
use crate::tray::TrayCommand;

/// Something that happened outside the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatformEvent {
    /// A primary-button press. Adapters that learn of it from a move or resize starting
    /// name the window.
    PointerDown {
        at: Point,
        mods: Mods,
        window: Option<WindowId>,
        /// The named window's frame when the pointer was at `at`, when the adapter knows it.
        /// KWin reports a press only once its move starts, past the drag threshold, so a
        /// frame read afterwards has already moved with the pointer.
        frame: Option<Rect>,
    },
    PointerDragged {
        at: Point,
        mods: Mods,
    },
    PointerUp {
        at: Point,
        mods: Mods,
    },
    ModifiersChanged {
        at: Point,
        mods: Mods,
    },
    HotKey(u32),
    /// Caps Lock went down or up, or something else was used meanwhile, while it is the
    /// Snappy key.
    CapsLock(CapsLockInput),
    /// Caps Lock stopped standing for the Snappy key without being asked, because something
    /// else changed the keyboard settings: a keymap loaded over Snappy's change, or keyboard
    /// settings saved without Snappy's option. The adapter has let go of it, so nothing of
    /// Snappy's is left to undo.
    CapsLockLost,
    WindowCreated {
        pid: u32,
    },
    /// The frontmost app or its focused window changed.
    FocusChanged {
        pid: u32,
        by_activation: bool,
    },
    /// A tracked window was moved, resized, or un-minimized by its app.
    WindowMoved(WindowId),
    WindowDestroyed(WindowId),
    AppLaunched {
        pid: u32,
    },
    AppTerminated {
        pid: u32,
    },
    DisplaysChanged,
    NativeDesktopChanged,
    PermissionChanged,
    /// What `Platform::live_control` answers changed, or the platform's own helper started
    /// over and lost what it held.
    LiveControlChanged,
    /// The adapter registered the shortcuts again on its own, as after a keyboard layout
    /// change, and some were refused; the engine registers them again to learn which.
    HotKeysRefused,
    /// What `Platform::notes` answers changed.
    NotesChanged,
    WillSleep,
    /// A second launch, a reopen, or the menu's "Open Snappy…".
    OpenAppRequested,
    Tray(TrayCommand),
    /// The menu's Quit, a termination signal, or the session ending: parked windows come
    /// back before the daemon exits.
    QuitRequested,
    /// Stop so that a new daemon can take over, as when installing a new build: every
    /// window stays where it is, parked ones included, for the next launch to pick up.
    RestartRequested,
}
