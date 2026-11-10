//! Typed requests and handlers behind each exported function.

use serde::{Deserialize, Serialize};
use snappy_core::bindings::{conflict_with_layout, first_conflict};
use snappy_core::editor::{self, ALIGN_TOLERANCE, ResizeHandle};
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{self, KeyChord, Modifier};
use snappy_core::layout::{self, Layout, Spacing, UnitRect, Zone, zone_frame};
use snappy_core::settings::{LayoutOptions, Settings, Workspace};

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
pub struct RecordedChordRequest {
    chord: KeyChord,
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
pub struct NewWorkspaceRequest {
    settings: Settings,
    id: String,
    number: usize,
}

#[derive(Serialize)]
pub struct ChordLabels {
    compact: String,
    long: String,
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

/// A recorded chord in stored form: holding every modifier the Snappy key stands for makes
/// it the Snappy key plus the rest. Parsing the chord normalizes it.
pub fn normalize_chord(request: RecordedChordRequest) -> KeyChord {
    request.chord
}

/// What the Snappy key is on the platform, for example "Control + Option + Command".
pub fn snappy_key_label(request: PlatformRequest) -> String {
    keys::snappy_key_label(request.platform.into())
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
