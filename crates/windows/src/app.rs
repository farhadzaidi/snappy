//! The main thread: a hidden top-level window and its message loop. A message-only
//! window would miss broadcasts, and this one needs display, work-area, power, session,
//! and taskbar-restart broadcasts as well as hotkeys and tray callbacks.
//!
//! Quitting is always the engine's job, so parked windows return first: a close
//! request, the end of the session, and a console Ctrl+C or close all ask the engine to
//! quit, and the session-end and console paths wait until it has.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use snappy_platform::PlatformEvent;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
    SetConsoleCtrlHandler,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, MSG, PBT_APMSUSPEND,
    PostMessageW, PostQuitMessage, RegisterWindowMessageW, SPI_SETWORKAREA, TranslateMessage, WM_CLOSE,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ENDSESSION, WM_HOTKEY, WM_POWERBROADCAST, WM_QUERYENDSESSION,
    WM_SETTINGCHANGE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{BOOL, w};

use crate::class;
use crate::desktop_guard::Guard;
use crate::dispatch::{Dispatcher, WM_RUN, WM_STOP, WM_TRAY};
use crate::error::WindowsError;
use crate::events::Hooks;
use crate::hotkeys::HotKeyTable;
use crate::main_state::{self, Main, run_jobs, with_main};
use crate::overlay::Overlays;
use crate::platform::WindowsPlatform;
use crate::shared::{Shared, lock};
use crate::tray::{self, Tray};

const CLASS: windows::core::PCWSTR = w!("Snappy.Daemon");
/// How long the session end or a console close waits for parked windows to return.
/// Windows ends the process soon after either handler returns.
const QUIT_WAIT: Duration = Duration::from_millis(4500);
const QUIT_POLL: Duration = Duration::from_millis(20);

/// Set once the engine has finished, so waiting quit paths can let the process end.
#[derive(Default)]
struct Finished {
    done: Mutex<bool>,
    changed: Condvar,
}

impl Finished {
    fn set(&self) {
        *lock(&self.done) = true;
        self.changed.notify_all();
    }

    /// Waits up to `timeout`; true once finished.
    fn wait(&self, timeout: Duration) -> bool {
        let done = lock(&self.done);
        let (done, _) = self
            .changed
            .wait_timeout_while(done, timeout, |done| !*done)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *done
    }
}

/// Owns the main thread's side of the adapter. Create it on the thread that will call
/// `run`.
pub struct App {
    window: HWND,
    shared: Arc<Shared>,
    dispatcher: Dispatcher,
    finished: Arc<Finished>,
}

/// Asks a running `App::run` to return. Callable from any thread.
#[derive(Clone)]
pub struct Stopper {
    window: isize,
    finished: Arc<Finished>,
}

/// Where the console handler, which runs on its own thread, sends quit requests.
static CONSOLE: OnceLock<(Mutex<Sender<PlatformEvent>>, Arc<Finished>)> = OnceLock::new();

/// `RegisterWindowMessage("TaskbarCreated")`, broadcast when Explorer restarts.
static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();

impl App {
    pub fn new(events: Sender<PlatformEvent>) -> Result<App, WindowsError> {
        // SAFETY: process-wide settings made before any window exists. Both fail
        // harmlessly when already set, for example by a manifest.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let window = create_window()?;
        let shared = Arc::new(Shared::default());
        let (jobs, queue) = mpsc::channel();
        let dispatcher = Dispatcher::new(window, jobs);
        let finished = Arc::new(Finished::default());

        let mut tray = Tray::default();
        tray.add(window);
        main_state::install(Main {
            window,
            events: events.clone(),
            shared: Arc::clone(&shared),
            jobs: queue,
            hooks: Hooks::install(),
            keyboard: None,
            mouse: None,
            drag: None,
            held: Default::default(),
            hotkeys: HotKeyTable::default(),
            overlays: Overlays::default(),
            tray,
            guard: Guard::default(),
        });
        // SAFETY: registers a message name; the value is the same in every process.
        TASKBAR_CREATED.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) });
        let _ = CONSOLE.set((Mutex::new(events), Arc::clone(&finished)));
        // SAFETY: a static handler; without a console it is never called.
        let _ = unsafe { SetConsoleCtrlHandler(Some(on_console), true) };

        Ok(App {
            window,
            shared,
            dispatcher,
            finished,
        })
    }

    /// The engine thread's handle.
    pub fn platform(&self) -> WindowsPlatform {
        WindowsPlatform::new(Arc::clone(&self.shared), self.dispatcher.clone())
    }

    pub fn stopper(&self) -> Stopper {
        Stopper {
            window: self.window.0 as isize,
            finished: Arc::clone(&self.finished),
        }
    }

    /// Runs the message loop until `Stopper::stop`, then removes everything Snappy
    /// installed.
    pub fn run(self) {
        let mut message = MSG::default();
        // SAFETY: the standard loop; GetMessageW returns 0 for WM_QUIT and -1 on error.
        unsafe {
            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        self.teardown();
    }

    fn teardown(self) {
        run_jobs();
        if let Some(mut main) = main_state::take() {
            main.set_input_hooks(false);
            main.unregister_hotkeys();
            main.hooks.remove_all();
            main.overlays.close_all();
            main.tray.remove();
            main.guard.close();
        }
        // SAFETY: the window belongs to this thread; COM was initialized in `new`.
        unsafe {
            let _ = DestroyWindow(self.window);
            CoUninitialize();
        }
    }
}

impl Stopper {
    pub fn stop(&self) {
        self.finished.set();
        // SAFETY: posting to the main window; a destroyed one fails harmlessly.
        let _ = unsafe { PostMessageW(Some(HWND(self.window as *mut _)), WM_STOP, WPARAM(0), LPARAM(0)) };
    }
}

fn create_window() -> Result<HWND, WindowsError> {
    class::register(CLASS, Some(window_proc));
    // SAFETY: creates a hidden window of the class just registered, on this thread.
    unsafe {
        let module = GetModuleHandleW(None).map_err(WindowsError::setup("module handle"))?;
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            CLASS,
            w!("Snappy"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(module.into()),
            None,
        )
        .map_err(WindowsError::setup("window"))
    }
}

fn emit(event: PlatformEvent) {
    with_main(|main| main.emit(event));
}

/// Asks the engine to quit and waits for it, still running main-thread jobs the
/// engine's shutdown queues.
fn quit_and_wait() {
    emit(PlatformEvent::QuitRequested);
    let Some((_, finished)) = CONSOLE.get() else {
        return;
    };
    let deadline = Instant::now() + QUIT_WAIT;
    while Instant::now() < deadline {
        run_jobs();
        if finished.wait(QUIT_POLL) {
            return;
        }
    }
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_RUN => run_jobs(),
        // SAFETY: ends this thread's message loop.
        WM_STOP => unsafe { PostQuitMessage(0) },
        WM_HOTKEY => {
            with_main(|main| main.hotkey_pressed(wparam.0 as i32));
        }
        WM_TRAY => tray::callback(window, wparam, lparam),
        WM_DISPLAYCHANGE | WM_DPICHANGED => emit(PlatformEvent::DisplaysChanged),
        WM_SETTINGCHANGE if wparam.0 == SPI_SETWORKAREA.0 as usize => emit(PlatformEvent::DisplaysChanged),
        WM_POWERBROADCAST if wparam.0 == PBT_APMSUSPEND as usize => emit(PlatformEvent::WillSleep),
        WM_CLOSE => emit(PlatformEvent::QuitRequested),
        WM_QUERYENDSESSION => return LRESULT(1),
        WM_ENDSESSION if wparam.0 != 0 => quit_and_wait(),
        _ if TASKBAR_CREATED.get() == Some(&message) => {
            with_main(|main| main.tray.add(window));
        }
        // SAFETY: default handling for every other message.
        _ => return unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
    LRESULT(0)
}

/// Console control events arrive on a thread of their own. Ctrl+C and Ctrl+Break ask
/// the engine to quit; closing the console, logging off, and shutting down also wait,
/// because the process ends as soon as this returns.
unsafe extern "system" fn on_console(kind: u32) -> BOOL {
    let Some((events, finished)) = CONSOLE.get() else {
        return BOOL(0);
    };
    let _ = lock(events).send(PlatformEvent::QuitRequested);
    match kind {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => {}
        CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => {
            finished.wait(QUIT_WAIT);
        }
        _ => {}
    }
    BOOL(1)
}
