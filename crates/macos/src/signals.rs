//! A termination request from outside, such as `kill` or `killall Snappy`, quits the way
//! the menu bar's Quit does, so parked windows come back first.

use std::ffi::c_void;

use dispatch2::{DispatchObject, DispatchQueue, DispatchRetained, DispatchSource};
use snappy_platform::PlatformEvent;

use crate::events::emit;

/// Deliver SIGTERM to the main queue instead of ending the process. Main thread.
pub fn watch_termination() {
    // SAFETY: ignoring the signal's default action is what lets the dispatch source see it.
    unsafe { libc::signal(libc::SIGTERM, libc::SIG_IGN) };
    let kind = (&raw const dispatch2::_dispatch_source_type_signal).cast_mut();
    // SAFETY: a signal source takes the signal number as its handle and no mask.
    let source: DispatchRetained<DispatchSource> =
        unsafe { DispatchSource::new(kind, libc::SIGTERM as usize, 0, Some(DispatchQueue::main())) };
    source.set_event_handler_f(quit_requested);
    source.resume();
    // The source lives as long as the process.
    std::mem::forget(source);
}

extern "C" fn quit_requested(_context: *mut c_void) {
    emit(PlatformEvent::QuitRequested);
}
