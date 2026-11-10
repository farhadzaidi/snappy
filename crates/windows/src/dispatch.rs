//! Running work on the main thread from the engine thread. Hotkeys, hooks, overlays, and
//! the notification-area icon belong to the thread that created them, so the engine
//! queues closures and wakes the main window to run them.

use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::main_state::Main;

/// Run the queued jobs.
pub const WM_RUN: u32 = WM_APP + 1;
/// Leave the message loop.
pub const WM_STOP: u32 = WM_APP + 2;
/// A notification-area icon callback.
pub const WM_TRAY: u32 = WM_APP + 3;

/// How long a call waits for the main thread, which never blocks for long.
const CALL_TIMEOUT: Duration = Duration::from_secs(2);

pub type Job = Box<dyn FnOnce(&mut Main) + Send>;

#[derive(Clone)]
pub struct Dispatcher {
    /// The main window, as an integer so the dispatcher can cross threads.
    window: isize,
    jobs: Sender<Job>,
}

impl Dispatcher {
    pub fn new(window: HWND, jobs: Sender<Job>) -> Dispatcher {
        Dispatcher {
            window: window.0 as isize,
            jobs,
        }
    }

    pub fn window(&self) -> HWND {
        HWND(self.window as *mut _)
    }

    /// Queue `job` without waiting for it.
    pub fn post(&self, job: impl FnOnce(&mut Main) + Send + 'static) {
        if self.jobs.send(Box::new(job)).is_ok() {
            // SAFETY: posting to a window this process owns; a destroyed one fails harmlessly.
            let _ = unsafe { PostMessageW(Some(self.window()), WM_RUN, WPARAM(0), LPARAM(0)) };
        }
    }

    /// Run `job` on the main thread and return its result, or `None` when the main
    /// thread has stopped. Never call this on the main thread.
    pub fn call<R: Send + 'static>(&self, job: impl FnOnce(&mut Main) -> R + Send + 'static) -> Option<R> {
        let (reply, result) = mpsc::sync_channel(1);
        self.post(move |main| {
            let _ = reply.send(job(main));
        });
        result.recv_timeout(CALL_TIMEOUT).ok()
    }
}
