//! Requests from outside. `SIGTERM`, from `kill`, `killall Snappy`, or logging out, quits
//! the way the menu bar's Quit does, so parked windows come back first, and so do `SIGINT`
//! and `SIGHUP`, from Ctrl+C or closing the terminal Snappy was started from. `SIGUSR2` stops
//! for a restart, as when installing a new build: windows stay where they are, and the next
//! launch puts them back in their workspaces.

use std::ffi::c_void;

use dispatch2::{DispatchObject, DispatchQueue, DispatchRetained, DispatchSource};
use snappy_platform::PlatformEvent;

use crate::events::emit;

/// Deliver the signals to the main queue instead of ending the process. Main thread.
pub fn watch_termination() {
    watch(libc::SIGTERM, quit_requested);
    watch(libc::SIGUSR2, restart_requested);
    // A launcher that ignores these, as `nohup` does `SIGHUP`, means Snappy to outlive the
    // terminal, so an ignored one stays ignored.
    for signal in [libc::SIGINT, libc::SIGHUP] {
        if !is_ignored(signal) {
            watch(signal, quit_requested);
        }
    }
}

fn is_ignored(signal: libc::c_int) -> bool {
    // SAFETY: a null new action only reads the current one into `current`.
    unsafe {
        let mut current: libc::sigaction = std::mem::zeroed();
        libc::sigaction(signal, std::ptr::null(), &mut current) == 0 && current.sa_sigaction == libc::SIG_IGN
    }
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

#[cfg(test)]
mod tests {
    use super::is_ignored;

    #[test]
    fn a_hangup_the_launcher_ignores_reads_as_ignored() {
        // SAFETY: ignoring a signal has no memory-safety preconditions.
        unsafe { libc::signal(libc::SIGHUP, libc::SIG_IGN) };
        assert!(is_ignored(libc::SIGHUP));
    }
}
