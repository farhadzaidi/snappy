//! Global shortcuts through Carbon `RegisterEventHotKey`. Main thread only.
//!
//! One event handler on the application target receives every press. Registrations
//! carry Snappy's signature and an ID derived from the chord; the availability probe
//! uses a separate signature so its press could never be mistaken for a shortcut. The key
//! Caps Lock arrives as, while it is the Snappy key, has a signature of its own, and its
//! releases count too.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;

use snappy_core::keys::Combo;
use snappy_platform::PlatformEvent;

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
    /// Chord-derived Carbon ID to the engine's shortcut ID.
    engine_ids: HashMap<u32, u32>,
    /// The key Caps Lock arrives as, under every combination of modifiers.
    #[cfg(not(feature = "mac-app-store"))]
    caps_lock: Vec<EventHotKeyRef>,
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
pub fn register(keys: &[(u32, Combo)]) -> Vec<u32> {
    unregister_all();
    let system = system_shortcuts::current();
    HOT_KEYS.with(|hot_keys| {
        let mut hot_keys = hot_keys.borrow_mut();
        let mut refused = Vec::new();
        for (engine_id, combo) in keys {
            let carbon =
                CarbonChord::from_combo(combo).filter(|carbon| !system_shortcuts::is_taken(*carbon, &system));
            let registered = carbon.and_then(|carbon| {
                let hot_key = register_chord(carbon, SIGNATURE, carbon.id)?;
                Some((carbon.id, hot_key))
            });
            match registered {
                Some((carbon_id, hot_key)) => {
                    hot_keys.registered.push(hot_key);
                    hot_keys.engine_ids.insert(carbon_id, *engine_id);
                }
                None => refused.push(*engine_id),
            }
        }
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
        hot_keys.engine_ids.clear();
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
    if id.signature != SIGNATURE || !pressed {
        return EVENT_NOT_HANDLED_ERR;
    }
    let engine_id = HOT_KEYS.with(|hot_keys| {
        hot_keys
            .try_borrow()
            .ok()
            .and_then(|hot_keys| hot_keys.engine_ids.get(&id.id).copied())
    });
    match engine_id {
        Some(engine_id) => {
            emit(PlatformEvent::HotKey(engine_id));
            NO_ERR
        }
        None => EVENT_NOT_HANDLED_ERR,
    }
}
