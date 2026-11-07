//! Who owns which shortcut: conflict rules, and the shortcuts to register. Chords are
//! compared by their combos, with the Snappy key expanded, so Snappy + 1 and the same key
//! with the Snappy key's modifiers held are one shortcut.

use std::collections::{BTreeMap, BTreeSet};

use crate::keys::{Combo, KeyChord, Platform, reserved_app_commands};
use crate::layout::Layout;
use crate::navigate::Direction;
use crate::settings::{
    HistoryStep, LayoutOptions, NEAREST_EMPTY_TITLE, Navigation, Settings, WorkspaceStep, Workspaces,
};

/// What a registered shortcut does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotKeyAction {
    PreviousWorkspace,
    NextWorkspace,
    /// Take the focused window to the previous workspace and switch there.
    MoveToPreviousWorkspace,
    /// Take the focused window to the next workspace and switch there.
    MoveToNextWorkspace,
    SwitchWorkspace(String),
    SendToWorkspace(String),
    SendAndSwitch(String),
    /// Move the focused window to the neighboring zone.
    Navigate(Direction),
    /// Move the focused window into the nearest empty zone.
    NearestEmptyZone,
    SelectLayout(String),
    /// A zone chord, resolved against the focused window's display layout.
    Zone,
    UndoMove,
    RedoMove,
}

impl HotKeyAction {
    fn of_step(step: WorkspaceStep) -> Self {
        match step {
            WorkspaceStep::Previous => HotKeyAction::PreviousWorkspace,
            WorkspaceStep::Next => HotKeyAction::NextWorkspace,
            WorkspaceStep::MovePrevious => HotKeyAction::MoveToPreviousWorkspace,
            WorkspaceStep::MoveNext => HotKeyAction::MoveToNextWorkspace,
        }
    }

    fn of_history(step: HistoryStep) -> Self {
        match step {
            HistoryStep::Undo => HotKeyAction::UndoMove,
            HistoryStep::Redo => HotKeyAction::RedoMove,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotKey {
    pub chord: KeyChord,
    pub action: HotKeyAction,
}

/// The shortcuts Snappy should register now, one per distinct chord.
pub fn desired_hotkeys(settings: &Settings, display_ids: &[String], snapping_live: bool) -> Vec<HotKey> {
    if !settings.snapping.shortcuts_enabled {
        return Vec::new();
    }
    let mut wanted: Vec<HotKey> = Vec::new();
    let mut add = |chord: &KeyChord, action: HotKeyAction| {
        wanted.push(HotKey {
            chord: chord.clone(),
            action,
        })
    };

    let workspaces = &settings.workspaces;
    if workspaces.enabled {
        for (step, chord) in workspaces.steps() {
            add(chord, HotKeyAction::of_step(step));
        }
        for item in &workspaces.items {
            item.switch_shortcut
                .iter()
                .for_each(|c| add(c, HotKeyAction::SwitchWorkspace(item.id.clone())));
            item.send_shortcut
                .iter()
                .for_each(|c| add(c, HotKeyAction::SendToWorkspace(item.id.clone())));
            item.send_and_switch_shortcut
                .iter()
                .for_each(|c| add(c, HotKeyAction::SendAndSwitch(item.id.clone())));
        }
    }

    if snapping_live {
        for (step, chord) in settings.history.chords() {
            add(chord, HotKeyAction::of_history(step));
        }
        for (direction, chord) in settings.navigation.chords() {
            add(chord, HotKeyAction::Navigate(direction));
        }
        if let Some(chord) = &settings.navigation.nearest_empty {
            add(chord, HotKeyAction::NearestEmptyZone);
        }
        for layout in settings.enabled_layouts() {
            if let Some(chord) = settings.options(layout).select_shortcut {
                add(&chord, HotKeyAction::SelectLayout(layout.id.clone()));
            }
        }
        for display in display_ids {
            let options = settings.options(settings.layout_for_display(display));
            if !options.enabled {
                continue;
            }
            for chord in options.zone_shortcuts.values() {
                add(chord, HotKeyAction::Zone);
            }
        }
    }

    let mut seen = BTreeSet::new();
    let mut unique: Vec<HotKey> = wanted
        .into_iter()
        .filter(|hotkey| seen.insert(hotkey.chord.combo()))
        .collect();
    unique.sort_by_key(|hotkey| hotkey.chord.combo());
    unique
}

/// The first shortcut conflict anywhere in `settings`, as a user-facing message.
pub fn first_conflict(settings: &Settings, platform: Platform) -> Option<String> {
    global_conflict(settings, platform)
        .or_else(|| workspace_conflict(settings, platform))
        .or_else(|| layout_conflict(settings, None, platform))
}

/// The first conflict if `layout` used `options` instead of its stored options. The
/// editor uses this to explain a conflict before saving.
pub fn conflict_with_layout(
    settings: &Settings,
    layout: &Layout,
    options: &LayoutOptions,
    platform: Platform,
) -> Option<String> {
    layout_conflict(settings, Some((layout, options)), platform)
}

fn reserved_owners(platform: Platform) -> BTreeMap<Combo, String> {
    reserved_app_commands(platform)
        .into_iter()
        .map(|combo| (combo, "a standard app command".to_owned()))
        .collect()
}

/// Navigation and window-history chords must differ from each other and the reserved
/// chords.
fn global_conflict(settings: &Settings, platform: Platform) -> Option<String> {
    let mut owners = reserved_owners(platform);
    claim_all(&mut owners, global_names(settings).into_iter(), platform)
}

/// The chords registered whatever the workspaces do: navigation, then window history.
fn global_names(settings: &Settings) -> Vec<(String, &KeyChord)> {
    let navigation = settings
        .navigation
        .chords()
        .map(|(direction, chord)| (format!("Move window {}", direction.name()), chord));
    let nearest_empty = settings
        .navigation
        .nearest_empty
        .iter()
        .map(|chord| (NEAREST_EMPTY_TITLE.to_owned(), chord));
    let history = settings
        .history
        .chords()
        .map(|(step, chord)| (step.title().to_owned(), chord));
    navigation.chain(nearest_empty).chain(history).collect()
}

/// The reserved chords and the global chords, by owner.
fn global_owners(settings: &Settings, platform: Platform) -> BTreeMap<Combo, String> {
    let mut owners = reserved_owners(platform);
    for (name, chord) in global_names(settings) {
        owners.insert(chord.combo(), name);
    }
    owners
}

fn workspace_names(workspaces: &Workspaces) -> Vec<(String, &KeyChord)> {
    let mut named: Vec<(String, &KeyChord)> = workspaces
        .steps()
        .map(|(step, chord)| (step.title().to_owned(), chord))
        .collect();
    for item in &workspaces.items {
        named.extend(
            item.switch_shortcut
                .iter()
                .map(|c| (format!("Switch to {}", item.name), c)),
        );
        named.extend(
            item.send_shortcut
                .iter()
                .map(|c| (format!("Move to {}", item.name), c)),
        );
        named.extend(
            item.send_and_switch_shortcut
                .iter()
                .map(|c| (format!("Move here and switch to {}", item.name), c)),
        );
    }
    named
}

/// Workspace chords must differ from each other, the reserved chords, and the global
/// chords, whether or not workspaces are on.
fn workspace_conflict(settings: &Settings, platform: Platform) -> Option<String> {
    let mut owners = global_owners(settings, platform);
    claim_all(
        &mut owners,
        workspace_names(&settings.workspaces).into_iter(),
        platform,
    )
}

/// Give each chord to its owner, reporting the first chord someone already owns.
fn claim_all<'a>(
    owners: &mut BTreeMap<Combo, String>,
    claims: impl Iterator<Item = (String, &'a KeyChord)>,
    platform: Platform,
) -> Option<String> {
    for (name, chord) in claims {
        if let Some(owner) = owners.get(&chord.combo()) {
            return Some(format!(
                "{} is already assigned to {owner}.",
                chord.long_label(platform)
            ));
        }
        owners.insert(chord.combo(), name);
    }
    None
}

/// Zone and selection chords of enabled layouts, with `replacement` standing in for one
/// layout's stored options.
fn layout_conflict(
    settings: &Settings,
    replacement: Option<(&Layout, &LayoutOptions)>,
    platform: Platform,
) -> Option<String> {
    let mut taken = global_owners(settings, platform);
    if settings.workspaces.enabled {
        for (name, chord) in workspace_names(&settings.workspaces) {
            taken.entry(chord.combo()).or_insert(name);
        }
    }
    let taken_message = |chord: &KeyChord, owner: &str| {
        format!("{} is already assigned to {owner}.", chord.long_label(platform))
    };

    let replaced_id = replacement.map(|(layout, _)| layout.id.as_str());
    let mut layouts: Vec<(&Layout, LayoutOptions)> = settings
        .layouts()
        .filter(|layout| Some(layout.id.as_str()) != replaced_id)
        .map(|layout| (layout, settings.options(layout)))
        .collect();
    if let Some((layout, options)) = replacement {
        layouts.push((layout, options.clone()));
    }
    let enabled: Vec<&(&Layout, LayoutOptions)> =
        layouts.iter().filter(|(_, options)| options.enabled).collect();

    let zone_chords: BTreeSet<Combo> = enabled
        .iter()
        .flat_map(|(_, options)| options.zone_shortcuts.values().map(KeyChord::combo))
        .collect();

    let mut selections: BTreeMap<Combo, &str> = BTreeMap::new();
    for (layout, options) in enabled {
        let mut in_layout = BTreeSet::new();
        for zone in &layout.zones {
            let Some(chord) = options.zone_shortcuts.get(&zone.id) else {
                continue;
            };
            if !in_layout.insert(chord.combo()) {
                return Some(format!(
                    "{} uses {} for more than one zone.",
                    layout.name,
                    chord.long_label(platform)
                ));
            }
            if let Some(owner) = taken.get(&chord.combo()) {
                return Some(taken_message(chord, owner));
            }
        }
        if let Some(chord) = &options.select_shortcut {
            if let Some(owner) = selections.get(&chord.combo()) {
                return Some(format!("{} already selects {owner}.", chord.long_label(platform)));
            }
            if let Some(owner) = taken.get(&chord.combo()) {
                return Some(taken_message(chord, owner));
            }
            selections.insert(chord.combo(), &layout.name);
        }
    }

    zone_chords.iter().find_map(|combo| {
        selections.get(combo).map(|name| {
            format!("A shortcut for selecting {name} is also assigned to a zone. Clear one assignment first.")
        })
    })
}

/// Every chord a binding uses. Disabled layouts and workspaces count, even while off.
pub fn bound_combos(settings: &Settings) -> BTreeSet<Combo> {
    let mut chords: Vec<KeyChord> = global_names(settings)
        .into_iter()
        .chain(workspace_names(&settings.workspaces))
        .map(|(_, chord)| chord.clone())
        .collect();
    for layout in settings.layouts() {
        let options = settings.options(layout);
        chords.extend(options.select_shortcut);
        chords.extend(options.zone_shortcuts.into_values());
    }
    chords.iter().map(KeyChord::combo).collect()
}

/// Default navigation chords, keeping only those no existing binding uses.
pub fn unused_navigation_defaults(settings: &Settings) -> Navigation {
    let others = Settings {
        navigation: Navigation::default(),
        ..settings.clone()
    };
    let used = bound_combos(&others);
    let mut navigation = Navigation::defaults();
    for direction in Direction::ALL {
        if navigation
            .get(direction)
            .is_some_and(|chord| used.contains(&chord.combo()))
        {
            navigation.set(direction, None);
        }
    }
    if navigation
        .nearest_empty
        .as_ref()
        .is_some_and(|chord| used.contains(&chord.combo()))
    {
        navigation.nearest_empty = None;
    }
    navigation
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifier;
    use crate::layout::preset;

    const MAC: Platform = Platform::MacOs;

    fn snappy(code: &str) -> KeyChord {
        KeyChord::snappy(code, [])
    }

    fn with_selection(settings: &mut Settings, layout_id: &str, chord: KeyChord) {
        let layout = settings.layout(layout_id).unwrap().clone();
        let mut options = settings.options(&layout);
        options.select_shortcut = Some(chord);
        settings.layouts.options.insert(layout_id.into(), options);
    }

    #[test]
    fn defaults_have_no_conflicts() {
        assert_eq!(first_conflict(&Settings::defaults(MAC), MAC), None);
    }

    #[test]
    fn zone_chords_are_unique_within_a_layout() {
        let settings = Settings::defaults(MAC);
        let halves = preset("halves").unwrap();
        let mut options = settings.options(halves);
        let first = options.zone_shortcuts["halves-0"].clone();
        options.zone_shortcuts.insert("halves-1".into(), first);
        let conflict = conflict_with_layout(&settings, halves, &options, MAC).unwrap();
        assert_eq!(conflict, "Halves uses Snappy + 1 for more than one zone.");
    }

    #[test]
    fn selection_chords_are_unique_and_not_zone_chords() {
        let mut settings = Settings::defaults(MAC);
        let chord = KeyChord::new("KeyL", [Modifier::Ctrl, Modifier::Alt]);
        with_selection(&mut settings, "halves", chord.clone());
        with_selection(&mut settings, "rows", chord);
        assert_eq!(
            first_conflict(&settings, MAC).unwrap(),
            "Control + Option + L already selects Halves."
        );

        let mut settings = Settings::defaults(MAC);
        with_selection(&mut settings, "halves", snappy("Digit1"));
        assert!(
            first_conflict(&settings, MAC)
                .unwrap()
                .starts_with("A shortcut for selecting Halves")
        );
    }

    #[test]
    fn shift_with_a_zone_or_arrow_chord_is_free_to_assign() {
        let mut settings = Settings::defaults(MAC);
        with_selection(
            &mut settings,
            "rows",
            KeyChord::snappy("Digit1", [Modifier::Shift]),
        );
        settings.workspaces.items[0].switch_shortcut = Some(KeyChord::snappy("ArrowLeft", [Modifier::Shift]));
        assert_eq!(first_conflict(&settings, MAC), None);
    }

    #[test]
    fn the_snappy_key_and_its_modifiers_held_by_hand_are_one_chord() {
        let mut settings = Settings::defaults(MAC);
        let by_hand = KeyChord::new("Digit1", [Modifier::Meta, Modifier::Alt, Modifier::Ctrl]);
        settings.workspaces.items[0].switch_shortcut = Some(by_hand.clone());
        settings.workspaces.enabled = true;
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + 1 is already assigned to Switch to Workspace 1.")
        );
        let registered: Vec<Combo> = desired_hotkeys(&settings, &["d".to_owned()], true)
            .iter()
            .map(|hotkey| hotkey.chord.combo())
            .collect();
        assert!(registered.contains(&Combo::new(
            "Digit1",
            [Modifier::Ctrl, Modifier::Alt, Modifier::Meta]
        )));
        assert!(registered.iter().all(|combo| !combo.mods.is_empty()));
    }

    #[test]
    fn disabled_layouts_reserve_nothing() {
        let mut settings = Settings::defaults(MAC);
        with_selection(&mut settings, "halves", snappy("Digit1"));
        for preset in crate::layout::presets() {
            let mut options = settings.options(preset);
            options.enabled = preset.id == "halves";
            settings.layouts.options.insert(preset.id.clone(), options);
        }
        let mut halves = settings.options(preset("halves").unwrap());
        halves.zone_shortcuts.clear();
        settings.layouts.options.insert("halves".into(), halves);
        assert_eq!(first_conflict(&settings, MAC), None);
    }

    #[test]
    fn workspace_chords_check_reserved_and_navigation() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.previous = Some(KeyChord::new("KeyQ", [Modifier::Meta]));
        assert_eq!(
            first_conflict(&settings, MAC).unwrap(),
            "Command + Q is already assigned to a standard app command."
        );
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[1].switch_shortcut = settings.navigation.left.clone();
        assert!(
            first_conflict(&settings, MAC)
                .unwrap()
                .ends_with("Move window left.")
        );
    }

    #[test]
    fn zone_chords_clash_with_workspaces_only_while_on() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[0].switch_shortcut = Some(KeyChord::new("Digit1", [Modifier::Ctrl]));
        let halves = preset("halves").unwrap();
        let mut options = settings.options(halves);
        options
            .zone_shortcuts
            .insert("halves-0".into(), KeyChord::new("Digit1", [Modifier::Ctrl]));
        settings.layouts.options.insert("halves".into(), options);
        assert_eq!(first_conflict(&settings, MAC), None);
        settings.workspaces.enabled = true;
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Control + 1 is already assigned to Switch to Workspace 1.")
        );
    }

    #[test]
    fn zone_chord_conflicts_name_the_owner() {
        let mut settings = Settings::defaults(MAC);
        let halves = preset("halves").unwrap();
        let mut options = settings.options(halves);
        let left = settings.navigation.left.clone().unwrap();
        options.zone_shortcuts.insert("halves-0".into(), left);
        settings.layouts.options.insert("halves".into(), options);
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + ← is already assigned to Move window left.")
        );
    }

    #[test]
    fn snapping_shortcuts_need_live_control() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.enabled = true;
        let displays = vec!["d".to_owned()];
        settings.history.undo = Some(snappy("KeyU"));
        let paused = desired_hotkeys(&settings, &displays, false);
        assert!(paused.iter().all(|hotkey| matches!(
            hotkey.action,
            HotKeyAction::PreviousWorkspace
                | HotKeyAction::NextWorkspace
                | HotKeyAction::MoveToPreviousWorkspace
                | HotKeyAction::MoveToNextWorkspace
                | HotKeyAction::SwitchWorkspace(_)
                | HotKeyAction::SendToWorkspace(_)
                | HotKeyAction::SendAndSwitch(_)
        )));
        assert_eq!(paused.len(), 4);
        let live = desired_hotkeys(&settings, &displays, true);
        let zones = live
            .iter()
            .filter(|hotkey| hotkey.action == HotKeyAction::Zone)
            .count();
        assert_eq!(zones, 3);
        assert!(live.iter().any(|hotkey| hotkey.action == HotKeyAction::UndoMove));
    }

    #[test]
    fn move_window_step_chords_check_the_same_rules_as_other_workspace_chords() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.move_next = settings.workspaces.previous.clone();
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + ; is already assigned to Previous workspace.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.workspaces.move_previous = Some(KeyChord::new("KeyQ", [Modifier::Meta]));
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Command + Q is already assigned to a standard app command.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[2].send_shortcut = settings.workspaces.move_next.clone();
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + / is already assigned to Move window to next workspace.")
        );
    }

    #[test]
    fn move_window_step_chords_register_while_workspaces_run() {
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
        assert!(registered.contains(&HotKeyAction::MoveToPreviousWorkspace));
        assert!(registered.contains(&HotKeyAction::MoveToNextWorkspace));
    }

    #[test]
    fn undo_and_redo_chords_conflict_with_other_shortcuts() {
        let mut settings = Settings::defaults(MAC);
        settings.history.undo = Some(snappy("KeyZ"));
        settings.history.redo = Some(snappy("KeyZ"));
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + Z is already assigned to Undo window move.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.history.redo = settings.navigation.left.clone();
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + ← is already assigned to Move window left.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.history.undo = settings.workspaces.next.clone();
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + ' is already assigned to Undo window move.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.history.undo = Some(snappy("KeyU"));
        with_selection(&mut settings, "halves", snappy("KeyU"));
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + U is already assigned to Undo window move.")
        );
    }

    #[test]
    fn the_nearest_empty_zone_chord_registers_only_while_live_control_is_on() {
        let settings = Settings::defaults(MAC);
        let displays = vec!["d".to_owned()];
        let nearest = |live: bool| {
            desired_hotkeys(&settings, &displays, live)
                .into_iter()
                .filter(|hotkey| hotkey.action == HotKeyAction::NearestEmptyZone)
                .map(|hotkey| hotkey.chord.combo())
                .collect::<Vec<_>>()
        };
        assert_eq!(nearest(true), vec![snappy("Space").combo()]);
        assert!(nearest(false).is_empty());
    }

    #[test]
    fn the_nearest_empty_zone_chord_conflicts_with_other_shortcuts() {
        let mut settings = Settings::defaults(MAC);
        settings.navigation.nearest_empty = settings.navigation.up.clone();
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + ↑ is already assigned to Move window up.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.history.redo = Some(snappy("Space"));
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + Space is already assigned to Move to the nearest empty zone.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[0].switch_shortcut = Some(snappy("Space"));
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + Space is already assigned to Move to the nearest empty zone.")
        );
    }

    #[test]
    fn snappy_space_is_not_a_standard_app_command_anywhere() {
        for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
            let settings = Settings::defaults(platform);
            let chord = settings.navigation.nearest_empty.as_ref().unwrap();
            assert!(
                !reserved_app_commands(platform).contains(&chord.combo()),
                "{platform:?}"
            );
            assert_eq!(first_conflict(&settings, platform), None, "{platform:?}");
        }
    }

    #[test]
    fn navigation_defaults_skip_a_nearest_empty_chord_in_use() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[1].send_shortcut = Some(snappy("Space"));
        let navigation = unused_navigation_defaults(&settings);
        assert!(navigation.nearest_empty.is_none());
        assert!(navigation.left.is_some());
    }

    #[test]
    fn navigation_defaults_skip_undo_and_redo_chords() {
        let mut settings = Settings::defaults(MAC);
        settings.history.redo = Navigation::defaults().up;
        let navigation = unused_navigation_defaults(&settings);
        assert!(navigation.up.is_none());
        assert!(navigation.down.is_some());
    }

    #[test]
    fn zone_and_arrow_chords_register_once_without_a_shift_form() {
        let settings = Settings::defaults(MAC);
        let live = desired_hotkeys(&settings, &["d".to_owned()], true);
        let left: Vec<&HotKey> = live
            .iter()
            .filter(|hotkey| hotkey.action == HotKeyAction::Navigate(Direction::Left))
            .collect();
        assert_eq!(left.len(), 1);
        assert_eq!(Some(&left[0].chord), settings.navigation.left.as_ref());
        assert!(
            live.iter()
                .all(|hotkey| !hotkey.chord.combo().mods.contains(Modifier::Shift))
        );
    }

    #[test]
    fn the_master_switch_disables_everything() {
        let mut settings = Settings::defaults(MAC);
        settings.snapping.shortcuts_enabled = false;
        assert!(desired_hotkeys(&settings, &["d".to_owned()], true).is_empty());
    }

    #[test]
    fn navigation_defaults_skip_chords_in_use() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[0].send_and_switch_shortcut = Navigation::defaults().left;
        let navigation = unused_navigation_defaults(&settings);
        assert!(navigation.left.is_none());
        assert!(navigation.right.is_some());
    }
}
