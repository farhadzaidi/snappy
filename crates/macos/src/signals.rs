//! Requests from outside. `SIGTERM`, from `kill`, `killall Snappy`, or logging out, quits
//! the way the menu bar's Quit does, so parked windows come back first. `SIGUSR2` stops for
//! a restart, as when installing a new build: windows stay where they are, and the next
//! launch puts them back in their workspaces.

use std::ffi::c_void;

use dispatch2::{DispatchObject, DispatchQueue, DispatchRetained, DispatchSource};
use snappy_platform::PlatformEvent;

use crate::events::emit;

/// Deliver the signals to the main queue instead of ending the process. Main thread.
pub fn watch_termination() {
    watch(libc::SIGTERM, quit_requested);
    watch(libc::SIGUSR2, restart_requested);
}

fn watch(signal: libc::c_int, handler: extern "C" fn(*mut c_void)) {
    // SAFETY: ignoring the signal's default action is what lets the dispatch source see it.
    unsafe { libc::signal(signal, libc::SIG_IGN) };
    let kind = (&raw const dispatch2::_dispatch_source_type_signal).cast_mut();
    // SAFETY: a signal source takes the signal number as its handle and no mask.
    let source: DispatchRetained<DispatchSource> =
        unsafe { DispatchSource::new(kind, signal as usize, 0, Some(DispatchQueue::main())) };
    source.set_event_handler_f(handler);
    source.resume();
    // The source lives as long as the process.
    std::mem::forget(source);
}

extern "C" fn quit_requested(_context: *mut c_void) {
    emit(PlatformEvent::QuitRequested);
}

extern "C" fn restart_requested(_context: *mut c_void) {
    emit(PlatformEvent::RestartRequested);
}
