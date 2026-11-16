//! The settings file: its model, defaults, and validation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::bindings;
use crate::keys::{
    ChordMods, Combo, KeyChord, Modifier, Mods, Platform, SnappyKey, SnappyKeyError, chords_the_system_keeps,
};
use crate::layout::{self, DEFAULT_LAYOUT_ID, FALLBACK_LAYOUT_ID, Layout, valid_id, valid_name};
use crate::navigate::Direction;
use crate::shortcuts::{self, ShortcutId};
use crate::swap::{SwapModifier, WhenTaken};

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_FILE_BYTES: usize = 2_000_000;
pub const MAX_CUSTOM_LAYOUTS: usize = 100;
pub const MAX_LAYOUT_OPTIONS: usize = 200;
pub const MAX_WORKSPACES: usize = 32;
pub const MAX_WORKSPACE_ID_LENGTH: usize = 128;
pub const MAX_DISPLAY_ASSIGNMENTS: usize = 32;
/// Workspaces numbered up to this get the Snappy key with their number to switch to them.
pub const NUMBERED_SWITCH_SHORTCUTS: usize = 9;

/// Display ID to layout ID.
pub type DisplayAssignments = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub snapping: Snapping,
    /// The modifiers every shortcut stored with the Snappy key holds. Files from before it
    /// could be chosen lack it, get the default, and have their chords read as they were
    /// then (`read_as_before_the_snappy_key`).
    #[serde(default)]
    pub snappy_key: SnappyKey,
    /// Caps Lock, held, stands for the Snappy key, and tapped alone still turns Caps Lock on
    /// or off. Off unless the user turns it on.
    #[serde(default)]
    pub caps_lock_as_snappy_key: bool,
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

/// Shortcuts for getting around: moving the focused window between zones, moving focus
/// between windows, and selecting windows.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Navigation {
    pub left: Option<KeyChord>,
    pub right: Option<KeyChord>,
    pub up: Option<KeyChord>,
    pub down: Option<KeyChord>,
    /// Move the focused window into the nearest empty zone, or the selection into empty
    /// zones.
    #[serde(default)]
    pub nearest_empty: Option<KeyChord>,
    #[serde(default)]
    pub focus_left: Option<KeyChord>,
    #[serde(default)]
    pub focus_right: Option<KeyChord>,
    #[serde(default)]
    pub focus_up: Option<KeyChord>,
    #[serde(default)]
    pub focus_down: Option<KeyChord>,
    /// Add the focused window to the selection, or take it out.
    #[serde(default)]
    pub select_window: Option<KeyChord>,
    #[serde(default)]
    pub clear_selection: Option<KeyChord>,
}

/// The name the settings app and conflict messages give the nearest-empty-zone shortcut.
pub const NEAREST_EMPTY_ZONE_TITLE: &str = "Move to the nearest empty zone";

/// Every shortcut in the navigation section, so each is listed once: for validation,
/// conflicts, registration, the defaults a file without it gets, and the command that
/// sets it. Written as the settings file field that holds it, such as `"left"` or
/// `"focusUp"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationShortcut {
    /// Move the focused window to the neighboring zone.
    Move(Direction),
    /// Move the focused window into the nearest empty zone, or the selection into empty
    /// zones.
    NearestEmptyZone,
    /// Focus the next window that way; moves no window.
    Focus(Direction),
    /// Add the focused window to the selection, or take it out.
    SelectWindow,
    ClearSelection,
}

impl NavigationShortcut {
    /// In the order conflicts are looked for.
    pub const ALL: [NavigationShortcut; 11] = [
        NavigationShortcut::Move(Direction::Left),
        NavigationShortcut::Move(Direction::Right),
        NavigationShortcut::Move(Direction::Up),
        NavigationShortcut::Move(Direction::Down),
        NavigationShortcut::NearestEmptyZone,
        NavigationShortcut::Focus(Direction::Up),
        NavigationShortcut::Focus(Direction::Left),
        NavigationShortcut::Focus(Direction::Down),
        NavigationShortcut::Focus(Direction::Right),
        NavigationShortcut::SelectWindow,
        NavigationShortcut::ClearSelection,
    ];

    /// The name the settings app and conflict messages use.
    pub fn title(self) -> String {
        match self {
            NavigationShortcut::Move(direction) => format!("Move window {}", direction.name()),
            NavigationShortcut::NearestEmptyZone => NEAREST_EMPTY_ZONE_TITLE.to_owned(),
            NavigationShortcut::Focus(direction) => format!("Focus {}", direction.name()),
            NavigationShortcut::SelectWindow => "Select window".to_owned(),
            NavigationShortcut::ClearSelection => "Clear selection".to_owned(),
        }
    }

    /// The field that holds this shortcut's chord in the settings file.
    pub(crate) fn field(self) -> &'static str {
        match self {
            NavigationShortcut::Move(direction) => direction.name(),
            NavigationShortcut::NearestEmptyZone => "nearestEmpty",
            NavigationShortcut::Focus(Direction::Up) => "focusUp",
            NavigationShortcut::Focus(Direction::Left) => "focusLeft",
            NavigationShortcut::Focus(Direction::Down) => "focusDown",
            NavigationShortcut::Focus(Direction::Right) => "focusRight",
            NavigationShortcut::SelectWindow => "selectWindow",
            NavigationShortcut::ClearSelection => "clearSelection",
        }
    }
}

impl Serialize for NavigationShortcut {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.field())
    }
}

impl<'de> Deserialize<'de> for NavigationShortcut {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let field = String::deserialize(deserializer)?;
        NavigationShortcut::ALL
            .into_iter()
            .find(|shortcut| shortcut.field() == field)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown navigation shortcut {field:?}")))
    }
}

impl From<Direction> for NavigationShortcut {
    fn from(direction: Direction) -> Self {
        NavigationShortcut::Move(direction)
    }
}

impl Navigation {
    pub fn get(&self, shortcut: impl Into<NavigationShortcut>) -> Option<&KeyChord> {
        match shortcut.into() {
            NavigationShortcut::Move(Direction::Left) => self.left.as_ref(),
            NavigationShortcut::Move(Direction::Right) => self.right.as_ref(),
            NavigationShortcut::Move(Direction::Up) => self.up.as_ref(),
            NavigationShortcut::Move(Direction::Down) => self.down.as_ref(),
            NavigationShortcut::NearestEmptyZone => self.nearest_empty.as_ref(),
            NavigationShortcut::Focus(Direction::Up) => self.focus_up.as_ref(),
            NavigationShortcut::Focus(Direction::Left) => self.focus_left.as_ref(),
            NavigationShortcut::Focus(Direction::Down) => self.focus_down.as_ref(),
            NavigationShortcut::Focus(Direction::Right) => self.focus_right.as_ref(),
            NavigationShortcut::SelectWindow => self.select_window.as_ref(),
            NavigationShortcut::ClearSelection => self.clear_selection.as_ref(),
        }
    }

    pub fn set(&mut self, shortcut: impl Into<NavigationShortcut>, chord: Option<KeyChord>) {
        *self.slot(shortcut.into()) = chord;
    }

    pub(crate) fn slot(&mut self, shortcut: NavigationShortcut) -> &mut Option<KeyChord> {
        match shortcut {
            NavigationShortcut::Move(Direction::Left) => &mut self.left,
            NavigationShortcut::Move(Direction::Right) => &mut self.right,
            NavigationShortcut::Move(Direction::Up) => &mut self.up,
            NavigationShortcut::Move(Direction::Down) => &mut self.down,
            NavigationShortcut::NearestEmptyZone => &mut self.nearest_empty,
            NavigationShortcut::Focus(Direction::Up) => &mut self.focus_up,
            NavigationShortcut::Focus(Direction::Left) => &mut self.focus_left,
            NavigationShortcut::Focus(Direction::Down) => &mut self.focus_down,
            NavigationShortcut::Focus(Direction::Right) => &mut self.focus_right,
            NavigationShortcut::SelectWindow => &mut self.select_window,
            NavigationShortcut::ClearSelection => &mut self.clear_selection,
        }
    }

    /// Every assigned chord, in `NavigationShortcut::ALL` order.
    pub fn chords(&self) -> impl Iterator<Item = (NavigationShortcut, &KeyChord)> {
        NavigationShortcut::ALL
            .into_iter()
            .filter_map(|shortcut| self.get(shortcut).map(|chord| (shortcut, chord)))
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
        *self.slot(step) = chord;
    }

    pub(crate) fn slot(&mut self, step: HistoryStep) -> &mut Option<KeyChord> {
        match step {
            HistoryStep::Undo => &mut self.undo,
            HistoryStep::Redo => &mut self.redo,
        }
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

/// A layout's own shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutShortcut {
    /// Apply the layout to the display under the pointer.
    Select,
    /// Move the focused window into the zone at this index of the layout's zones.
    Zone(usize),
}

impl LayoutShortcut {
    /// The name the settings app and conflict messages use for the layout named `layout`.
    pub fn title(self, layout: &str) -> String {
        match self {
            LayoutShortcut::Select => format!("Select {layout}"),
            LayoutShortcut::Zone(index) => format!("Zone {} of {layout}", index + 1),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspaces {
    pub enabled: bool,
    pub active_id: String,
    pub previous: Option<KeyChord>,
    pub next: Option<KeyChord>,
    /// Send the focused window, or the selection, to the previous workspace and stay.
    #[serde(default)]
    pub send_previous: Option<KeyChord>,
    /// Send the focused window, or the selection, to the next workspace and stay.
    #[serde(default)]
    pub send_next: Option<KeyChord>,
    pub items: Vec<Workspace>,
}

/// The shortcuts for every workspace at once: stepping to the previous or next one, and
/// sending windows there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceStep {
    Previous,
    Next,
    SendPrevious,
    SendNext,
}

impl WorkspaceStep {
    pub const ALL: [WorkspaceStep; 4] = [
        WorkspaceStep::Previous,
        WorkspaceStep::Next,
        WorkspaceStep::SendPrevious,
        WorkspaceStep::SendNext,
    ];

    /// The name the settings app and conflict messages use.
    pub fn title(self) -> &'static str {
        match self {
            WorkspaceStep::Previous => "Previous workspace",
            WorkspaceStep::Next => "Next workspace",
            WorkspaceStep::SendPrevious => "Send window to previous workspace",
            WorkspaceStep::SendNext => "Send window to next workspace",
        }
    }

    /// The field that holds this step's chord in the settings file.
    fn field(self) -> &'static str {
        match self {
            WorkspaceStep::Previous => "previous",
            WorkspaceStep::Next => "next",
            WorkspaceStep::SendPrevious => "sendPrevious",
            WorkspaceStep::SendNext => "sendNext",
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

    pub fn get(&self, shortcut: WorkspaceShortcut) -> Option<&KeyChord> {
        match shortcut {
            WorkspaceShortcut::Switch => self.switch_shortcut.as_ref(),
            WorkspaceShortcut::Send => self.send_shortcut.as_ref(),
            WorkspaceShortcut::SendAndSwitch => self.send_and_switch_shortcut.as_ref(),
        }
    }

    pub fn set(&mut self, shortcut: WorkspaceShortcut, chord: Option<KeyChord>) {
        *self.slot(shortcut) = chord;
    }

    pub(crate) fn slot(&mut self, shortcut: WorkspaceShortcut) -> &mut Option<KeyChord> {
        match shortcut {
            WorkspaceShortcut::Switch => &mut self.switch_shortcut,
            WorkspaceShortcut::Send => &mut self.send_shortcut,
            WorkspaceShortcut::SendAndSwitch => &mut self.send_and_switch_shortcut,
        }
    }

    /// Every assigned chord of this workspace's own.
    pub fn chords(&self) -> impl Iterator<Item = (WorkspaceShortcut, &KeyChord)> {
        WorkspaceShortcut::ALL
            .into_iter()
            .filter_map(|shortcut| self.get(shortcut).map(|chord| (shortcut, chord)))
    }

    fn is_valid(&self) -> bool {
        !self.id.is_empty()
            && self.id.chars().count() <= MAX_WORKSPACE_ID_LENGTH
            && valid_name(&self.name)
            && self.displays.len() <= MAX_DISPLAY_ASSIGNMENTS
            && self.chords().all(|(_, chord)| chord.is_valid())
    }
}

/// A workspace's own shortcuts. Written as the field of `Workspace` that holds it, such as
/// `"switchShortcut"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceShortcut {
    #[serde(rename = "switchShortcut")]
    Switch,
    /// Send the focused window, or the selection, there and stay.
    #[serde(rename = "sendShortcut")]
    Send,
    /// Send the focused window there, then switch, taking the selection along.
    #[serde(rename = "sendAndSwitchShortcut")]
    SendAndSwitch,
}

impl WorkspaceShortcut {
    pub const ALL: [WorkspaceShortcut; 3] = [
        WorkspaceShortcut::Switch,
        WorkspaceShortcut::Send,
        WorkspaceShortcut::SendAndSwitch,
    ];

    /// The name the settings app and conflict messages use for the workspace named
    /// `workspace`.
    pub fn title(self, workspace: &str) -> String {
        match self {
            WorkspaceShortcut::Switch => format!("Switch to {workspace}"),
            WorkspaceShortcut::Send => format!("Send window to {workspace}"),
            WorkspaceShortcut::SendAndSwitch => format!("Send window to {workspace} and switch"),
        }
    }
}

/// The Snappy key with the workspace's number, for workspaces 1 to 9.
pub(crate) fn numbered_switch_shortcut(number: usize) -> Option<KeyChord> {
    (1..=NUMBERED_SWITCH_SHORTCUTS)
        .contains(&number)
        .then(|| KeyChord::snappy(&format!("Digit{number}"), []).with_label(number.to_string()))
}

impl Workspaces {
    /// Three workspaces, switched to with the Snappy key and their number, and stepped
    /// through with the Snappy key and the semicolon and quote keys, or with the period and
    /// slash keys to send the focused window away. Both pairs sit together under the right
    /// hand, on every keyboard layout's home and bottom rows.
    pub fn defaults() -> Self {
        let chord = |code: &str, label: &str| Some(KeyChord::snappy(code, []).with_label(label));
        Self {
            enabled: false,
            active_id: "workspace-1".to_owned(),
            previous: chord("Semicolon", ";"),
            next: chord("Quote", "'"),
            send_previous: chord("Period", "."),
            send_next: chord("Slash", "/"),
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
        }
    }

    pub fn set_step(&mut self, step: WorkspaceStep, chord: Option<KeyChord>) {
        *self.step_slot(step) = chord;
    }

    pub(crate) fn step_slot(&mut self, step: WorkspaceStep) -> &mut Option<KeyChord> {
        match step {
            WorkspaceStep::Previous => &mut self.previous,
            WorkspaceStep::Next => &mut self.next,
            WorkspaceStep::SendPrevious => &mut self.send_previous,
            WorkspaceStep::SendNext => &mut self.send_next,
        }
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

    pub fn item_mut(&mut self, id: &str) -> Option<&mut Workspace> {
        self.items.iter_mut().find(|item| item.id == id)
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
    /// The arrows move the focused window, and I, J, K, and L, which sit like arrows under
    /// the right hand, move focus. Return selects and backslash clears the selection;
    /// Delete can't, since macOS never passes the Snappy key with Delete on to apps.
    pub fn defaults() -> Self {
        let chord = |code: &str, label: &str| Some(KeyChord::snappy(code, []).with_label(label));
        let key = |code: &str| Some(KeyChord::snappy(code, []));
        Self {
            left: chord("ArrowLeft", "←"),
            right: chord("ArrowRight", "→"),
            up: chord("ArrowUp", "↑"),
            down: chord("ArrowDown", "↓"),
            nearest_empty: key("Space"),
            focus_left: key("KeyJ"),
            focus_right: key("KeyL"),
            focus_up: key("KeyI"),
            focus_down: key("KeyK"),
            select_window: key("Enter"),
            clear_selection: chord("Backslash", "\\"),
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
            snappy_key: SnappyKey::DEFAULT,
            caps_lock_as_snappy_key: false,
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
            .filter(|chord| !used.contains(&chord.combo(self.snappy_key)));
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
    pub fn validate(self, platform: Platform) -> Result<Settings, SettingsError> {
        self.checked(platform, bindings::first_conflict)
    }

    /// These settings with the Snappy key standing for `modifiers`, checked as a whole:
    /// every Snappy key chord moves to the new modifiers, so each must still differ from
    /// every other binding and from the chords the OS and apps keep. Chords stored with
    /// concrete modifiers keep them, even ones the new key's modifiers match, which then
    /// conflict with any Snappy key chord on the same key. Whether the OS leaves the new
    /// chords free is the caller's to probe (`bindings::availability_probes`).
    pub fn with_snappy_key(&self, modifiers: Mods, platform: Platform) -> Result<Settings, SettingsError> {
        let mut changed = self.clone();
        changed.snappy_key = SnappyKey::new(modifiers)?;
        changed.checked(platform, bindings::snappy_key_conflict)
    }

    fn checked(
        mut self,
        platform: Platform,
        first_conflict: fn(&Settings, Platform) -> Option<String>,
    ) -> Result<Settings, SettingsError> {
        if self.version != FORMAT_VERSION {
            return Err(SettingsError::UnsupportedVersion(self.version));
        }
        self.check_shapes()?;
        self.normalize();
        if let Some(conflict) = first_conflict(&self, platform) {
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
    /// uses its chord. Shortcuts the file has, cleared ones included, are kept as written,
    /// except chords the OS keeps for itself, which read as missing, and the chords of a
    /// file from before the Snappy key could be chosen (`read_as_before_the_snappy_key`).
    /// Defaults and kept chords compare chords with the file's Snappy key, so it is read
    /// first.
    pub fn from_json(bytes: &[u8], platform: Platform) -> Result<Settings, SettingsError> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(SettingsError::Invalid);
        }
        let mut written: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| SettingsError::Invalid)?;
        let written_key: Option<SnappyKey> = match written.get("snappyKey") {
            Some(key) => Some(serde_json::from_value(key.clone()).map_err(|_| SettingsError::Invalid)?),
            None => None,
        };
        let snappy_key = written_key.unwrap_or_default();
        let kept: Vec<Combo> = chords_the_system_keeps(platform)
            .into_iter()
            .map(|kept| kept.combo)
            .collect();
        forget_kept_chords(&mut written, &kept, snappy_key);
        let file: SettingsFile = serde_json::from_value(written).map_err(|_| SettingsError::Invalid)?;
        let defaults = Settings::defaults(platform);
        let missing = missing_shortcuts(&file, &defaults.workspaces);
        let navigation = match file.navigation {
            Some(written) => serde_json::from_value(written).map_err(|_| SettingsError::Invalid)?,
            None => Navigation::default(),
        };
        let workspaces = match file.workspaces {
            Some(written) => serde_json::from_value(written).map_err(|_| SettingsError::Invalid)?,
            None => defaults.workspaces.clone(),
        };
        let mut settings = Settings {
            version: file.version.unwrap_or(FORMAT_VERSION),
            snapping: file.snapping.unwrap_or_else(|| defaults.snapping.clone()),
            snappy_key,
            caps_lock_as_snappy_key: file
                .caps_lock_as_snappy_key
                .unwrap_or(defaults.caps_lock_as_snappy_key),
            navigation,
            layouts: file.layouts.unwrap_or_else(|| defaults.layouts.clone()),
            workspaces,
            history: file.history.unwrap_or_default(),
            debug_log: file.debug_log.unwrap_or(defaults.debug_log),
        };
        shortcuts::give_defaults(&mut settings, &missing);
        if written_key.is_none() {
            settings.read_as_before_the_snappy_key();
        }
        settings.validate(platform)
    }

    /// Before the Snappy key could be chosen, files had no `snappyKey`, and every chord
    /// holding all of its default's modifiers (Control, Option, and Command), concrete or
    /// not, loaded as the Snappy key plus the rest. Such a file still loads that way, as
    /// recording the chord with the default key would store it. A file that names its key
    /// was saved with every chord in the form recording gave it, so it is read as written.
    fn read_as_before_the_snappy_key(&mut self) {
        let key = SnappyKey::DEFAULT;
        shortcuts::for_each_chord(self, |chord| {
            chord.mods = ChordMods::recorded(chord.mods.expanded(key), key)
        });
    }

    pub fn to_json(&self) -> Result<Vec<u8>, SettingsError> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| SettingsError::Invalid)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(SettingsError::Invalid);
        }
        Ok(bytes)
    }
}

/// Remove every shortcut in a written file that the OS keeps for itself. Such a shortcut
/// never fired, and files saved before Snappy knew that may hold one, so it reads as if
/// the file never had it instead of failing the whole file.
fn forget_kept_chords(value: &mut serde_json::Value, kept: &[Combo], key: SnappyKey) {
    match value {
        serde_json::Value::Object(fields) => {
            fields.retain(|_, field| !is_kept_chord(field, kept, key));
            fields
                .values_mut()
                .for_each(|field| forget_kept_chords(field, kept, key));
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .for_each(|item| forget_kept_chords(item, kept, key)),
        _ => {}
    }
}

fn is_kept_chord(value: &serde_json::Value, kept: &[Combo], key: SnappyKey) -> bool {
    value.get("code").is_some()
        && serde_json::from_value::<KeyChord>(value.clone())
            .is_ok_and(|chord| kept.contains(&chord.combo(key)))
}

/// Every shortcut `file` doesn't mention, as files written before it existed don't, in the
/// order their defaults are given out. A file without workspaces gets `defaults`, so it
/// lacks every one of their shortcuts.
fn missing_shortcuts(file: &SettingsFile, defaults: &Workspaces) -> Vec<ShortcutId> {
    let lacks = |section: &Option<serde_json::Value>, field: &str| {
        section
            .as_ref()
            .is_none_or(|written| written.get(field).is_none())
    };
    let navigation = NavigationShortcut::ALL
        .into_iter()
        .filter(|shortcut| lacks(&file.navigation, shortcut.field()))
        .map(ShortcutId::from);
    let steps = WorkspaceStep::ALL
        .into_iter()
        .filter(|step| lacks(&file.workspaces, step.field()))
        .map(ShortcutId::from);
    let switch = |id: &str| ShortcutId::workspace(id, WorkspaceShortcut::Switch);
    let switches: Vec<ShortcutId> = match &file.workspaces {
        Some(written) => written
            .get("items")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|item| item.get("switchShortcut").is_none())
            .filter_map(|item| item.get("id").and_then(serde_json::Value::as_str))
            .map(switch)
            .collect(),
        None => defaults.items.iter().map(|item| switch(&item.id)).collect(),
    };
    navigation.chain(steps).chain(switches).collect()
}

/// The settings file as written, every section optional.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsFile {
    version: Option<u32>,
    snapping: Option<Snapping>,
    caps_lock_as_snappy_key: Option<bool>,
    /// Kept as written so shortcuts added after the file was saved can be told apart.
    navigation: Option<serde_json::Value>,
    layouts: Option<Layouts>,
    /// Kept as written so shortcuts added after the file was saved can be told apart.
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
    #[error("{0}")]
    SnappyKey(#[from] SnappyKeyError),
    #[error(
        "The existing settings file couldn't be loaded and has not been changed. Repair or move it, then reopen Snappy before saving."
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
        Some(KeyChord::snappy(code, []).combo(SnappyKey::DEFAULT))
    }

    #[test]
    fn default_shortcuts_are_the_snappy_key_and_one_key_on_every_platform() {
        let combo = |chord: Option<&KeyChord>| chord.map(|chord| chord.combo(SnappyKey::DEFAULT));
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
            let navigation = NavigationShortcut::ALL.map(|shortcut| combo(settings.navigation.get(shortcut)));
            assert_eq!(
                navigation,
                [
                    snappy("ArrowLeft"),
                    snappy("ArrowRight"),
                    snappy("ArrowUp"),
                    snappy("ArrowDown"),
                    snappy("Space"),
                    snappy("KeyI"),
                    snappy("KeyJ"),
                    snappy("KeyK"),
                    snappy("KeyL"),
                    snappy("Enter"),
                    snappy("Backslash"),
                ],
                "{platform:?}"
            );
            assert_eq!(settings.history, MoveHistory::default());

            let every_chord = bindings::bound_combos(&settings);
            assert_eq!(every_chord.len(), 4 + 3 + 5 + 6, "{platform:?}");
            let snappy_key = SnappyKey::DEFAULT.modifiers();
            assert!(
                every_chord.iter().all(|combo| combo.mods == snappy_key),
                "nothing but the Snappy key and one key: {every_chord:?}"
            );
        }
        assert_eq!(
            Settings::defaults(Platform::Windows).workspaces,
            Settings::defaults(MAC).workspaces
        );
        assert_eq!(
            Settings::defaults(Platform::Linux).navigation,
            Settings::defaults(MAC).navigation
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
    fn a_file_without_send_steps_gets_their_defaults() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.next = None;
        let bytes = written_without(settings, &["sendPrevious", "sendNext"]);
        let loaded = Settings::from_json(&bytes, MAC).unwrap();
        assert_eq!(
            loaded
                .workspaces
                .send_previous
                .map(|c| c.combo(SnappyKey::DEFAULT)),
            snappy("Period")
        );
        assert_eq!(
            loaded.workspaces.send_next.map(|c| c.combo(SnappyKey::DEFAULT)),
            snappy("Slash")
        );
        assert_eq!(loaded.workspaces.next, None, "written steps are kept");
        assert_eq!(loaded.history, MoveHistory::default());
    }

    #[test]
    fn a_file_without_send_steps_skips_defaults_another_binding_uses() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[0].switch_shortcut = Some(KeyChord::snappy("Slash", []));
        settings.workspaces.send_next = None;
        let bytes = written_without(settings, &["sendPrevious", "sendNext"]);
        let loaded = Settings::from_json(&bytes, MAC).unwrap();
        assert_eq!(
            loaded
                .workspaces
                .send_previous
                .map(|c| c.combo(SnappyKey::DEFAULT)),
            snappy("Period")
        );
        assert_eq!(loaded.workspaces.send_next, None);
    }

    /// The defaults written without some navigation fields, as older files are.
    fn written_without_navigation(settings: Settings, fields: &[&str]) -> Vec<u8> {
        let mut written = serde_json::to_value(settings).unwrap();
        let navigation = written["navigation"].as_object_mut().unwrap();
        for field in fields {
            navigation.remove(*field);
        }
        serde_json::to_vec(&written).unwrap()
    }

    const FOCUS_FIELDS: [&str; 6] = [
        "focusUp",
        "focusLeft",
        "focusDown",
        "focusRight",
        "selectWindow",
        "clearSelection",
    ];

    #[test]
    fn a_file_without_focus_and_selection_shortcuts_gets_their_defaults() {
        let bytes = written_without_navigation(Settings::defaults(MAC), &FOCUS_FIELDS);
        let loaded = Settings::from_json(&bytes, MAC).unwrap();
        assert_eq!(loaded.navigation, Navigation::defaults());
    }

    #[test]
    fn a_file_without_focus_and_selection_shortcuts_skips_defaults_another_binding_uses() {
        let mut settings = Settings::defaults(MAC);
        settings.history.undo = Some(KeyChord::snappy("Enter", []));
        settings.workspaces.items[0].send_shortcut = Some(KeyChord::snappy("KeyK", []));
        let bytes = written_without_navigation(settings, &FOCUS_FIELDS);
        let loaded = Settings::from_json(&bytes, MAC).unwrap();
        assert_eq!(loaded.navigation.select_window, None);
        assert_eq!(loaded.navigation.focus_down, None);
        assert_eq!(
            loaded.navigation.focus_up.map(|c| c.combo(SnappyKey::DEFAULT)),
            snappy("KeyI")
        );
        assert_eq!(
            loaded
                .navigation
                .clear_selection
                .map(|c| c.combo(SnappyKey::DEFAULT)),
            snappy("Backslash")
        );
        assert_eq!(
            loaded.history.undo.map(|c| c.combo(SnappyKey::DEFAULT)),
            snappy("Enter")
        );
    }

    /// The defaults written with the Snappy key and Delete clearing the selection, as
    /// files saved before macOS was known to keep that chord are.
    fn written_clearing_with_delete(settings: Settings) -> Vec<u8> {
        let mut written = serde_json::to_value(settings).unwrap();
        written["navigation"]["clearSelection"] =
            serde_json::json!({ "code": "Backspace", "mods": ["snappy"] });
        serde_json::to_vec(&written).unwrap()
    }

    #[test]
    fn a_macos_file_clearing_with_the_snappy_key_and_delete_loads_with_backslash_instead() {
        let bytes = written_clearing_with_delete(Settings::defaults(MAC));
        let loaded = Settings::from_json(&bytes, MAC).unwrap();
        assert_eq!(loaded.navigation, Navigation::defaults());
        assert_eq!(
            loaded.navigation.clear_selection.unwrap().long_label(MAC),
            "Snappy + \\"
        );
    }

    #[test]
    fn a_macos_file_clearing_with_delete_leaves_clearing_unassigned_when_backslash_is_taken() {
        let mut settings = Settings::defaults(MAC);
        settings.history.undo = Some(KeyChord::snappy("Backslash", []));
        let bytes = written_clearing_with_delete(settings);
        let loaded = Settings::from_json(&bytes, MAC).unwrap();
        assert_eq!(loaded.navigation.clear_selection, None);
        assert_eq!(
            loaded.history.undo.map(|c| c.combo(SnappyKey::DEFAULT)),
            snappy("Backslash")
        );
    }

    #[test]
    fn a_macos_file_with_the_snappy_key_and_delete_anywhere_else_loads_without_it() {
        let mut settings = Settings::defaults(MAC);
        settings.history.redo = Some(KeyChord::snappy("Backspace", []));
        settings.workspaces.items[1].send_shortcut = Some(KeyChord::snappy("Backspace", []));
        let halves = layout::preset("halves").unwrap();
        let mut options = settings.options(halves);
        options
            .zone_shortcuts
            .insert("halves-0".into(), KeyChord::snappy("Backspace", []));
        options
            .zone_shortcuts
            .insert("halves-1".into(), KeyChord::snappy("KeyM", []));
        settings.layouts.options.insert("halves".into(), options);
        let bytes = serde_json::to_vec(&settings).unwrap();

        let loaded = Settings::from_json(&bytes, MAC).unwrap();
        assert_eq!(loaded.history.redo, None);
        assert_eq!(loaded.workspaces.items[1].send_shortcut, None);
        let zones = loaded.options(halves).zone_shortcuts;
        assert_eq!(zones.keys().collect::<Vec<_>>(), ["halves-1"]);
        assert_eq!(loaded.navigation, Navigation::defaults());
    }

    #[test]
    fn windows_and_linux_files_keep_the_snappy_key_and_backspace() {
        for platform in [Platform::Windows, Platform::Linux] {
            let bytes = written_clearing_with_delete(Settings::defaults(platform));
            let loaded = Settings::from_json(&bytes, platform).unwrap();
            assert_eq!(
                loaded
                    .navigation
                    .clear_selection
                    .map(|c| c.combo(SnappyKey::DEFAULT)),
                snappy("Backspace"),
                "{platform:?}"
            );
        }
    }

    #[test]
    fn a_saved_cleared_focus_or_selection_shortcut_stays_cleared() {
        let mut settings = Settings::defaults(MAC);
        settings.navigation.set(NavigationShortcut::SelectWindow, None);
        settings
            .navigation
            .set(NavigationShortcut::Focus(Direction::Left), None);
        let json = String::from_utf8(settings.to_json().unwrap()).unwrap();
        assert!(json.contains(r#""selectWindow": null"#), "{json}");
        let loaded = Settings::from_json(json.as_bytes(), MAC).unwrap();
        assert_eq!(loaded.navigation, settings.navigation);
    }

    #[test]
    fn a_file_with_the_former_take_along_shortcut_loads_and_selects_with_its_chord() {
        let mut written = serde_json::to_value(Settings::defaults(MAC)).unwrap();
        let take_along = serde_json::json!({ "code": "Enter", "mods": ["snappy"] });
        written["workspaces"]
            .as_object_mut()
            .unwrap()
            .insert("takeAlong".into(), take_along);
        let navigation = written["navigation"].as_object_mut().unwrap();
        for field in FOCUS_FIELDS {
            navigation.remove(field);
        }
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC).unwrap();
        assert_eq!(
            loaded
                .navigation
                .select_window
                .as_ref()
                .map(|chord| chord.combo(SnappyKey::DEFAULT)),
            snappy("Enter")
        );
        let saved = String::from_utf8(loaded.to_json().unwrap()).unwrap();
        assert!(!saved.contains("takeAlong"), "{saved}");
    }

    #[test]
    fn a_file_with_the_former_move_steps_ignores_them_and_gets_the_send_steps() {
        let mut written = serde_json::to_value(Settings::defaults(MAC)).unwrap();
        let workspaces = written["workspaces"].as_object_mut().unwrap();
        for field in ["sendPrevious", "sendNext"] {
            workspaces.remove(field);
        }
        let former = |code: &str| serde_json::json!({ "code": code, "mods": ["snappy", "shift"] });
        workspaces.insert("movePrevious".into(), former("BracketLeft"));
        workspaces.insert("moveNext".into(), former("BracketRight"));
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC).unwrap();
        assert_eq!(
            loaded
                .workspaces
                .send_previous
                .as_ref()
                .map(|chord| chord.combo(SnappyKey::DEFAULT)),
            snappy("Period")
        );
        assert_eq!(
            loaded
                .workspaces
                .send_next
                .as_ref()
                .map(|chord| chord.combo(SnappyKey::DEFAULT)),
            snappy("Slash")
        );
        let saved = String::from_utf8(loaded.to_json().unwrap()).unwrap();
        assert!(!saved.contains("moveNext"), "{saved}");
    }

    #[test]
    fn a_saved_cleared_workspace_shortcut_stays_cleared() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.set_step(WorkspaceStep::SendNext, None);
        settings.workspaces.items[1].switch_shortcut = None;
        let loaded = Settings::from_json(&settings.to_json().unwrap(), MAC).unwrap();
        assert_eq!(loaded.workspaces.send_next, None);
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
            loaded.layouts.options["halves"].zone_shortcuts["halves-0"].combo(SnappyKey::DEFAULT),
            KeyChord::snappy("Digit1", []).combo(SnappyKey::DEFAULT)
        );
        assert!(
            loaded
                .workspaces
                .items
                .iter()
                .all(|item| item.switch_shortcut.is_none())
        );
        assert_eq!(
            loaded.workspaces.next.map(|c| c.combo(SnappyKey::DEFAULT)),
            snappy("Quote")
        );
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
            .map(|item| {
                item.switch_shortcut
                    .as_ref()
                    .map(|chord| chord.combo(SnappyKey::DEFAULT))
            })
            .collect();
        assert_eq!(switches, [snappy("Digit1"), None, snappy("Digit3")]);
    }

    #[test]
    fn workspaces_one_to_nine_switch_with_their_digit_and_later_ones_have_no_shortcuts() {
        let fourth = Workspace::numbered("workspace-4", 4);
        assert_eq!(fourth.name, "Workspace 4");
        assert_eq!(
            fourth
                .switch_shortcut
                .as_ref()
                .map(|chord| chord.combo(SnappyKey::DEFAULT)),
            snappy("Digit4")
        );
        assert_eq!(
            fourth.switch_shortcut.as_ref().unwrap().long_label(MAC),
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
            added
                .switch_shortcut
                .as_ref()
                .map(|chord| chord.combo(SnappyKey::DEFAULT)),
            snappy("Digit4")
        );

        settings.workspaces.items[2].switch_shortcut = Some(KeyChord::snappy("Digit4", []));
        let added = settings.new_workspace("workspace-4", 4);
        assert_eq!(added.switch_shortcut, None);
        settings.workspaces.items.push(added);
        assert_eq!(settings.validate(MAC).map(|_| ()), Ok(()));
    }

    #[test]
    fn a_file_without_an_arrow_shortcut_gets_its_default_unless_another_binding_uses_it() {
        let mut settings = Settings::defaults(MAC);
        settings.history.redo = Some(KeyChord::snappy("ArrowRight", []));
        let bytes = written_without_navigation(settings, &["left", "right"]);
        let loaded = Settings::from_json(&bytes, MAC).unwrap();
        assert_eq!(loaded.navigation.left, Navigation::defaults().left);
        assert_eq!(loaded.navigation.right, None);
        assert_eq!(
            loaded.history.redo.map(|c| c.combo(SnappyKey::DEFAULT)),
            snappy("ArrowRight")
        );
    }

    #[test]
    fn a_file_without_navigation_gets_the_defaults_no_other_binding_uses() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[0].send_and_switch_shortcut = Navigation::defaults().left;
        settings.workspaces.items[1].send_shortcut = Some(KeyChord::snappy("Space", []));
        settings.workspaces.items[2].send_shortcut = Some(KeyChord::snappy("Enter", []));
        settings.history.redo = Navigation::defaults().up;
        settings.history.undo = Some(KeyChord::snappy("KeyL", []));
        let mut written = serde_json::to_value(settings).unwrap();
        written.as_object_mut().unwrap().remove("navigation");
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC).unwrap();

        let taken = [
            NavigationShortcut::Move(Direction::Left),
            NavigationShortcut::Move(Direction::Up),
            NavigationShortcut::NearestEmptyZone,
            NavigationShortcut::SelectWindow,
            NavigationShortcut::Focus(Direction::Right),
        ];
        for shortcut in NavigationShortcut::ALL {
            let expected = (!taken.contains(&shortcut))
                .then(|| Navigation::defaults().get(shortcut).cloned())
                .flatten();
            assert_eq!(loaded.navigation.get(shortcut), expected.as_ref(), "{shortcut:?}");
        }
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
            loaded
                .navigation
                .nearest_empty
                .map(|c| c.combo(SnappyKey::DEFAULT)),
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
        assert_eq!(
            loaded.history.undo.map(|c| c.combo(SnappyKey::DEFAULT)),
            snappy("Space")
        );
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
            .map(|hotkey| hotkey.combo.clone())
            .collect();
        assert!(
            registered
                .contains(&KeyChord::new("KeyW", [Modifier::Ctrl, Modifier::Alt]).combo(SnappyKey::DEFAULT))
        );
        assert!(
            registered
                .contains(&KeyChord::new("KeyN", [Modifier::Ctrl, Modifier::Alt]).combo(SnappyKey::DEFAULT))
        );
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
    fn a_file_from_before_the_snappy_key_setting_loads_its_modifiers_as_the_snappy_key() {
        let mut written = serde_json::to_value(Settings::defaults(MAC)).unwrap();
        written.as_object_mut().unwrap().remove("snappyKey");
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

        let saved = loaded.to_json().unwrap();
        let written: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            written["navigation"]["left"]["mods"],
            serde_json::json!(["snappy"])
        );
        assert_eq!(
            written["workspaces"]["sendNext"]["mods"],
            serde_json::json!(["snappy", "shift"])
        );
        assert_eq!(written["snappyKey"], serde_json::json!(["ctrl", "alt", "meta"]));
        assert_eq!(Settings::from_json(&saved, MAC).unwrap(), loaded);

        let mut partial = serde_json::to_value(Settings::defaults(MAC)).unwrap();
        partial.as_object_mut().unwrap().remove("snappyKey");
        partial["history"]["undo"] = serde_json::json!({ "code": "KeyU", "mods": ["ctrl", "alt"] });
        assert_eq!(
            Settings::from_json(&serde_json::to_vec(&partial).unwrap(), MAC)
                .unwrap()
                .history
                .undo,
            Some(KeyChord::new("KeyU", [Modifier::Ctrl, Modifier::Alt])),
            "only the default key's modifiers were ever read as the Snappy key"
        );
    }

    #[test]
    fn a_file_with_a_snappy_key_keeps_chords_written_with_its_modifiers_as_they_are() {
        let mut settings = Settings::defaults(MAC);
        let by_hand = KeyChord::new("ArrowLeft", [Modifier::Ctrl, Modifier::Alt, Modifier::Meta]);
        settings.navigation.left = Some(by_hand.clone());
        let saved = settings.to_json().unwrap();
        let written: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            written["navigation"]["left"]["mods"],
            serde_json::json!(["ctrl", "alt", "meta"])
        );
        let loaded = Settings::from_json(&saved, MAC).unwrap();
        assert_eq!(loaded.navigation.left, Some(by_hand));
        assert_eq!(
            Settings::from_json(&loaded.to_json().unwrap(), MAC).unwrap(),
            loaded
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

    #[test]
    fn caps_lock_is_not_the_snappy_key_unless_turned_on() {
        assert!(!Settings::defaults(MAC).caps_lock_as_snappy_key);
        let loaded = Settings::from_json(br#"{ "version": 1 }"#, MAC).unwrap();
        assert!(!loaded.caps_lock_as_snappy_key);

        let mut on = Settings::defaults(MAC);
        on.caps_lock_as_snappy_key = true;
        let saved = on.to_json().unwrap();
        assert!(String::from_utf8_lossy(&saved).contains(r#""capsLockAsSnappyKey": true"#));
        assert!(Settings::from_json(&saved, MAC).unwrap().caps_lock_as_snappy_key);
    }

    #[test]
    fn a_file_without_a_snappy_key_loads_with_the_default() {
        let mut written = serde_json::to_value(Settings::defaults(MAC)).unwrap();
        assert_eq!(written["snappyKey"], serde_json::json!(["ctrl", "alt", "meta"]));
        written.as_object_mut().unwrap().remove("snappyKey");
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC).unwrap();
        assert_eq!(loaded.snappy_key, SnappyKey::DEFAULT);
        assert_eq!(loaded, Settings::defaults(MAC).validate(MAC).unwrap());
    }

    #[test]
    fn a_file_with_a_chosen_snappy_key_keeps_it_and_its_chords_as_written() {
        let mut written = serde_json::to_value(Settings::defaults(Platform::Windows)).unwrap();
        written["snappyKey"] = serde_json::json!(["alt", "ctrl"]);
        written["history"] = serde_json::json!({
            "undo": { "code": "KeyU", "mods": ["shift", "ctrl", "alt"] },
            "redo": { "code": "KeyR", "mods": ["snappy", "ctrl"] },
        });
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), Platform::Windows).unwrap();
        let control_alt = Mods::from_iter([Modifier::Ctrl, Modifier::Alt]);
        assert_eq!(loaded.snappy_key.modifiers(), control_alt);
        assert_eq!(
            loaded.history.undo,
            Some(KeyChord::new(
                "KeyU",
                [Modifier::Shift, Modifier::Ctrl, Modifier::Alt]
            ))
        );
        assert_eq!(
            loaded.history.redo,
            Some(KeyChord::snappy("KeyR", [Modifier::Ctrl]))
        );
        assert_eq!(
            loaded
                .history
                .redo
                .as_ref()
                .map(|chord| chord.combo(loaded.snappy_key)),
            Some(crate::keys::Combo::new("KeyR", [Modifier::Ctrl, Modifier::Alt]))
        );
        assert_eq!(
            loaded
                .navigation
                .left
                .as_ref()
                .map(|chord| chord.combo(loaded.snappy_key)),
            Some(crate::keys::Combo::new(
                "ArrowLeft",
                [Modifier::Ctrl, Modifier::Alt]
            ))
        );
        let saved = loaded.to_json().unwrap();
        assert_eq!(Settings::from_json(&saved, Platform::Windows).unwrap(), loaded);
    }

    #[test]
    fn a_file_whose_snappy_key_has_one_modifier_is_invalid() {
        let mut written = serde_json::to_value(Settings::defaults(MAC)).unwrap();
        written["snappyKey"] = serde_json::json!(["meta"]);
        assert_eq!(
            Settings::from_json(&serde_json::to_vec(&written).unwrap(), MAC),
            Err(SettingsError::Invalid)
        );
    }

    #[test]
    fn a_concrete_chord_stays_as_recorded_when_the_snappy_key_comes_to_match_it() {
        let mut settings = Settings::defaults(MAC);
        let concrete = KeyChord::new("KeyU", [Modifier::Ctrl, Modifier::Alt, Modifier::Shift]);
        settings.history.undo = Some(concrete.clone());
        let settings = settings.validate(MAC).unwrap();
        assert_eq!(settings.history.undo, Some(concrete.clone()));

        let control_option = Mods::from_iter([Modifier::Ctrl, Modifier::Alt]);
        let changed = settings.with_snappy_key(control_option, MAC).unwrap();
        assert_eq!(changed.history.undo, Some(concrete.clone()));
        assert_eq!(
            changed
                .history
                .undo
                .as_ref()
                .map(|chord| chord.combo(changed.snappy_key)),
            Some(crate::keys::Combo::new(
                "KeyU",
                [Modifier::Ctrl, Modifier::Alt, Modifier::Shift]
            )),
            "the keys to press stay the same"
        );

        let matching = changed
            .with_snappy_key(
                Mods::from_iter([Modifier::Ctrl, Modifier::Alt, Modifier::Shift]),
                MAC,
            )
            .unwrap();
        assert_eq!(
            matching.history.undo,
            Some(concrete.clone()),
            "exactly the key's modifiers, and still not a Snappy key chord"
        );
        let saved = matching.to_json().unwrap();
        assert_eq!(Settings::from_json(&saved, MAC).unwrap(), matching);

        let back = matching
            .with_snappy_key(SnappyKey::DEFAULT.modifiers(), MAC)
            .unwrap();
        assert_eq!(back.history.undo, Some(concrete));
        assert_eq!(
            back.history
                .undo
                .as_ref()
                .map(|chord| chord.combo(back.snappy_key)),
            Some(crate::keys::Combo::new(
                "KeyU",
                [Modifier::Ctrl, Modifier::Alt, Modifier::Shift]
            )),
            "the keys to press never changed"
        );
    }

    #[test]
    fn navigation_shortcuts_are_written_as_their_settings_file_fields() {
        let written = serde_json::to_value(Navigation::defaults()).unwrap();
        for shortcut in NavigationShortcut::ALL {
            let name = serde_json::to_value(shortcut).unwrap();
            let field = name.as_str().unwrap();
            assert!(written.get(field).is_some(), "{field} is a navigation field");
            assert_eq!(
                serde_json::from_value::<NavigationShortcut>(name).unwrap(),
                shortcut
            );
        }
        assert!(serde_json::from_str::<NavigationShortcut>(r#""items""#).is_err());
    }
}
