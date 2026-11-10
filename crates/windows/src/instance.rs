//! One daemon per session: the first launch holds the named mutex
//! `Local\Snappy.Daemon`; a second launch signals the named event
//! `Local\Snappy.OpenSettings`, which carries no other command, and exits.

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use snappy_platform::PlatformEvent;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForMultipleObjects,
};
use windows::core::{PCWSTR, w};

use crate::process::OwnedHandle;

const DAEMON_MUTEX: PCWSTR = w!(r"Local\Snappy.Daemon");
const OPEN_SETTINGS_EVENT: PCWSTR = w!(r"Local\Snappy.OpenSettings");

pub enum Claim {
    Primary(Instance),
    /// Another daemon runs and has been asked to open the settings window.
    Secondary,
}

/// The running daemon's claim. Dropping it stops listening and releases the mutex.
pub struct Instance {
    _mutex: OwnedHandle,
    open_settings: OwnedHandle,
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
        signal_running_daemon();
        return Ok(Claim::Secondary);
    }
    // SAFETY: creates the auto-reset event a second launch signals.
    let open_settings =
        OwnedHandle(unsafe { CreateEventW(None, false, false, OPEN_SETTINGS_EVENT) }.map_err(io_error)?);
    // SAFETY: an unnamed manual-reset event that stops the listener.
    let stop = OwnedHandle(unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(io_error)?);
    Ok(Claim::Primary(Instance {
        _mutex: mutex,
        open_settings,
        stop,
        listener: None,
    }))
}

fn signal_running_daemon() {
    // SAFETY: opens the named event only to set it.
    if let Ok(event) = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, OPEN_SETTINGS_EVENT) } {
        let event = OwnedHandle(event);
        // SAFETY: a valid event handle.
        let _ = unsafe { SetEvent(event.0) };
    }
}

impl Instance {
    /// Reports second launches to `events` as `OpenSettingsRequested`. The listener
    /// thread sleeps in the kernel until one arrives, so it never wakes while idle.
    pub fn publish(&mut self, events: Sender<PlatformEvent>) -> std::io::Result<()> {
        let handles = [self.open_settings.0.0 as isize, self.stop.0.0 as isize];
        let listener = std::thread::Builder::new()
            .name("snappy-instance".into())
            .spawn(move || {
                let handles = handles.map(|handle| HANDLE(handle as *mut _));
                loop {
                    // SAFETY: both handles stay open until this thread is joined.
                    let woke = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
                    if woke != WAIT_OBJECT_0 || events.send(PlatformEvent::OpenSettingsRequested).is_err() {
                        return;
                    }
                }
            })?;
        self.listener = Some(listener);
        Ok(())
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
