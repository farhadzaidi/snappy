//! Commands from the app: layouts, shortcuts and recording, workspaces, and
//! preferences.

use std::rc::Rc;

use snappy_core::keys::Mods;
use snappy_core::layout::Layout;
use snappy_core::settings::Reset;
use snappy_core::settings::{LayoutOptions, Settings, SettingsError};
use snappy_core::shortcuts::{Probe, Refusal, availability_probes};
use snappy_ipc::{Command, ToApp};
use snappy_platform::Platform;

use crate::caps_lock;
use crate::log_event::LogEvent;
use crate::ops::{Shared, history_scope};
use crate::state::EngineState;
use crate::workspaces;

pub(crate) fn handle<P: Platform + 'static>(shared: &Rc<Shared<P>>, id: u64, command: Command) {
    shared.core.borrow_mut().trace(
        LogEvent::Command,
        || serde_json::json!({ "command": described(&command) }),
    );
    let result = run(shared, command);
    let mut core = shared.core.borrow_mut();
    if let Err(refusal) = &result {
        core.flag(
            LogEvent::CommandFailed,
            || serde_json::json!({ "error": refusal.message }),
        );
        core.announce_error(&refusal.message);
    }
    core.send_to_app(ToApp::Reply {
        id,
        error: result.err(),
    });
    core.publish_state();
}

/// The command as the debug log records it. An imported file's text, up to megabytes of it,
/// is logged by its size only.
fn described(command: &Command) -> String {
    match command {
        Command::ImportSettings { contents } => format!("ImportSettings {{ {} bytes }}", contents.len()),
        other => format!("{other:?}"),
    }
}

fn run<P: Platform + 'static>(shared: &Rc<Shared<P>>, command: Command) -> Result<(), Refusal> {
    match command {
        Command::BeginRecording | Command::EndRecording => {
            let recording = matches!(command, Command::BeginRecording);
            shared.cancel_current();
            let mut core = shared.core.borrow_mut();
            core.recording = recording;
            core.end_drag();
            core.configure_hotkeys();
            Ok(())
        }
        Command::DeleteWorkspace { workspace } => delete_workspace(shared, &workspace).map_err(Refusal::from),
        Command::SetWorkspacesEnabled { enabled: false } => {
            shared.cancel_current();
            let mut core = shared.core.borrow_mut();
            workspaces::stop(&mut core)
                .map_err(|error| format!("{error} Turn workspaces off again to retry."))?;
            change(&mut core, |settings| settings.workspaces.enabled = false).map_err(Refusal::from)
        }
        command => {
            let changes_settings = !matches!(
                command,
                Command::ShowPreview { .. }
                    | Command::HidePreview
                    | Command::ProbeShortcut { .. }
                    | Command::SetLoginItem { .. }
                    | Command::OpenPermissionSettings
                    | Command::ApplyRemedy { .. }
                    | Command::DismissStatus
                    | Command::ExportSettings
            );
            if changes_settings {
                shared.cancel_current();
            }
            let mut core = shared.core.borrow_mut();
            if changes_settings {
                core.end_drag();
                core.set_preview(None);
            }
            apply(&mut core, command)
        }
    }
}

fn apply<P: Platform>(core: &mut EngineState<P>, command: Command) -> Result<(), Refusal> {
    let result = match command {
        Command::SetSnapping { enabled } => set_snapping(core, enabled),
        Command::SetDragModifier { modifier } => change(core, |s| s.snapping.drag_modifier = modifier),
        Command::SetSwapModifier { modifier } => change(core, |s| s.snapping.swap_modifier = modifier),
        Command::SetWhenTaken { choice } => change(core, |s| s.snapping.when_taken = choice),
        Command::SetShortcutsEnabled { enabled } => change(core, |s| s.snapping.shortcuts_enabled = enabled),
        // The one refusal that names a shortcut, for the app to point to.
        Command::SetSnappyKey { modifiers } => return set_snappy_key(core, modifiers),
        Command::SetCapsLockAsSnappyKey { enabled } => caps_lock::set_enabled(core, enabled),
        Command::SetNavigationShortcut { shortcut, chord } => {
            change(core, |s| s.navigation.set(shortcut, chord))
        }
        Command::ApplyLayout { display, layout } => {
            apply_layout(core, &display, &layout);
            Ok(())
        }
        Command::SaveLayout {
            layout,
            options,
            display,
        } => save_layout(core, layout, options, &display),
        Command::DeleteLayout { layout } => delete_layout(core, &layout),
        Command::SetLayoutEnabled { layout, enabled } => set_layout_enabled(core, &layout, enabled),
        Command::ShowPreview { display, layout } => {
            core.set_preview(Some((display, layout)));
            Ok(())
        }
        Command::HidePreview => {
            core.set_preview(None);
            Ok(())
        }
        Command::ProbeShortcut { chord } => {
            let available = core.platform.is_available(&chord.combo(core.settings.snappy_key));
            core.send_to_app(ToApp::ProbeResult { available });
            Ok(())
        }
        Command::SetWorkspacesEnabled { .. } => enable_workspaces(core),
        Command::AddWorkspace => add_workspace(core),
        Command::UpdateWorkspace { workspace } => change(core, |s| s.update_workspace(workspace)),
        Command::MoveWorkspaces { from, to } => change(core, |s| s.reorder_workspaces(&from, to)),
        Command::SetWorkspaceStepShortcut { step, chord } => {
            change(core, |s| s.workspaces.set_step(step, chord))
        }
        Command::SetHistoryShortcut { step, chord } => change(core, |s| s.history.set(step, chord)),
        Command::SetLoginItem { enabled } => core.platform.set_enabled(enabled),
        Command::SetDebugLog { enabled } => change(core, |s| s.debug_log = enabled),
        Command::OpenPermissionSettings => {
            core.platform.open_settings_page();
            Ok(())
        }
        // Only what the platform offers now: a stale window must not log anyone out.
        Command::ApplyRemedy { remedy } if core.live_control.remedy() == Some(remedy) => {
            core.platform.apply_remedy(remedy)
        }
        Command::ApplyRemedy { .. } => Ok(()),
        Command::Reset { reset } => reset_settings(core, &reset),
        Command::ImportSettings { contents } => import(core, &contents),
        Command::ExportSettings => export(core),
        Command::DismissStatus => {
            core.dismiss_status();
            Ok(())
        }
        Command::BeginRecording | Command::EndRecording | Command::DeleteWorkspace { .. } => Ok(()),
    };
    result.map_err(Refusal::from)
}

/// Validate and save a change, then reconfigure what depends on settings.
pub(crate) fn change<P: Platform>(
    core: &mut EngineState<P>,
    edit: impl FnOnce(&mut Settings),
) -> Result<(), String> {
    let mut next = core.settings.clone();
    edit(&mut next);
    let platform = core.platform_name();
    let valid = next.validate(platform).map_err(refusal)?;
    commit(core, valid)
}

/// What to tell the user about settings that failed validation.
fn refusal(error: SettingsError) -> String {
    match error {
        // Shortcut conflicts and Snappy key problems explain themselves.
        SettingsError::Conflict(message) => message,
        SettingsError::SnappyKey(error) => error.to_string(),
        other => format!("Couldn't save settings: {other}"),
    }
}

/// Save validated settings, then reconfigure what depends on them.
fn commit<P: Platform>(core: &mut EngineState<P>, valid: Settings) -> Result<(), String> {
    if valid == core.settings {
        return Ok(());
    }
    core.store.save(&valid).map_err(|error| match error {
        SettingsError::Protected => error.to_string(),
        other => format!("Couldn't save settings: {other}"),
    })?;
    core.settings = valid;
    settings_changed(core);
    Ok(())
}

pub(crate) fn settings_changed<P: Platform>(core: &mut EngineState<P>) {
    let debug_log = core.settings.debug_log;
    core.debug.configure(debug_log);
    workspaces::configure(core);
    core.refresh_live(false);
}

/// Make the Snappy key stand for `modifiers`, checking every shortcut with the new
/// modifiers first: against each other and the chords the system keeps
/// (`Settings::with_snappy_key`), then with the OS for each chord the change moves. Nothing
/// changes when any check fails, and the reply names what clashed.
fn set_snappy_key<P: Platform>(core: &mut EngineState<P>, modifiers: Mods) -> Result<(), Refusal> {
    let current = core.settings.snappy_key;
    if modifiers == current.modifiers() {
        return Ok(());
    }
    let platform = core.platform_name();
    let names = |mods: Mods| -> Vec<&'static str> {
        mods.in_display_order()
            .map(|modifier| modifier.name(platform))
            .collect()
    };
    let (from, to) = (names(current.modifiers()), names(modifiers));

    let checked = core
        .settings
        .with_snappy_key(modifiers, platform)
        .map_err(|error| Refusal::from(refusal(error)))
        .and_then(|next| {
            let probes = availability_probes(&next, platform);
            probe(core, &probes).map(|()| next)
        });
    let saved = checked.and_then(|next| {
        let key = next.snappy_key;
        commit(core, next).map(|()| key).map_err(Refusal::from)
    });
    match saved {
        Ok(key) => {
            core.trace(
                LogEvent::SnappyKeyChanged,
                || serde_json::json!({ "from": from, "to": to }),
            );
            core.announce(&format!("The Snappy key is now {}.", key.label(platform)));
            Ok(())
        }
        Err(reason) => {
            // Probing may have taken Snappy's own shortcuts away; put the current ones back.
            core.configure_hotkeys();
            core.flag(
                LogEvent::SnappyKeyRefused,
                || serde_json::json!({ "from": from, "to": to, "reason": reason.message }),
            );
            Err(reason)
        }
    }
}

/// Ask the OS about each probe, with Snappy's own shortcuts unregistered first, as while
/// recording: a probed chord can be one Snappy holds now, such as Snappy + Shift + 1 in one
/// layout becoming Snappy + 1 of another once Shift joins the key, and the OS reports
/// those as taken. Whoever calls this registers shortcuts again afterwards.
fn probe<P: Platform>(core: &mut EngineState<P>, probes: &[Probe]) -> Result<(), Refusal> {
    core.release_hotkeys();
    match probes
        .iter()
        .find(|probe| !core.platform.is_available(&probe.combo))
    {
        Some(taken) => Err(taken.refusal.clone()),
        None => Ok(()),
    }
}

fn set_snapping<P: Platform>(core: &mut EngineState<P>, enabled: bool) -> Result<(), String> {
    change(core, |s| s.snapping.enabled = enabled)?;
    if !enabled {
        core.announce("Snapping is paused.");
    } else if core.is_live() {
        let modifier = core
            .settings
            .snapping
            .drag_modifier
            .modifier()
            .label(core.platform_name());
        core.announce(&format!(
            "Snapping is on. Hold {modifier} while dragging a window."
        ));
    } else {
        ask_for_access(core, "Snapping");
    }
    Ok(())
}

/// Ask for permission once and say what will happen, or how to grant it by hand when the
/// request can't be made. `feature` names what starts on the grant.
fn ask_for_access<P: Platform>(core: &mut EngineState<P>, feature: &str) {
    if !core.capabilities.needs_permission || core.has_permission() {
        return;
    }
    if core.platform.request_access() {
        core.announce(&format!(
            "Allow Snappy in Accessibility. {feature} will start automatically."
        ));
    } else {
        core.announce_error("Open System Settings → Privacy & Security → Accessibility and allow Snappy.");
    }
}

/// Save the defaults and ask for permission once, so a new install needs one step to
/// start snapping.
pub(crate) fn first_launch<P: Platform>(core: &mut EngineState<P>) {
    if let Err(error) = core.store.save(&core.settings) {
        core.announce_error(&error.to_string());
        return;
    }
    if core.settings.snapping.enabled && !core.is_live() && core.capabilities.needs_permission {
        let _ = set_snapping(core, true);
    }
}

/// Assign a layout to a display.
pub(crate) fn apply_layout<P: Platform>(core: &mut EngineState<P>, display: &str, layout_id: &str) {
    let mut next = core.settings.clone();
    let Some(layout) = next.assign_layout(display, layout_id) else {
        return;
    };
    let result = change(core, |s| *s = next);
    match result {
        // A quiet update would dismiss the refused-shortcuts error the change raised.
        Ok(()) if core.shortcuts_refused() => {}
        Ok(()) => {
            let name = core
                .displays
                .iter()
                .find(|d| d.id == display)
                .map_or("this display".to_owned(), |d| d.name.clone());
            core.announce_quietly(&format!("{} selected for {name}.", layout.name));
        }
        Err(message) => core.announce_error(&message),
    }
}

fn save_layout<P: Platform>(
    core: &mut EngineState<P>,
    requested: Layout,
    options: LayoutOptions,
    display: &str,
) -> Result<(), String> {
    let mut next = core.settings.clone();
    let layout = next
        .save_layout(requested, options, display, core.platform_name())
        .map_err(|error| error.to_string())?;
    change(core, |s| *s = next)?;
    core.announce(&format!("{} saved.", layout.name));
    Ok(())
}

fn delete_layout<P: Platform>(core: &mut EngineState<P>, id: &str) -> Result<(), String> {
    let mut next = core.settings.clone();
    if !next.remove_layout(id) {
        return Ok(());
    }
    change(core, |s| *s = next)?;
    core.announce("Custom layout deleted.");
    Ok(())
}

fn set_layout_enabled<P: Platform>(core: &mut EngineState<P>, id: &str, enabled: bool) -> Result<(), String> {
    let mut next = core.settings.clone();
    let layout = next
        .set_layout_enabled(id, enabled, core.platform_name())
        .map_err(|error| error.to_string())?;
    change(core, |s| *s = next)?;
    let state = if enabled { "enabled" } else { "disabled" };
    core.announce(&format!("{} {state}.", layout.name));
    Ok(())
}

/// Turn workspaces on, seeding per-workspace layouts the first time.
fn enable_workspaces<P: Platform>(core: &mut EngineState<P>) -> Result<(), String> {
    let displays = core.display_ids();
    change(core, |s| s.enable_workspaces(&displays))?;
    ask_for_access(core, "Workspaces");
    Ok(())
}

/// Add a workspace, and tell the app which, so it can open it.
fn add_workspace<P: Platform>(core: &mut EngineState<P>) -> Result<(), String> {
    let mut next = core.settings.clone();
    let Some(id) = next.add_workspace() else {
        return Ok(());
    };
    change(core, |s| *s = next)?;
    core.added_workspace = Some(id);
    Ok(())
}

/// Delete a workspace, moving its windows without closing them.
fn delete_workspace<P: Platform + 'static>(shared: &Rc<Shared<P>>, id: &str) -> Result<(), String> {
    shared.cancel_current();
    let mut core = shared.core.borrow_mut();
    let mut next = core.settings.clone();
    let Some(destination) = next.remove_workspace(id) else {
        return Ok(());
    };

    let previous = core.settings.clone();
    change(&mut core, |s| *s = next).map_err(|e| format!("Couldn't remove workspace: {e}"))?;

    if let Err(error) = workspaces::remove_workspace(&mut core, id, &destination) {
        let _ = core.store.save(&previous);
        core.settings = previous;
        settings_changed(&mut core);
        return Err(format!("Couldn't remove workspace: {error}"));
    }
    drop(core);
    shared.histories.borrow_mut().remove(&history_scope(Some(id)));
    Ok(())
}

/// Put settings back to their defaults as `reset` says, validated and saved as any change.
/// Resetting everything removes the workspaces, so they stop first, bringing their windows
/// back; when a window can't come back yet, nothing is reset.
fn reset_settings<P: Platform>(core: &mut EngineState<P>, reset: &Reset) -> Result<(), String> {
    if *reset == Reset::Everything {
        workspaces::stop(core).map_err(
            |_| "A hidden window couldn't be brought back yet, so nothing was reset. Try again in a moment.",
        )?;
    }
    let before = core.settings.clone();
    let platform = core.platform_name();
    change(core, |settings| *settings = before.reset(reset, platform))?;
    core.announce(&reset.message(&before));
    Ok(())
}

/// Replace settings with a settings file's text. Never turns snapping on.
fn import<P: Platform>(core: &mut EngineState<P>, contents: &str) -> Result<(), String> {
    let mut imported = Settings::from_json(contents.as_bytes(), core.platform_name())
        .map_err(|error| format!("Import failed: {error}"))?;
    workspaces::stop(core).map_err(
        |_| "A hidden window couldn't be brought back yet, so nothing was imported. Try again in a moment.",
    )?;
    imported.workspaces.enabled = false;
    imported.snapping.enabled = core.settings.snapping.enabled;
    change(core, |s| *s = imported).map_err(|error| format!("Import failed: {error}"))?;
    core.announce("Layouts and preferences imported.");
    Ok(())
}

/// Send the settings as a settings file's text, for the app to write where the user chose.
/// The app says when the file is written, so nothing is announced here.
fn export<P: Platform>(core: &mut EngineState<P>) -> Result<(), String> {
    let bytes = core
        .settings
        .to_json()
        .map_err(|error| format!("Couldn't export settings: {error}"))?;
    let contents = String::from_utf8(bytes).map_err(|_| "Couldn't export settings.".to_owned())?;
    core.send_to_app(ToApp::ExportedSettings { contents });
    Ok(())
}
