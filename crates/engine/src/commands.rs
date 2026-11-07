//! Commands from the settings app: layouts, shortcuts and recording, workspaces, and
//! preferences.

use std::path::Path;
use std::rc::Rc;

use snappy_core::bindings::conflict_with_layout;
use snappy_core::editor::is_separated;
use snappy_core::layout::{self, DEFAULT_LAYOUT_ID, Layout};
use snappy_core::settings::{LayoutOptions, Settings, SettingsError, Workspace};
use snappy_ipc::{Command, ToSettings};
use snappy_platform::Platform;

use crate::core::Core;
use crate::ops::Shared;
use crate::store::SettingsStore;
use crate::workspaces;

pub(crate) fn handle<P: Platform + 'static>(shared: &Rc<Shared<P>>, id: u64, command: Command) {
    shared.core.borrow_mut().trace(
        "command",
        || serde_json::json!({ "command": format!("{command:?}") }),
    );
    let result = run(shared, command);
    let mut core = shared.core.borrow_mut();
    if let Err(message) = &result {
        core.flag("commandFailed", || serde_json::json!({ "error": message }));
        core.announce_error(message);
    }
    core.outbox.push(ToSettings::Reply {
        id,
        error: result.err(),
    });
    core.publish_state();
}

fn run<P: Platform + 'static>(shared: &Rc<Shared<P>>, command: Command) -> Result<(), String> {
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
        Command::DeleteWorkspace { workspace } => delete_workspace(shared, &workspace),
        Command::SetWorkspacesEnabled { enabled: false } => {
            shared.cancel_current();
            let mut core = shared.core.borrow_mut();
            if !workspaces::stop(&mut core) {
                return Ok(());
            }
            change(&mut core, |settings| settings.workspaces.enabled = false)
        }
        command => {
            let changes_settings = !matches!(
                command,
                Command::ShowPreview { .. }
                    | Command::HidePreview
                    | Command::ProbeShortcut { .. }
                    | Command::SetLoginItem { .. }
                    | Command::OpenPermissionSettings
                    | Command::DismissStatus
                    | Command::ExportSettings { .. }
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

fn apply<P: Platform>(core: &mut Core<P>, command: Command) -> Result<(), String> {
    match command {
        Command::SetSnapping { enabled } => set_snapping(core, enabled),
        Command::SetDragModifier { modifier } => change(core, |s| s.snapping.drag_modifier = modifier),
        Command::SetSwapModifier { modifier } => change(core, |s| s.snapping.swap_modifier = modifier),
        Command::SetWhenTaken { choice } => change(core, |s| s.snapping.when_taken = choice),
        Command::SetShortcutsEnabled { enabled } => change(core, |s| s.snapping.shortcuts_enabled = enabled),
        Command::SetNavigationShortcut { direction, chord } => {
            change(core, |s| s.navigation.set(direction, chord))
        }
        Command::SetNearestEmptyShortcut { chord } => change(core, |s| s.navigation.nearest_empty = chord),
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
            let available = core.platform.is_available(&chord.combo());
            core.outbox.push(ToSettings::ProbeResult { available });
            Ok(())
        }
        Command::SetWorkspacesEnabled { .. } => enable_workspaces(core),
        Command::AddWorkspace => add_workspace(core),
        Command::UpdateWorkspace { workspace } => update_workspace(core, workspace),
        Command::MoveWorkspaces { from, to } => move_workspaces(core, &from, to),
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
        Command::ImportSettings { path } => import(core, Path::new(&path)),
        Command::ExportSettings { path } => {
            SettingsStore::write_file(Path::new(&path), &core.settings).map_err(|e| e.to_string())?;
            core.announce("Layouts and preferences exported.", false, true);
            Ok(())
        }
        Command::DismissStatus => {
            core.status = None;
            Ok(())
        }
        Command::BeginRecording | Command::EndRecording | Command::DeleteWorkspace { .. } => Ok(()),
    }
}

/// Validate and save a change, then reconfigure what depends on settings.
pub(crate) fn change<P: Platform>(
    core: &mut Core<P>,
    edit: impl FnOnce(&mut Settings),
) -> Result<(), String> {
    let mut next = core.settings.clone();
    edit(&mut next);
    let platform = core.platform_name();
    let valid = next.validate(platform).map_err(|error| match error {
        // A shortcut conflict explains itself.
        SettingsError::Conflict(message) => message,
        other => format!("Could not save settings: {other}"),
    })?;
    if valid == core.settings {
        return Ok(());
    }
    core.store.save(&valid).map_err(|error| match error {
        SettingsError::Protected => error.to_string(),
        other => format!("Could not save settings: {other}"),
    })?;
    core.settings = valid;
    settings_changed(core);
    Ok(())
}

pub(crate) fn settings_changed<P: Platform>(core: &mut Core<P>) {
    let debug_log = core.settings.debug_log;
    core.debug.configure(debug_log);
    workspaces::configure(core);
    core.refresh_live(false);
}

fn set_snapping<P: Platform>(core: &mut Core<P>, enabled: bool) -> Result<(), String> {
    change(core, |s| s.snapping.enabled = enabled)?;
    if !enabled {
        core.announce("Snapping is paused.", false, true);
    } else if core.live {
        let modifier = core
            .settings
            .snapping
            .drag_modifier
            .modifier()
            .label(core.platform_name());
        core.announce(
            &format!("Snapping is on. Hold {modifier} while dragging a window."),
            false,
            true,
        );
    } else {
        ask_for_access(core, "Snapping");
    }
    Ok(())
}

/// Ask for permission once and say what will happen, or how to grant it by hand when the
/// request can't be made. `feature` names what starts on the grant.
fn ask_for_access<P: Platform>(core: &mut Core<P>, feature: &str) {
    if !core.capabilities.needs_permission || core.permission_granted {
        return;
    }
    if core.platform.request_access() {
        core.announce(
            &format!("Allow Snappy in Accessibility. {feature} will start automatically."),
            false,
            true,
        );
    } else {
        core.announce_error("Open System Settings → Privacy & Security → Accessibility and allow Snappy.");
    }
}

/// Save the defaults and ask for permission once, so a new install needs one step to
/// start snapping.
pub(crate) fn first_launch<P: Platform>(core: &mut Core<P>) {
    if let Err(error) = core.store.save(&core.settings) {
        core.announce_error(&error.to_string());
        return;
    }
    if core.settings.snapping.enabled && !core.live && core.capabilities.needs_permission {
        let _ = set_snapping(core, true);
    }
}

/// Assign a layout to a display.
pub(crate) fn apply_layout<P: Platform>(core: &mut Core<P>, display: &str, layout_id: &str) {
    let Some(layout) = core.settings.layout(layout_id).cloned() else {
        return;
    };
    if !core.settings.is_enabled(&layout) {
        return;
    }
    let result = change(core, |s| assign(s, display, layout_id));
    match result {
        // A quiet update would dismiss the refused-shortcuts error the change raised.
        Ok(()) if core.registration_failed => {}
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

fn assign(settings: &mut Settings, display: &str, layout_id: &str) {
    if settings.workspaces.enabled {
        let active = settings.workspaces.active_id.clone();
        if let Some(item) = settings
            .workspaces
            .items
            .iter_mut()
            .find(|item| item.id == active)
        {
            item.displays.insert(display.to_owned(), layout_id.to_owned());
        }
    } else {
        settings
            .layouts
            .displays
            .insert(display.to_owned(), layout_id.to_owned());
        settings.layouts.default_layout_id = layout_id.to_owned();
    }
}

fn save_layout<P: Platform>(
    core: &mut Core<P>,
    requested: Layout,
    options: LayoutOptions,
    display: &str,
) -> Result<(), String> {
    // Presets keep their shapes and names whatever the editor sends.
    let layout = layout::preset(&requested.id).cloned().unwrap_or(requested);
    if !layout.is_valid() || !layout.zones.iter().all(|zone| zone.rect.is_valid()) {
        return Err("Give this layout a name and 1–32 valid zones.".to_owned());
    }
    if !is_separated(&layout.zones) {
        return Err("Move or resize overlapping zones apart before saving.".to_owned());
    }
    if let Some(conflict) = conflict_with_layout(&core.settings, &layout, &options, core.platform_name()) {
        return Err(conflict);
    }
    change(core, |s| {
        if !layout.is_preset() {
            match s
                .layouts
                .custom
                .iter_mut()
                .find(|existing| existing.id == layout.id)
            {
                Some(existing) => *existing = layout.clone(),
                None => s.layouts.custom.push(layout.clone()),
            }
        }
        s.layouts.options.insert(layout.id.clone(), options);
        assign(s, display, &layout.id);
    })?;
    core.announce(&format!("{} saved.", layout.name), false, true);
    Ok(())
}

fn delete_layout<P: Platform>(core: &mut Core<P>, id: &str) -> Result<(), String> {
    if layout::is_preset_id(id) {
        return Ok(());
    }
    change(core, |s| {
        s.layouts.custom.retain(|layout| layout.id != id);
        s.layouts.options.remove(id);
        s.layouts.displays.retain(|_, layout| layout != id);
        if s.layouts.default_layout_id == id {
            s.layouts.default_layout_id = DEFAULT_LAYOUT_ID.to_owned();
        }
    })?;
    core.announce("Custom layout deleted.", false, true);
    Ok(())
}

fn set_layout_enabled<P: Platform>(core: &mut Core<P>, id: &str, enabled: bool) -> Result<(), String> {
    let layout = core.settings.layout(id).cloned().ok_or("Unknown layout.")?;
    let mut options = core.settings.options(&layout);
    options.enabled = enabled;
    if let Some(conflict) = conflict_with_layout(&core.settings, &layout, &options, core.platform_name()) {
        return Err(conflict);
    }
    change(core, |s| {
        s.layouts.options.insert(id.to_owned(), options);
    })?;
    let state = if enabled { "enabled" } else { "disabled" };
    core.announce(&format!("{} {state}.", layout.name), false, true);
    Ok(())
}

/// Turn workspaces on, seeding per-workspace layouts the first time.
fn enable_workspaces<P: Platform>(core: &mut Core<P>) -> Result<(), String> {
    let displays = core.display_ids();
    change(core, |s| {
        if s.workspaces.items.iter().all(|item| item.displays.is_empty()) {
            let mut seeded = s.layouts.displays.clone();
            for display in &displays {
                seeded
                    .entry(display.clone())
                    .or_insert_with(|| s.layouts.default_layout_id.clone());
            }
            for item in &mut s.workspaces.items {
                item.displays = seeded.clone();
            }
        }
        s.workspaces.enabled = true;
    })?;
    ask_for_access(core, "Workspaces");
    Ok(())
}

/// A new workspace with the lowest free number and no shortcuts.
fn add_workspace<P: Platform>(core: &mut Core<P>) -> Result<(), String> {
    let items = &core.settings.workspaces.items;
    if items.len() >= snappy_core::settings::MAX_WORKSPACES {
        return Ok(());
    }
    let number = (1..=snappy_core::settings::MAX_WORKSPACES)
        .find(|n| !items.iter().any(|item| item.name == format!("Workspace {n}")))
        .unwrap_or(items.len() + 1);
    let id = new_id(core, number);
    let mut workspace = Workspace::numbered(id.clone(), number);
    workspace.displays = core
        .settings
        .workspaces
        .active()
        .map(|active| active.displays.clone())
        .unwrap_or_default();
    change(core, |s| s.workspaces.items.push(workspace))?;
    core.added_workspace = Some(id);
    Ok(())
}

fn new_id<P: Platform>(core: &Core<P>, number: usize) -> String {
    let mut id = format!("workspace-{number}");
    let mut suffix = 1;
    while core.settings.workspaces.item(&id).is_some() {
        suffix += 1;
        id = format!("workspace-{number}-{suffix}");
    }
    id
}

fn update_workspace<P: Platform>(core: &mut Core<P>, workspace: Workspace) -> Result<(), String> {
    change(core, |s| {
        if let Some(item) = s.workspaces.items.iter_mut().find(|item| item.id == workspace.id) {
            *item = workspace;
        }
    })
}

/// Reorder workspace cards. Invalid moves change nothing.
fn move_workspaces<P: Platform>(core: &mut Core<P>, from: &[usize], to: usize) -> Result<(), String> {
    let count = core.settings.workspaces.items.len();
    if from.is_empty() || from.iter().any(|&i| i >= count) || to > count {
        return Ok(());
    }
    change(core, |s| {
        let items = &mut s.workspaces.items;
        let moving: Vec<Workspace> = from.iter().map(|&i| items[i].clone()).collect();
        let before = from.iter().filter(|&&i| i < to).count();
        let mut remaining: Vec<Workspace> = items
            .iter()
            .enumerate()
            .filter(|(i, _)| !from.contains(i))
            .map(|(_, item)| item.clone())
            .collect();
        let insert_at = (to - before).min(remaining.len());
        remaining.splice(insert_at..insert_at, moving);
        *items = remaining;
    })
}

/// Delete a workspace, moving its windows without closing them.
fn delete_workspace<P: Platform + 'static>(shared: &Rc<Shared<P>>, id: &str) -> Result<(), String> {
    shared.cancel_current();
    let mut core = shared.core.borrow_mut();
    let workspaces = &core.settings.workspaces;
    if workspaces.items.len() < 2 || workspaces.item(id).is_none() {
        return Ok(());
    }
    let destination = if workspaces.active_id == id {
        workspaces
            .items
            .iter()
            .find(|item| item.id != id)
            .map(|item| item.id.clone())
    } else {
        Some(workspaces.active_id.clone())
    }
    .unwrap_or_default();

    let previous = core.settings.clone();
    change(&mut core, |s| {
        s.workspaces.items.retain(|item| item.id != id);
        if s.workspaces.active_id == id {
            s.workspaces.active_id = destination.clone();
        }
    })
    .map_err(|e| format!("Could not remove workspace: {e}"))?;

    if let Err(error) = workspaces::remove_workspace(&mut core, id, &destination) {
        let _ = core.store.save(&previous);
        core.settings = previous;
        settings_changed(&mut core);
        return Err(format!("Could not remove workspace: {error}"));
    }
    drop(core);
    shared.histories.borrow_mut().remove(&format!("workspace:{id}"));
    Ok(())
}

/// Replace settings with a file's. Never turns snapping on.
fn import<P: Platform>(core: &mut Core<P>, path: &Path) -> Result<(), String> {
    let mut imported = SettingsStore::read_file(path, core.platform_name())
        .map_err(|error| format!("Import failed: {error}"))?;
    if !workspaces::stop(core) {
        return Ok(());
    }
    imported.workspaces.enabled = false;
    imported.snapping.enabled = core.settings.snapping.enabled;
    change(core, |s| *s = imported).map_err(|error| format!("Import failed: {error}"))?;
    core.announce("Layouts and preferences imported.", false, true);
    Ok(())
}
