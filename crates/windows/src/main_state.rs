//! The main thread's state. Window procedures and hook callbacks reach it through a
//! thread-local; a callback that arrives while it is in use, such as during a modal menu,
//! is dropped rather than nested.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use snappy_core::keys::Mods;
use snappy_platform::PlatformEvent;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::HHOOK;

use crate::desktop_guard::Guard;
use crate::dispatch::Job;
use crate::events::Hooks;
use crate::hotkeys::HotKeyTable;
use crate::overlay::Overlays;
use crate::shared::Shared;
use crate::tray::Tray;

pub struct Main {
    /// The hidden window that receives hotkeys, broadcasts, and tray callbacks.
    pub window: HWND,
    pub events: Sender<PlatformEvent>,
    pub shared: Arc<Shared>,
    pub jobs: Receiver<Job>,
    pub hooks: Hooks,
    /// The low-level keyboard and mouse hooks, installed only while drag monitoring is on.
    pub keyboard: Option<HHOOK>,
    pub mouse: Option<HHOOK>,
    /// The window in a move or size loop.
    pub drag: Option<HWND>,
    /// Modifiers last reported during drag monitoring.
    pub held: Mods,
    pub hotkeys: HotKeyTable,
    pub overlays: Overlays,
    pub tray: Tray,
    pub guard: Guard,
}

thread_local! {
    static MAIN: RefCell<Option<Main>> = const { RefCell::new(None) };
}

pub fn install(main: Main) {
    MAIN.with(|cell| *cell.borrow_mut() = Some(main));
}

pub fn take() -> Option<Main> {
    MAIN.with(|cell| cell.try_borrow_mut().ok()?.take())
}

/// Runs `f` with the state, or returns `None` when it's absent or already in use.
pub fn with_main<R>(f: impl FnOnce(&mut Main) -> R) -> Option<R> {
    MAIN.with(|cell| {
        let mut state = cell.try_borrow_mut().ok()?;
        state.as_mut().map(f)
    })
}

/// Runs every queued job.
pub fn run_jobs() {
    while let Some(job) = with_main(|main| main.jobs.try_recv().ok()).flatten() {
        with_main(job);
    }
}

impl Main {
    pub fn emit(&self, event: PlatformEvent) {
        let _ = self.events.send(event);
    }
}
