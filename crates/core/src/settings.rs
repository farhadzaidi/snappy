//! The settings file: its model, defaults, and validation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::bindings;
use crate::keys::{KeyChord, Modifier, Platform};
use crate::layout::{self, DEFAULT_LAYOUT_ID, FALLBACK_LAYOUT_ID, Layout, valid_id, valid_name};
use crate::navigate::Direction;
use crate::swap::{SwapModifier, WhenTaken};

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_FILE_BYTES: usize = 2_000_000;
pub const MAX_CUSTOM_LAYOUTS: usize = 100;
pub const MAX_LAYOUT_OPTIONS: usize = 200;
pub const MAX_WORKSPACES: usize = 32;
pub const MAX_WORKSPACE_ID_LENGTH: usize = 128;
pub const MAX_DISPLAY_ASSIGNMENTS: usize = 32;
/// Zone shortcuts are created by default for this many leading zones.
const DEFAULT_ZONE_SHORTCUTS: usize = 9;

/// Display ID to layout ID.
pub type DisplayAssignments = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub snapping: Snapping,
    pub navigation: Navigation,
    pub layouts: Layouts,
    pub workspaces: Workspaces,
    #[serde(default)]
    pub history: MoveHistory,
    /// Record what Snappy does to the debug log.
    #[serde(default)]
    pub debug_log: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapping {
    pub enabled: bool,
    pub drag_modifier: DragModifier,
    /// Held during a drag, it does the opposite of `when_taken` for that drop.
    pub swap_modifier: SwapModifier,
    /// What a keyboard move or drop into a zone another window fills does.
    #[serde(default)]
    pub when_taken: WhenTaken,
    pub shortcuts_enabled: bool,
}

/// The key held while dragging to show zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DragModifier {
    Shift,
    Alt,
    Ctrl,
}

impl DragModifier {
    pub fn modifier(self) -> Modifier {
        match self {
            DragModifier::Shift => Modifier::Shift,
            DragModifier::Alt => Modifier::Alt,
            DragModifier::Ctrl => Modifier::Ctrl,
        }
    }
}

/// Move-between-zones shortcuts.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Navigation {
    pub left: Option<KeyChord>,
    pub right: Option<KeyChord>,
    pub up: Option<KeyChord>,
    pub down: Option<KeyChord>,
    /// Move the focused window into the nearest empty zone.
    #[serde(default)]
    pub nearest_empty: Option<KeyChord>,
}

/// The name the settings app and conflict messages give the nearest-empty-zone shortcut.
pub const NEAREST_EMPTY_TITLE: &str = "Move to the nearest empty zone";

impl Navigation {
    pub fn get(&self, direction: Direction) -> Option<&KeyChord> {
        match direction {
            Direction::Left => self.left.as_ref(),
            Direction::Right => self.right.as_ref(),
            Direction::Up => self.up.as_ref(),
            Direction::Down => self.down.as_ref(),
        }
    }

    pub fn set(&mut self, direction: Direction, chord: Option<KeyChord>) {
        let slot = match direction {
            Direction::Left => &mut self.left,
            Direction::Right => &mut self.right,
            Direction::Up => &mut self.up,
            Direction::Down => &mut self.down,
        };
        *slot = chord;
    }

    pub fn chords(&self) -> impl Iterator<Item = (Direction, &KeyChord)> {
        Direction::ALL
            .into_iter()
            .filter_map(|direction| self.get(direction).map(|chord| (direction, chord)))
    }
}

/// Undo and redo window-move shortcuts. Neither has a default.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MoveHistory {
    pub undo: Option<KeyChord>,
    pub redo: Option<KeyChord>,
}

/// Which way through the window-move history a shortcut goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryStep {
    Undo,
    Redo,
}

impl HistoryStep {
    pub const ALL: [HistoryStep; 2] = [HistoryStep::Undo, HistoryStep::Redo];

    /// The name the settings app and conflict messages use.
    pub fn title(self) -> &'static str {
        match self {
            HistoryStep::Undo => "Undo window move",
            HistoryStep::Redo => "Redo window move",
        }
    }
}

impl MoveHistory {
    pub fn get(&self, step: HistoryStep) -> Option<&KeyChord> {
        match step {
            HistoryStep::Undo => self.undo.as_ref(),
            HistoryStep::Redo => self.redo.as_ref(),
        }
    }

    pub fn set(&mut self, step: HistoryStep, chord: Option<KeyChord>) {
        let slot = match step {
            HistoryStep::Undo => &mut self.undo,
            HistoryStep::Redo => &mut self.redo,
        };
        *slot = chord;
    }

    pub fn chords(&self) -> impl Iterator<Item = (HistoryStep, &KeyChord)> {
        HistoryStep::ALL
            .into_iter()
            .filter_map(|step| self.get(step).map(|chord| (step, chord)))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layouts {
    pub default_layout_id: String,
    /// Each display's own layout, used while workspaces are off.
    pub displays: DisplayAssignments,
    pub custom: Vec<Layout>,
    pub options: BTreeMap<String, LayoutOptions>,
}

/// Per-layout activation and shortcuts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutOptions {
    pub enabled: bool,
    pub select_shortcut: Option<KeyChord>,
    pub zone_shortcuts: BTreeMap<String, KeyChord>,
}

impl LayoutOptions {
    pub fn empty() -> Self {
        Self {
            enabled: true,
            select_shortcut: None,
            zone_shortcuts: BTreeMap::new(),
        }
    }

    /// Options for a layout that has none stored: the Snappy key with 1…9 on the first nine
    /// zones.
    pub fn defaults_for(layout: &Layout) -> Self {
        let zone_shortcuts = layout
            .zones
            .iter()
            .take(DEFAULT_ZONE_SHORTCUTS)
            .enumerate()
            .map(|(index, zone)| {
                let digit = index + 1;
                let chord = KeyChord::snappy(&format!("Digit{digit}"), []);
                (zone.id.clone(), chord.with_label(digit.to_string()))
            })
            .collect();
        Self {
            enabled: true,
            select_shortcut: None,
            zone_shortcuts,
        }
    }

    fn is_valid(&self) -> bool {
        self.zone_shortcuts.len() <= layout::MAX_ZONES
            && self.select_shortcut.as_ref().is_none_or(KeyChord::is_valid)
            && self.zone_shortcuts.values().all(KeyChord::is_valid)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspaces {
    pub enabled: bool,
    pub active_id: String,
    pub previous: Option<KeyChord>,
    pub next: Option<KeyChord>,
    /// Take the focused window to the previous workspace and switch there.
    #[serde(default)]
    pub move_previous: Option<KeyChord>,
    /// Take the focused window to the next workspace and switch there.
    #[serde(default)]
    pub move_next: Option<KeyChord>,
    pub items: Vec<Workspace>,
}

/// The shortcuts that step to the previous or next workspace, alone or taking the focused
/// window along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceStep {
    Previous,
    Next,
    MovePrevious,
    MoveNext,
}

impl WorkspaceStep {
    pub const ALL: [WorkspaceStep; 4] = [
        WorkspaceStep::Previous,
        WorkspaceStep::Next,
        WorkspaceStep::MovePrevious,
        WorkspaceStep::MoveNext,
    ];

    /// The steps that settings files written before they existed lack.
    const ADDED_LATER: [WorkspaceStep; 2] = [WorkspaceStep::MovePrevious, WorkspaceStep::MoveNext];

    /// How far through the saved order this step goes.
    pub fn offset(self) -> isize {
        match self {
            WorkspaceStep::Previous | WorkspaceStep::MovePrevious => -1,
            WorkspaceStep::Next | WorkspaceStep::MoveNext => 1,
        }
    }

    /// True when the focused window goes along.
    pub fn takes_window(self) -> bool {
        matches!(self, WorkspaceStep::MovePrevious | WorkspaceStep::MoveNext)
    }

    /// The name the settings app and conflict messages use.
    pub fn title(self) -> &'static str {
        match self {
            WorkspaceStep::Previous => "Previous workspace",
            WorkspaceStep::Next => "Next workspace",
            WorkspaceStep::MovePrevious => "Move window to previous workspace",
            WorkspaceStep::MoveNext => "Move window to next workspace",
        }
    }

    /// The field that holds this step's chord in the settings file.
    fn field(self) -> &'static str {
        match self {
            WorkspaceStep::Previous => "previous",
            WorkspaceStep::Next => "next",
            WorkspaceStep::MovePrevious => "movePrevious",
            WorkspaceStep::MoveNext => "moveNext",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub displays: DisplayAssignments,
    pub switch_shortcut: Option<KeyChord>,
    pub send_shortcut: Option<KeyChord>,
    pub send_and_switch_shortcut: Option<KeyChord>,
}

impl Workspace {
    /// "Workspace <number>", with no shortcuts of its own.
    pub fn numbered(id: impl Into<String>, number: usize) -> Self {
        Self {
            id: id.into(),
            name: format!("Workspace {number}"),
            displays: DisplayAssignments::new(),
            switch_shortcut: None,
            send_shortcut: None,
            send_and_switch_shortcut: None,
        }
    }

    pub fn chords(&self) -> impl Iterator<Item = &KeyChord> {
        [
            &self.switch_shortcut,
            &self.send_shortcut,
            &self.send_and_switch_shortcut,
        ]
        .into_iter()
        .flatten()
    }

    fn is_valid(&self) -> bool {
        !self.id.is_empty()
            && self.id.chars().count() <= MAX_WORKSPACE_ID_LENGTH
            && valid_name(&self.name)
            && self.displays.len() <= MAX_DISPLAY_ASSIGNMENTS
            && self.chords().all(KeyChord::is_valid)
    }
}

impl Workspaces {
    /// Three workspaces, stepped through with the Snappy key and the semicolon and quote
    /// keys, or with the period and slash keys to take the focused window along. Both pairs
    /// sit together under the right hand, on every keyboard layout's home and bottom rows.
    pub fn defaults() -> Self {
        let chord = |code: &str, label: &str| Some(KeyChord::snappy(code, []).with_label(label));
        Self {
            enabled: false,
            active_id: "workspace-1".to_owned(),
            previous: chord("Semicolon", ";"),
            next: chord("Quote", "'"),
            move_previous: chord("Period", "."),
            move_next: chord("Slash", "/"),
            items: (1..=3)
                .map(|n| Workspace::numbered(format!("workspace-{n}"), n))
                .collect(),
        }
    }

    pub fn step(&self, step: WorkspaceStep) -> Option<&KeyChord> {
        match step {
            WorkspaceStep::Previous => self.previous.as_ref(),
            WorkspaceStep::Next => self.next.as_ref(),
            WorkspaceStep::MovePrevious => self.move_previous.as_ref(),
            WorkspaceStep::MoveNext => self.move_next.as_ref(),
        }
    }

    pub fn set_step(&mut self, step: WorkspaceStep, chord: Option<KeyChord>) {
        let slot = match step {
            WorkspaceStep::Previous => &mut self.previous,
            WorkspaceStep::Next => &mut self.next,
            WorkspaceStep::MovePrevious => &mut self.move_previous,
            WorkspaceStep::MoveNext => &mut self.move_next,
        };
        *slot = chord;
    }

    /// Every assigned step chord.
    pub fn steps(&self) -> impl Iterator<Item = (WorkspaceStep, &KeyChord)> {
        WorkspaceStep::ALL
            .into_iter()
            .filter_map(|step| self.step(step).map(|chord| (step, chord)))
    }

    pub fn active(&self) -> Option<&Workspace> {
        self.items.iter().find(|item| item.id == self.active_id)
    }

    pub fn item(&self, id: &str) -> Option<&Workspace> {
        self.items.iter().find(|item| item.id == id)
    }

    /// The workspace `steps` away from the active one in saved order, wrapping.
    pub fn adjacent(&self, from: &str, steps: isize) -> Option<&Workspace> {
        let count = self.items.len() as isize;
        let index = self.items.iter().position(|item| item.id == from).unwrap_or(0) as isize;
        let target = (index + steps).rem_euclid(count.max(1));
        self.items.get(target as usize)
    }
}

impl Navigation {
    pub fn defaults() -> Self {
        let chord = |code: &str, label: &str| Some(KeyChord::snappy(code, []).with_label(label));
        Self {
            left: chord("ArrowLeft", "←"),
            right: chord("ArrowRight", "→"),
            up: chord("ArrowUp", "↑"),
            down: chord("ArrowDown", "↓"),
            nearest_empty: Some(KeyChord::snappy("Space", [])),
        }
    }
}

impl Settings {
    pub fn defaults(platform: Platform) -> Self {
        Self {
            version: FORMAT_VERSION,
            snapping: Snapping {
                enabled: true,
                drag_modifier: DragModifier::Shift,
                swap_modifier: match platform {
                    Platform::MacOs => SwapModifier::Meta,
                    Platform::Windows | Platform::Linux => SwapModifier::Ctrl,
                },
                when_taken: WhenTaken::default(),
                shortcuts_enabled: true,
            },
            navigation: Navigation::defaults(),
            layouts: Layouts {
                default_layout_id: DEFAULT_LAYOUT_ID.to_owned(),
                displays: DisplayAssignments::new(),
                custom: Vec::new(),
                options: BTreeMap::new(),
            },
            workspaces: Workspaces::defaults(),
            history: MoveHistory::default(),
            debug_log: false,
        }
    }

    /// Presets first, then custom layouts.
    pub fn layouts(&self) -> impl Iterator<Item = &Layout> {
        layout::presets().iter().chain(&self.layouts.custom)
    }

    pub fn layout(&self, id: &str) -> Option<&Layout> {
        self.layouts().find(|layout| layout.id == id)
    }

    /// Stored options, or the defaults for a layout that has none.
    pub fn options(&self, layout: &Layout) -> LayoutOptions {
        self.layouts
            .options
            .get(&layout.id)
            .cloned()
            .unwrap_or_else(|| LayoutOptions::defaults_for(layout))
    }

    pub fn is_enabled(&self, layout: &Layout) -> bool {
        self.options(layout).enabled
    }

    pub fn enabled_layouts(&self) -> impl Iterator<Item = &Layout> {
        self.layouts().filter(|layout| self.is_enabled(layout))
    }

    /// The layout a display uses.
    pub fn layout_for_display(&self, display_id: &str) -> &Layout {
        let workspace_choice = self
            .workspaces
            .enabled
            .then(|| {
                self.workspaces
                    .active()
                    .and_then(|workspace| workspace.displays.get(display_id))
            })
            .flatten();
        let chosen = workspace_choice
            .or_else(|| self.layouts.displays.get(display_id))
            .unwrap_or(&self.layouts.default_layout_id);

        self.layout(chosen)
            .filter(|layout| self.is_enabled(layout))
            .or_else(|| self.enabled_layouts().next())
            .or_else(|| layout::preset(FALLBACK_LAYOUT_ID))
            .unwrap_or(&layout::presets()[0])
    }

    /// Check every rule and return the normalized settings, or the first problem.
    pub fn validate(mut self, platform: Platform) -> Result<Settings, SettingsError> {
        if self.version != FORMAT_VERSION {
            return Err(SettingsError::UnsupportedVersion(self.version));
        }
        self.check_shapes()?;
        self.normalize();
        if let Some(conflict) = bindings::first_conflict(&self, platform) {
            return Err(SettingsError::Conflict(conflict));
        }
        Ok(self)
    }

    fn check_shapes(&self) -> Result<(), SettingsError> {
        let custom_ids: BTreeSet<&str> = self
            .layouts
            .custom
            .iter()
            .map(|layout| layout.id.as_str())
            .collect();
        let workspace_ids: BTreeSet<&str> = self
            .workspaces
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        let valid = self.layouts.custom.len() <= MAX_CUSTOM_LAYOUTS
            && custom_ids.len() == self.layouts.custom.len()
            && self
                .layouts
                .custom
                .iter()
                .all(|layout| layout.is_valid() && !layout.is_preset())
            && self.layouts.options.len() <= MAX_LAYOUT_OPTIONS
            && self.layouts.options.values().all(LayoutOptions::is_valid)
            && (1..=MAX_WORKSPACES).contains(&self.workspaces.items.len())
            && workspace_ids.len() == self.workspaces.items.len()
            && workspace_ids.contains(self.workspaces.active_id.as_str())
            && self.workspaces.items.iter().all(Workspace::is_valid)
            && self.workspaces.steps().all(|(_, chord)| chord.is_valid())
            && self.navigation.chords().all(|(_, chord)| chord.is_valid())
            && self
                .navigation
                .nearest_empty
                .as_ref()
                .is_none_or(KeyChord::is_valid)
            && self.history.chords().all(|(_, chord)| chord.is_valid());
        if valid {
            Ok(())
        } else {
            Err(SettingsError::Invalid)
        }
    }

    fn normalize(&mut self) {
        self.snapping.swap_modifier = self
            .snapping
            .swap_modifier
            .effective(self.snapping.drag_modifier.modifier());

        let known: BTreeSet<String> = self.layouts().map(|layout| layout.id.clone()).collect();
        self.layouts
            .displays
            .retain(|_, layout_id| known.contains(layout_id));
        for workspace in &mut self.workspaces.items {
            workspace
                .displays
                .retain(|_, layout_id| known.contains(layout_id));
        }
        if !known.contains(&self.layouts.default_layout_id) {
            self.layouts.default_layout_id = DEFAULT_LAYOUT_ID.to_owned();
        }

        let options: BTreeMap<String, LayoutOptions> = self
            .layouts()
            .map(|layout| {
                let mut options = self.options(layout);
                options
                    .zone_shortcuts
                    .retain(|zone_id, _| layout.zone(zone_id).is_some());
                (layout.id.clone(), options)
            })
            .collect();
        self.layouts.options = options;
    }

    /// Parse a settings file. Fields missing from the file take their defaults; a file
    /// without `navigation`, without the nearest-empty-zone shortcut, or without the
    /// move-window workspace steps, gets those default chords only where unused.
    pub fn from_json(bytes: &[u8], platform: Platform) -> Result<Settings, SettingsError> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(SettingsError::Invalid);
        }
        let file: SettingsFile = serde_json::from_slice(bytes).map_err(|_| SettingsError::Invalid)?;
        let defaults = Settings::defaults(platform);
        let navigation_missing = file.navigation.is_none();
        let nearest_empty_missing = file
            .navigation
            .as_ref()
            .is_some_and(|written| written.get("nearestEmpty").is_none());
        let navigation = match file.navigation {
            Some(written) => serde_json::from_value(written).map_err(|_| SettingsError::Invalid)?,
            None => Navigation::default(),
        };
        let missing_steps: Vec<WorkspaceStep> = file
            .workspaces
            .as_ref()
            .map(|written| {
                WorkspaceStep::ADDED_LATER
                    .into_iter()
                    .filter(|step| written.get(step.field()).is_none())
                    .collect()
            })
            .unwrap_or_default();
        let workspaces = match file.workspaces {
            Some(written) => serde_json::from_value(written).map_err(|_| SettingsError::Invalid)?,
            None => defaults.workspaces.clone(),
        };
        let mut settings = Settings {
            version: file.version.unwrap_or(FORMAT_VERSION),
            snapping: file.snapping.unwrap_or(defaults.snapping),
            navigation,
            layouts: file.layouts.unwrap_or(defaults.layouts),
            workspaces,
            history: file.history.unwrap_or_default(),
            debug_log: file.debug_log.unwrap_or(defaults.debug_log),
        };
        if navigation_missing {
            settings.navigation = bindings::unused_navigation_defaults(&settings);
        }
        let used = bindings::bound_combos(&settings);
        for step in missing_steps {
            let chord = defaults
                .workspaces
                .step(step)
                .filter(|chord| !used.contains(&chord.combo()));
            settings.workspaces.set_step(step, chord.cloned());
        }
        if nearest_empty_missing {
            settings.navigation.nearest_empty = defaults
                .navigation
                .nearest_empty
                .filter(|chord| !used.contains(&chord.combo()));
        }
        settings.validate(platform)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, SettingsError> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| SettingsError::Invalid)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(SettingsError::Invalid);
        }
        Ok(bytes)
    }
}

/// The settings file as written, every section optional.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsFile {
    version: Option<u32>,
    snapping: Option<Snapping>,
    /// Kept as written so shortcuts added after the file was saved can be told apart.
    navigation: Option<serde_json::Value>,
    layouts: Option<Layouts>,
    /// Kept as written so steps added after the file was saved can be told apart.
    workspaces: Option<serde_json::Value>,
    history: Option<MoveHistory>,
    debug_log: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SettingsError {
    #[error("This file uses settings version {0}. Snappy reads version 1.")]
    UnsupportedVersion(u32),
    #[error("The settings file contains invalid data.")]
    Invalid,
    #[error("{0}")]
    Conflict(String),
    #[error(
        "The existing settings file could not be loaded and has not been changed. Repair or move it, then reopen Snappy before saving."
    )]
    Protected,
}

/// True when `id` names a valid custom layout ID.
pub fn is_custom_layout_id(id: &str) -> bool {
    valid_id(id) && !layout::is_preset_id(id)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::keys::Mods;
    use crate::layout::{UnitRect, Zone};

    const MAC: Platform = Platform::MacOs;

    fn platform() -> impl Strategy<Value = Platform> {
        prop_oneof![
            Just(Platform::MacOs),
            Just(Platform::Windows),
            Just(Platform::Linux)
        ]
    }

    /// A chord on a few keys with any modifiers, so some are invalid or conflict.
    fn chord() -> impl Strategy<Value = KeyChord> {
        let codes = prop::sample::select(vec![
            "Digit1",
            "KeyK",
            "KeyQ",
            "ArrowLeft",
            "Comma",
            "BracketLeft",
        ]);
        (codes, prop::collection::vec(any::<bool>(), 4)).prop_map(|(code, held)| {
            let modifiers = [Modifier::Shift, Modifier::Ctrl, Modifier::Alt, Modifier::Meta];
            let mods: Mods = modifiers
                .into_iter()
                .zip(held)
                .filter_map(|(modifier, down)| down.then_some(modifier))
                .collect();
            KeyChord::new(code, mods.iter())
        })
    }

    /// The defaults with random edits: choices, display assignments to known and unknown
    /// layouts, options for presets and unknown zones, and a few shortcuts.
    fn edited_settings() -> impl Strategy<Value = (Settings, Platform)> {
        let layout_ids = prop::sample::select(vec!["halves", "rows", "focus", "six", "missing", "gone"]);
        (
            platform(),
            prop::sample::select(vec![DragModifier::Shift, DragModifier::Alt, DragModifier::Ctrl]),
            prop::sample::select(vec![
                SwapModifier::Meta,
                SwapModifier::Ctrl,
                SwapModifier::Alt,
                SwapModifier::Shift,
                SwapModifier::Off,
            ]),
            any::<bool>(),
            prop::collection::btree_map("[a-c]", layout_ids.clone(), 0..3),
            layout_ids,
            prop::collection::vec((any::<bool>(), any::<bool>()), 8),
            any::<bool>(),
            prop::option::of(chord()),
            prop::option::of(chord()),
        )
            .prop_map(
                |(platform, drag, swap, stack, displays, default, options, workspaces, switch, undo)| {
                    let mut settings = Settings::defaults(platform);
                    settings.snapping.drag_modifier = drag;
                    settings.snapping.swap_modifier = swap;
                    if stack {
                        settings.snapping.when_taken = WhenTaken::Stack;
                    }
                    settings.layouts.displays =
                        displays.into_iter().map(|(d, l)| (d, l.to_owned())).collect();
                    settings.layouts.default_layout_id = default.to_owned();
                    for (preset, (enabled, ghost)) in layout::presets().iter().zip(options) {
                        let mut options = LayoutOptions::defaults_for(preset);
                        options.enabled = enabled;
                        if ghost {
                            options
                                .zone_shortcuts
                                .insert("ghost".into(), KeyChord::snappy("KeyG", []));
                        }
                        settings.layouts.options.insert(preset.id.clone(), options);
                    }
                    settings.workspaces.enabled = workspaces;
                    settings.workspaces.items[0].switch_shortcut = switch;
                    settings.history.undo = undo;
                    (settings, platform)
                },
            )
    }

    proptest! {
        #[test]
        fn validation_is_idempotent_and_survives_a_save((settings, platform) in edited_settings()) {
            if let Ok(valid) = settings.validate(platform) {
                prop_assert_eq!(valid.clone().validate(platform), Ok(valid.clone()));
                let saved = valid.to_json().unwrap();
                prop_assert_eq!(Settings::from_json(&saved, platform), Ok(valid));
            }
        }
    }

    fn custom(id: &str) -> Layout {
        Layout {
            id: id.into(),
            name: "Custom".into(),
            zones: vec![Zone::new("z", UnitRect::new(0.0, 0.0, 1.0, 1.0))],
        }
    }

    #[test]
    fn settings_have_no_spacing_controls() {
        let json = String::from_utf8(Settings::defaults(MAC).to_json().unwrap()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut keys = Vec::new();
        collect_keys(&value, &mut keys);
        for key in keys {
            let key = key.to_lowercase();
            assert!(
                !["gap", "padding", "spacing", "margin"]
                    .iter()
                    .any(|control| key.contains(control)),
                "unexpected spacing setting {key}"
            );
        }
    }

    fn collect_keys(value: &serde_json::Value, keys: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    keys.push(key.clone());
                    collect_keys(child, keys);
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|item| collect_keys(item, keys)),
            _ => {}
        }
    }

    #[test]
    fn defaults_are_valid_and_round_trip() {
        let settings = Settings::defaults(MAC).validate(MAC).unwrap();
        let reloaded = Settings::from_json(&settings.to_json().unwrap(), MAC).unwrap();
        assert_eq!(reloaded, settings);
        assert_eq!(Settings::from_json(b"{}", MAC).unwrap(), settings);
    }

    #[test]
    fn bad_files_are_reported() {
        assert_eq!(Settings::from_json(b"not json", MAC), Err(SettingsError::Invalid));
        assert_eq!(
            Settings::from_json(br#"{"version": 9}"#, MAC),
            Err(SettingsError::UnsupportedVersion(9))
        );
    }

    #[test]
    fn validation_normalizes() {
        let mut settings = Settings::defaults(MAC);
        settings.snapping.swap_modifier = SwapModifier::Shift;
        settings.layouts.displays.insert("d".into(), "missing".into());
        settings.layouts.default_layout_id = "missing".into();
        let valid = settings.validate(MAC).unwrap();
        assert_eq!(valid.snapping.swap_modifier, SwapModifier::Meta);
        assert!(valid.layouts.displays.is_empty());
        assert_eq!(valid.layouts.default_layout_id, "focus");
        assert_eq!(valid.layouts.options.len(), layout::presets().len());
    }

    #[test]
    fn preset_ids_and_duplicates_are_rejected() {
        let mut settings = Settings::defaults(MAC);
        settings.layouts.custom = vec![custom("halves")];
        assert_eq!(settings.clone().validate(MAC), Err(SettingsError::Invalid));
        settings.layouts.custom = vec![custom("a"), custom("a")];
        assert_eq!(settings.validate(MAC), Err(SettingsError::Invalid));
    }

    #[test]
    fn a_displays_layout_resolves_in_order() {
        let mut settings = Settings::defaults(MAC);
        assert_eq!(settings.layout_for_display("d").id, "focus");
        settings.layouts.displays.insert("d".into(), "halves".into());
        assert_eq!(settings.layout_for_display("d").id, "halves");
        settings.workspaces.enabled = true;
        settings.workspaces.items[0]
            .displays
            .insert("d".into(), "rows".into());
        assert_eq!(settings.layout_for_display("d").id, "rows");
        settings.layouts.options.insert(
            "rows".into(),
            LayoutOptions {
                enabled: false,
                ..LayoutOptions::empty()
            },
        );
        assert_eq!(
            settings.layout_for_display("d").id,
            "halves",
            "falls to the first enabled layout"
        );
    }

    #[test]
    fn presets_get_default_zone_shortcuts() {
        let settings = Settings::defaults(MAC).validate(MAC).unwrap();
        let focus = &settings.layouts.options["focus"];
        assert_eq!(focus.zone_shortcuts.len(), 3);
        assert_eq!(focus.zone_shortcuts["focus-0"].code.as_str(), "Digit1");
    }

    #[test]
    fn stepping_through_workspaces_wraps_around() {
        let workspaces = Workspaces::defaults();
        assert_eq!(workspaces.adjacent("workspace-1", -1).unwrap().id, "workspace-3");
        assert_eq!(workspaces.adjacent("workspace-3", 1).unwrap().id, "workspace-1");
    }

    const PLATFORMS: [Platform; 3] = [Platform::MacOs, Platform::Windows, Platform::Linux];

    fn snappy(code: &str) -> Option<crate::keys::Combo> {
        Some(KeyChord::snappy(code, []).combo())
    }

    #[test]
    fn default_shortcuts_use_the_snappy_key_on_every_platform() {
        let combo = |chord: Option<&KeyChord>| chord.map(KeyChord::combo);
        for platform in PLATFORMS {
            let settings = Settings::defaults(platform).validate(platform).unwrap();
            let workspaces = &settings.workspaces;
            let steps = WorkspaceStep::ALL.map(|step| combo(workspaces.step(step)));
            assert_eq!(
                steps,
                [
                    snappy("Semicolon"),
                    snappy("Quote"),
                    snappy("Period"),
                    snappy("Slash"),
                ],
                "{platform:?}"
            );
            assert!(workspaces.steps().all(|(_, chord)| chord.mods.has_snappy_key()));
            assert!(workspaces.items.iter().all(|item| item.chords().count() == 0));
            assert_eq!(combo(settings.navigation.left.as_ref()), snappy("ArrowLeft"));
            assert_eq!(combo(settings.navigation.nearest_empty.as_ref()), snappy("Space"));
            assert!(
                settings
                    .navigation
                    .nearest_empty
                    .as_ref()
                    .unwrap()
                    .mods
                    .has_snappy_key()
            );
            let focus = &settings.layouts.options["focus"];
            assert_eq!(Some(focus.zone_shortcuts["focus-0"].combo()), snappy("Digit1"));
            assert!(focus.zone_shortcuts["focus-0"].mods.has_snappy_key());
            assert_eq!(settings.history, MoveHistory::default());
        }
        assert_eq!(
            Settings::defaults(Platform::Windows).workspaces,
            Settings::defaults(MAC).workspaces
        );
    }

    #[test]
    fn a_file_without_move_steps_gets_their_defaults() {
        let mut written = serde_json::to_value(Settings::defaults(MAC)).unwrap();
        let workspaces = written["workspaces"].as_object_mut().unwrap();
        workspaces.remove("movePrevious");
        workspaces.remove("moveNext");
        workspaces.insert("next".into(), serde_json::Value::Null);
        let bytes = serde_json::to_vec(&written).unwrap();
        let loaded = Settings::from_json(&bytes, MAC).unwrap();
        assert_eq!(
            loaded.workspaces.move_previous.map(|c| c.combo()),
            snappy("Period")
        );
        assert_eq!(loaded.workspaces.move_next.map(|c| c.combo()), snappy("Slash"));
        assert_eq!(loaded.workspaces.next, None, "written steps are kept");
        assert_eq!(loaded.history, MoveHistory::default());
    }

    #[test]
    fn a_file_without_move_steps_skips_defaults_another_binding_uses() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.move_next = None;
        settings.workspaces.items[0].switch_shortcut = Some(KeyChord::snappy("Slash", []));
        let mut written = serde_json::to_value(settings).unwrap();
        let workspaces = written["workspaces"].as_object_mut().unwrap();
        workspaces.remove("movePrevious");
        workspaces.remove("moveNext");
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC).unwrap();
        assert_eq!(
            loaded.workspaces.move_previous.map(|c| c.combo()),
            snappy("Period")
        );
        assert_eq!(loaded.workspaces.move_next, None);
    }

    #[test]
    fn a_saved_cleared_move_step_stays_cleared() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.set_step(WorkspaceStep::MoveNext, None);
        let loaded = Settings::from_json(&settings.to_json().unwrap(), MAC).unwrap();
        assert_eq!(loaded.workspaces.move_next, None);
    }

    /// The defaults written without the nearest-empty-zone shortcut, as older files are.
    fn written_without_nearest_empty(settings: Settings) -> Vec<u8> {
        let mut written = serde_json::to_value(settings).unwrap();
        let navigation = written["navigation"].as_object_mut().unwrap();
        navigation.remove("nearestEmpty");
        serde_json::to_vec(&written).unwrap()
    }

    #[test]
    fn a_file_without_the_nearest_empty_shortcut_gets_its_default() {
        let loaded =
            Settings::from_json(&written_without_nearest_empty(Settings::defaults(MAC)), MAC).unwrap();
        assert_eq!(
            loaded.navigation.nearest_empty.map(|c| c.combo()),
            snappy("Space")
        );
        assert_eq!(loaded.navigation.left, Navigation::defaults().left);
    }

    #[test]
    fn a_file_without_the_nearest_empty_shortcut_skips_the_default_another_binding_uses() {
        let mut settings = Settings::defaults(MAC);
        settings.navigation.nearest_empty = None;
        settings.history.undo = Some(KeyChord::snappy("Space", []));
        let loaded = Settings::from_json(&written_without_nearest_empty(settings), MAC).unwrap();
        assert_eq!(loaded.navigation.nearest_empty, None);
        assert_eq!(loaded.history.undo.map(|c| c.combo()), snappy("Space"));
    }

    #[test]
    fn a_saved_cleared_nearest_empty_shortcut_stays_cleared() {
        let mut settings = Settings::defaults(MAC);
        settings.navigation.nearest_empty = None;
        let json = String::from_utf8(settings.to_json().unwrap()).unwrap();
        assert!(json.contains(r#""nearestEmpty": null"#), "{json}");
        let loaded = Settings::from_json(json.as_bytes(), MAC).unwrap();
        assert_eq!(loaded.navigation.nearest_empty, None);
    }

    #[test]
    fn new_workspaces_have_no_shortcuts() {
        let workspace = Workspace::numbered("workspace-4", 4);
        assert_eq!(workspace.name, "Workspace 4");
        assert_eq!(workspace.chords().count(), 0);
    }

    #[test]
    fn workspace_shortcuts_can_be_changed_or_cleared() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.enabled = true;
        settings.workspaces.previous = None;
        settings.workspaces.next = Some(KeyChord::new("KeyN", [Modifier::Ctrl, Modifier::Alt]));
        let first = &mut settings.workspaces.items[0];
        first.switch_shortcut = Some(KeyChord::new("KeyW", [Modifier::Ctrl, Modifier::Alt]));
        first.send_shortcut = None;
        first.send_and_switch_shortcut = None;
        let valid = settings.clone().validate(MAC).unwrap();
        assert_eq!(valid.workspaces, settings.workspaces);

        let registered: Vec<_> = bindings::desired_hotkeys(&valid, &[], false)
            .into_iter()
            .map(|hotkey| hotkey.chord.combo())
            .collect();
        assert!(registered.contains(&KeyChord::new("KeyW", [Modifier::Ctrl, Modifier::Alt]).combo()));
        assert!(registered.contains(&KeyChord::new("KeyN", [Modifier::Ctrl, Modifier::Alt]).combo()));
        assert!(!registered.contains(&snappy("Semicolon").unwrap()));
        assert!(registered.contains(&snappy("Period").unwrap()));
    }

    #[test]
    fn a_file_without_when_taken_swaps() {
        let mut written = serde_json::to_value(Settings::defaults(MAC)).unwrap();
        let snapping = written["snapping"].as_object_mut().unwrap();
        snapping.remove("whenTaken");
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC).unwrap();
        assert_eq!(loaded.snapping.when_taken, WhenTaken::Swap);
        assert_eq!(Settings::defaults(MAC).snapping.when_taken, WhenTaken::Swap);

        let mut settings = Settings::defaults(MAC);
        settings.snapping.when_taken = WhenTaken::Stack;
        let json = String::from_utf8(settings.to_json().unwrap()).unwrap();
        assert!(json.contains(r#""whenTaken": "stack""#), "{json}");
        let loaded = Settings::from_json(json.as_bytes(), MAC).unwrap();
        assert_eq!(loaded.snapping.when_taken, WhenTaken::Stack);
    }

    #[test]
    fn a_file_with_concrete_snappy_key_modifiers_loads_them_as_the_snappy_key() {
        let mut written = serde_json::to_value(Settings::defaults(MAC)).unwrap();
        let concrete = serde_json::json!(["ctrl", "alt", "meta"]);
        written["navigation"]["left"]["mods"] = concrete.clone();
        written["workspaces"]["previous"]["mods"] = concrete;
        written["workspaces"]["moveNext"]["mods"] = serde_json::json!(["shift", "ctrl", "alt", "meta"]);
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC).unwrap();
        assert_eq!(loaded.navigation, Navigation::defaults());
        assert_eq!(loaded.workspaces.previous, Workspaces::defaults().previous);
        assert_eq!(
            loaded.workspaces.move_next,
            Some(KeyChord::snappy("Slash", [Modifier::Shift]).with_label("/"))
        );

        let saved: serde_json::Value = serde_json::from_slice(&loaded.to_json().unwrap()).unwrap();
        assert_eq!(saved["navigation"]["left"]["mods"], serde_json::json!(["snappy"]));
        assert_eq!(
            saved["workspaces"]["moveNext"]["mods"],
            serde_json::json!(["snappy", "shift"])
        );
    }

    #[test]
    fn a_file_with_the_old_shift_bracket_move_steps_keeps_them() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.move_previous = Some(KeyChord::snappy("BracketLeft", [Modifier::Shift]));
        settings.workspaces.move_next = Some(KeyChord::snappy("BracketRight", [Modifier::Shift]));
        let loaded = Settings::from_json(&settings.to_json().unwrap(), MAC).unwrap();
        assert_eq!(loaded.workspaces, settings.workspaces);
    }

    #[test]
    fn the_drag_modifier_starts_as_shift_and_offers_three_choices() {
        assert_eq!(
            Settings::defaults(MAC).snapping.drag_modifier,
            DragModifier::Shift
        );
        let parse = |name: &str| serde_json::from_str::<DragModifier>(&format!("\"{name}\"")).ok();
        assert_eq!(
            ["shift", "alt", "ctrl", "meta"].map(parse),
            [
                Some(DragModifier::Shift),
                Some(DragModifier::Alt),
                Some(DragModifier::Ctrl),
                None
            ]
        );
    }
}
