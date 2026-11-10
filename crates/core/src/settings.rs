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
/// Workspaces numbered up to this get the Snappy key with their number to switch to them.
const NUMBERED_SWITCH_SHORTCUTS: usize = 9;

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
    /// Send the focused window to the previous workspace and stay.
    #[serde(default)]
    pub send_previous: Option<KeyChord>,
    /// Send the focused window to the next workspace and stay.
    #[serde(default)]
    pub send_next: Option<KeyChord>,
    /// Pick up or put down the focused window; the next switch brings picked-up windows.
    #[serde(default)]
    pub take_along: Option<KeyChord>,
    pub items: Vec<Workspace>,
}

/// The shortcuts for every workspace at once: stepping to the previous or next one,
/// sending the focused window there, and taking windows along to the next switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceStep {
    Previous,
    Next,
    SendPrevious,
    SendNext,
    TakeAlong,
}

impl WorkspaceStep {
    pub const ALL: [WorkspaceStep; 5] = [
        WorkspaceStep::Previous,
        WorkspaceStep::Next,
        WorkspaceStep::SendPrevious,
        WorkspaceStep::SendNext,
        WorkspaceStep::TakeAlong,
    ];

    /// The name the settings app and conflict messages use.
    pub fn title(self) -> &'static str {
        match self {
            WorkspaceStep::Previous => "Previous workspace",
            WorkspaceStep::Next => "Next workspace",
            WorkspaceStep::SendPrevious => "Send window to previous workspace",
            WorkspaceStep::SendNext => "Send window to next workspace",
            WorkspaceStep::TakeAlong => "Take along",
        }
    }

    /// The field that holds this step's chord in the settings file.
    fn field(self) -> &'static str {
        match self {
            WorkspaceStep::Previous => "previous",
            WorkspaceStep::Next => "next",
            WorkspaceStep::SendPrevious => "sendPrevious",
            WorkspaceStep::SendNext => "sendNext",
            WorkspaceStep::TakeAlong => "takeAlong",
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
    /// "Workspace <number>", switched to with the Snappy key and its number when that is a
    /// single digit, and with no other shortcuts.
    pub fn numbered(id: impl Into<String>, number: usize) -> Self {
        Self {
            id: id.into(),
            name: format!("Workspace {number}"),
            displays: DisplayAssignments::new(),
            switch_shortcut: numbered_switch_shortcut(number),
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

/// The Snappy key with the workspace's number, for workspaces 1 to 9.
fn numbered_switch_shortcut(number: usize) -> Option<KeyChord> {
    (1..=NUMBERED_SWITCH_SHORTCUTS)
        .contains(&number)
        .then(|| KeyChord::snappy(&format!("Digit{number}"), []).with_label(number.to_string()))
}

impl Workspaces {
    /// Three workspaces, switched to with the Snappy key and their number, and stepped
    /// through with the Snappy key and the semicolon and quote keys, or with the period and
    /// slash keys to send the focused window away. Both pairs sit together under the right
    /// hand, on every keyboard layout's home and bottom rows. Return takes windows along.
    pub fn defaults() -> Self {
        let chord = |code: &str, label: &str| Some(KeyChord::snappy(code, []).with_label(label));
        Self {
            enabled: false,
            active_id: "workspace-1".to_owned(),
            previous: chord("Semicolon", ";"),
            next: chord("Quote", "'"),
            send_previous: chord("Period", "."),
            send_next: chord("Slash", "/"),
            take_along: Some(KeyChord::snappy("Enter", [])),
            items: (1..=3)
                .map(|n| Workspace::numbered(format!("workspace-{n}"), n))
                .collect(),
        }
    }

    pub fn step(&self, step: WorkspaceStep) -> Option<&KeyChord> {
        match step {
            WorkspaceStep::Previous => self.previous.as_ref(),
            WorkspaceStep::Next => self.next.as_ref(),
            WorkspaceStep::SendPrevious => self.send_previous.as_ref(),
            WorkspaceStep::SendNext => self.send_next.as_ref(),
            WorkspaceStep::TakeAlong => self.take_along.as_ref(),
        }
    }

    pub fn set_step(&mut self, step: WorkspaceStep, chord: Option<KeyChord>) {
        let slot = match step {
            WorkspaceStep::Previous => &mut self.previous,
            WorkspaceStep::Next => &mut self.next,
            WorkspaceStep::SendPrevious => &mut self.send_previous,
            WorkspaceStep::SendNext => &mut self.send_next,
            WorkspaceStep::TakeAlong => &mut self.take_along,
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

    /// Stored options, or empty ones for a layout that has none.
    pub fn options(&self, layout: &Layout) -> LayoutOptions {
        self.layouts
            .options
            .get(&layout.id)
            .cloned()
            .unwrap_or_else(LayoutOptions::empty)
    }

    /// A workspace to add: `Workspace::numbered`, without the switch shortcut when another
    /// binding already uses it. A workspace keeps its shortcuts when others are reordered
    /// or deleted, so its number's chord may belong to another workspace by then.
    pub fn new_workspace(&self, id: impl Into<String>, number: usize) -> Workspace {
        let mut workspace = Workspace::numbered(id, number);
        let used = bindings::bound_combos(self);
        workspace.switch_shortcut = workspace
            .switch_shortcut
            .filter(|chord| !used.contains(&chord.combo()));
        workspace
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

    /// Parse a settings file. Fields missing from the file take their defaults, except
    /// that a default shortcut is left unassigned when another binding the file has already
    /// uses its chord. Shortcuts the file has, cleared ones included, are kept.
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
        let missing_workspace_chords = match &file.workspaces {
            Some(written) => MissingChord::in_written(written),
            None => MissingChord::all(&defaults.workspaces),
        };
        let mut workspaces: Workspaces = match file.workspaces {
            Some(written) => serde_json::from_value(written).map_err(|_| SettingsError::Invalid)?,
            None => defaults.workspaces.clone(),
        };
        for missing in &missing_workspace_chords {
            missing.clear(&mut workspaces);
        }
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
        let mut used = bindings::bound_combos(&settings);
        let mut unused = |chord: Option<KeyChord>| chord.filter(|chord| used.insert(chord.combo()));
        if nearest_empty_missing {
            settings.navigation.nearest_empty = unused(defaults.navigation.nearest_empty);
        }
        for missing in missing_workspace_chords {
            missing.fill(&mut settings.workspaces, &mut unused);
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

/// A workspace shortcut a settings file doesn't mention, as files written before it
/// existed don't.
#[derive(Debug, Clone, Copy)]
enum MissingChord {
    Step(WorkspaceStep),
    /// The switch shortcut of the workspace at this position.
    Switch(usize),
}

impl MissingChord {
    fn in_written(workspaces: &serde_json::Value) -> Vec<MissingChord> {
        let steps = WorkspaceStep::ALL
            .into_iter()
            .filter(|step| workspaces.get(step.field()).is_none())
            .map(MissingChord::Step);
        let items = workspaces.get("items").and_then(serde_json::Value::as_array);
        let switches = items
            .into_iter()
            .flatten()
            .enumerate()
            .filter(|(_, item)| item.get("switchShortcut").is_none())
            .map(|(index, _)| MissingChord::Switch(index));
        steps.chain(switches).collect()
    }

    /// Every workspace shortcut, for a file without workspaces.
    fn all(workspaces: &Workspaces) -> Vec<MissingChord> {
        let steps = WorkspaceStep::ALL.into_iter().map(MissingChord::Step);
        let switches = (0..workspaces.items.len()).map(MissingChord::Switch);
        steps.chain(switches).collect()
    }

    fn clear(self, workspaces: &mut Workspaces) {
        match self {
            MissingChord::Step(step) => workspaces.set_step(step, None),
            MissingChord::Switch(index) => {
                if let Some(item) = workspaces.items.get_mut(index) {
                    item.switch_shortcut = None;
                }
            }
        }
    }

    /// Assign the default chord, passed through `unused`, which drops chords in use.
    fn fill(
        self,
        workspaces: &mut Workspaces,
        unused: &mut impl FnMut(Option<KeyChord>) -> Option<KeyChord>,
    ) {
        match self {
            MissingChord::Step(step) => {
                let chord = Workspaces::defaults().step(step).cloned();
                workspaces.set_step(step, unused(chord));
            }
            MissingChord::Switch(index) => {
                if let Some(item) = workspaces.items.get_mut(index) {
                    item.switch_shortcut = unused(numbered_switch_shortcut(index + 1));
                }
            }
        }
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
                        let mut options = LayoutOptions::empty();
                        options.enabled = enabled;
                        options
                            .zone_shortcuts
                            .insert(preset.zones[0].id.clone(), KeyChord::snappy("Digit1", []));
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
    fn zones_have_no_default_shortcuts() {
        let settings = Settings::defaults(MAC).validate(MAC).unwrap();
        assert_eq!(settings.layouts.options.len(), layout::presets().len());
        assert!(
            settings
                .layouts
                .options
                .values()
                .all(|options| options.zone_shortcuts.is_empty())
        );
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
    fn default_shortcuts_are_the_snappy_key_and_one_key_on_every_platform() {
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
                    snappy("Enter"),
                ],
                "{platform:?}"
            );
            let switches: Vec<_> = workspaces
                .items
                .iter()
                .map(|item| combo(item.switch_shortcut.as_ref()))
                .collect();
            assert_eq!(switches, [snappy("Digit1"), snappy("Digit2"), snappy("Digit3")]);
            assert!(
                workspaces
                    .items
                    .iter()
                    .all(|item| item.send_shortcut.is_none() && item.send_and_switch_shortcut.is_none())
            );
            assert_eq!(combo(settings.navigation.left.as_ref()), snappy("ArrowLeft"));
            assert_eq!(combo(settings.navigation.nearest_empty.as_ref()), snappy("Space"));
            assert_eq!(settings.history, MoveHistory::default());

            let every_chord = bindings::bound_combos(&settings);
            assert_eq!(every_chord.len(), 5 + 3 + 5, "{platform:?}");
            let snappy_key = crate::keys::snappy_key_modifiers();
            assert!(
                every_chord.iter().all(|combo| combo.mods == snappy_key),
                "nothing but the Snappy key and one key: {every_chord:?}"
            );
        }
        assert_eq!(
            Settings::defaults(Platform::Windows).workspaces,
            Settings::defaults(MAC).workspaces
        );
    }

    /// The defaults written without some workspace fields, as older files are.
    fn written_without(settings: Settings, fields: &[&str]) -> Vec<u8> {
        let mut written = serde_json::to_value(settings).unwrap();
        let workspaces = written["workspaces"].as_object_mut().unwrap();
        for field in fields {
            workspaces.remove(*field);
        }
        serde_json::to_vec(&written).unwrap()
    }

    #[test]
    fn a_file_without_send_steps_or_take_along_gets_their_defaults() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.next = None;
        let bytes = written_without(settings, &["sendPrevious", "sendNext", "takeAlong"]);
        let loaded = Settings::from_json(&bytes, MAC).unwrap();
        assert_eq!(
            loaded.workspaces.send_previous.map(|c| c.combo()),
            snappy("Period")
        );
        assert_eq!(loaded.workspaces.send_next.map(|c| c.combo()), snappy("Slash"));
        assert_eq!(loaded.workspaces.take_along.map(|c| c.combo()), snappy("Enter"));
        assert_eq!(loaded.workspaces.next, None, "written steps are kept");
        assert_eq!(loaded.history, MoveHistory::default());
    }

    #[test]
    fn a_file_without_send_steps_or_take_along_skips_defaults_another_binding_uses() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[0].switch_shortcut = Some(KeyChord::snappy("Slash", []));
        settings.history.undo = Some(KeyChord::snappy("Enter", []));
        settings.workspaces.send_next = None;
        settings.workspaces.take_along = None;
        let bytes = written_without(settings, &["sendPrevious", "sendNext", "takeAlong"]);
        let loaded = Settings::from_json(&bytes, MAC).unwrap();
        assert_eq!(
            loaded.workspaces.send_previous.map(|c| c.combo()),
            snappy("Period")
        );
        assert_eq!(loaded.workspaces.send_next, None);
        assert_eq!(loaded.workspaces.take_along, None);
        assert_eq!(loaded.history.undo.map(|c| c.combo()), snappy("Enter"));
    }

    #[test]
    fn a_file_with_the_former_move_steps_ignores_them_and_gets_the_send_steps() {
        let mut written = serde_json::to_value(Settings::defaults(MAC)).unwrap();
        let workspaces = written["workspaces"].as_object_mut().unwrap();
        for field in ["sendPrevious", "sendNext", "takeAlong"] {
            workspaces.remove(field);
        }
        let former = |code: &str| serde_json::json!({ "code": code, "mods": ["snappy", "shift"] });
        workspaces.insert("movePrevious".into(), former("BracketLeft"));
        workspaces.insert("moveNext".into(), former("BracketRight"));
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC).unwrap();
        assert_eq!(
            loaded.workspaces.send_previous.as_ref().map(KeyChord::combo),
            snappy("Period")
        );
        assert_eq!(
            loaded.workspaces.send_next.as_ref().map(KeyChord::combo),
            snappy("Slash")
        );
        let saved = String::from_utf8(loaded.to_json().unwrap()).unwrap();
        assert!(!saved.contains("moveNext"), "{saved}");
    }

    #[test]
    fn a_saved_cleared_workspace_shortcut_stays_cleared() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.set_step(WorkspaceStep::SendNext, None);
        settings.workspaces.set_step(WorkspaceStep::TakeAlong, None);
        settings.workspaces.items[1].switch_shortcut = None;
        let loaded = Settings::from_json(&settings.to_json().unwrap(), MAC).unwrap();
        assert_eq!(loaded.workspaces.send_next, None);
        assert_eq!(loaded.workspaces.take_along, None);
        assert_eq!(loaded.workspaces.items[1].switch_shortcut, None);
    }

    /// Layout options as files written while zones had digit shortcuts store them.
    fn zones_on_the_digits(settings: &mut Settings) {
        for preset in layout::presets() {
            let mut options = LayoutOptions::empty();
            for (index, zone) in preset.zones.iter().enumerate().take(9) {
                let digit = index + 1;
                options
                    .zone_shortcuts
                    .insert(zone.id.clone(), KeyChord::snappy(&format!("Digit{digit}"), []));
            }
            settings.layouts.options.insert(preset.id.clone(), options);
        }
    }

    #[test]
    fn a_file_without_workspaces_keeps_its_zone_digits_and_skips_those_switch_defaults() {
        let mut settings = Settings::defaults(MAC);
        zones_on_the_digits(&mut settings);
        let mut written = serde_json::to_value(&settings).unwrap();
        written.as_object_mut().unwrap().remove("workspaces");
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC).unwrap();
        assert_eq!(
            loaded.layouts.options["halves"].zone_shortcuts["halves-0"].combo(),
            KeyChord::snappy("Digit1", []).combo()
        );
        assert!(
            loaded
                .workspaces
                .items
                .iter()
                .all(|item| item.switch_shortcut.is_none())
        );
        assert_eq!(loaded.workspaces.take_along.map(|c| c.combo()), snappy("Enter"));
        assert_eq!(loaded.workspaces.items.len(), 3);
    }

    #[test]
    fn a_file_with_zone_digits_and_cleared_switch_shortcuts_loads_unchanged() {
        let mut settings = Settings::defaults(MAC);
        zones_on_the_digits(&mut settings);
        for item in &mut settings.workspaces.items {
            item.switch_shortcut = None;
        }
        let loaded = Settings::from_json(&settings.to_json().unwrap(), MAC).unwrap();
        assert_eq!(loaded.workspaces, settings.workspaces);
        assert_eq!(loaded.layouts.options["halves"].zone_shortcuts.len(), 2);
    }

    #[test]
    fn workspaces_without_a_written_switch_shortcut_get_their_positions_digit_where_free() {
        let mut settings = Settings::defaults(MAC);
        settings.history.redo = Some(KeyChord::snappy("Digit2", []));
        settings.workspaces.items[1].switch_shortcut = None;
        let mut written = serde_json::to_value(&settings).unwrap();
        for item in written["workspaces"]["items"].as_array_mut().unwrap() {
            item.as_object_mut().unwrap().remove("switchShortcut");
        }
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC).unwrap();
        let switches: Vec<_> = loaded
            .workspaces
            .items
            .iter()
            .map(|item| item.switch_shortcut.as_ref().map(KeyChord::combo))
            .collect();
        assert_eq!(switches, [snappy("Digit1"), None, snappy("Digit3")]);
    }

    #[test]
    fn workspaces_one_to_nine_switch_with_their_digit_and_later_ones_have_no_shortcuts() {
        let fourth = Workspace::numbered("workspace-4", 4);
        assert_eq!(fourth.name, "Workspace 4");
        assert_eq!(
            fourth.switch_shortcut.as_ref().map(KeyChord::combo),
            snappy("Digit4")
        );
        assert_eq!(
            fourth.switch_shortcut.as_ref().unwrap().compact_label(MAC),
            "Snappy + 4"
        );
        assert_eq!(fourth.send_shortcut, None);
        assert_eq!(fourth.send_and_switch_shortcut, None);
        assert!(Workspace::numbered("workspace-9", 9).switch_shortcut.is_some());
        assert_eq!(Workspace::numbered("workspace-10", 10).chords().count(), 0);
    }

    #[test]
    fn a_new_workspace_skips_its_digit_when_another_binding_uses_it() {
        let mut settings = Settings::defaults(MAC);
        let added = settings.new_workspace("workspace-4", 4);
        assert_eq!(
            added.switch_shortcut.as_ref().map(KeyChord::combo),
            snappy("Digit4")
        );

        settings.workspaces.items[2].switch_shortcut = Some(KeyChord::snappy("Digit4", []));
        let added = settings.new_workspace("workspace-4", 4);
        assert_eq!(added.switch_shortcut, None);
        settings.workspaces.items.push(added);
        assert_eq!(settings.validate(MAC).map(|_| ()), Ok(()));
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
        written["workspaces"]["sendNext"]["mods"] = serde_json::json!(["shift", "ctrl", "alt", "meta"]);
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC).unwrap();
        assert_eq!(loaded.navigation, Navigation::defaults());
        assert_eq!(loaded.workspaces.previous, Workspaces::defaults().previous);
        assert_eq!(
            loaded.workspaces.send_next,
            Some(KeyChord::snappy("Slash", [Modifier::Shift]).with_label("/"))
        );

        let saved: serde_json::Value = serde_json::from_slice(&loaded.to_json().unwrap()).unwrap();
        assert_eq!(saved["navigation"]["left"]["mods"], serde_json::json!(["snappy"]));
        assert_eq!(
            saved["workspaces"]["sendNext"]["mods"],
            serde_json::json!(["snappy", "shift"])
        );
    }

    #[test]
    fn a_file_with_shift_bracket_send_steps_keeps_them() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.send_previous = Some(KeyChord::snappy("BracketLeft", [Modifier::Shift]));
        settings.workspaces.send_next = Some(KeyChord::snappy("BracketRight", [Modifier::Shift]));
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
