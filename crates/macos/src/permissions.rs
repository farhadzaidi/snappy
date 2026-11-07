//! Accessibility trust: the grant poll and revocation detection.
//!
//! The main thread remembers the last trust value it reported and emits
//! `PermissionChanged` only when it flips.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSTimer, NSURL};
use snappy_platform::PlatformEvent;

use crate::events::emit;
use crate::thresholds::{PERMISSION_POLL, PERMISSION_RECHECK_DELAY};
use crate::{ax, main_thread};

const SETTINGS_PAGE: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

thread_local! {
    static LAST_TRUSTED: Cell<bool> = const { Cell::new(false) };
    static POLL: RefCell<Option<Retained<NSTimer>>> = const { RefCell::new(None) };
}

/// Records the current trust value. Main thread.
pub fn start() {
    LAST_TRUSTED.set(ax::is_trusted());
}

/// Reports a change in trust. Main thread.
pub fn recheck() {
    let trusted = ax::is_trusted();
    if LAST_TRUSTED.replace(trusted) != trusted {
        emit(PlatformEvent::PermissionChanged);
    }
}

/// Re-checks after trust has had time to settle, for the distributed notification
/// and for Accessibility calls that fail with `kAXErrorAPIDisabled`. Any thread.
pub fn recheck_soon() {
    main_thread::after(PERMISSION_RECHECK_DELAY, |_| recheck());
}

/// Polls every second while a grant is awaited, and stops otherwise. Main thread.
pub fn set_polling(on: bool) {
    POLL.with(|poll| {
        let mut poll = poll.borrow_mut();
        match (on, poll.is_some()) {
            (true, false) => *poll = Some(schedule_poll()),
            (false, true) => {
                if let Some(timer) = poll.take() {
                    timer.invalidate();
                }
            }
            _ => {}
        }
    });
}

fn schedule_poll() -> Retained<NSTimer> {
    let tick = RcBlock::new(|_: NonNull<NSTimer>| recheck());
    // SAFETY: the block is 'static and the timer runs on this thread's run loop.
    unsafe {
        NSTimer::scheduledTimerWithTimeInterval_repeats_block(PERMISSION_POLL.as_secs_f64(), true, &tick)
    }
}

/// Shows the system's Accessibility request, which also adds Snappy to the Accessibility
/// list so the user only has to switch it on. Main thread.
pub fn request_access() {
    let prompt = NSString::from_str(ax::TRUSTED_CHECK_OPTION_PROMPT);
    let options = NSDictionary::from_slices(&[&*prompt], &[&*NSNumber::new_bool(true)]);
    ax::is_trusted_with_options(&options);
}

/// Opens System Settings at the Accessibility list. Any thread.
pub fn open_settings_page() -> bool {
    NSURL::URLWithString(&NSString::from_str(SETTINGS_PAGE))
        .is_some_and(|url| NSWorkspace::sharedWorkspace().openURL(&url))
}
