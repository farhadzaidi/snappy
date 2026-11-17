//! One daemon per session: the first launch holds the named mutex
//! `Local\Snappy.Daemon`; a second launch signals the named event
//! `Local\Snappy.OpenApp`, which carries no other command, and exits. Setting the
//! named event `Local\Snappy.Restart`, as `snappy --stop-for-restart` does, stops the
//! daemon for a restart: windows stay where they are, and the next launch puts them back
//! in their workspaces. `snappy --uninstall` sends the daemon's hidden window a close
//! request instead, which quits it as its menu's Quit does.

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use snappy_platform::PlatformEvent;
use snappy_platform::uninstall::QuitRequest;
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HANDLE, LPARAM, WAIT_ABANDONED, WAIT_EVENT, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, OpenMutexW, ReleaseMutex,
    SYNCHRONIZATION_SYNCHRONIZE, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
use windows::core::{PCWSTR, w};

use crate::app::CLASS;
use crate::handle::OwnedHandle;

const DAEMON_MUTEX: PCWSTR = w!(r"Local\Snappy.Daemon");
const OPEN_APP_EVENT: PCWSTR = w!(r"Local\Snappy.OpenApp");
const RESTART_EVENT: PCWSTR = w!(r"Local\Snappy.Restart");

pub enum Claim {
    Primary(Instance),
    /// Another daemon runs and has been asked to open the app's window.
    Secondary,
}

/// The running daemon's claim. Dropping it stops listening and releases the mutex.
pub struct Instance {
    _mutex: OwnedHandle,
    open_app: OwnedHandle,
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
        set_named_event(OPEN_APP_EVENT);
        return Ok(Claim::Secondary);
    }
    // SAFETY: creates the auto-reset event a second launch signals.
    let open_app =
        OwnedHandle(unsafe { CreateEventW(None, false, false, OPEN_APP_EVENT) }.map_err(io_error)?);
    // SAFETY: creates the auto-reset event a restart request signals.
    let restart = OwnedHandle(unsafe { CreateEventW(None, false, false, RESTART_EVENT) }.map_err(io_error)?);
    // SAFETY: an unnamed manual-reset event that stops the listener.
    let stop = OwnedHandle(unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(io_error)?);
    Ok(Claim::Primary(Instance {
        _mutex: mutex,
        open_app,
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
    if waited_for_exit(&mutex, wait) {
        RestartRequest::Stopped
    } else {
        RestartRequest::StillRunning
    }
}

/// Asks the running daemon to quit as its menu's Quit does, with a close request to its
/// hidden window, then waits up to `wait` for it to exit.
pub fn request_quit(wait: Duration) -> QuitRequest {
    // SAFETY: opens the daemon's mutex only to wait on it.
    let Ok(mutex) = (unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, DAEMON_MUTEX) }) else {
        return QuitRequest::NotRunning;
    };
    let mutex = OwnedHandle(mutex);
    // SAFETY: looks up a top-level window by its class name.
    let Ok(window) = (unsafe { FindWindowW(CLASS, PCWSTR::null()) }) else {
        return QuitRequest::StillRunning;
    };
    // SAFETY: posts a plain message to another process's window.
    if unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_err() {
        return QuitRequest::StillRunning;
    }
    if waited_for_exit(&mutex, wait) {
        QuitRequest::Quit
    } else {
        QuitRequest::StillRunning
    }
}

/// Waits up to `wait` for the daemon owning `mutex` to exit. The daemon owns the mutex for
/// its whole life, so getting it means the daemon is gone.
fn waited_for_exit(mutex: &OwnedHandle, wait: Duration) -> bool {
    let millis = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
    // SAFETY: a valid mutex handle.
    let woke = unsafe { WaitForSingleObject(mutex.0, millis) };
    let exited = woke == WAIT_OBJECT_0 || woke == WAIT_ABANDONED;
    if exited {
        // SAFETY: this thread owns the mutex after a successful wait.
        let _ = unsafe { ReleaseMutex(mutex.0) };
    }
    exited
}

impl Instance {
    /// Reports second launches to `events` as `OpenAppRequested`, and restart requests
    /// as `RestartRequested`. The listener thread sleeps in the kernel until one arrives, so
    /// it never wakes while idle.
    pub fn publish(&mut self, events: Sender<PlatformEvent>) -> std::io::Result<()> {
        let handles = [
            self.open_app.0.0 as isize,
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
        Some(0) => Some(PlatformEvent::OpenAppRequested),
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
        assert_eq!(woken_event(WAIT_OBJECT_0), Some(PlatformEvent::OpenAppRequested));
        assert_eq!(
            woken_event(WAIT_EVENT(WAIT_OBJECT_0.0 + 1)),
            Some(PlatformEvent::RestartRequested)
        );
        assert_eq!(woken_event(WAIT_EVENT(WAIT_OBJECT_0.0 + 2)), None, "stopping");
        assert_eq!(woken_event(WAIT_ABANDONED), None);
    }
}
