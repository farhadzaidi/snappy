//! The shortcuts to register with the OS, and what each one does.

use std::collections::BTreeSet;

use super::{Bound, ShortcutId, bound_global, bound_select, bound_workspace, bound_zones};
use crate::keys::{Combo, KeyChord};
use crate::placement::navigate::Direction;
use crate::settings::{HistoryStep, NavigationShortcut, Settings, WorkspaceShortcut, WorkspaceStep};

/// What a registered shortcut does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotKeyAction {
    PreviousWorkspace,
    NextWorkspace,
    /// Send the focused window, or the selection, to the previous workspace and stay.
    SendToPreviousWorkspace,
    /// Send the focused window, or the selection, to the next workspace and stay.
    SendToNextWorkspace,
    SwitchWorkspace(String),
    SendToWorkspace(String),
    SendAndSwitch(String),
    /// Move the focused window to the neighboring zone.
    MoveWindow(Direction),
    /// Move the focused window into the nearest empty zone, or the selection into empty
    /// zones.
    NearestEmptyZone,
    /// Focus the next window in a direction.
    FocusWindow(Direction),
    /// Add the focused window to the selection, or take it out.
    SelectWindow,
    ClearSelection,
    SelectLayout(String),
    /// A zone chord, resolved against the focused window's display layout.
    Zone,
    UndoMove,
    RedoMove,
}

impl HotKeyAction {
    /// The action's name in the debug log, such as `"focusLeft"`. It never changes, so logs
    /// from different versions read alike.
    pub fn name(&self) -> &'static str {
        match self {
            HotKeyAction::PreviousWorkspace => "previousWorkspace",
            HotKeyAction::NextWorkspace => "nextWorkspace",
            HotKeyAction::SendToPreviousWorkspace => "sendToPreviousWorkspace",
            HotKeyAction::SendToNextWorkspace => "sendToNextWorkspace",
            HotKeyAction::SwitchWorkspace(_) => "switchWorkspace",
            HotKeyAction::SendToWorkspace(_) => "sendToWorkspace",
            HotKeyAction::SendAndSwitch(_) => "sendAndSwitch",
            HotKeyAction::MoveWindow(Direction::Left) => "moveLeft",
            HotKeyAction::MoveWindow(Direction::Right) => "moveRight",
            HotKeyAction::MoveWindow(Direction::Up) => "moveUp",
            HotKeyAction::MoveWindow(Direction::Down) => "moveDown",
            HotKeyAction::NearestEmptyZone => "nearestEmptyZone",
            HotKeyAction::FocusWindow(Direction::Left) => "focusLeft",
            HotKeyAction::FocusWindow(Direction::Right) => "focusRight",
            HotKeyAction::FocusWindow(Direction::Up) => "focusUp",
            HotKeyAction::FocusWindow(Direction::Down) => "focusDown",
            HotKeyAction::SelectWindow => "selectWindow",
            HotKeyAction::ClearSelection => "clearSelection",
            HotKeyAction::SelectLayout(_) => "selectLayout",
            HotKeyAction::Zone => "zone",
            HotKeyAction::UndoMove => "undoMove",
            HotKeyAction::RedoMove => "redoMove",
        }
    }

    /// The workspace an action is for, by ID.
    pub fn workspace(&self) -> Option<&str> {
        match self {
            HotKeyAction::SwitchWorkspace(workspace)
            | HotKeyAction::SendToWorkspace(workspace)
            | HotKeyAction::SendAndSwitch(workspace) => Some(workspace),
            _ => None,
        }
    }

    /// The layout an action is for, by ID.
    pub fn layout(&self) -> Option<&str> {
        match self {
            HotKeyAction::SelectLayout(layout) => Some(layout),
            _ => None,
        }
    }

    /// What pressing `shortcut` does.
    pub fn of(shortcut: &ShortcutId) -> Self {
        match shortcut {
            ShortcutId::Navigation { shortcut } => match shortcut {
                NavigationShortcut::Move(direction) => HotKeyAction::MoveWindow(*direction),
                NavigationShortcut::NearestEmptyZone => HotKeyAction::NearestEmptyZone,
                NavigationShortcut::Focus(direction) => HotKeyAction::FocusWindow(*direction),
                NavigationShortcut::SelectWindow => HotKeyAction::SelectWindow,
                NavigationShortcut::ClearSelection => HotKeyAction::ClearSelection,
            },
            ShortcutId::History { step } => match step {
                HistoryStep::Undo => HotKeyAction::UndoMove,
                HistoryStep::Redo => HotKeyAction::RedoMove,
            },
            ShortcutId::WorkspaceStep { step } => match step {
                WorkspaceStep::Previous => HotKeyAction::PreviousWorkspace,
                WorkspaceStep::Next => HotKeyAction::NextWorkspace,
                WorkspaceStep::SendPrevious => HotKeyAction::SendToPreviousWorkspace,
                WorkspaceStep::SendNext => HotKeyAction::SendToNextWorkspace,
            },
            ShortcutId::Workspace { workspace, shortcut } => {
                let workspace = workspace.clone();
                match shortcut {
                    WorkspaceShortcut::Switch => HotKeyAction::SwitchWorkspace(workspace),
                    WorkspaceShortcut::Send => HotKeyAction::SendToWorkspace(workspace),
                    WorkspaceShortcut::SendAndSwitch => HotKeyAction::SendAndSwitch(workspace),
                }
            }
            ShortcutId::LayoutSelect { layout } => HotKeyAction::SelectLayout(layout.clone()),
            ShortcutId::Zone { .. } => HotKeyAction::Zone,
        }
    }
}

/// A shortcut to register: its chord as stored, the keys to hold for it with the settings'
/// Snappy key, and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotKey {
    pub chord: KeyChord,
    pub combo: Combo,
    pub action: HotKeyAction,
}

/// The shortcuts Snappy should register now, one per distinct chord.
pub fn desired_hotkeys(settings: &Settings, display_ids: &[String], snapping_live: bool) -> Vec<HotKey> {
    if !settings.snapping.shortcuts_enabled {
        return Vec::new();
    }
    let mut wanted: Vec<Bound> = Vec::new();
    if settings.workspaces.enabled {
        wanted.extend(bound_workspace(&settings.workspaces));
    }
    if snapping_live {
        wanted.extend(bound_global(settings));
        for layout in settings.enabled_layouts() {
            wanted.extend(bound_select(layout, &settings.options(layout)));
        }
        for display in display_ids {
            let layout = settings.layout_for_display(display);
            let options = settings.options(layout);
            if options.enabled {
                wanted.extend(bound_zones(layout, &options));
            }
        }
    }

    let key = settings.snappy_key;
    let mut seen = BTreeSet::new();
    let mut unique: Vec<HotKey> = wanted
        .into_iter()
        .map(|bound| HotKey {
            combo: bound.chord.combo(key),
            action: HotKeyAction::of(&bound.id),
            chord: bound.chord,
        })
        .filter(|hotkey| seen.insert(hotkey.combo.clone()))
        .collect();
    unique.sort_by(|a, b| a.combo.cmp(&b.combo));
    unique
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Modifier, SnappyKey};
    use crate::shortcuts::test_support::*;

    #[test]
    fn every_action_has_a_camel_case_name_of_its_own() {
        let workspace = || "workspace-2".to_owned();
        let mut actions = vec![
            HotKeyAction::SwitchWorkspace(workspace()),
            HotKeyAction::SendToWorkspace(workspace()),
            HotKeyAction::SendAndSwitch(workspace()),
            HotKeyAction::SelectLayout("halves".to_owned()),
            HotKeyAction::Zone,
        ];
        actions.extend(ShortcutId::global().map(|id| HotKeyAction::of(&id)));
        actions.extend(WorkspaceStep::ALL.map(|step| HotKeyAction::of(&step.into())));
        let names: BTreeSet<&str> = actions.iter().map(HotKeyAction::name).collect();
        assert_eq!(names.len(), actions.len());
        for name in names {
            assert!(
                name.starts_with(|c: char| c.is_ascii_lowercase())
                    && name.chars().all(|c| c.is_ascii_alphabetic()),
                "{name}"
            );
        }
        assert_eq!(
            HotKeyAction::of(&NavigationShortcut::Focus(Direction::Left).into()).name(),
            "focusLeft"
        );
        assert_eq!(
            HotKeyAction::SendAndSwitch(workspace()).workspace(),
            Some("workspace-2")
        );
        assert_eq!(
            HotKeyAction::SelectLayout("halves".to_owned()).layout(),
            Some("halves")
        );
        assert_eq!(HotKeyAction::Zone.workspace(), None);
    }

    #[test]
    fn snapping_shortcuts_need_live_control() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.enabled = true;
        let displays = vec!["d".to_owned()];
        settings.history.undo = Some(snappy("KeyU"));
        with_zone(&mut settings, "focus-0", snappy("KeyM"));
        with_zone(&mut settings, "focus-1", snappy("KeyN"));
        let paused = desired_hotkeys(&settings, &displays, false);
        assert!(paused.iter().all(|hotkey| matches!(
            hotkey.action,
            HotKeyAction::PreviousWorkspace
                | HotKeyAction::NextWorkspace
                | HotKeyAction::SendToPreviousWorkspace
                | HotKeyAction::SendToNextWorkspace
                | HotKeyAction::SwitchWorkspace(_)
                | HotKeyAction::SendToWorkspace(_)
                | HotKeyAction::SendAndSwitch(_)
        )));
        assert_eq!(paused.len(), 4 + 3);
        let live = desired_hotkeys(&settings, &displays, true);
        let zones = live
            .iter()
            .filter(|hotkey| hotkey.action == HotKeyAction::Zone)
            .count();
        assert_eq!(zones, 2);
        assert!(live.iter().any(|hotkey| hotkey.action == HotKeyAction::UndoMove));
    }

    #[test]
    fn focus_and_selection_shortcuts_register_whenever_live_control_is_on() {
        let settings = Settings::defaults(MAC);
        assert!(!settings.workspaces.enabled);
        let actions = |live: bool| -> Vec<(Combo, HotKeyAction)> {
            desired_hotkeys(&settings, &["d".to_owned()], live)
                .into_iter()
                .filter(|hotkey| {
                    matches!(
                        hotkey.action,
                        HotKeyAction::FocusWindow(_)
                            | HotKeyAction::SelectWindow
                            | HotKeyAction::ClearSelection
                    )
                })
                .map(|hotkey| (hotkey.combo.clone(), hotkey.action))
                .collect()
        };
        assert!(actions(false).is_empty());
        let mut live = actions(true);
        live.sort_by_key(|(combo, _)| combo.code.clone());
        assert_eq!(
            live,
            [
                (
                    snappy("Backslash").combo(SnappyKey::DEFAULT),
                    HotKeyAction::ClearSelection
                ),
                (
                    snappy("Enter").combo(SnappyKey::DEFAULT),
                    HotKeyAction::SelectWindow
                ),
                (
                    snappy("KeyI").combo(SnappyKey::DEFAULT),
                    HotKeyAction::FocusWindow(Direction::Up)
                ),
                (
                    snappy("KeyJ").combo(SnappyKey::DEFAULT),
                    HotKeyAction::FocusWindow(Direction::Left)
                ),
                (
                    snappy("KeyK").combo(SnappyKey::DEFAULT),
                    HotKeyAction::FocusWindow(Direction::Down)
                ),
                (
                    snappy("KeyL").combo(SnappyKey::DEFAULT),
                    HotKeyAction::FocusWindow(Direction::Right)
                ),
            ]
        );
    }

    #[test]
    fn workspace_shortcuts_register_while_workspaces_run() {
        let mut settings = Settings::defaults(MAC);
        let actions = |settings: &Settings| -> Vec<HotKeyAction> {
            desired_hotkeys(settings, &[], false)
                .into_iter()
                .map(|hotkey| hotkey.action)
                .collect()
        };
        assert!(actions(&settings).is_empty());
        settings.workspaces.enabled = true;
        let registered = actions(&settings);
        assert!(registered.contains(&HotKeyAction::SendToPreviousWorkspace));
        assert!(registered.contains(&HotKeyAction::SendToNextWorkspace));
        assert!(
            !registered.contains(&HotKeyAction::SelectWindow),
            "selecting needs live control"
        );
        for id in ["workspace-1", "workspace-2", "workspace-3"] {
            assert!(registered.contains(&HotKeyAction::SwitchWorkspace(id.to_owned())));
        }
    }

    #[test]
    fn the_nearest_empty_zone_chord_registers_only_while_live_control_is_on() {
        let settings = Settings::defaults(MAC);
        let displays = vec!["d".to_owned()];
        let nearest = |live: bool| {
            desired_hotkeys(&settings, &displays, live)
                .into_iter()
                .filter(|hotkey| hotkey.action == HotKeyAction::NearestEmptyZone)
                .map(|hotkey| hotkey.combo.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(nearest(true), vec![snappy("Space").combo(SnappyKey::DEFAULT)]);
        assert!(nearest(false).is_empty());
    }

    #[test]
    fn zone_and_arrow_chords_register_once_without_a_shift_form() {
        let settings = Settings::defaults(MAC);
        let live = desired_hotkeys(&settings, &["d".to_owned()], true);
        let left: Vec<&HotKey> = live
            .iter()
            .filter(|hotkey| hotkey.action == HotKeyAction::MoveWindow(Direction::Left))
            .collect();
        assert_eq!(left.len(), 1);
        assert_eq!(Some(&left[0].chord), settings.navigation.left.as_ref());
        assert!(
            live.iter()
                .all(|hotkey| !hotkey.combo.clone().mods.contains(Modifier::Shift))
        );
    }

    #[test]
    fn the_master_switch_disables_everything() {
        let mut settings = Settings::defaults(MAC);
        settings.snapping.shortcuts_enabled = false;
        assert!(desired_hotkeys(&settings, &["d".to_owned()], true).is_empty());
    }

    #[test]
    fn a_snappy_key_change_registers_snappy_chords_with_the_new_modifiers_and_keeps_concrete_ones() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.enabled = true;
        settings.history.undo = Some(KeyChord::new("KeyU", [Modifier::Ctrl, Modifier::Shift]));
        let changed = settings
            .with_snappy_key(modifiers(&[Modifier::Alt, Modifier::Meta]), MAC)
            .unwrap();
        let registered: Vec<(Combo, HotKeyAction)> = desired_hotkeys(&changed, &["d".to_owned()], true)
            .into_iter()
            .map(|hotkey| (hotkey.combo, hotkey.action))
            .collect();
        assert!(registered.contains(&(
            Combo::new("ArrowLeft", [Modifier::Alt, Modifier::Meta]),
            HotKeyAction::MoveWindow(Direction::Left)
        )));
        assert!(registered.contains(&(
            Combo::new("Digit1", [Modifier::Alt, Modifier::Meta]),
            HotKeyAction::SwitchWorkspace("workspace-1".to_owned())
        )));
        assert!(registered.contains(&(
            Combo::new("KeyU", [Modifier::Ctrl, Modifier::Shift]),
            HotKeyAction::UndoMove
        )));
        assert_eq!(
            changed.navigation, settings.navigation,
            "chords stay stored as Snappy"
        );
        assert_eq!(changed.history, settings.history);
    }
}
