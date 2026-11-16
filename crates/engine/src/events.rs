//! Routing platform events to the features that handle them.

use snappy_ipc::ToSettings;
use snappy_platform::{Platform, PlatformEvent, TrayCommand};

use crate::engine::Engine;
use crate::log_event::LogEvent;
use crate::workspaces::{self, SwitchFocus};
use crate::{actions, caps_lock, commands, drag, focus, selection};

impl<P: Platform + 'static> Engine<P> {
    pub(crate) fn handle_event(&self, event: PlatformEvent) {
        let shared = &self.shared;
        // Pointer motion is too frequent to be useful; presses and releases are kept.
        if !matches!(
            event,
            PlatformEvent::PointerDragged { .. } | PlatformEvent::ModifiersChanged { .. }
        ) {
            shared.core.borrow_mut().trace(
                LogEvent::Platform,
                || serde_json::json!({ "received": format!("{event:?}") }),
            );
        }
        match event {
            PlatformEvent::PointerDown {
                at,
                mods,
                window,
                frame,
            } => drag::pointer_down(shared, at, mods, window, frame),
            PlatformEvent::PointerDragged { at, mods } | PlatformEvent::ModifiersChanged { at, mods } => {
                drag::pointer_moved(shared, at, mods)
            }
            PlatformEvent::PointerUp { at, mods } => drag::pointer_up(shared, at, mods),
            PlatformEvent::HotKey(id) => actions::hotkey(shared, id),
            PlatformEvent::CapsLock(input) => caps_lock::input(&mut shared.core.borrow_mut(), input),
            PlatformEvent::CapsLockLost => caps_lock::lost(&mut shared.core.borrow_mut()),
            PlatformEvent::WindowCreated { .. } => workspaces::focus_changed(shared, false),
            PlatformEvent::FocusChanged { by_activation, .. } => {
                workspaces::focus_changed(shared, by_activation)
            }
            PlatformEvent::WindowMoved(window) => {
                shared.core.borrow_mut().marked_window_moved(window);
                workspaces::window_moved(shared, window)
            }
            PlatformEvent::WindowDestroyed(window) => {
                let mut core = shared.core.borrow_mut();
                selection::remove(&mut core, window, "closed");
                focus::window_closed(&mut core, window);
                workspaces::window_destroyed(&mut core, window)
            }
            PlatformEvent::AppLaunched { .. } => {}
            PlatformEvent::AppTerminated { pid } => {
                let mut core = shared.core.borrow_mut();
                workspaces::app_terminated(&mut core, pid);
                selection::prune(&mut core);
            }
            PlatformEvent::DisplaysChanged => workspaces::displays_changed(shared),
            PlatformEvent::NativeDesktopChanged => {
                // Selected windows of the desktop left behind no longer show.
                selection::clear(&mut shared.core.borrow_mut(), "native desktop changed");
                workspaces::native_desktop_changed(shared)
            }
            PlatformEvent::PermissionChanged => {
                let mut core = shared.core.borrow_mut();
                core.refresh_live(true);
                workspaces::configure(&mut core);
            }
            PlatformEvent::LiveControlChanged => {
                let available = {
                    let mut core = shared.core.borrow_mut();
                    core.live_control = core.platform.live_control();
                    // Work areas may only now be known, so displays are read before anything
                    // that places windows starts.
                    core.displays = core.platform.displays();
                    if core.live_control.is_available() {
                        core.forget_registered_hotkeys();
                    }
                    core.refresh_live(true);
                    workspaces::configure(&mut core);
                    core.live_control.is_available()
                };
                // Control came back, or the platform started over and may have shown hidden
                // windows: running workspaces bind again and hide them again.
                if available {
                    workspaces::resume(shared);
                }
                shared.core.borrow_mut().publish_state();
            }
            PlatformEvent::HotKeysRefused => {
                let mut state = shared.core.borrow_mut();
                state.forget_registered_hotkeys();
                state.configure_hotkeys();
            }
            PlatformEvent::NotesChanged => {
                let mut core = shared.core.borrow_mut();
                core.platform_notes = core.platform.notes();
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
                workspaces::request_switch(shared, &id, SwitchFocus::Workspace)
            }
            PlatformEvent::QuitRequested => self.shutdown(),
            PlatformEvent::RestartRequested => self.shutdown_for_restart(),
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
            Some(owner) => workspaces::request_switch(&self.shared, &owner, SwitchFocus::Settings),
            None => self.shared.core.borrow_mut().send_to_settings(ToSettings::Focus),
        }
    }
}
