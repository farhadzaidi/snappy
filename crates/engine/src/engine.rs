//! The engine's public face: what the app shell feeds it and asks of it.

use std::rc::Rc;

use snappy_core::settings::Settings;
use snappy_ipc::{Command, ToApp};
use snappy_platform::{Platform, PlatformEvent};

use crate::diagnostics::SwitchTiming;
use crate::ops::Shared;
use crate::state::EngineState;
use crate::store::Paths;
use crate::{caps_lock, commands, workspaces};

/// Something for the engine to handle.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Platform(PlatformEvent),
    Command { id: u64, command: Command },
    AppStarted { pid: u32 },
    AppExited,
}

pub struct Engine<P: Platform + 'static> {
    pub(crate) shared: Rc<Shared<P>>,
}

impl<P: Platform + 'static> Engine<P> {
    pub fn new(platform: P, paths: Paths) -> Self {
        Self {
            shared: Shared::new(EngineState::new(platform, paths)),
        }
    }

    /// Load settings and start everything they ask for.
    pub fn start(&self) {
        let mut core = self.shared.core.borrow_mut();
        let first_launch = core.store.is_new();
        match core.store.load() {
            Ok(settings) => {
                core.debug.configure(settings.debug_log);
                core.settings = settings;
            }
            Err(error) => {
                let message = format!("Couldn't load settings: {error} The original file is preserved.");
                core.announce_error(&message);
            }
        }
        core.displays = core.platform.displays();
        caps_lock::start(&mut core);
        core.refresh_live(false);
        workspaces::configure(&mut core);
        if first_launch {
            commands::first_launch(&mut core);
        }
    }

    /// Record to the debug log for this run whatever the setting says.
    pub fn force_debug_log(&self) {
        self.shared.core.borrow_mut().debug.force_on();
    }

    pub fn handle(&self, input: Input) {
        match input {
            Input::Platform(event) => self.handle_event(event),
            Input::Command { id, command } => commands::handle(&self.shared, id, command),
            Input::AppStarted { pid } => {
                let mut core = self.shared.core.borrow_mut();
                core.app_pid = Some(pid);
                core.publish_state();
            }
            Input::AppExited => {
                let mut core = self.shared.core.borrow_mut();
                core.app_pid = None;
                core.set_preview(None);
            }
        }
    }

    /// Return parked windows and write timings before quitting.
    pub fn shutdown(&self) {
        self.shared.cancel_current();
        let mut core = self.shared.core.borrow_mut();
        // Quitting goes on regardless; a window that couldn't come back stays journaled, and
        // the next launch brings it back.
        let _ = workspaces::stop(&mut core);
        Self::release(&mut core);
    }

    /// Stop for a new daemon to take over: windows stay where they are, parked ones
    /// included, and the journal and membership stay for the next launch.
    pub fn shutdown_for_restart(&self) {
        self.shared.cancel_current();
        let mut core = self.shared.core.borrow_mut();
        workspaces::detach(&mut core);
        Self::release(&mut core);
    }

    /// Write timings and give up input, shortcuts, overlays, and Caps Lock.
    fn release(core: &mut EngineState<P>) {
        core.diagnostics.export_timings();
        caps_lock::stop(core);
        core.platform.set_press_monitoring(false);
        core.platform.set_drag_monitoring(false);
        core.platform.unregister_all();
        core.hide_overlay();
    }

    /// Messages for the app produced since the last call.
    pub fn take_outbox(&self) -> Vec<ToApp> {
        self.shared.core.borrow_mut().take_outbox()
    }

    /// Signalled whenever a message for the app is queued, including by work that
    /// finishes between inputs.
    pub fn outbox_ready(&self) -> std::rc::Rc<tokio::sync::Notify> {
        self.shared.core.borrow().outbox_ready()
    }

    /// Wait until no window operation is running.
    pub async fn idle(&self) {
        self.shared.idle().await;
    }

    pub fn settings(&self) -> Settings {
        self.shared.core.borrow().settings.clone()
    }

    /// Recent workspace switch timings.
    pub fn switch_timings(&self) -> Vec<SwitchTiming> {
        self.shared.core.borrow().diagnostics.timings().cloned().collect()
    }

    /// The latest status message.
    pub fn status(&self) -> Option<snappy_ipc::Status> {
        self.shared.core.borrow().status().cloned()
    }

    pub fn is_live(&self) -> bool {
        self.shared.core.borrow().is_live()
    }

    /// Whether Snappy has what it needs to act on the desktop: the permission, and live
    /// control. Workspaces and Caps Lock depend on this, whether snapping is on or not.
    pub fn controls_the_desktop(&self) -> bool {
        let core = self.shared.core.borrow();
        core.has_permission() && core.live_control.is_available()
    }

    /// Direct access to the platform, for the app shell and tests.
    pub fn with_platform<R>(&self, f: impl FnOnce(&mut P) -> R) -> R {
        f(&mut self.shared.core.borrow_mut().platform)
    }
}
