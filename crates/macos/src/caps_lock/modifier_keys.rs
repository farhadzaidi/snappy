//! The Modifier Keys setting (System Settings, Keyboard, Keyboard Shortcuts, Modifier Keys).
//! System Settings keeps each keyboard's pairs in the current host's global preferences, one
//! `com.apple.keyboard.modifiermapping.<keyboard>` entry per keyboard, in the same form as the
//! user key mapping. The HID services they are applied to don't report them back.

use objc2_core_foundation::{
    CFPreferencesCopyKeyList, CFPreferencesCopyValue, CFString, kCFPreferencesAnyApplication,
    kCFPreferencesCurrentHost, kCFPreferencesCurrentUser,
};

use super::hid::mappings;
use super::key_mapping::KeyMapping;

const PREFIX: &str = "com.apple.keyboard.modifiermapping.";

/// Every keyboard's modifier key pairs.
pub fn pairs() -> Vec<KeyMapping> {
    // SAFETY: reading immutable framework constants.
    let (application, user, host) = unsafe {
        (
            kCFPreferencesAnyApplication,
            kCFPreferencesCurrentUser,
            kCFPreferencesCurrentHost,
        )
    };
    let Some(keys) = CFPreferencesCopyKeyList(application, user, host) else {
        return Vec::new();
    };
    // SAFETY: preference keys are strings; each is checked before use.
    let keys = unsafe { keys.cast_unchecked::<objc2_core_foundation::CFType>() };
    keys.iter()
        .filter_map(|key| key.downcast::<CFString>().ok())
        .filter(|key| key.to_string().starts_with(PREFIX))
        .filter_map(|key| CFPreferencesCopyValue(&key, application, user, host))
        .flat_map(|value| mappings(&value))
        .collect()
}
