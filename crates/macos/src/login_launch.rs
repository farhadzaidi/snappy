//! Whether macOS opened Snappy at login, read from the Apple event that opened it. The Mac
//! App Store build is opened at login as the app itself, without the launch agent's
//! `--background`, so this is how it tells a login from the user opening Snappy.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager};

/// `kAEOpenApplication`, the event an app is opened with.
const OPEN_APPLICATION: u32 = u32::from_be_bytes(*b"oapp");
/// `keyAEPropData`, the open event's parameter that says why the app was opened.
const PROPERTY_DATA: u32 = u32::from_be_bytes(*b"prdt");
/// `keyAELaunchedAsLogInItem`, that parameter at login.
const LAUNCHED_AS_LOGIN_ITEM: u32 = u32::from_be_bytes(*b"lgit");

/// Whether the event being handled opened Snappy at login. Main thread, from
/// `applicationDidFinishLaunching:`, while the open event is still the current one.
pub fn launched_at_login() -> bool {
    let Some(event) = NSAppleEventManager::sharedAppleEventManager().currentAppleEvent() else {
        return false;
    };
    // The generated bindings for these two need another crate, `objc2-core-services`, for
    // their four-character code types, which are plain `u32`s.
    // SAFETY: both take and return plain values on a valid descriptor.
    let (event_id, reason): (u32, Option<Retained<NSAppleEventDescriptor>>) = unsafe {
        (
            msg_send![&event, eventID],
            msg_send![&event, paramDescriptorForKeyword: PROPERTY_DATA],
        )
    };
    is_login_open(event_id, reason.map(|reason| reason.enumCodeValue()))
}

fn is_login_open(event_id: u32, reason: Option<u32>) -> bool {
    event_id == OPEN_APPLICATION && reason == Some(LAUNCHED_AS_LOGIN_ITEM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_open_event_marked_as_a_login_item_is_a_login() {
        assert!(is_login_open(OPEN_APPLICATION, Some(LAUNCHED_AS_LOGIN_ITEM)));
    }

    #[test]
    fn an_open_event_without_the_login_mark_is_the_user_opening_snappy() {
        assert!(!is_login_open(OPEN_APPLICATION, None));
        assert!(!is_login_open(
            OPEN_APPLICATION,
            Some(u32::from_be_bytes(*b"null"))
        ));
    }

    #[test]
    fn other_events_are_never_a_login() {
        let reopen = u32::from_be_bytes(*b"rapp");
        assert!(!is_login_open(reopen, Some(LAUNCHED_AS_LOGIN_ITEM)));
    }
}
