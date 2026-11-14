//! The HID event system: reading and writing its user key mapping, and turning Caps Lock on
//! or off. Not the main thread: each call is a
//! round trip to the HID service.

use std::ptr::NonNull;

use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFString, CFType};

use super::key_mapping::KeyMapping;
use crate::ffi::hid::{
    HID_CAPS_LOCK_STATE, HID_PARAM_CONNECT_TYPE, HID_SYSTEM_CLASS, IOHIDEventSystemClientCopyProperty,
    IOHIDEventSystemClientCreateSimpleClient, IOHIDEventSystemClientSetProperty, IOHIDGetModifierLockState,
    IOHIDSetModifierLockState, IOObjectRelease, IOServiceClose, IOServiceGetMatchingService,
    IOServiceMatching, IOServiceOpen, IoConnect, KERN_SUCCESS, MAIN_PORT_DEFAULT, mach_task_self_,
};

const USER_KEY_MAPPING: &str = "UserKeyMapping";
const SOURCE: &str = "HIDKeyboardModifierMappingSrc";
const DESTINATION: &str = "HIDKeyboardModifierMappingDst";

/// A client of the HID event system, allowed to read and write its user key mapping.
pub struct EventSystem(CFRetained<CFType>);

impl EventSystem {
    pub fn connect() -> Option<EventSystem> {
        // SAFETY: a null allocator is the default one; the client is a +1 CF object.
        let client = unsafe { IOHIDEventSystemClientCreateSimpleClient(std::ptr::null()) };
        // SAFETY: as above, the reference is ours to release.
        NonNull::new(client.cast_mut()).map(|client| EventSystem(unsafe { CFRetained::from_raw(client) }))
    }

    /// The pairs every keyboard's keys go through.
    pub fn user_key_mapping(&self) -> Vec<KeyMapping> {
        let key = CFString::from_static_str(USER_KEY_MAPPING);
        // SAFETY: a live client and key; the result is +1 or null.
        let value = unsafe { IOHIDEventSystemClientCopyProperty(self.client(), &key) };
        owned(value).map_or_else(Vec::new, |value| mappings(&value))
    }

    /// Replace the pairs every keyboard's keys go through. False when the HID service refused.
    pub fn set_user_key_mapping(&self, mappings: &[KeyMapping]) -> bool {
        let (source, destination) = (
            CFString::from_static_str(SOURCE),
            CFString::from_static_str(DESTINATION),
        );
        let entries: Vec<CFRetained<CFDictionary<CFString, CFNumber>>> = mappings
            .iter()
            .map(|mapping| {
                let values = [usage(mapping.source), usage(mapping.destination)];
                CFDictionary::from_slices(&[&*source, &*destination], &[&*values[0], &*values[1]])
            })
            .collect();
        let array = CFArray::from_retained_objects(&entries);
        let key = CFString::from_static_str(USER_KEY_MAPPING);
        // SAFETY: a live client, key, and property; the call copies what it keeps.
        unsafe { IOHIDEventSystemClientSetProperty(self.client(), &key, array.as_ref()) != 0 }
    }

    fn client(&self) -> *const CFType {
        &*self.0
    }
}

/// Take ownership of a +1 reference a `Copy` function returned.
fn owned(value: *const CFType) -> Option<CFRetained<CFType>> {
    // SAFETY: callers pass only +1 references, or null.
    NonNull::new(value.cast_mut()).map(|value| unsafe { CFRetained::from_raw(value) })
}

fn usage(value: u64) -> CFRetained<CFNumber> {
    CFNumber::new_i64(i64::from_ne_bytes(value.to_ne_bytes()))
}

/// The pairs in an array of source and destination dictionaries; anything else in it is
/// skipped.
pub(super) fn mappings(value: &CFType) -> Vec<KeyMapping> {
    let Some(array) = value.downcast_ref::<CFArray>() else {
        return Vec::new();
    };
    // SAFETY: the elements are CF objects; each is checked before use.
    let array = unsafe { array.cast_unchecked::<CFType>() };
    let (source, destination) = (
        CFString::from_static_str(SOURCE),
        CFString::from_static_str(DESTINATION),
    );
    array
        .iter()
        .filter_map(|entry| {
            let entry = entry.downcast_ref::<CFDictionary>()?;
            // SAFETY: the keys are strings and the values CF objects, each checked before use.
            let entry = unsafe { entry.cast_unchecked::<CFString, CFType>() };
            let read = |key: &CFString| {
                let number = entry.get(key)?;
                let value = number.downcast_ref::<CFNumber>()?.as_i64()?;
                Some(u64::from_ne_bytes(value.to_ne_bytes()))
            };
            Some(KeyMapping {
                source: read(&source)?,
                destination: read(&destination)?,
            })
        })
        .collect()
}

/// Turn Caps Lock on or off through the HID system, which sets every keyboard's light to
/// match. False when it couldn't be reached.
pub fn toggle_caps_lock() -> bool {
    // SAFETY: a valid class name; matching dictionaries are consumed by the lookup.
    let service = unsafe {
        IOServiceGetMatchingService(MAIN_PORT_DEFAULT, IOServiceMatching(HID_SYSTEM_CLASS.as_ptr()))
    };
    if service == 0 {
        return false;
    }
    let mut connect: IoConnect = 0;
    // SAFETY: a live service and out-pointer; the service reference is released either way.
    let opened = unsafe {
        let opened = IOServiceOpen(service, mach_task_self_, HID_PARAM_CONNECT_TYPE, &mut connect);
        IOObjectRelease(service);
        opened
    };
    if opened != KERN_SUCCESS {
        return false;
    }
    let mut on = false;
    // SAFETY: an open connection and out-pointer; the connection is closed either way.
    unsafe {
        let toggled = IOHIDGetModifierLockState(connect, HID_CAPS_LOCK_STATE, &mut on) == KERN_SUCCESS
            && IOHIDSetModifierLockState(connect, HID_CAPS_LOCK_STATE, !on) == KERN_SUCCESS;
        IOServiceClose(connect);
        toggled
    }
}
