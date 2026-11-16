//! What the keyboard hook does with each key while Caps Lock is the Snappy key: whether it
//! passes the key on or swallows it, and what it reports. Caps Lock itself is always
//! swallowed, so it never turns itself on; while it is down, each other key stands for the
//! Snappy key with that key, and fires the Snappy shortcut it matches directly.

use std::collections::BTreeSet;

use snappy_core::keys::Mods;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL;

use crate::input::modifier_of;

/// One key event as the hook sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookKey {
    pub vk: u16,
    pub down: bool,
    /// Injected by Snappy itself to turn Caps Lock on or off.
    pub ours: bool,
}

/// Something the hook reports for a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Pressed,
    /// Another key or a mouse button was used while Caps Lock was down.
    OtherInput,
    Released,
    /// The registered shortcut with this engine ID was pressed.
    Shortcut(u32),
}

/// What to do with a key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Verdict {
    pub swallow: bool,
    pub effects: Vec<Effect>,
}

impl Verdict {
    fn pass() -> Verdict {
        Verdict::default()
    }

    fn swallow(effects: Vec<Effect>) -> Verdict {
        Verdict {
            swallow: true,
            effects,
        }
    }
}

/// Caps Lock's state while it is the Snappy key.
#[derive(Debug)]
pub struct CapsLockKeys {
    /// The Snappy key's modifiers, which Caps Lock stands for while down.
    modifiers: Mods,
    held: bool,
    /// Something else was used during this press, and was reported.
    used: bool,
    /// Keys whose press was swallowed, so their repeats and release are too, even after
    /// Caps Lock comes up.
    swallowed: BTreeSet<u16>,
}

impl CapsLockKeys {
    pub fn new(modifiers: Mods) -> CapsLockKeys {
        CapsLockKeys {
            modifiers,
            held: false,
            used: false,
            swallowed: BTreeSet::new(),
        }
    }

    pub fn set_modifiers(&mut self, modifiers: Mods) {
        self.modifiers = modifiers;
    }

    pub fn is_held(&self) -> bool {
        self.held
    }

    /// Decide about `key`. `real` are the modifier keys down besides, and `shortcut` finds
    /// the registered shortcut for a virtual key with modifiers held.
    pub fn key(
        &mut self,
        key: HookKey,
        real: Mods,
        shortcut: impl FnOnce(Mods, u16) -> Option<u32>,
    ) -> Verdict {
        if key.ours {
            return Verdict::pass();
        }
        if key.vk == VK_CAPITAL.0 {
            return self.caps_lock(key.down);
        }
        if !key.down {
            return if self.swallowed.remove(&key.vk) {
                Verdict::swallow(Vec::new())
            } else {
                Verdict::pass()
            };
        }
        if self.swallowed.contains(&key.vk) {
            // A repeat: shortcuts fire once per press.
            return Verdict::swallow(Vec::new());
        }
        if !self.held {
            return Verdict::pass();
        }
        let mut effects = self.note_other_input().into_iter().collect::<Vec<_>>();
        if modifier_of(u32::from(key.vk)).is_some() {
            return Verdict {
                swallow: false,
                effects,
            };
        }
        self.swallowed.insert(key.vk);
        if let Some(id) = shortcut(real.union(self.modifiers), key.vk) {
            effects.push(Effect::Shortcut(id));
        }
        Verdict::swallow(effects)
    }

    fn caps_lock(&mut self, down: bool) -> Verdict {
        match (down, self.held) {
            (true, true) => Verdict::swallow(Vec::new()),
            (true, false) => {
                self.held = true;
                self.used = false;
                Verdict::swallow(vec![Effect::Pressed])
            }
            (false, true) => {
                self.held = false;
                Verdict::swallow(vec![Effect::Released])
            }
            // A release whose press came before Snappy took Caps Lock.
            (false, false) => Verdict::pass(),
        }
    }

    /// A mouse button went down.
    pub fn button(&mut self) -> Option<Effect> {
        self.note_other_input()
    }

    /// The first use of something else during a press, reported once.
    fn note_other_input(&mut self) -> Option<Effect> {
        if !self.held || self.used {
            return None;
        }
        self.used = true;
        Some(Effect::OtherInput)
    }
}

#[cfg(test)]
mod tests {
    use snappy_core::keys::{Modifier, SnappyKey};
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LSHIFT, VK_RIGHT};

    use super::*;

    const KEY_J: u16 = 0x4A;

    fn caps(down: bool) -> HookKey {
        HookKey {
            vk: VK_CAPITAL.0,
            down,
            ours: false,
        }
    }

    fn key(vk: u16, down: bool) -> HookKey {
        HookKey {
            vk,
            down,
            ours: false,
        }
    }

    fn keys() -> CapsLockKeys {
        CapsLockKeys::new(SnappyKey::DEFAULT.modifiers())
    }

    fn none(_: Mods, _: u16) -> Option<u32> {
        None
    }

    #[test]
    fn caps_lock_is_swallowed_and_reported_once_however_long_it_repeats() {
        let mut keys = keys();
        assert_eq!(
            keys.key(caps(true), Mods::NONE, none),
            Verdict::swallow(vec![Effect::Pressed])
        );
        assert_eq!(
            keys.key(caps(true), Mods::NONE, none),
            Verdict::swallow(Vec::new())
        );
        assert!(keys.is_held());
        assert_eq!(
            keys.key(caps(false), Mods::NONE, none),
            Verdict::swallow(vec![Effect::Released])
        );
        assert!(!keys.is_held());
    }

    #[test]
    fn snappys_own_caps_lock_presses_pass_so_windows_toggles_it() {
        let mut keys = keys();
        let ours = HookKey {
            ours: true,
            ..caps(true)
        };
        assert_eq!(keys.key(ours, Mods::NONE, none), Verdict::pass());
        assert!(!keys.is_held());
    }

    #[test]
    fn a_release_from_before_caps_lock_was_taken_passes() {
        assert_eq!(keys().key(caps(false), Mods::NONE, none), Verdict::pass());
    }

    #[test]
    fn a_key_while_held_fires_the_snappy_shortcut_with_the_modifiers_down_besides() {
        let mut keys = keys();
        keys.key(caps(true), Mods::NONE, none);
        let shift = Mods::from_iter([Modifier::Shift]);
        let mut asked = None;
        let verdict = keys.key(key(VK_RIGHT.0, true), shift, |mods, vk| {
            asked = Some((mods, vk));
            Some(7)
        });
        assert_eq!(
            verdict,
            Verdict::swallow(vec![Effect::OtherInput, Effect::Shortcut(7)])
        );
        assert_eq!(
            asked,
            Some((SnappyKey::DEFAULT.modifiers().with(Modifier::Shift), VK_RIGHT.0))
        );
        let repeat = keys.key(key(VK_RIGHT.0, true), shift, |_, _| Some(7));
        assert_eq!(repeat, Verdict::swallow(Vec::new()), "fires once per press");
    }

    #[test]
    fn a_key_matching_no_shortcut_is_swallowed_and_counts_as_other_input() {
        let mut keys = keys();
        keys.key(caps(true), Mods::NONE, none);
        assert_eq!(
            keys.key(key(KEY_J, true), Mods::NONE, none),
            Verdict::swallow(vec![Effect::OtherInput])
        );
        assert_eq!(
            keys.key(key(VK_RIGHT.0, true), Mods::NONE, none),
            Verdict::swallow(Vec::new()),
            "other input is reported once per press"
        );
    }

    #[test]
    fn a_swallowed_keys_release_is_swallowed_even_after_caps_lock_comes_up() {
        let mut keys = keys();
        keys.key(caps(true), Mods::NONE, none);
        keys.key(key(KEY_J, true), Mods::NONE, none);
        keys.key(caps(false), Mods::NONE, none);
        assert_eq!(
            keys.key(key(KEY_J, false), Mods::NONE, none),
            Verdict::swallow(Vec::new())
        );
        assert_eq!(keys.key(key(KEY_J, false), Mods::NONE, none), Verdict::pass());
    }

    #[test]
    fn modifier_keys_pass_and_still_count_as_other_input() {
        let mut keys = keys();
        keys.key(caps(true), Mods::NONE, none);
        assert_eq!(
            keys.key(key(VK_LSHIFT.0, true), Mods::NONE, none),
            Verdict {
                swallow: false,
                effects: vec![Effect::OtherInput]
            }
        );
        assert_eq!(
            keys.key(key(VK_LSHIFT.0, false), Mods::NONE, none),
            Verdict::pass()
        );
    }

    #[test]
    fn keys_pass_untouched_while_caps_lock_is_up() {
        let mut keys = keys();
        assert_eq!(
            keys.key(key(KEY_J, true), Mods::NONE, |_, _| Some(1)),
            Verdict::pass()
        );
        assert_eq!(keys.key(key(KEY_J, false), Mods::NONE, none), Verdict::pass());
        assert_eq!(keys.button(), None);
    }

    #[test]
    fn a_mouse_button_while_held_counts_as_other_input_once() {
        let mut keys = keys();
        keys.key(caps(true), Mods::NONE, none);
        assert_eq!(keys.button(), Some(Effect::OtherInput));
        assert_eq!(keys.button(), None);
        keys.key(caps(false), Mods::NONE, none);
        keys.key(caps(true), Mods::NONE, none);
        assert_eq!(
            keys.button(),
            Some(Effect::OtherInput),
            "each press starts afresh"
        );
    }
}
