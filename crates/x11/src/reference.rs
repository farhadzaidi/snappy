//! Read by the live-test harness, so its expectations use Snappy's own view of displays
//! and keys.

use snappy_core::keys::{KeyCode, Modifier};
use snappy_platform::Display;
use x11rb::rust_connection::RustConnection;

use crate::error::X11Error;
use crate::shared::Keyboard;
use crate::x::{Atoms, ScreenInfo};
use snappy_linux::key_codes::keysym;

/// Every display on screen `screen`, primary first, as the adapter reports them.
pub fn displays(conn: &RustConnection, screen: usize) -> Result<Vec<Display>, X11Error> {
    let screen = ScreenInfo::read(conn, screen)?;
    let atoms = Atoms::new(conn)?.reply()?;
    Ok(crate::displays::read(conn, &screen, &atoms))
}

/// The server's keyboard mapping, resolved the way shortcuts are grabbed.
pub struct Keys {
    keyboard: Keyboard,
}

impl Keys {
    pub fn read(conn: &RustConnection) -> Keys {
        Keys {
            keyboard: Keyboard::read(conn),
        }
    }

    /// The keycode a shortcut on `code` is grabbed with.
    pub fn keycode(&self, code: &KeyCode) -> Option<u8> {
        self.keyboard.keymap.keycode_for(code)
    }

    /// The keycode of the left-hand key for `modifier`.
    pub fn modifier_keycode(&self, modifier: Modifier) -> Option<u8> {
        let sym = match modifier {
            Modifier::Shift => keysym::SHIFT_L,
            Modifier::Ctrl => keysym::CONTROL_L,
            Modifier::Alt => keysym::ALT_L,
            Modifier::Meta => keysym::SUPER_L,
        };
        self.keyboard.keymap.find(sym)
    }
}
