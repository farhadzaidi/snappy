//! The event connection and the state its users share: the event thread reads from it,
//! the overlay thread draws on it, and the engine thread grabs keys and selects input on
//! it so those events arrive where they are read.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ChangeWindowAttributesAux, ConnectionExt, EventMask, Window};
use x11rb::rust_connection::RustConnection;

use crate::hotkeys::HotKeyTable;
use crate::input::Monitoring;
use crate::keys::{Keymap, ModMasks, ModifierKeys};
use crate::x::{Atoms, ScreenInfo, lock};

pub struct Shared {
    pub conn: RustConnection,
    pub screen: ScreenInfo,
    pub atoms: Atoms,
    /// An unmapped window that receives the stop request.
    pub control: Window,
    pub xinput: bool,
    pub keyboard: Mutex<Keyboard>,
    pub hotkeys: Mutex<HotKeyTable>,
    pub monitoring: Mutex<Monitoring>,
    /// Display IDs to scale factors, from the last display read.
    pub scales: Mutex<HashMap<String, f64>>,
    tracked: Mutex<HashSet<Window>>,
}

/// The keyboard mapping as the server last reported it.
#[derive(Debug, Clone, Default)]
pub struct Keyboard {
    pub keymap: Keymap,
    pub masks: ModMasks,
    pub modifier_keys: ModifierKeys,
}

impl Keyboard {
    pub fn read(conn: &RustConnection) -> Keyboard {
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let keymap = conn
            .get_keyboard_mapping(min, max - min + 1)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| Keymap::new(min, reply.keysyms_per_keycode, reply.keysyms))
            .unwrap_or_default();
        let (masks, modifier_keys) = conn
            .get_modifier_mapping()
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| {
                let per_modifier = usize::from(reply.keycodes_per_modifier());
                (
                    ModMasks::from_mapping(&reply.keycodes, per_modifier, &keymap),
                    ModifierKeys::from_mapping(&reply.keycodes, per_modifier),
                )
            })
            .unwrap_or_default();
        Keyboard {
            keymap,
            masks,
            modifier_keys,
        }
    }
}

impl Shared {
    pub fn new(
        conn: RustConnection,
        screen: ScreenInfo,
        atoms: Atoms,
        control: Window,
        xinput: bool,
    ) -> Shared {
        let keyboard = Keyboard::read(&conn);
        Shared {
            conn,
            screen,
            atoms,
            control,
            xinput,
            keyboard: Mutex::new(keyboard),
            hotkeys: Mutex::new(HotKeyTable::default()),
            monitoring: Mutex::new(Monitoring::default()),
            scales: Mutex::new(HashMap::new()),
            tracked: Mutex::new(HashSet::new()),
        }
    }

    /// Starts reporting a client's moves, resizes, and remaps (`WindowMoved`).
    pub fn track(&self, window: Window) {
        if !lock(&self.tracked).insert(window) {
            return;
        }
        let events = ChangeWindowAttributesAux::new().event_mask(EventMask::STRUCTURE_NOTIFY);
        if let Ok(cookie) = self.conn.change_window_attributes(window, &events) {
            cookie.ignore_error();
        }
        let _ = self.conn.flush();
    }

    pub fn is_tracked(&self, window: Window) -> bool {
        lock(&self.tracked).contains(&window)
    }

    pub fn untrack(&self, window: Window) {
        lock(&self.tracked).remove(&window);
    }

    pub fn refresh_keyboard(&self) {
        *lock(&self.keyboard) = Keyboard::read(&self.conn);
    }

    pub fn scale(&self, display: &str) -> f64 {
        lock(&self.scales).get(display).copied().unwrap_or(1.0)
    }
}
