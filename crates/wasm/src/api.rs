//! Typed requests and handlers behind each exported function.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use snappy_core::bindings::{availability_probes, conflict_with_layout, first_conflict};
use snappy_core::editor::{self, ALIGN_TOLERANCE, ResizeHandle};
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::guide::{self, GuideEntry};
use snappy_core::keys::{KeyChord, Modifier, Mods, SnappyKey};
use snappy_core::layout::{self, Layout, Spacing, UnitRect, Zone, zone_frame};
use snappy_core::settings::{
    HistoryStep, LayoutOptions, LayoutShortcut, NavigationShortcut, Settings, Workspace, WorkspaceShortcut,
    WorkspaceStep,
};

use crate::gestures::{self, Drawn, Edit};
use crate::wire::{Axis, Divider, Field, Handle, HandlePosition, Platform};

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
pub struct ZoneRequest {
    zones: Vec<Zone>,
    id: String,
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
#[serde(rename_all = "camelCase")]
pub struct CutRequest {
    zones: Vec<Zone>,
    id: String,
    axis: Axis,
    at: f64,
    tolerance: f64,
    #[serde(default)]
    new_id: String,
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

/// Every shortcut's title, by section, as the settings app shows them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutTitles {
    navigation: Titles,
    workspace_steps: Titles,
    history: Titles,
}

#[derive(Serialize)]
pub struct ChordLabels {
    compact: String,
    long: String,
}

/// A chord to ask the OS about before the Snappy key changes: its concrete modifiers, and
/// what to say when it is taken.
#[derive(Serialize)]
pub struct AvailabilityProbe {
    chord: KeyChord,
    refusal: String,
}

#[derive(Serialize)]
pub struct ModifierNames {
    name: &'static str,
    short: &'static str,
    /// The name with its symbol, as messages show it: "Shift ⇧".
    label: String,
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

pub fn duplicate_rect(request: ZoneRequest) -> Option<UnitRect> {
    let source = request.zones.iter().find(|zone| zone.id == request.id)?;
    editor::duplicate_rect(source.rect, &request.zones)
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

pub fn cut_line(request: CutRequest) -> f64 {
    gestures::cut_line(
        &request.zones,
        &request.id,
        request.axis.into(),
        request.at,
        request.tolerance,
    )
}

pub fn cut_zone(request: CutRequest) -> Option<Vec<Zone>> {
    gestures::cut(
        &request.zones,
        &request.id,
        request.axis.into(),
        request.at,
        request.tolerance,
        &request.new_id,
    )
}

pub fn set_field(request: FieldRequest) -> Vec<Zone> {
    gestures::set_field(&request.zones, &request.id, request.field, request.value)
}

pub fn chord_labels(request: ChordRequest) -> ChordLabels {
    let platform = request.platform.into();
    ChordLabels {
        compact: request.chord.compact_label(platform),
        long: request.chord.long_label(platform),
    }
}

pub fn is_valid_chord(request: ChordRequest) -> bool {
    request.chord.is_valid()
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
    request.snappy_key.label(request.platform.into())
}

/// The settings with the Snappy key changed, or why the change is refused.
pub fn change_snappy_key(request: SnappyKeyChangeRequest) -> Validation {
    match request
        .settings
        .with_snappy_key(request.modifiers, request.platform.into())
    {
        Ok(settings) => Validation::Valid(Box::new(settings)),
        Err(error) => Validation::Invalid(error.to_string()),
    }
}

/// The chords to ask the OS about for settings whose Snappy key just changed.
pub fn snappy_key_probes(request: SettingsRequest) -> Vec<AvailabilityProbe> {
    availability_probes(&request.settings, request.platform.into())
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

pub fn shortcut_titles(_: Empty) -> ShortcutTitles {
    ShortcutTitles {
        navigation: titles(NavigationShortcut::ALL.map(|shortcut| (shortcut, shortcut.title()))),
        workspace_steps: titles(WorkspaceStep::ALL.map(|step| (step, step.title().to_owned()))),
        history: titles(HistoryStep::ALL.map(|step| (step, step.title().to_owned()))),
    }
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
    guide::default_shortcuts(request.platform.into())
}

pub fn modifier_names(request: ModifierRequest) -> ModifierNames {
    let platform = request.platform.into();
    ModifierNames {
        name: request.modifier.name(platform),
        short: request.modifier.short_name(platform),
        label: request.modifier.label(platform),
    }
}

pub fn presets(_: Empty) -> Vec<Layout> {
    layout::presets().to_vec()
}

pub fn is_valid_layout(request: LayoutRequest) -> bool {
    request.layout.is_valid()
}

pub fn default_settings(request: PlatformRequest) -> Settings {
    Settings::defaults(request.platform.into())
}

pub fn validate_settings(request: SettingsRequest) -> Validation {
    match request.settings.validate(request.platform.into()) {
        Ok(settings) => Validation::Valid(Box::new(settings)),
        Err(error) => Validation::Invalid(error.to_string()),
    }
}

pub fn settings_conflict(request: SettingsRequest) -> Option<String> {
    first_conflict(&request.settings, request.platform.into())
}

pub fn layout_conflict(request: LayoutConflictRequest) -> Option<String> {
    conflict_with_layout(
        &request.settings,
        &request.layout,
        &request.options,
        request.platform.into(),
    )
}

pub fn layout_for_display(request: DisplayLayoutRequest) -> Layout {
    request.settings.layout_for_display(&request.display).clone()
}

pub fn layout_options(request: LayoutOptionsRequest) -> LayoutOptions {
    request.settings.options(&request.layout)
}

pub fn new_workspace(request: NewWorkspaceRequest) -> Workspace {
    request.settings.new_workspace(request.id, request.number)
}
