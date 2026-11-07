//! Carbon Event Manager declarations for global hotkeys, as documented in
//! `CarbonEvents.h`. All of these run on the main thread.

use std::ffi::c_void;

pub type OSStatus = i32;
pub type EventTargetRef = *mut c_void;
pub type EventHandlerRef = *mut c_void;
pub type EventHandlerCallRef = *mut c_void;
pub type EventHotKeyRef = *mut c_void;
pub type EventRef = *mut c_void;

/// On 64-bit systems a universal procedure pointer is the function itself.
pub type EventHandlerUPP =
    unsafe extern "C" fn(call: EventHandlerCallRef, event: EventRef, user_data: *mut c_void) -> OSStatus;

pub const NO_ERR: OSStatus = 0;
pub const EVENT_NOT_HANDLED_ERR: OSStatus = -9874;

/// `kEventClassKeyboard`, the four-character code `'keyb'`.
pub const EVENT_CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
/// `kEventHotKeyPressed`.
pub const EVENT_HOT_KEY_PRESSED: u32 = 5;
/// `kEventParamDirectObject`, `'----'`.
pub const EVENT_PARAM_DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
/// `typeEventHotKeyID`, `'hkid'`.
pub const TYPE_EVENT_HOT_KEY_ID: u32 = u32::from_be_bytes(*b"hkid");

/// Modifier masks from `Events.h`.
pub const CMD_KEY: u32 = 1 << 8;
pub const SHIFT_KEY: u32 = 1 << 9;
pub const OPTION_KEY: u32 = 1 << 11;
pub const CONTROL_KEY: u32 = 1 << 12;

#[repr(C)]
pub struct EventTypeSpec {
    pub event_class: u32,
    pub event_kind: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EventHotKeyID {
    pub signature: u32,
    pub id: u32,
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    pub fn GetApplicationEventTarget() -> EventTargetRef;
    pub fn InstallEventHandler(
        target: EventTargetRef,
        handler: EventHandlerUPP,
        type_count: usize,
        types: *const EventTypeSpec,
        user_data: *mut c_void,
        handler_ref: *mut EventHandlerRef,
    ) -> OSStatus;
    pub fn RegisterEventHotKey(
        key_code: u32,
        modifiers: u32,
        id: EventHotKeyID,
        target: EventTargetRef,
        options: u32,
        hot_key: *mut EventHotKeyRef,
    ) -> OSStatus;
    pub fn UnregisterEventHotKey(hot_key: EventHotKeyRef) -> OSStatus;
    pub fn GetEventParameter(
        event: EventRef,
        name: u32,
        desired_type: u32,
        actual_type: *mut u32,
        buffer_size: usize,
        actual_size: *mut usize,
        data: *mut c_void,
    ) -> OSStatus;
}
