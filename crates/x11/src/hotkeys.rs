//! Global shortcuts through `XGrabKey` on the root window. Each chord is
//! grabbed once per Caps Lock and Num Lock combination; `BadAccess` means another client
//! holds it, which doubles as the availability probe.

use std::collections::HashMap;

use snappy_core::keys::Combo;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, GrabMode, ModMask};
use x11rb::wrapper::ConnectionExt as _;

use crate::shared::Shared;
use crate::x::lock;

/// A key as grabbed: keycode and modifier mask without lock bits.
type Grab = (u8, u16);

/// Registered shortcuts by grab.
#[derive(Debug, Default)]
pub struct HotKeyTable {
    ids: HashMap<Grab, u32>,
}

impl HotKeyTable {
    /// The shortcut a key press with `relevant` modifier bits triggers.
    pub fn lookup(&self, keycode: u8, relevant: u16) -> Option<u32> {
        self.ids.get(&(keycode, relevant)).copied()
    }

    fn contains(&self, grab: Grab) -> bool {
        self.ids.contains_key(&grab)
    }
}

/// The keycode and modifier mask for `combo` on this server.
fn resolve(shared: &Shared, combo: &Combo) -> Option<Grab> {
    let keyboard = lock(&shared.keyboard);
    let keycode = keyboard.keymap.keycode_for(&combo.code)?;
    Some((keycode, keyboard.masks.mask(combo.mods)))
}

fn lock_variants(shared: &Shared) -> Vec<u16> {
    lock(&shared.keyboard).masks.lock_variants()
}

/// Grabs every lock variant of `grab`, or none of them.
fn grab(shared: &Shared, (keycode, mask): Grab) -> bool {
    let conn = &shared.conn;
    let root = shared.screen.root;
    let cookies: Vec<_> = lock_variants(shared)
        .into_iter()
        .filter_map(|variant| {
            let modifiers = ModMask::from(mask | variant);
            let cookie = conn
                .grab_key(false, root, modifiers, keycode, GrabMode::ASYNC, GrabMode::ASYNC)
                .ok()?;
            Some((variant, cookie))
        })
        .collect();
    let mut granted = Vec::new();
    let mut refused = false;
    for (variant, cookie) in cookies {
        if cookie.check().is_ok() {
            granted.push(variant);
        } else {
            refused = true;
        }
    }
    if refused {
        for variant in granted {
            ungrab(shared, (keycode, mask | variant));
        }
    }
    let _ = conn.flush();
    !refused
}

fn ungrab(shared: &Shared, (keycode, modifiers): Grab) {
    if let Ok(cookie) = shared
        .conn
        .ungrab_key(keycode, shared.screen.root, ModMask::from(modifiers))
    {
        cookie.ignore_error();
    }
}

/// Replaces every registration with `keys`. Returns the IDs that couldn't be grabbed.
pub fn register(shared: &Shared, keys: &[(u32, Combo)]) -> Vec<u32> {
    unregister_all(shared);
    let mut table = HotKeyTable::default();
    let mut refused = Vec::new();
    for (id, combo) in keys {
        match resolve(shared, combo) {
            Some(key) if table.contains(key) || grab(shared, key) => {
                table.ids.insert(key, *id);
            }
            _ => refused.push(*id),
        }
    }
    *lock(&shared.hotkeys) = table;
    refused
}

pub fn unregister_all(shared: &Shared) {
    let table = std::mem::take(&mut *lock(&shared.hotkeys));
    let variants = lock_variants(shared);
    for (keycode, mask) in table.ids.into_keys() {
        for variant in &variants {
            ungrab(shared, (keycode, mask | variant));
        }
    }
    // A round trip, so other clients find the keys free as soon as this returns.
    let _ = shared.conn.sync();
}

/// Whether `combo` is free: Snappy's own registrations count as free, anything else is
/// probed by grabbing and releasing it.
pub fn is_available(shared: &Shared, combo: &Combo) -> bool {
    let Some(key) = resolve(shared, combo) else {
        return false;
    };
    if lock(&shared.hotkeys).contains(key) {
        return true;
    }
    if !grab(shared, key) {
        return false;
    }
    for variant in lock_variants(shared) {
        ungrab(shared, (key.0, key.1 | variant));
    }
    let _ = shared.conn.sync();
    true
}
