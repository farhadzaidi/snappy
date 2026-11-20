//! Global shortcuts through Carbon `RegisterEventHotKey`. Main thread only.
//!
//! One event handler on the application target receives every press and release.
//! Registrations carry Snappy's signature and an ID derived from the chord; the
//! availability probe uses a separate signature so its press could never be mistaken for a
//! shortcut. The key Caps Lock arrives as, while it is the Snappy key, has a signature of
//! its own. Releases count for it, and for the shortcuts that report being let go.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::time::Instant;

use snappy_core::keys::Combo;
use snappy_platform::{HeldShortcuts, HotKeyRequest, KeyRepeat, PlatformEvent};

#[cfg(not(feature = "mac-app-store"))]
use crate::caps_lock;
use crate::events::emit;
#[cfg(not(feature = "mac-app-store"))]
use crate::ffi::carbon::{CMD_KEY, CONTROL_KEY, OPTION_KEY, SHIFT_KEY};
use crate::ffi::carbon::{
    EVENT_CLASS_KEYBOARD, EVENT_HOT_KEY_PRESSED, EVENT_HOT_KEY_RELEASED, EVENT_NOT_HANDLED_ERR,
    EVENT_PARAM_DIRECT_OBJECT, EventHandlerCallRef, EventHandlerRef, EventHotKeyID, EventHotKeyRef, EventRef,
    EventTypeSpec, GetApplicationEventTarget, GetEventKind, GetEventParameter, InstallEventHandler, NO_ERR,
    OSStatus, RegisterEventHotKey, TYPE_EVENT_HOT_KEY_ID, UnregisterEventHotKey,
};
use crate::keys::CarbonChord;
use crate::system_shortcuts;

const SIGNATURE: u32 = u32::from_be_bytes(*b"SNAP");
const PROBE_SIGNATURE: u32 = u32::from_be_bytes(*b"SNPR");
#[cfg(not(feature = "mac-app-store"))]
const CAPS_LOCK_SIGNATURE: u32 = u32::from_be_bytes(*b"SNCL");

#[derive(Default)]
struct HotKeys {
    handler: Option<EventHandlerRef>,
    registered: Vec<EventHotKeyRef>,
    shortcuts: Shortcuts,
    /// The key Caps Lock arrives as, under every combination of modifiers.
    #[cfg(not(feature = "mac-app-store"))]
    caps_lock: Vec<EventHotKeyRef>,
}

/// The engine's shortcuts by chord-derived Carbon ID, and which of those that report being
/// let go are down. Carbon never repeats a held hotkey, so a press while one is down means
/// its release went unseen.
struct Shortcuts {
    by_carbon_id: HashMap<u32, u32>,
    held: HeldShortcuts,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            by_carbon_id: HashMap::new(),
            held: HeldShortcuts::new(KeyRepeat::Never),
        }
    }
}

impl Shortcuts {
    /// Take `registered`, each a Carbon ID and the shortcut it stands for, in place of any
    /// before, forgetting any still down: once unregistered, its release never comes.
    fn replace(&mut self, registered: &[(u32, HotKeyRequest)]) {
        self.by_carbon_id = registered
            .iter()
            .map(|(carbon_id, shortcut)| (*carbon_id, shortcut.id))
            .collect();
        let shortcuts: Vec<HotKeyRequest> = registered.iter().map(|(_, shortcut)| shortcut.clone()).collect();
        self.held.register(&shortcuts);
    }

    /// What the press or release of the shortcut with `carbon_id` reports.
    fn event(&mut self, carbon_id: u32, pressed: bool) -> Option<PlatformEvent> {
        let &id = self.by_carbon_id.get(&carbon_id)?;
        if pressed {
            self.held.pressed(id, Instant::now())
        } else {
            self.held.released(id)
        }
    }
}

thread_local! {
    static HOT_KEYS: RefCell<HotKeys> = RefCell::new(HotKeys::default());
}

/// Installs the press handler once.
pub fn install_handler() {
    HOT_KEYS.with(|hot_keys| {
        let mut hot_keys = hot_keys.borrow_mut();
        if hot_keys.handler.is_some() {
            return;
        }
        let specs = [EVENT_HOT_KEY_PRESSED, EVENT_HOT_KEY_RELEASED].map(|kind| EventTypeSpec {
            event_class: EVENT_CLASS_KEYBOARD,
            event_kind: kind,
        });
        let mut handler = ptr::null_mut();
        // SAFETY: valid target, handler, type specs, and out-pointer.
        let status = unsafe {
            InstallEventHandler(
                GetApplicationEventTarget(),
                on_hot_key,
                specs.len(),
                specs.as_ptr(),
                ptr::null_mut(),
                &mut handler,
            )
        };
        if status == NO_ERR {
            hot_keys.handler = Some(handler);
        }
    });
}

/// Replaces every registration. Returns the engine IDs that couldn't be registered.
pub fn register(shortcuts: &[HotKeyRequest]) -> Vec<u32> {
    unregister_all();
    let system = system_shortcuts::current();
    HOT_KEYS.with(|hot_keys| {
        let mut hot_keys = hot_keys.borrow_mut();
        let mut refused = Vec::new();
        let mut held = Vec::new();
        for shortcut in shortcuts {
            let carbon = CarbonChord::from_combo(&shortcut.combo)
                .filter(|carbon| !system_shortcuts::is_taken(*carbon, &system));
            let registered = carbon.and_then(|carbon| {
                let hot_key = register_chord(carbon, SIGNATURE, carbon.id)?;
                Some((carbon.id, hot_key))
            });
            match registered {
                Some((carbon_id, hot_key)) => {
                    hot_keys.registered.push(hot_key);
                    held.push((carbon_id, shortcut.clone()));
                }
                None => refused.push(shortcut.id),
            }
        }
        hot_keys.shortcuts.replace(&held);
        refused
    })
}

pub fn unregister_all() {
    HOT_KEYS.with(|hot_keys| {
        let mut hot_keys = hot_keys.borrow_mut();
        for hot_key in hot_keys.registered.drain(..) {
            // SAFETY: each reference came from a successful registration.
            unsafe { UnregisterEventHotKey(hot_key) };
        }
        hot_keys.shortcuts.replace(&[]);
    });
}

/// Watch `key_code`, the key Caps Lock arrives as, with or without any other modifier held,
/// so Caps Lock stands for the Snappy key whatever else is down. False when Carbon refused.
#[cfg(not(feature = "mac-app-store"))]
pub fn register_caps_lock_key(key_code: u32) -> bool {
    HOT_KEYS.with(|hot_keys| {
        let mut hot_keys = hot_keys.borrow_mut();
        if !hot_keys.caps_lock.is_empty() {
            return true;
        }
        let modifiers = [CMD_KEY, SHIFT_KEY, OPTION_KEY, CONTROL_KEY];
        for held in 0..1u32 << modifiers.len() {
            let mask = modifiers
                .iter()
                .enumerate()
                .filter(|(bit, _)| held & (1 << bit) != 0)
                .fold(0, |mask, (_, modifier)| mask | modifier);
            let carbon = CarbonChord {
                key_code,
                modifiers: mask,
                id: mask | key_code,
            };
            match register_chord(carbon, CAPS_LOCK_SIGNATURE, carbon.id) {
                Some(hot_key) => hot_keys.caps_lock.push(hot_key),
                None => return false,
            }
        }
        true
    })
}

#[cfg(not(feature = "mac-app-store"))]
pub fn unregister_caps_lock_key() {
    HOT_KEYS.with(|hot_keys| {
        for hot_key in hot_keys.borrow_mut().caps_lock.drain(..) {
            // SAFETY: each reference came from a successful registration.
            unsafe { UnregisterEventHotKey(hot_key) };
        }
    });
}

/// Whether macOS and other apps leave `combo` free.
pub fn is_available(combo: &Combo) -> bool {
    let Some(carbon) = CarbonChord::from_combo(combo) else {
        return false;
    };
    if system_shortcuts::is_taken(carbon, &system_shortcuts::current()) {
        return false;
    }
    match register_chord(carbon, PROBE_SIGNATURE, 1) {
        Some(hot_key) => {
            // SAFETY: just registered.
            unsafe { UnregisterEventHotKey(hot_key) };
            true
        }
        None => false,
    }
}

fn register_chord(carbon: CarbonChord, signature: u32, id: u32) -> Option<EventHotKeyRef> {
    let mut hot_key = ptr::null_mut();
    let id = EventHotKeyID { signature, id };
    // SAFETY: valid target and out-pointer.
    let status = unsafe {
        RegisterEventHotKey(
            carbon.key_code,
            carbon.modifiers,
            id,
            GetApplicationEventTarget(),
            0,
            &mut hot_key,
        )
    };
    (status == NO_ERR && !hot_key.is_null()).then_some(hot_key)
}

unsafe extern "C" fn on_hot_key(_call: EventHandlerCallRef, event: EventRef, _data: *mut c_void) -> OSStatus {
    let mut id = EventHotKeyID::default();
    // SAFETY: Carbon passes a valid hotkey event; `id` matches the requested type's size.
    let status = unsafe {
        GetEventParameter(
            event,
            EVENT_PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOT_KEY_ID,
            ptr::null_mut(),
            size_of::<EventHotKeyID>(),
            ptr::null_mut(),
            (&mut id as *mut EventHotKeyID).cast(),
        )
    };
    if status != NO_ERR {
        return EVENT_NOT_HANDLED_ERR;
    }
    // SAFETY: Carbon passes a valid event.
    let pressed = unsafe { GetEventKind(event) } == EVENT_HOT_KEY_PRESSED;
    #[cfg(not(feature = "mac-app-store"))]
    if id.signature == CAPS_LOCK_SIGNATURE {
        caps_lock::key_changed(pressed);
        return NO_ERR;
    }
    if id.signature != SIGNATURE {
        return EVENT_NOT_HANDLED_ERR;
    }
    let event = HOT_KEYS.with(|hot_keys| {
        hot_keys
            .try_borrow_mut()
            .ok()
            .and_then(|mut hot_keys| hot_keys.shortcuts.event(id.id, pressed))
    });
    let Some(event) = event else {
        return EVENT_NOT_HANDLED_ERR;
    };
    emit(event);
    NO_ERR
}

#[cfg(test)]
mod tests {
    use snappy_core::keys::{KeyChord, SnappyKey};

    use super::*;

    fn shortcuts() -> Shortcuts {
        let combo = |code: &str| KeyChord::snappy(code, []).combo(SnappyKey::DEFAULT);
        let mut shortcuts = Shortcuts::default();
        shortcuts.replace(&[
            (100, HotKeyRequest::new(7, combo("KeyJ"))),
            (
                200,
                HotKeyRequest {
                    reports_release: true,
                    ..HotKeyRequest::new(8, combo("KeyB"))
                },
            ),
        ]);
        shortcuts
    }

    #[test]
    fn a_carbon_hotkey_reports_the_shortcut_it_stands_for_and_its_release_only_when_asked() {
        let mut shortcuts = shortcuts();
        assert_eq!(shortcuts.event(100, true), Some(PlatformEvent::HotKey(7)));
        assert_eq!(shortcuts.event(100, false), None);
        assert_eq!(shortcuts.event(200, true), Some(PlatformEvent::HotKey(8)));
        assert_eq!(
            shortcuts.event(200, false),
            Some(PlatformEvent::HotKeyReleased(8))
        );
        assert_eq!(shortcuts.event(300, true), None, "not Snappy's");
    }
}
