//! Typed requests and handlers behind each exported function.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use snappy_core::bindings::{
    Refusal, availability_probes, conflict_with_layout, recording_conflict, unavailable_refusal,
};
use snappy_core::editor::{self, ALIGN_TOLERANCE, ResizeHandle};
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::guide::{self, GuideEntry};
use snappy_core::keys::{KeyChord, LabelParts, Modifier, Mods, Platform, SnappyKey};
use snappy_core::layout::{self, Layout, MAX_ZONES, Spacing, UnitRect, Zone, zone_frame};
use snappy_core::operations::UNDO_LIMIT;
use snappy_core::reset::{Reset, ShortcutSection};
use snappy_core::settings::{
    LayoutOptions, LayoutShortcut, MAX_WORKSPACES, Settings, Workspace, WorkspaceShortcut,
};
use snappy_core::shortcuts::ShortcutId;

use crate::gestures::{self, CutPreview, Drawn, Edit};
use crate::wire::{Axis, CutScope, Divider, Field, Handle, HandlePosition};

#[derive(Deserialize)]
pub struct Empty {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneFramesRequest {
    zones: Vec<Zone>,
    work_area: Rect,
    spacing_factor: f64,
}

#[derive(Deserialize)]
pub struct ZonesRequest {
    zones: Vec<Zone>,
}

#[derive(Deserialize)]
pub struct HandleAtRequest {
    point: Point,
    size: Size,
}

#[derive(Deserialize)]
pub struct SizeRequest {
    size: Size,
}

#[derive(Deserialize)]
pub struct MoveRequest {
    zones: Vec<Zone>,
    id: String,
    delta: Point,
    tolerance: Size,
}

#[derive(Deserialize)]
pub struct ResizeRequest {
    zones: Vec<Zone>,
    id: String,
    handle: Handle,
    delta: Point,
    tolerance: Size,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DividerRequest {
    zones: Vec<Zone>,
    zone_id: String,
    axis: Axis,
    line: f64,
    delta: f64,
    tolerance: f64,
}

#[derive(Deserialize)]
pub struct DrawRequest {
    zones: Vec<Zone>,
    from: Point,
    to: Point,
    tolerance: Size,
}

#[derive(Deserialize)]
pub struct CopiesRequest {
    zones: Vec<Zone>,
    copied: Vec<UnitRect>,
}

#[derive(Deserialize)]
pub struct CutPreviewRequest {
    zones: Vec<Zone>,
    scope: CutScope,
    axis: Axis,
    at: f64,
    tolerance: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CutRequest {
    zones: Vec<Zone>,
    scope: CutScope,
    axis: Axis,
    /// The line itself, already snapped by the preview.
    at: f64,
    /// At least one per zone the cut may split.
    new_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    zones: Vec<Zone>,
    ids: Vec<String>,
    zone_shortcuts: BTreeMap<String, KeyChord>,
}

#[derive(Deserialize)]
pub struct FieldRequest {
    zones: Vec<Zone>,
    id: String,
    field: Field,
    value: f64,
}

#[derive(Deserialize)]
pub struct ChordRequest {
    chord: KeyChord,
    platform: Platform,
}

#[derive(Deserialize)]
pub struct UnavailableRequest {
    chord: KeyChord,
    /// The title of the shortcut being recorded.
    title: String,
    platform: Platform,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyedChordRequest {
    chord: KeyChord,
    snappy_key: SnappyKey,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnappyKeyRequest {
    snappy_key: SnappyKey,
    platform: Platform,
}

#[derive(Deserialize)]
pub struct SnappyKeyChangeRequest {
    settings: Settings,
    modifiers: Mods,
    platform: Platform,
}

#[derive(Deserialize)]
pub struct ModifierRequest {
    modifier: Modifier,
    platform: Platform,
}

#[derive(Deserialize)]
pub struct PlatformRequest {
    platform: Platform,
}

#[derive(Deserialize)]
pub struct SettingsRequest {
    settings: Settings,
    platform: Platform,
}

#[derive(Deserialize)]
pub struct RecordingRequest {
    settings: Settings,
    /// The shortcut being recorded, which the message names from its side.
    shortcut: ShortcutId,
    chord: KeyChord,
    platform: Platform,
}

#[derive(Deserialize)]
pub struct ResetRequest {
    settings: Settings,
    reset: Reset,
    platform: Platform,
}

/// Settings put back to their defaults, whether that changes anything, and what to say.
#[derive(Serialize)]
pub struct ResetOutcome {
    settings: Settings,
    changed: bool,
    message: String,
}

#[derive(Deserialize)]
pub struct LayoutConflictRequest {
    settings: Settings,
    layout: Layout,
    options: LayoutOptions,
    platform: Platform,
}

#[derive(Deserialize)]
pub struct LayoutRequest {
    layout: Layout,
}

#[derive(Deserialize)]
pub struct DisplayLayoutRequest {
    settings: Settings,
    display: String,
}

#[derive(Deserialize)]
pub struct LayoutOptionsRequest {
    settings: Settings,
    layout: Layout,
}

#[derive(Deserialize)]
pub struct SectionsRequest {
    settings: Settings,
}

#[derive(Deserialize)]
pub struct WorkspaceNameRequest {
    workspace: String,
}

#[derive(Deserialize)]
pub struct LayoutTitlesRequest {
    layout: String,
    zones: usize,
}

#[derive(Deserialize)]
pub struct NewWorkspaceRequest {
    settings: Settings,
    id: String,
    number: usize,
}

/// Titles by the name the settings file gives each shortcut, such as `"focusUp"`.
type Titles = BTreeMap<String, String>;

/// The titles of a layout's own shortcuts.
#[derive(Serialize)]
pub struct LayoutShortcutTitles {
    select: String,
    /// One per zone, in the layout's order.
    zones: Vec<String>,
}

/// One section of global shortcuts, as the Settings page shows it.
#[derive(Serialize)]
pub struct SectionOfShortcuts {
    section: ShortcutSection,
    title: &'static str,
    shortcuts: Vec<ListedShortcut>,
}

/// A shortcut as a section lists it: what it is, its title, and its chord.
#[derive(Serialize)]
pub struct ListedShortcut {
    id: ShortcutId,
    title: String,
    chord: Option<KeyChord>,
}

/// A chord's label as messages and assistive technology say it, and in two parts for
/// recorders, cards, and references, which draw the Snappy key their own way.
#[derive(Serialize)]
pub struct ChordLabels {
    long: String,
    parts: LabelParts,
}

/// A chord to ask the OS about before the Snappy key changes: its concrete modifiers, and
/// what to say when it is taken.
#[derive(Serialize)]
pub struct AvailabilityProbe {
    chord: KeyChord,
    refusal: Refusal,
}

#[derive(Serialize)]
pub struct ModifierNames {
    name: &'static str,
    /// The name with its symbol, as messages show it: "Shift ⇧".
    label: String,
}

/// The most of each thing the settings app counts that core allows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    zones: usize,
    workspaces: usize,
    /// Window moves that undo remembers.
    undo_steps: usize,
}

/// A layout's zones and zone shortcuts after a merge.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedLayout {
    zones: Vec<Zone>,
    zone_shortcuts: BTreeMap<String, KeyChord>,
    /// The merged zone's ID.
    kept: String,
}

/// The outcome of merging zones: the merged layout, or why the zones can't be merged.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeOutcome {
    Merged(MergedLayout),
    Refused(String),
}

/// Where copies go, or why they can't be placed.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaceOutcome {
    Placed(Vec<UnitRect>),
    Refused(String),
}

/// The outcome of changing the Snappy key: nothing to change, the settings with the new key,
/// or why it is refused.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SnappyKeyChange {
    Unchanged,
    Changed(Box<Settings>),
    Refused(String),
}

/// The outcome of validating settings: the normalized settings, or the first problem.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Validation {
    Valid(Box<Settings>),
    Invalid(String),
}

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
    gestures::drag_move(&request.zones, &request.id, request.delta, request.tolerance)
}

pub fn drag_resize(request: ResizeRequest) -> Edit {
    gestures::drag_resize(
        &request.zones,
        &request.id,
        request.handle.into(),
        request.delta,
        request.tolerance,
    )
}

pub fn drag_divider(request: DividerRequest) -> Edit {
    gestures::drag_divider(
        &request.zones,
        &request.zone_id,
        request.axis.into(),
        request.line,
        request.delta,
        request.tolerance,
    )
}

pub fn draw_zone(request: DrawRequest) -> Drawn {
    gestures::draw_zone(&request.zones, request.from, request.to, request.tolerance)
}

pub fn cut_preview(request: CutPreviewRequest) -> CutPreview {
    gestures::cut_preview(
        &request.zones,
        request.scope.to_core(),
        request.axis.into(),
        request.at,
        request.tolerance,
    )
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
    gestures::set_field(&request.zones, &request.id, request.field, request.value)
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
    unavailable_refusal(&request.chord, &request.title, request.platform)
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
    request.settings.layout_for_display(&request.display).clone()
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

pub fn new_workspace(request: NewWorkspaceRequest) -> Workspace {
    request.settings.new_workspace(request.id, request.number)
}
