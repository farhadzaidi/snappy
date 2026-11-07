//! The engine: the single owner of Snappy's runtime state. It turns platform events and
//! settings-app commands into decisions and window operations.
//!
//! The engine runs on one thread inside a Tokio `LocalSet`. Window operations are tasks on
//! that set and run one at a time; a new operation cancels the one in flight.

mod actions;
mod commands;
mod core;
mod debug_log;
mod diagnostics;
mod drag;
mod ops;
mod store;
mod workspaces;

use std::rc::Rc;

use snappy_core::settings::Settings;
use snappy_ipc::{Command, ToSettings};
use snappy_platform::{Platform, PlatformEvent, TrayCommand};

pub use crate::diagnostics::SwitchTiming;
pub use crate::store::Paths;

use crate::ops::Shared;

/// Something for the engine to handle.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Platform(PlatformEvent),
    Command { id: u64, command: Command },
    SettingsAppStarted { pid: u32 },
    SettingsAppExited,
}

pub struct Engine<P: Platform + 'static> {
    shared: Rc<Shared<P>>,
}

impl<P: Platform + 'static> Engine<P> {
    pub fn new(platform: P, paths: Paths) -> Self {
        Self {
            shared: Shared::new(crate::core::Core::new(platform, paths)),
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
                let message = format!("Could not load settings: {error} The original file is preserved.");
                core.announce_error(&message);
            }
        }
        core.displays = core.platform.displays();
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
            Input::SettingsAppStarted { pid } => {
                let mut core = self.shared.core.borrow_mut();
                core.settings_app_pid = Some(pid);
                core.publish_state();
            }
            Input::SettingsAppExited => {
                let mut core = self.shared.core.borrow_mut();
                core.settings_app_pid = None;
                core.set_preview(None);
            }
        }
    }

    fn handle_event(&self, event: PlatformEvent) {
        let shared = &self.shared;
        // Pointer motion is too frequent to be useful; presses and releases are kept.
        if !matches!(
            event,
            PlatformEvent::PointerDragged { .. } | PlatformEvent::ModifiersChanged { .. }
        ) {
            shared.core.borrow_mut().trace(
                "platform",
                || serde_json::json!({ "received": format!("{event:?}") }),
            );
        }
        match event {
            PlatformEvent::PointerDown { at, mods, window } => drag::pointer_down(shared, at, mods, window),
            PlatformEvent::PointerDragged { at, mods } | PlatformEvent::ModifiersChanged { at, mods } => {
                drag::pointer_moved(shared, at, mods)
            }
            PlatformEvent::PointerUp { at, mods } => drag::pointer_up(shared, at, mods),
            PlatformEvent::HotKey(id) => actions::hotkey(shared, id),
            PlatformEvent::WindowCreated { .. } => workspaces::focus_changed(shared, false),
            PlatformEvent::FocusChanged { by_activation, .. } => {
                workspaces::focus_changed(shared, by_activation)
            }
            PlatformEvent::WindowMoved(window) => {
                workspaces::window_moved(&mut shared.core.borrow_mut(), window)
            }
            PlatformEvent::WindowDestroyed(window) => {
                workspaces::window_destroyed(&mut shared.core.borrow_mut(), window)
            }
            PlatformEvent::AppLaunched { .. } => {}
            PlatformEvent::AppTerminated { pid } => {
                workspaces::app_terminated(&mut shared.core.borrow_mut(), pid)
            }
            PlatformEvent::DisplaysChanged => workspaces::displays_changed(shared),
            PlatformEvent::NativeDesktopChanged => {
                workspaces::native_desktop_changed(&mut shared.core.borrow_mut())
            }
            PlatformEvent::PermissionChanged => {
                let mut core = shared.core.borrow_mut();
                core.refresh_live(true);
                workspaces::configure(&mut core);
            }
            PlatformEvent::LiveControlChanged => {
                let mut core = shared.core.borrow_mut();
                core.live_control = core.platform.live_control();
                // Work areas may only now be known, so displays are read before anything
                // that places windows starts.
                core.displays = core.platform.displays();
                core.refresh_live(true);
                workspaces::configure(&mut core);
                core.publish_state();
            }
            PlatformEvent::WillSleep => {
                shared.cancel_current();
                shared.core.borrow_mut().end_drag();
            }
            PlatformEvent::OpenSettingsRequested | PlatformEvent::Tray(TrayCommand::ManageWorkspaces) => {
                self.reveal_settings()
            }
            PlatformEvent::Tray(TrayCommand::SelectLayout { display, layout }) => {
                commands::apply_layout(&mut shared.core.borrow_mut(), &display, &layout)
            }
            PlatformEvent::Tray(TrayCommand::SwitchWorkspace(id)) => {
                workspaces::request_switch(shared, &id, false)
            }
            PlatformEvent::QuitRequested => self.shutdown(),
        }
    }

    /// Bring the settings window forward, switching to its workspace first.
    fn reveal_settings(&self) {
        let owner = {
            let mut core = self.shared.core.borrow_mut();
            let pid = core.settings_app_pid;
            let window = core
                .platform
                .all_windows()
                .into_iter()
                .find(|info| Some(info.pid) == pid)
                .map(|info| info.id);
            let session = core.workspaces.session.as_ref();
            let owner = window
                .and_then(|window| session.and_then(|session| session.member(window)))
                .map(|member| member.workspace.clone());
            let active = core.settings.workspaces.active_id.clone();
            owner.filter(|owner| *owner != active && core.workspaces.is_running())
        };
        match owner {
            Some(owner) => workspaces::request_switch(&self.shared, &owner, true),
            None => self.shared.core.borrow_mut().outbox.push(ToSettings::Focus),
        }
    }

    /// Return parked windows and write timings before quitting.
    pub fn shutdown(&self) {
        self.shared.cancel_current();
        let mut core = self.shared.core.borrow_mut();
        workspaces::stop(&mut core);
        core.diagnostics.export_timings();
        core.platform.set_press_monitoring(false);
        core.platform.set_drag_monitoring(false);
        core.platform.unregister_all();
        core.platform.hide();
    }

    /// Messages for the settings app produced since the last call.
    pub fn take_outbox(&self) -> Vec<ToSettings> {
        std::mem::take(&mut self.shared.core.borrow_mut().outbox)
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
        self.shared.core.borrow().status.clone()
    }

    pub fn is_live(&self) -> bool {
        self.shared.core.borrow().live
    }

    /// Direct access to the platform, for the app shell and tests.
    pub fn with_platform<R>(&self, f: impl FnOnce(&mut P) -> R) -> R {
        f(&mut self.shared.core.borrow_mut().platform)
    }
}
