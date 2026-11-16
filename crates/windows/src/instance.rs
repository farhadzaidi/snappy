//! One daemon per session: the first launch holds the named mutex
//! `Local\Snappy.Daemon`; a second launch signals the named event
//! `Local\Snappy.OpenSettings`, which carries no other command, and exits. Setting the
//! named event `Local\Snappy.Restart`, as `snappy --stop-for-restart` does, stops the
//! daemon for a restart: windows stay where they are, and the next launch puts them back
//! in their workspaces.

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use snappy_platform::PlatformEvent;
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_EVENT, WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, OpenMutexW, ReleaseMutex,
    SYNCHRONIZATION_SYNCHRONIZE, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
};
use windows::core::{PCWSTR, w};

use crate::handle::OwnedHandle;

const DAEMON_MUTEX: PCWSTR = w!(r"Local\Snappy.Daemon");
const OPEN_SETTINGS_EVENT: PCWSTR = w!(r"Local\Snappy.OpenSettings");
const RESTART_EVENT: PCWSTR = w!(r"Local\Snappy.Restart");

pub enum Claim {
    Primary(Instance),
    /// Another daemon runs and has been asked to open the settings window.
    Secondary,
}

/// The running daemon's claim. Dropping it stops listening and releases the mutex.
pub struct Instance {
    _mutex: OwnedHandle,
    open_settings: OwnedHandle,
    restart: OwnedHandle,
    stop: OwnedHandle,
    listener: Option<JoinHandle<()>>,
}

fn io_error(error: windows::core::Error) -> std::io::Error {
    std::io::Error::other(error.message())
}

pub fn claim() -> std::io::Result<Claim> {
    // SAFETY: creates or opens a named mutex owned by this process.
    let mutex = OwnedHandle(unsafe { CreateMutexW(None, true, DAEMON_MUTEX) }.map_err(io_error)?);
    // SAFETY: reads the status of the call just made.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        set_named_event(OPEN_SETTINGS_EVENT);
        return Ok(Claim::Secondary);
    }
    // SAFETY: creates the auto-reset event a second launch signals.
    let open_settings =
        OwnedHandle(unsafe { CreateEventW(None, false, false, OPEN_SETTINGS_EVENT) }.map_err(io_error)?);
    // SAFETY: creates the auto-reset event a restart request signals.
    let restart = OwnedHandle(unsafe { CreateEventW(None, false, false, RESTART_EVENT) }.map_err(io_error)?);
    // SAFETY: an unnamed manual-reset event that stops the listener.
    let stop = OwnedHandle(unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(io_error)?);
    Ok(Claim::Primary(Instance {
        _mutex: mutex,
        open_settings,
        restart,
        stop,
        listener: None,
    }))
}

/// Set a named event the running daemon waits on. False when no daemon created it.
fn set_named_event(name: PCWSTR) -> bool {
    // SAFETY: opens the named event only to set it.
    let Ok(event) = (unsafe { OpenEventW(EVENT_MODIFY_STATE, false, name) }) else {
        return false;
    };
    let event = OwnedHandle(event);
    // SAFETY: a valid event handle.
    unsafe { SetEvent(event.0) }.is_ok()
}

/// What asking the running daemon to stop for a restart came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartRequest {
    /// The daemon stopped, leaving every window where it was.
    Stopped,
    /// No daemon was running.
    NotRunning,
    /// The daemon runs but is a build that can't stop for a restart.
    Unsupported,
    /// The daemon was asked but still runs after the wait.
    StillRunning,
}

/// Asks the running daemon to stop for a restart, then waits up to `wait` for it to exit.
pub fn request_restart(wait: Duration) -> RestartRequest {
    // SAFETY: opens the daemon's mutex only to wait on it.
    let Ok(mutex) = (unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, DAEMON_MUTEX) }) else {
        return RestartRequest::NotRunning;
    };
    let mutex = OwnedHandle(mutex);
    if !set_named_event(RESTART_EVENT) {
        return RestartRequest::Unsupported;
    }
    let millis = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
    // The daemon owns the mutex for its whole life, so getting it means the daemon is gone.
    // SAFETY: a valid mutex handle.
    let woke = unsafe { WaitForSingleObject(mutex.0, millis) };
    if woke == WAIT_OBJECT_0 || woke == WAIT_ABANDONED {
        // SAFETY: this thread owns the mutex after a successful wait.
        let _ = unsafe { ReleaseMutex(mutex.0) };
        RestartRequest::Stopped
    } else {
        RestartRequest::StillRunning
    }
}

impl Instance {
    /// Reports second launches to `events` as `OpenSettingsRequested`, and restart requests
    /// as `RestartRequested`. The listener thread sleeps in the kernel until one arrives, so
    /// it never wakes while idle.
    pub fn publish(&mut self, events: Sender<PlatformEvent>) -> std::io::Result<()> {
        let handles = [
            self.open_settings.0.0 as isize,
            self.restart.0.0 as isize,
            self.stop.0.0 as isize,
        ];
        let listener = std::thread::Builder::new()
            .name("snappy-instance".into())
            .spawn(move || {
                let handles = handles.map(|handle| HANDLE(handle as *mut _));
                loop {
                    // SAFETY: every handle stays open until this thread is joined.
                    let woke = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
                    let Some(event) = woken_event(woke) else {
                        return;
                    };
                    if events.send(event).is_err() {
                        return;
                    }
                }
            })?;
        self.listener = Some(listener);
        Ok(())
    }
}

/// The event a wake of the listener stands for, in the order of its handles; `None` stops it.
fn woken_event(woke: WAIT_EVENT) -> Option<PlatformEvent> {
    match woke.0.checked_sub(WAIT_OBJECT_0.0) {
        Some(0) => Some(PlatformEvent::OpenSettingsRequested),
        Some(1) => Some(PlatformEvent::RestartRequested),
        _ => None,
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        // SAFETY: a valid event handle.
        let _ = unsafe { SetEvent(self.stop.0) };
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listener_reports_second_launches_and_restart_requests_and_stops_otherwise() {
        assert_eq!(
            woken_event(WAIT_OBJECT_0),
            Some(PlatformEvent::OpenSettingsRequested)
        );
        assert_eq!(
            woken_event(WAIT_EVENT(WAIT_OBJECT_0.0 + 1)),
            Some(PlatformEvent::RestartRequested)
        );
        assert_eq!(woken_event(WAIT_EVENT(WAIT_OBJECT_0.0 + 2)), None, "stopping");
        assert_eq!(woken_event(WAIT_ABANDONED), None);
    }
}
