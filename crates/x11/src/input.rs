//! Pointer and keyboard observation through XInput 2 raw events on the root window,
//! which never grab and keep arriving while another client holds a grab.

use std::collections::BTreeSet;

use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _, XIEventMask};

use crate::keys::ModifierKeys;
use crate::shared::Shared;
use crate::x::lock;

/// The primary button's raw button number.
pub const PRIMARY_BUTTON: u32 = 1;

/// What the engine asked to observe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Monitoring {
    /// Primary-button presses and releases.
    pub press: bool,
    /// Pointer motion and modifier changes during a drag.
    pub drag: bool,
    /// How many times drag monitoring has turned on, so the event thread can tell a new
    /// stretch of it, with key events it hasn't seen in between, from the one it follows.
    pub drag_stretch: u64,
}

impl Monitoring {
    /// The raw events to select. Keys are observed only while drag monitoring is on.
    pub fn raw_events(self) -> XIEventMask {
        let buttons = XIEventMask::RAW_BUTTON_PRESS | XIEventMask::RAW_BUTTON_RELEASE;
        let drag = XIEventMask::RAW_MOTION | XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE;
        match (self.press, self.drag) {
            (_, true) => buttons | drag,
            (true, false) => buttons,
            (false, false) => XIEventMask::from(0u32),
        }
    }
}

/// Records the wanted observation and selects exactly those events.
pub fn set(shared: &Shared, change: impl FnOnce(&mut Monitoring)) {
    let monitoring = {
        let mut monitoring = lock(&shared.monitoring);
        let was_dragging = monitoring.drag;
        change(&mut monitoring);
        if monitoring.drag && !was_dragging {
            monitoring.drag_stretch += 1;
        }
        *monitoring
    };
    select(shared, monitoring.raw_events());
}

/// The modifier keys held down, followed in event order from raw key events. A button
/// release then reports the modifiers held when it happened, not when it is handled: by
/// then the user may have let go of the drag modifier too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldModifiers {
    /// The stretch of drag monitoring these keys were followed through.
    pub stretch: u64,
    keys: BTreeSet<u8>,
}

impl HeldModifiers {
    /// Starts from the modifier keys down in `keymap`, the bit per keycode that
    /// `QueryKeymap` returns. Raw key events only arrive while drag monitoring is on, so
    /// following starts from the keyboard's state when it turns on.
    pub fn new(stretch: u64, keymap: &[u8], modifiers: &ModifierKeys) -> HeldModifiers {
        let down = |keycode: &u8| {
            keymap
                .get(usize::from(keycode / 8))
                .is_some_and(|byte| byte & (1 << (keycode % 8)) != 0)
        };
        let keys = (0..=u8::MAX)
            .filter(|keycode| down(keycode) && modifiers.is_modifier(*keycode))
            .collect();
        HeldModifiers { stretch, keys }
    }

    /// A raw key press or release. Only modifier keys are kept.
    pub fn key(&mut self, keycode: u8, pressed: bool, modifiers: &ModifierKeys) {
        if !modifiers.is_modifier(keycode) {
            return;
        }
        if pressed {
            self.keys.insert(keycode);
        } else {
            self.keys.remove(&keycode);
        }
    }

    /// The X state mask of the held modifiers.
    pub fn mask(&self, modifiers: &ModifierKeys) -> u16 {
        modifiers.mask(&self.keys)
    }
}

pub fn select(shared: &Shared, mask: XIEventMask) {
    if !shared.xinput {
        return;
    }
    let masks = [xinput::EventMask {
        deviceid: xinput::Device::ALL_MASTER.into(),
        mask: vec![mask],
    }];
    if let Ok(cookie) = shared.conn.xinput_xi_select_events(shared.screen.root, &masks) {
        cookie.ignore_error();
    }
    let _ = shared.conn.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(mask: XIEventMask, wanted: XIEventMask) -> bool {
        u32::from(mask) & u32::from(wanted) == u32::from(wanted)
    }

    #[test]
    fn press_monitoring_selects_only_buttons() {
        let mask = Monitoring {
            press: true,
            ..Monitoring::default()
        }
        .raw_events();
        assert!(has(
            mask,
            XIEventMask::RAW_BUTTON_PRESS | XIEventMask::RAW_BUTTON_RELEASE
        ));
        assert!(!has(mask, XIEventMask::RAW_MOTION));
        assert!(!has(mask, XIEventMask::RAW_KEY_PRESS));
    }

    #[test]
    fn keys_are_observed_only_while_drag_monitoring_is_on() {
        let dragging = Monitoring {
            press: true,
            drag: true,
            ..Monitoring::default()
        }
        .raw_events();
        assert!(has(
            dragging,
            XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE
        ));
        assert!(has(
            dragging,
            XIEventMask::RAW_MOTION | XIEventMask::RAW_BUTTON_RELEASE
        ));
        let operation_only = Monitoring {
            drag: true,
            ..Monitoring::default()
        }
        .raw_events();
        assert!(has(operation_only, XIEventMask::RAW_KEY_PRESS));
        assert_eq!(u32::from(Monitoring::default().raw_events()), 0);
    }

    /// Shift on keycodes 50 and 62, Control on 37.
    fn modifier_keys() -> ModifierKeys {
        let mut rows = vec![0u8; 16];
        rows[..2].copy_from_slice(&[50, 62]);
        rows[4] = 37;
        ModifierKeys::from_mapping(&rows, 2)
    }

    fn keymap_with(down: &[u8]) -> Vec<u8> {
        let mut keymap = vec![0u8; 32];
        for keycode in down {
            keymap[usize::from(keycode / 8)] |= 1 << (keycode % 8);
        }
        keymap
    }

    #[test]
    fn following_starts_from_the_modifier_keys_already_down() {
        let keys = modifier_keys();
        let held = HeldModifiers::new(1, &keymap_with(&[62, 38]), &keys);
        assert_eq!(held.mask(&keys), 1, "Shift, and the letter key isn't kept");
        assert_eq!(held.keys.len(), 1);
        assert_eq!(HeldModifiers::new(1, &[], &keys).mask(&keys), 0);
    }

    #[test]
    fn a_modifier_counts_until_its_last_key_is_released() {
        let keys = modifier_keys();
        let mut held = HeldModifiers::new(1, &keymap_with(&[]), &keys);
        held.key(50, true, &keys);
        held.key(62, true, &keys);
        held.key(37, true, &keys);
        held.key(38, true, &keys);
        assert_eq!(held.mask(&keys), 1 | 4);
        held.key(50, false, &keys);
        assert_eq!(held.mask(&keys), 1 | 4, "the other Shift is still down");
        held.key(62, false, &keys);
        held.key(37, false, &keys);
        assert_eq!(held.mask(&keys), 0);
    }
}
