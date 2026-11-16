//! The handler behind each exported function: a typed request in, a typed answer out.

use serde::Serialize;
use snappy_core::editor::{self, ALIGN_TOLERANCE, ResizeHandle};
use snappy_core::geometry::Rect;
use snappy_core::keys::KeyChord;
use snappy_core::layout::{self, Layout, MAX_NAME_LENGTH, MAX_ZONES, Spacing, Zone, zone_frame};
use snappy_core::placement::operations::UNDO_LIMIT;
use snappy_core::settings::{LayoutOptions, LayoutShortcut, MAX_WORKSPACES, Settings, WorkspaceShortcut};
use snappy_core::shortcuts::ShortcutSection;
use snappy_core::shortcuts::guide::{self, GuideEntry};
use snappy_core::shortcuts::{
    Refusal, availability_probes, conflict_with_layout, recording_conflict, unavailable_refusal,
};

use crate::wire::*;

pub fn zone_frames(request: ZoneFramesRequest) -> Vec<Rect> {
    let spacing = Spacing::STANDARD.scaled(request.spacing_factor);
    request
        .zones
        .iter()
        .map(|zone| zone_frame(zone.rect, request.work_area, spacing))
        .collect()
}

pub fn dividers(request: ZonesRequest) -> Vec<Divider> {
    editor::dividers(&request.zones)
        .into_iter()
        .map(Into::into)
        .collect()
}

pub fn is_separated(request: ZonesRequest) -> bool {
    editor::is_separated(&request.zones)
}

pub fn align_edges(request: ZonesRequest) -> Option<Vec<Zone>> {
    editor::align_edges(&request.zones, ALIGN_TOLERANCE)
}

pub fn place_copies(request: CopiesRequest) -> PlaceOutcome {
    match editor::place_copies(&request.copied, &request.zones) {
        Ok(placed) => PlaceOutcome::Placed(placed),
        Err(refusal) => PlaceOutcome::Refused(refusal.to_string()),
    }
}

pub fn merge_zones(request: MergeRequest) -> MergeOutcome {
    match editor::merge(&request.zones, &request.ids) {
        Ok(merged) => MergeOutcome::Merged(MergedLayout {
            zone_shortcuts: merged.shortcuts(&request.zone_shortcuts),
            zones: merged.zones,
            kept: merged.kept,
        }),
        Err(refusal) => MergeOutcome::Refused(refusal.to_string()),
    }
}

pub fn handle_at(request: HandleAtRequest) -> Option<Handle> {
    editor::handle_at(request.point, request.size).map(Into::into)
}

pub fn handle_positions(request: SizeRequest) -> Vec<HandlePosition> {
    ResizeHandle::ALL
        .into_iter()
        .map(|handle| HandlePosition {
            handle: handle.into(),
            center: editor::handle_center(handle, request.size),
        })
        .collect()
}

pub fn drag_move(request: MoveRequest) -> Edit {
    editor::drag_move(&request.zones, &request.id, request.delta, request.tolerance).into()
}

pub fn drag_resize(request: ResizeRequest) -> Edit {
    editor::drag_resize(
        &request.zones,
        &request.id,
        request.handle.into(),
        request.delta,
        request.tolerance,
    )
    .into()
}

pub fn drag_divider(request: DividerRequest) -> Edit {
    editor::drag_divider(
        &request.zones,
        &request.zone_id,
        request.axis.into(),
        request.line,
        request.delta,
        request.tolerance,
    )
    .into()
}

pub fn draw_zone(request: DrawRequest) -> Drawn {
    editor::draw_zone(&request.zones, request.from, request.to, request.tolerance).into()
}

pub fn cut_preview(request: CutPreviewRequest) -> CutPreview {
    editor::cut_preview(
        &request.zones,
        request.scope.to_core(),
        request.axis.into(),
        request.at,
        request.tolerance,
    )
    .into()
}

pub fn cut_zones(request: CutRequest) -> Option<Vec<Zone>> {
    editor::cut(
        &request.zones,
        request.scope.to_core(),
        request.axis.into(),
        request.at,
        &request.new_ids,
    )
}

pub fn set_field(request: FieldRequest) -> Vec<Zone> {
    editor::set_field(&request.zones, &request.id, request.field.into(), request.value)
}

pub fn chord_labels(request: ChordRequest) -> ChordLabels {
    let platform = request.platform;
    ChordLabels {
        long: request.chord.long_label(platform),
        parts: request.chord.label_parts(platform),
    }
}

/// Why the chord can't be a shortcut, as a recorder says it, or nothing when it can.
pub fn chord_problem(request: ChordRequest) -> Option<String> {
    request
        .chord
        .check()
        .err()
        .map(|problem| problem.hint(request.platform))
}

/// A chord pressed with its modifiers held, as recording stores it: holding every modifier
/// the Snappy key stands for makes it the Snappy key plus the rest, and anything else stays
/// exactly as held.
pub fn record_chord(request: KeyedChordRequest) -> KeyChord {
    request.chord.recorded(request.snappy_key)
}

/// The chord with the Snappy key expanded: the concrete modifiers to hold.
pub fn expand_chord(request: KeyedChordRequest) -> KeyChord {
    request.chord.combo(request.snappy_key).into()
}

/// What the Snappy key is on the platform, for example "Control + Option + Command".
pub fn snappy_key_label(request: SnappyKeyRequest) -> String {
    request.snappy_key.label(request.platform)
}

/// The settings with the Snappy key changed, or why the change is refused. A key the
/// settings already have changes nothing, as the daemon takes it.
pub fn change_snappy_key(request: SnappyKeyChangeRequest) -> SnappyKeyChange {
    if request.modifiers == request.settings.snappy_key.modifiers() {
        return SnappyKeyChange::Unchanged;
    }
    match request
        .settings
        .with_snappy_key(request.modifiers, request.platform)
    {
        Ok(settings) => SnappyKeyChange::Changed(Box::new(settings)),
        Err(error) => SnappyKeyChange::Refused(error.to_string()),
    }
}

/// A recorder's refusal of a chord the OS or another app already holds, as the Snappy key's
/// probes say it.
pub fn unavailable_shortcut(request: UnavailableRequest) -> Refusal {
    unavailable_refusal(&request.chord, request.shortcut, request.platform)
}

/// The chords to ask the OS about for settings whose Snappy key just changed.
pub fn snappy_key_probes(request: SettingsRequest) -> Vec<AvailabilityProbe> {
    availability_probes(&request.settings, request.platform)
        .into_iter()
        .map(|probe| AvailabilityProbe {
            chord: probe.combo.into(),
            refusal: probe.refusal,
        })
        .collect()
}

/// Each shortcut by the name its settings field has, with its title.
fn titles<T: Serialize>(shortcuts: impl IntoIterator<Item = (T, String)>) -> Titles {
    shortcuts
        .into_iter()
        .filter_map(|(shortcut, title)| match serde_json::to_value(shortcut) {
            Ok(serde_json::Value::String(name)) => Some((name, title)),
            _ => None,
        })
        .collect()
}

/// The Settings page's sections of global shortcuts, each with its shortcuts as `settings`
/// have them.
pub fn shortcut_sections(request: SectionsRequest) -> Vec<SectionOfShortcuts> {
    let settings = &request.settings;
    ShortcutSection::ALL
        .into_iter()
        .map(|section| SectionOfShortcuts {
            section,
            title: section.title(),
            shortcuts: section
                .shortcuts()
                .into_iter()
                .filter_map(|id| {
                    let title = id.title(settings)?;
                    let chord = id.get(settings).cloned();
                    Some(ListedShortcut { id, title, chord })
                })
                .collect(),
        })
        .collect()
}

/// The titles of a workspace's own shortcuts, for the workspace named `workspace`.
pub fn workspace_shortcut_titles(request: WorkspaceNameRequest) -> Titles {
    titles(WorkspaceShortcut::ALL.map(|shortcut| (shortcut, shortcut.title(&request.workspace))))
}

/// The titles of a layout's own shortcuts, for the layout named `layout` with `zones` zones.
pub fn layout_shortcut_titles(request: LayoutTitlesRequest) -> LayoutShortcutTitles {
    LayoutShortcutTitles {
        select: LayoutShortcut::Select.title(&request.layout),
        zones: (0..request.zones)
            .map(|index| LayoutShortcut::Zone(index).title(&request.layout))
            .collect(),
    }
}

/// What each family of default shortcuts does, as help lists them.
pub fn default_shortcuts(request: PlatformRequest) -> Vec<GuideEntry> {
    guide::default_shortcuts(request.platform)
}

pub fn modifier_names(request: ModifierRequest) -> ModifierNames {
    let platform = request.platform;
    ModifierNames {
        name: request.modifier.name(platform),
        label: request.modifier.label(platform),
    }
}

pub fn limits(_: Empty) -> Limits {
    Limits {
        zones: MAX_ZONES,
        workspaces: MAX_WORKSPACES,
        undo_steps: UNDO_LIMIT,
        name_length: MAX_NAME_LENGTH,
    }
}

pub fn presets(_: Empty) -> Vec<Layout> {
    layout::presets().to_vec()
}

pub fn is_valid_layout(request: LayoutRequest) -> bool {
    request.layout.is_valid()
}

pub fn default_settings(request: PlatformRequest) -> Settings {
    Settings::defaults(request.platform)
}

pub fn validate_settings(request: SettingsRequest) -> Validation {
    match request.settings.validate(request.platform) {
        Ok(settings) => Validation::Valid(Box::new(settings)),
        Err(error) => Validation::Invalid(error.to_string()),
    }
}

/// A recorder's conflict, naming the shortcut holding the chord when it is Snappy's.
pub fn recording_conflict_of(request: RecordingRequest) -> Option<Refusal> {
    recording_conflict(
        &request.settings,
        &request.shortcut,
        request.chord,
        request.platform,
    )
}

pub fn layout_conflict(request: LayoutConflictRequest) -> Option<String> {
    conflict_with_layout(
        &request.settings,
        &request.layout,
        &request.options,
        request.platform,
    )
}

pub fn layout_for_display(request: DisplayLayoutRequest) -> Layout {
    let settings = &request.settings;
    match &request.workspace {
        Some(id) => settings
            .layout_in(settings.workspaces.item(id), &request.display)
            .clone(),
        None => settings.layout_for_display(&request.display).clone(),
    }
}

pub fn layout_options(request: LayoutOptionsRequest) -> LayoutOptions {
    request.settings.options(&request.layout)
}

/// The settings with `reset` applied. They are compared once both are normalized, so a reset
/// to what the settings already are counts as no change.
pub fn reset_settings(request: ResetRequest) -> ResetOutcome {
    let platform = request.platform;
    let normalized = |settings: Settings| settings.clone().validate(platform).unwrap_or(settings);
    let settings = normalized(request.settings.reset(&request.reset, platform));
    ResetOutcome {
        changed: settings != normalized(request.settings.clone()),
        message: request.reset.message(&request.settings),
        settings,
    }
}

/// A settings edit by the rules the daemon applies. The daemon validates the result as it
/// saves, so the caller validates it too.
pub fn apply_edit(request: EditRequest) -> EditOutcome {
    let EditRequest {
        mut settings,
        edit,
        platform,
    } = request;
    let mut added_workspace = None;
    let changed = match edit {
        SettingsEdit::AssignLayout { display, layout } => settings.assign_layout(&display, &layout).is_some(),
        SettingsEdit::SaveLayout {
            layout,
            options,
            display,
        } => match settings.save_layout(layout, options, &display, platform) {
            Ok(_) => true,
            Err(error) => return EditOutcome::Refused(error.to_string()),
        },
        SettingsEdit::RemoveLayout { layout } => settings.remove_layout(&layout),
        SettingsEdit::SetLayoutEnabled { layout, enabled } => {
            match settings.set_layout_enabled(&layout, enabled, platform) {
                Ok(_) => true,
                Err(error) => return EditOutcome::Refused(error.to_string()),
            }
        }
        SettingsEdit::EnableWorkspaces { displays } => {
            settings.enable_workspaces(&displays);
            true
        }
        SettingsEdit::AddWorkspace => {
            added_workspace = settings.add_workspace();
            added_workspace.is_some()
        }
        SettingsEdit::UpdateWorkspace { workspace } => {
            settings.update_workspace(workspace);
            true
        }
        SettingsEdit::ReorderWorkspaces { from, to } => {
            settings.reorder_workspaces(&from, to);
            true
        }
        SettingsEdit::RemoveWorkspace { workspace } => settings.remove_workspace(&workspace).is_some(),
    };
    if changed {
        EditOutcome::Edited(Box::new(Edited {
            settings,
            added_workspace,
        }))
    } else {
        EditOutcome::Unchanged
    }
}
