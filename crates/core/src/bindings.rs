//! Who owns which shortcut: conflict rules, and the shortcuts to register. Chords are
//! compared by their combos, with the Snappy key expanded to the modifiers the settings
//! choose, so Snappy + 1 and the same key with the Snappy key's modifiers held are one
//! shortcut.

use std::collections::{BTreeMap, BTreeSet};

use crate::keys::{Combo, KeyChord, Platform, SnappyKey, chords_the_system_keeps, reserved_app_commands};
use crate::layout::Layout;
use crate::navigate::Direction;
use crate::settings::{
    FocusShortcut, HistoryStep, LayoutOptions, LayoutShortcut, NavigationShortcut, Settings,
    WorkspaceShortcut, WorkspaceStep, Workspaces,
};

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
    Navigate(Direction),
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
            HotKeyAction::Navigate(Direction::Left) => "moveLeft",
            HotKeyAction::Navigate(Direction::Right) => "moveRight",
            HotKeyAction::Navigate(Direction::Up) => "moveUp",
            HotKeyAction::Navigate(Direction::Down) => "moveDown",
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

    fn of_step(step: WorkspaceStep) -> Self {
        match step {
            WorkspaceStep::Previous => HotKeyAction::PreviousWorkspace,
            WorkspaceStep::Next => HotKeyAction::NextWorkspace,
            WorkspaceStep::SendPrevious => HotKeyAction::SendToPreviousWorkspace,
            WorkspaceStep::SendNext => HotKeyAction::SendToNextWorkspace,
        }
    }

    fn of_navigation(shortcut: NavigationShortcut) -> Self {
        match shortcut {
            NavigationShortcut::Move(direction) => HotKeyAction::Navigate(direction),
            NavigationShortcut::NearestEmpty => HotKeyAction::NearestEmptyZone,
            NavigationShortcut::Focus(FocusShortcut::FocusUp) => HotKeyAction::FocusWindow(Direction::Up),
            NavigationShortcut::Focus(FocusShortcut::FocusLeft) => HotKeyAction::FocusWindow(Direction::Left),
            NavigationShortcut::Focus(FocusShortcut::FocusDown) => HotKeyAction::FocusWindow(Direction::Down),
            NavigationShortcut::Focus(FocusShortcut::FocusRight) => {
                HotKeyAction::FocusWindow(Direction::Right)
            }
            NavigationShortcut::Focus(FocusShortcut::SelectWindow) => HotKeyAction::SelectWindow,
            NavigationShortcut::Focus(FocusShortcut::ClearSelection) => HotKeyAction::ClearSelection,
        }
    }

    fn of_workspace(shortcut: WorkspaceShortcut, workspace: &str) -> Self {
        let workspace = workspace.to_owned();
        match shortcut {
            WorkspaceShortcut::Switch => HotKeyAction::SwitchWorkspace(workspace),
            WorkspaceShortcut::Send => HotKeyAction::SendToWorkspace(workspace),
            WorkspaceShortcut::SendAndSwitch => HotKeyAction::SendAndSwitch(workspace),
        }
    }

    fn of_history(step: HistoryStep) -> Self {
        match step {
            HistoryStep::Undo => HotKeyAction::UndoMove,
            HistoryStep::Redo => HotKeyAction::RedoMove,
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
    let key = settings.snappy_key;
    let mut wanted: Vec<HotKey> = Vec::new();
    let mut add = |chord: &KeyChord, action: HotKeyAction| {
        wanted.push(HotKey {
            chord: chord.clone(),
            combo: chord.combo(key),
            action,
        })
    };

    let workspaces = &settings.workspaces;
    if workspaces.enabled {
        for (step, chord) in workspaces.steps() {
            add(chord, HotKeyAction::of_step(step));
        }
        for item in &workspaces.items {
            for (shortcut, chord) in item.chords() {
                add(chord, HotKeyAction::of_workspace(shortcut, &item.id));
            }
        }
    }

    if snapping_live {
        for (step, chord) in settings.history.chords() {
            add(chord, HotKeyAction::of_history(step));
        }
        for (shortcut, chord) in settings.navigation.chords() {
            add(chord, HotKeyAction::of_navigation(shortcut));
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
        .filter(|hotkey| seen.insert(hotkey.combo.clone()))
        .collect();
    unique.sort_by(|a, b| a.combo.cmp(&b.combo));
    unique
}

/// The first shortcut conflict anywhere in `settings`, as a user-facing message.
pub fn first_conflict(settings: &Settings, platform: Platform) -> Option<String> {
    find_conflict(settings, platform).map(|conflict| conflict.message(platform))
}

/// The first shortcut conflict in `settings` just after their Snappy key changed, named by
/// the shortcut that the change moved onto another's chord.
pub fn snappy_key_conflict(settings: &Settings, platform: Platform) -> Option<String> {
    find_conflict(settings, platform).map(|conflict| conflict.after_key_change(platform))
}

/// The first conflict if `layout` used `options` instead of its stored options. The
/// editor uses this to explain a conflict before saving.
pub fn conflict_with_layout(
    settings: &Settings,
    layout: &Layout,
    options: &LayoutOptions,
    platform: Platform,
) -> Option<String> {
    layout_conflict(settings, Some((layout, options)), platform).map(|conflict| conflict.message(platform))
}

fn find_conflict(settings: &Settings, platform: Platform) -> Option<Conflict> {
    global_conflict(settings, platform)
        .or_else(|| workspace_conflict(settings, platform))
        .or_else(|| layout_conflict(settings, None, platform))
}

/// Who holds a chord already.
#[derive(Debug, Clone)]
enum Owner {
    /// A binding by name, with its chord as stored, or a standard app command, which has
    /// no chord of Snappy's.
    Named { name: String, chord: Option<KeyChord> },
    /// The OS, which never passes the chord on.
    System(&'static str),
}

impl Owner {
    fn binding(name: String, chord: &KeyChord) -> Self {
        Owner::Named {
            name,
            chord: Some(chord.clone()),
        }
    }

    fn app_command() -> Self {
        Owner::Named {
            name: "a standard app command".to_owned(),
            chord: None,
        }
    }
}

/// Two owners for one chord, named in messages by their chords as stored.
#[derive(Debug, Clone)]
enum Conflict {
    /// `claimant` wants `chord`, which `owner` holds.
    Taken {
        chord: KeyChord,
        claimant: String,
        owner: Owner,
    },
    /// A clash within the layouts, explained in full.
    Other(String),
}

impl Conflict {
    fn message(&self, platform: Platform) -> String {
        match self {
            Conflict::Taken {
                chord,
                owner: Owner::Named { name, .. },
                ..
            } => format!("{} is already assigned to {name}.", chord.long_label(platform)),
            Conflict::Taken {
                owner: Owner::System(message),
                ..
            } => (*message).to_owned(),
            Conflict::Other(message) => message.clone(),
        }
    }

    /// The message after a Snappy key change, told from the side the change moved: only
    /// Snappy key chords move, so when the claimant's chord is concrete and the owner's
    /// holds the Snappy key, the owner is the one that landed on the other.
    fn after_key_change(&self, platform: Platform) -> String {
        match self {
            Conflict::Taken {
                chord,
                claimant,
                owner: Owner::System(message),
            } => format!(
                "{} for {claimant} wouldn't work. {message}",
                chord.long_label(platform)
            ),
            Conflict::Taken {
                chord,
                claimant,
                owner:
                    Owner::Named {
                        name,
                        chord: Some(held),
                    },
            } if !chord.mods.has_snappy_key() && held.mods.has_snappy_key() => format!(
                "{} for {name} would collide with {claimant}.",
                held.long_label(platform)
            ),
            Conflict::Taken {
                chord,
                claimant,
                owner: Owner::Named { name, .. },
            } => format!(
                "{} for {claimant} would collide with {name}.",
                chord.long_label(platform)
            ),
            Conflict::Other(message) => message.clone(),
        }
    }
}

fn reserved_owners(platform: Platform) -> BTreeMap<Combo, Owner> {
    let app_commands = reserved_app_commands(platform)
        .into_iter()
        .map(|combo| (combo, Owner::app_command()));
    let kept = chords_the_system_keeps(platform)
        .into_iter()
        .map(|kept| (kept.combo, Owner::System(kept.message)));
    app_commands.chain(kept).collect()
}

/// Navigation and window-history chords must differ from each other and the reserved
/// chords.
fn global_conflict(settings: &Settings, platform: Platform) -> Option<Conflict> {
    let mut owners = reserved_owners(platform);
    claim_all(&mut owners, global_names(settings).into_iter(), settings)
}

/// The chords registered whatever the workspaces do: navigation (focus and selection
/// included), then window history.
fn global_names(settings: &Settings) -> Vec<(String, &KeyChord)> {
    let navigation = settings
        .navigation
        .chords()
        .map(|(shortcut, chord)| (shortcut.title(), chord));
    let history = settings
        .history
        .chords()
        .map(|(step, chord)| (step.title().to_owned(), chord));
    navigation.chain(history).collect()
}

/// The reserved chords and the global chords, by owner.
fn global_owners(settings: &Settings, platform: Platform) -> BTreeMap<Combo, Owner> {
    let mut owners = reserved_owners(platform);
    for (name, chord) in global_names(settings) {
        owners.insert(chord.combo(settings.snappy_key), Owner::binding(name, chord));
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
            item.chords()
                .map(|(shortcut, chord)| (shortcut.title(&item.name), chord)),
        );
    }
    named
}

/// Workspace chords must differ from each other, the reserved chords, and the global
/// chords, whether or not workspaces are on.
fn workspace_conflict(settings: &Settings, platform: Platform) -> Option<Conflict> {
    let mut owners = global_owners(settings, platform);
    claim_all(
        &mut owners,
        workspace_names(&settings.workspaces).into_iter(),
        settings,
    )
}

/// Give each chord to its owner, reporting the first chord someone already owns.
fn claim_all<'a>(
    owners: &mut BTreeMap<Combo, Owner>,
    claims: impl Iterator<Item = (String, &'a KeyChord)>,
    settings: &Settings,
) -> Option<Conflict> {
    for (name, chord) in claims {
        let combo = chord.combo(settings.snappy_key);
        if let Some(owner) = owners.get(&combo) {
            return Some(Conflict::Taken {
                chord: chord.clone(),
                claimant: name,
                owner: owner.clone(),
            });
        }
        owners.insert(combo, Owner::binding(name, chord));
    }
    None
}

/// Zone and selection chords of enabled layouts, with `replacement` standing in for one
/// layout's stored options. Each layout's zones are checked, then its selection shortcut,
/// layout by layout; zones and selections of different layouts can't share a chord either.
fn layout_conflict(
    settings: &Settings,
    replacement: Option<(&Layout, &LayoutOptions)>,
    platform: Platform,
) -> Option<Conflict> {
    let key = settings.snappy_key;
    let taken = layout_owners(settings, platform);
    let layouts = enabled_layouts(settings, replacement);
    let mut selections: BTreeMap<Combo, &str> = BTreeMap::new();
    for (layout, options) in &layouts {
        let conflict = zone_conflict(layout, options, &taken, key, platform)
            .or_else(|| selection_conflict(layout, options, &taken, &mut selections, key, platform));
        if conflict.is_some() {
            return conflict;
        }
    }
    zone_selected(&layouts, &selections, key)
}

/// The chords no layout may use: the reserved and global chords, and the workspaces' while
/// workspaces are on.
fn layout_owners(settings: &Settings, platform: Platform) -> BTreeMap<Combo, Owner> {
    let mut taken = global_owners(settings, platform);
    if settings.workspaces.enabled {
        for (name, chord) in workspace_names(&settings.workspaces) {
            taken
                .entry(chord.combo(settings.snappy_key))
                .or_insert_with(|| Owner::binding(name, chord));
        }
    }
    taken
}

/// Every enabled layout with its options, `replacement` standing in for its layout's.
fn enabled_layouts<'a>(
    settings: &'a Settings,
    replacement: Option<(&'a Layout, &LayoutOptions)>,
) -> Vec<(&'a Layout, LayoutOptions)> {
    let replaced_id = replacement.map(|(layout, _)| layout.id.as_str());
    let mut layouts: Vec<(&Layout, LayoutOptions)> = settings
        .layouts()
        .filter(|layout| Some(layout.id.as_str()) != replaced_id)
        .map(|layout| (layout, settings.options(layout)))
        .collect();
    if let Some((layout, options)) = replacement {
        layouts.push((layout, options.clone()));
    }
    layouts.retain(|(_, options)| options.enabled);
    layouts
}

/// A zone chord `layout` uses twice, or one `taken` already holds.
fn zone_conflict(
    layout: &Layout,
    options: &LayoutOptions,
    taken: &BTreeMap<Combo, Owner>,
    key: SnappyKey,
    platform: Platform,
) -> Option<Conflict> {
    let mut in_layout = BTreeSet::new();
    for (index, zone) in layout.zones.iter().enumerate() {
        let Some(chord) = options.zone_shortcuts.get(&zone.id) else {
            continue;
        };
        if !in_layout.insert(chord.combo(key)) {
            return Some(Conflict::Other(format!(
                "{} uses {} for more than one zone.",
                layout.name,
                chord.long_label(platform)
            )));
        }
        if let Some(owner) = taken.get(&chord.combo(key)) {
            return Some(Conflict::Taken {
                chord: chord.clone(),
                claimant: LayoutShortcut::Zone(index).title(&layout.name),
                owner: owner.clone(),
            });
        }
    }
    None
}

/// A selection shortcut of `layout` that another layout's selection or `taken` already
/// holds. Otherwise it joins `selections`.
fn selection_conflict<'a>(
    layout: &'a Layout,
    options: &LayoutOptions,
    taken: &BTreeMap<Combo, Owner>,
    selections: &mut BTreeMap<Combo, &'a str>,
    key: SnappyKey,
    platform: Platform,
) -> Option<Conflict> {
    let chord = options.select_shortcut.as_ref()?;
    if let Some(owner) = selections.get(&chord.combo(key)) {
        return Some(Conflict::Other(format!(
            "{} already selects {owner}.",
            chord.long_label(platform)
        )));
    }
    if let Some(owner) = taken.get(&chord.combo(key)) {
        return Some(Conflict::Taken {
            chord: chord.clone(),
            claimant: LayoutShortcut::Select.title(&layout.name),
            owner: owner.clone(),
        });
    }
    selections.insert(chord.combo(key), &layout.name);
    None
}

/// A selection chord that some layout also gives a zone.
fn zone_selected(
    layouts: &[(&Layout, LayoutOptions)],
    selections: &BTreeMap<Combo, &str>,
    key: SnappyKey,
) -> Option<Conflict> {
    let zone_chords: BTreeSet<Combo> = layouts
        .iter()
        .flat_map(|(_, options)| options.zone_shortcuts.values().map(|chord| chord.combo(key)))
        .collect();
    zone_chords.iter().find_map(|combo| {
        selections.get(combo).map(|name| {
            Conflict::Other(format!(
                "A shortcut for selecting {name} is also assigned to a zone. Clear one assignment first."
            ))
        })
    })
}

/// Every binding's chord with its name: navigation, focus and selection, and history, then
/// workspaces, then each layout's selection and zones. Disabled layouts and workspaces
/// count, even while off.
fn named_bindings(settings: &Settings) -> Vec<(String, KeyChord)> {
    let mut named: Vec<(String, KeyChord)> = global_names(settings)
        .into_iter()
        .chain(workspace_names(&settings.workspaces))
        .map(|(name, chord)| (name, chord.clone()))
        .collect();
    for layout in settings.layouts() {
        let options = settings.options(layout);
        if let Some(chord) = options.select_shortcut {
            named.push((LayoutShortcut::Select.title(&layout.name), chord));
        }
        for (index, zone) in layout.zones.iter().enumerate() {
            if let Some(chord) = options.zone_shortcuts.get(&zone.id) {
                named.push((LayoutShortcut::Zone(index).title(&layout.name), chord.clone()));
            }
        }
    }
    named
}

/// Every chord a binding uses. Disabled layouts and workspaces count, even while off.
pub fn bound_combos(settings: &Settings) -> BTreeSet<Combo> {
    named_bindings(settings)
        .iter()
        .map(|(_, chord)| chord.combo(settings.snappy_key))
        .collect()
}

/// A chord to ask the OS about before the Snappy key changes, and what to say when the OS
/// or another app already holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub combo: Combo,
    pub refusal: String,
}

/// One probe for each distinct chord that holds the Snappy key in `settings`, which have
/// the new key. Disabled layouts and workspaces count, as they do for conflicts, so turning
/// them on later can't fail on a chord the change took.
pub fn availability_probes(settings: &Settings, platform: Platform) -> Vec<Probe> {
    let key = settings.snappy_key;
    let mut seen = BTreeSet::new();
    named_bindings(settings)
        .into_iter()
        .filter(|(_, chord)| chord.mods.has_snappy_key())
        .filter_map(|(name, chord)| {
            let combo = chord.combo(key);
            if !seen.insert(combo.clone()) {
                return None;
            }
            let held = KeyChord {
                label: chord.label.clone(),
                ..KeyChord::from(combo.clone())
            };
            let refusal = format!(
                "{} for {name} would be {}, which {} or another app already uses.",
                chord.long_label(platform),
                held.long_label(platform),
                platform.system_name()
            );
            Some(Probe { combo, refusal })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Modifier, Mods, SnappyKey};
    use crate::layout::preset;
    use crate::settings::SettingsError;

    const MAC: Platform = Platform::MacOs;

    fn snappy(code: &str) -> KeyChord {
        KeyChord::snappy(code, [])
    }

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
        actions.extend(NavigationShortcut::ALL.map(HotKeyAction::of_navigation));
        actions.extend(WorkspaceStep::ALL.map(HotKeyAction::of_step));
        actions.extend(HistoryStep::ALL.map(HotKeyAction::of_history));
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
            HotKeyAction::of_navigation(FocusShortcut::FocusLeft.into()).name(),
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

    fn with_selection(settings: &mut Settings, layout_id: &str, chord: KeyChord) {
        let layout = settings.layout(layout_id).unwrap().clone();
        let mut options = settings.options(&layout);
        options.select_shortcut = Some(chord);
        settings.layouts.options.insert(layout_id.into(), options);
    }

    fn with_zone(settings: &mut Settings, zone_id: &str, chord: KeyChord) {
        let layout_id = zone_id.rsplit_once('-').map_or(zone_id, |(layout, _)| layout);
        let layout = settings.layout(layout_id).unwrap().clone();
        let mut options = settings.options(&layout);
        options.zone_shortcuts.insert(zone_id.into(), chord);
        settings.layouts.options.insert(layout_id.into(), options);
    }

    #[test]
    fn defaults_have_no_conflicts() {
        for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
            let mut settings = Settings::defaults(platform);
            assert_eq!(first_conflict(&settings, platform), None, "{platform:?}");
            settings.workspaces.enabled = true;
            assert_eq!(first_conflict(&settings, platform), None, "{platform:?}");
        }
    }

    #[test]
    fn zone_chords_are_unique_within_a_layout() {
        let mut settings = Settings::defaults(MAC);
        with_zone(&mut settings, "halves-0", snappy("KeyM"));
        let halves = preset("halves").unwrap();
        let mut options = settings.options(halves);
        options.zone_shortcuts.insert("halves-1".into(), snappy("KeyM"));
        let conflict = conflict_with_layout(&settings, halves, &options, MAC).unwrap();
        assert_eq!(conflict, "Halves uses Snappy + M for more than one zone.");
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
        with_zone(&mut settings, "rows-0", snappy("KeyM"));
        with_selection(&mut settings, "halves", snappy("KeyM"));
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
        settings.workspaces.items[1].switch_shortcut = Some(by_hand.clone());
        settings.workspaces.enabled = true;
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Control + Option + Command + 1 is already assigned to Switch to Workspace 1."),
            "named as recorded"
        );
        let registered: Vec<Combo> = desired_hotkeys(&settings, &["d".to_owned()], true)
            .iter()
            .map(|hotkey| hotkey.combo.clone())
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
        with_selection(&mut settings, "halves", snappy("KeyM"));
        with_zone(&mut settings, "rows-0", snappy("KeyM"));
        assert!(first_conflict(&settings, MAC).is_some());
        for preset in crate::layout::presets() {
            let mut options = settings.options(preset);
            options.enabled = preset.id == "halves";
            settings.layouts.options.insert(preset.id.clone(), options);
        }
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
    fn focus_and_selection_chords_conflict_with_other_shortcuts() {
        let mut settings = Settings::defaults(MAC);
        settings.navigation.focus_up = settings.navigation.up.clone();
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + ↑ is already assigned to Move window up.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.history.undo = Some(snappy("KeyJ"));
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + J is already assigned to Focus left.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[1].send_and_switch_shortcut = settings.navigation.select_window.clone();
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + ↩ is already assigned to Select window.")
        );

        let mut settings = Settings::defaults(MAC);
        with_zone(&mut settings, "halves-0", snappy("Backslash"));
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + \\ is already assigned to Clear selection.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.navigation.clear_selection = settings.navigation.nearest_empty.clone();
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + Space is already assigned to Move to the nearest empty zone.")
        );
    }

    #[test]
    fn macos_refuses_the_snappy_key_with_delete_wherever_it_is_assigned() {
        let refusal = "macOS reserves Control + Option + Command + Delete; it never reaches apps.";
        let delete = snappy("Backspace");

        let mut settings = Settings::defaults(MAC);
        settings.navigation.clear_selection = Some(delete.clone());
        assert_eq!(first_conflict(&settings, MAC).as_deref(), Some(refusal));

        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[0].send_shortcut = Some(delete.clone());
        assert_eq!(first_conflict(&settings, MAC).as_deref(), Some(refusal));

        let mut settings = Settings::defaults(MAC);
        with_zone(&mut settings, "halves-0", delete.clone());
        assert_eq!(first_conflict(&settings, MAC).as_deref(), Some(refusal));

        let mut settings = Settings::defaults(MAC);
        with_zone(
            &mut settings,
            "halves-0",
            KeyChord::snappy("Backspace", [Modifier::Shift]),
        );
        assert_eq!(
            first_conflict(&settings, MAC),
            None,
            "only the exact chord is kept"
        );

        for platform in [Platform::Windows, Platform::Linux] {
            let mut settings = Settings::defaults(platform);
            settings.navigation.clear_selection = Some(delete.clone());
            assert_eq!(first_conflict(&settings, platform), None, "{platform:?}");
        }
    }

    #[test]
    fn send_chords_check_the_same_rules_as_other_workspace_chords() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.send_next = settings.workspaces.previous.clone();
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + ; is already assigned to Previous workspace.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.workspaces.send_previous = Some(KeyChord::new("KeyQ", [Modifier::Meta]));
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Command + Q is already assigned to a standard app command.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[2].send_shortcut = settings.workspaces.send_next.clone();
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + / is already assigned to Send window to next workspace.")
        );

        let mut settings = Settings::defaults(MAC);
        settings.workspaces.send_previous = settings.navigation.nearest_empty.clone();
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + Space is already assigned to Move to the nearest empty zone.")
        );
    }

    #[test]
    fn workspace_switches_by_number_clash_with_zone_digits_only_while_workspaces_are_on() {
        let mut settings = Settings::defaults(MAC);
        with_zone(&mut settings, "halves-0", snappy("Digit1"));
        assert_eq!(first_conflict(&settings, MAC), None);
        settings.workspaces.enabled = true;
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + 1 is already assigned to Switch to Workspace 1.")
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
                .map(|hotkey| hotkey.combo.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(nearest(true), vec![snappy("Space").combo(SnappyKey::DEFAULT)]);
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
                !reserved_app_commands(platform).contains(&chord.combo(SnappyKey::DEFAULT)),
                "{platform:?}"
            );
            assert_eq!(first_conflict(&settings, platform), None, "{platform:?}");
        }
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
                .all(|hotkey| !hotkey.combo.clone().mods.contains(Modifier::Shift))
        );
    }

    #[test]
    fn the_master_switch_disables_everything() {
        let mut settings = Settings::defaults(MAC);
        settings.snapping.shortcuts_enabled = false;
        assert!(desired_hotkeys(&settings, &["d".to_owned()], true).is_empty());
    }

    fn modifiers(held: &[Modifier]) -> Mods {
        held.iter().copied().collect()
    }

    const CONTROL_OPTION: [Modifier; 2] = [Modifier::Ctrl, Modifier::Alt];

    #[test]
    fn a_snappy_key_change_onto_another_binding_is_refused_naming_both() {
        let mut settings = Settings::defaults(MAC);
        settings.history.undo = Some(KeyChord::new("KeyJ", CONTROL_OPTION));
        assert_eq!(first_conflict(&settings, MAC), None);
        assert_eq!(
            settings.with_snappy_key(modifiers(&CONTROL_OPTION), MAC),
            Err(SettingsError::Conflict(
                "Snappy + J for Focus left would collide with Undo window move.".to_owned()
            )),
            "the Snappy key chord moved onto the concrete one"
        );

        let mut settings = Settings::defaults(MAC);
        settings.navigation.left = Some(KeyChord::new("KeyU", CONTROL_OPTION));
        settings.history.undo = Some(snappy("KeyU"));
        assert_eq!(
            settings.with_snappy_key(modifiers(&CONTROL_OPTION), MAC),
            Err(SettingsError::Conflict(
                "Snappy + U for Undo window move would collide with Move window left.".to_owned()
            )),
            "whichever comes first, the message starts from the chord that moved"
        );
    }

    #[test]
    fn a_snappy_key_change_onto_a_standard_app_command_is_refused() {
        let mut settings = Settings::defaults(MAC);
        settings.history.undo = Some(snappy("KeyZ"));
        assert_eq!(
            settings.with_snappy_key(modifiers(&[Modifier::Shift, Modifier::Meta]), MAC),
            Err(SettingsError::Conflict(
                "Snappy + Z for Undo window move would collide with a standard app command.".to_owned()
            ))
        );
        assert!(
            settings
                .with_snappy_key(modifiers(&[Modifier::Shift, Modifier::Meta]), Platform::Windows)
                .is_ok(),
            "Windows keeps Ctrl+Z, not Shift+Win+Z"
        );
    }

    #[test]
    fn a_snappy_key_change_onto_a_chord_macos_keeps_is_refused() {
        let mut settings = Settings::defaults(MAC)
            .with_snappy_key(modifiers(&CONTROL_OPTION), MAC)
            .unwrap();
        settings.navigation.clear_selection = Some(snappy("Backspace"));
        let settings = settings.validate(MAC).unwrap();
        assert_eq!(
            settings.with_snappy_key(SnappyKey::DEFAULT.modifiers(), MAC),
            Err(SettingsError::Conflict(
                "Snappy + ⌫ for Clear selection wouldn't work. macOS reserves Control + Option + Command + Delete; it never reaches apps.".to_owned()
            ))
        );
    }

    #[test]
    fn a_snappy_key_change_that_doubles_a_zone_chord_is_refused() {
        let mut settings = Settings::defaults(MAC);
        with_zone(&mut settings, "halves-0", KeyChord::new("KeyM", CONTROL_OPTION));
        with_zone(&mut settings, "halves-1", snappy("KeyM"));
        assert_eq!(first_conflict(&settings, MAC), None);
        assert_eq!(
            settings.with_snappy_key(modifiers(&CONTROL_OPTION), MAC),
            Err(SettingsError::Conflict(
                "Halves uses Snappy + M for more than one zone.".to_owned()
            ))
        );
    }

    #[test]
    fn the_snappy_key_needs_two_modifiers_to_change() {
        let settings = Settings::defaults(MAC);
        let refused = settings.with_snappy_key(modifiers(&[Modifier::Meta]), MAC);
        assert_eq!(
            refused,
            Err(SettingsError::SnappyKey(
                crate::keys::SnappyKeyError::TooFewModifiers
            ))
        );
        assert_eq!(
            refused.unwrap_err().to_string(),
            "The Snappy key needs at least two modifiers."
        );
    }

    #[test]
    fn conflicts_after_a_snappy_key_change_compare_the_new_modifiers() {
        let settings = Settings::defaults(MAC)
            .with_snappy_key(modifiers(&CONTROL_OPTION), MAC)
            .unwrap();
        let mut recorded = settings.clone();
        recorded.history.redo = Some(KeyChord::new("KeyJ", CONTROL_OPTION));
        assert_eq!(
            recorded.validate(MAC),
            Err(SettingsError::Conflict(
                "Control + Option + J is already assigned to Focus left.".to_owned()
            ))
        );
        let mut old_key = settings.clone();
        let by_hand = KeyChord::new("KeyJ", [Modifier::Ctrl, Modifier::Alt, Modifier::Meta]);
        old_key.history.redo = Some(by_hand.clone());
        let valid = old_key.validate(MAC).unwrap();
        assert_eq!(
            valid.history.redo,
            Some(by_hand),
            "the old key's modifiers are other keys now, kept as given"
        );
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
            HotKeyAction::Navigate(Direction::Left)
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

    #[test]
    fn availability_probes_ask_once_for_each_snappy_key_chord_and_say_what_it_would_be() {
        let mut settings = Settings::defaults(MAC)
            .with_snappy_key(modifiers(&CONTROL_OPTION), MAC)
            .unwrap();
        settings.history.undo = Some(KeyChord::new("KeyU", [Modifier::Ctrl]));
        settings.history.redo = Some(snappy("KeyR"));
        let probes = availability_probes(&settings, MAC);
        assert!(
            probes
                .iter()
                .all(|probe| probe.combo.mods == modifiers(&CONTROL_OPTION))
        );
        assert_eq!(
            probes.len(),
            bound_combos(&settings).len() - 1,
            "all but the concrete undo"
        );
        let space = probes
            .iter()
            .find(|probe| probe.combo.code.as_str() == "Space")
            .unwrap();
        assert_eq!(
            space.refusal,
            "Snappy + Space for Move to the nearest empty zone would be Control + Option + Space, which macOS or another app already uses."
        );
        let redo = probes
            .iter()
            .find(|probe| probe.combo.code.as_str() == "KeyR")
            .unwrap();
        assert_eq!(
            redo.refusal,
            "Snappy + R for Redo window move would be Control + Option + R, which macOS or another app already uses."
        );
    }
}
