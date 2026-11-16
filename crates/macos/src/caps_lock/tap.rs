//! Caps Lock held, on the main thread. macOS makes Caps Lock arrive as F18 (`key_mapping`);
//! a Carbon hotkey on F18 says when it goes down and up, and only while it is down does an
//! event tap see other keys and buttons, so Snappy observes no keys at all at idle.
//!
//! While F18 is down the tap adds the Snappy key's modifier flags to every other key and
//! button event, rather than Snappy running its shortcuts itself: everything downstream, its
//! own Carbon shortcuts and drag monitors included, then sees exactly what holding the
//! modifiers gives, and there is only one way a shortcut fires. The tap reports that
//! something else was used, never what.
//!
//! The tap also sees F18 itself come up, before the hotkey does, and takes that as the
//! release, so a hotkey release that never arrives can't leave the tap adding the Snappy
//! key's flags to every key and click.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::{self, NonNull};

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
};
use snappy_core::keys::{KeyCode, Mods};
use snappy_platform::{CapsLockInput, PlatformEvent};

use crate::events::emit;
use crate::hotkeys;
use crate::keycodes::{event_flags, virtual_key_code};

/// The events the tap sees while Caps Lock is down: keys, modifier changes, and buttons.
const OBSERVED: [CGEventType; 12] = [
    CGEventType::KeyDown,
    CGEventType::KeyUp,
    CGEventType::FlagsChanged,
    CGEventType::LeftMouseDown,
    CGEventType::LeftMouseUp,
    CGEventType::LeftMouseDragged,
    CGEventType::RightMouseDown,
    CGEventType::RightMouseUp,
    CGEventType::RightMouseDragged,
    CGEventType::OtherMouseDown,
    CGEventType::OtherMouseUp,
    CGEventType::OtherMouseDragged,
];

struct Tap {
    port: CFRetained<CFMachPort>,
    source: CFRetained<CFRunLoopSource>,
}

#[derive(Default)]
struct Held {
    tap: Option<Tap>,
    /// The flags of the modifiers Caps Lock stands for.
    flags: u64,
    /// The virtual key code macOS gives Caps Lock.
    key: Option<i64>,
    down: bool,
    /// Something else was used during this press, already reported.
    reported_other_input: bool,
    /// A modifier change went out with the Snappy key's flags added during this press.
    changed_modifiers: bool,
}

thread_local! {
    static HELD: RefCell<Held> = RefCell::new(Held::default());
}

/// Hold Caps Lock as `modifiers`: watch its key and get the tap ready, disabled until the key
/// goes down. False when either couldn't be set up, as without Accessibility.
pub fn hold_as(modifiers: Mods) -> bool {
    let Some(key) = virtual_key_code(&KeyCode::new("F18")) else {
        return false;
    };
    HELD.with(|held| {
        let mut held = held.borrow_mut();
        held.flags = event_flags(modifiers);
        held.key = Some(i64::from(key));
        if held.tap.is_none() {
            held.tap = create_tap();
        }
        held.tap.is_some() && hotkeys::register_caps_lock_key(key)
    })
}

/// Stop watching Caps Lock and remove the tap.
pub fn stop() {
    hotkeys::unregister_caps_lock_key();
    let tap = HELD.with(|held| std::mem::take(&mut *held.borrow_mut()).tap);
    if let Some(tap) = tap {
        // SAFETY: reading an immutable framework constant.
        let mode = unsafe { kCFRunLoopCommonModes };
        if let Some(main) = CFRunLoop::main() {
            main.remove_source(Some(&tap.source), mode);
        }
        tap.port.invalidate();
    }
}

/// Caps Lock went down or up, as its hotkey or the tap reports, whichever comes first.
/// Repeats while down, and the second report of a release, are ignored.
pub fn key_changed(down: bool) {
    let changed = HELD.with(|held| {
        let mut held = held.borrow_mut();
        if held.down == down || held.tap.is_none() {
            return None;
        }
        held.down = down;
        held.reported_other_input = false;
        if let Some(tap) = &held.tap {
            CGEvent::tap_enable(&tap.port, down);
        }
        Some(std::mem::take(&mut held.changed_modifiers))
    });
    match changed {
        Some(_) if down => emit(PlatformEvent::CapsLock(CapsLockInput::Pressed)),
        Some(changed_modifiers) => {
            if changed_modifiers {
                announce_real_modifiers();
            }
            emit(PlatformEvent::CapsLock(CapsLockInput::Released));
        }
        None => {}
    }
}

fn create_tap() -> Option<Tap> {
    let mask = OBSERVED.iter().fold(0u64, |mask, kind| mask | (1u64 << kind.0));
    // SAFETY: the callback matches the tap's signature and uses no user data.
    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::HIDEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            mask,
            Some(on_event),
            ptr::null_mut(),
        )
    }?;
    // The tap starts enabled; it runs only while Caps Lock is down.
    CGEvent::tap_enable(&port, false);
    let source = CFMachPort::new_run_loop_source(None, Some(&port), 0)?;
    // SAFETY: reading an immutable framework constant.
    let mode = unsafe { kCFRunLoopCommonModes };
    CFRunLoop::main()?.add_source(Some(&source), mode);
    Some(Tap { port, source })
}

/// Apps saw modifier changes with the Snappy key's flags in them, and no key tells them it
/// is gone, so one more change carries the flags really held.
fn announce_real_modifiers() {
    let Some(event) = CGEvent::new(None) else {
        return;
    };
    CGEvent::set_type(Some(&event), CGEventType::FlagsChanged);
    CGEvent::set_flags(
        Some(&event),
        CGEventSource::flags_state(CGEventSourceStateID::HIDSystemState),
    );
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
}

/// What one event the tap saw means for the press.
enum Seen {
    Nothing,
    /// Another key or a button was used, the first time this press.
    OtherInput,
    /// Caps Lock itself came up.
    Released,
}

unsafe extern "C-unwind" fn on_event(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    _user_info: *mut c_void,
) -> *mut CGEvent {
    // SAFETY: the tap passes a valid event for the duration of the call.
    let event_ref = unsafe { event.as_ref() };
    let seen = HELD.with(|held| {
        let Ok(mut held) = held.try_borrow_mut() else {
            return Seen::Nothing;
        };
        if kind == CGEventType::TapDisabledByTimeout || kind == CGEventType::TapDisabledByUserInput {
            if let (true, Some(tap)) = (held.down, &held.tap) {
                CGEvent::tap_enable(&tap.port, true);
            }
            return Seen::Nothing;
        }
        if !held.down {
            return Seen::Nothing;
        }
        let is_key = matches!(
            kind,
            CGEventType::KeyDown | CGEventType::KeyUp | CGEventType::FlagsChanged
        );
        if is_key
            && Some(CGEvent::integer_value_field(
                Some(event_ref),
                CGEventField::KeyboardEventKeycode,
            )) == held.key
        {
            return if kind == CGEventType::KeyUp {
                Seen::Released
            } else {
                Seen::Nothing
            };
        }
        let flags = CGEvent::flags(Some(event_ref));
        CGEvent::set_flags(
            Some(event_ref),
            flags | CGEventFlags::from_bits_retain(held.flags),
        );
        if kind == CGEventType::FlagsChanged {
            held.changed_modifiers = true;
        }
        let used = match kind {
            CGEventType::KeyDown => {
                CGEvent::integer_value_field(Some(event_ref), CGEventField::KeyboardEventAutorepeat) == 0
            }
            CGEventType::LeftMouseDown | CGEventType::RightMouseDown | CGEventType::OtherMouseDown => true,
            _ => false,
        };
        let first = used && !held.reported_other_input;
        held.reported_other_input |= used;
        if first { Seen::OtherInput } else { Seen::Nothing }
    });
    match seen {
        Seen::Nothing => {}
        Seen::OtherInput => emit(PlatformEvent::CapsLock(CapsLockInput::OtherInput)),
        // Ends the press and disables the tap; the hotkey's own release then changes nothing.
        Seen::Released => key_changed(false),
    }
    event.as_ptr()
}
