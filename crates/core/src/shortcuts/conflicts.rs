//! Who owns which shortcut: conflict rules, and the shortcuts to register. Chords are
//! compared by their combos, with the Snappy key expanded to the modifiers the settings
//! choose, so Snappy + 1 and the same key with the Snappy key's modifiers held are one
//! shortcut.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{Bound, NamedShortcut, ShortcutId, bound_global, bound_select, bound_workspace, bound_zones};
use crate::keys::{Combo, KeyChord, Os, SnappyKey, chords_the_system_keeps, reserved_app_commands};
use crate::layout::Layout;
use crate::settings::{LayoutOptions, Settings};

/// The first shortcut conflict anywhere in `settings`, as a user-facing message.
pub fn first_conflict(settings: &Settings, platform: Os) -> Option<String> {
    find_conflict(settings, platform).map(|conflict| conflict.message(platform))
}

/// The first shortcut conflict in `settings` just after their Snappy key changed, named by
/// the shortcut that the change moved onto another's chord.
pub fn snappy_key_conflict(settings: &Settings, platform: Os) -> Option<String> {
    find_conflict(settings, platform).map(|conflict| conflict.after_key_change(platform))
}

/// Why a change was refused, as the settings app shows it: the message, and the shortcut the
/// message names, when it names one of Snappy's, so the app can point to that shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    pub message: String,
    pub shortcut: Option<NamedShortcut>,
}

/// A refusal that names no shortcut.
impl From<String> for Refusal {
    fn from(message: String) -> Self {
        Self {
            message,
            shortcut: None,
        }
    }
}

/// The first shortcut conflict in `settings`, told from the side of `changed`, the shortcut
/// just set: when `changed` is the one found holding the chord first, the message names the
/// other, so a recorder never says its chord is already assigned to itself.
pub fn conflict_for(settings: &Settings, changed: &ShortcutId, platform: Os) -> Option<Refusal> {
    find_conflict(settings, platform).map(|conflict| {
        let conflict = conflict.seen_from(changed);
        Refusal {
            message: conflict.message(platform),
            shortcut: conflict.holder().map(NamedShortcut::from),
        }
    })
}

/// The conflict that giving `shortcut` the chord `chord` would create, told from its side:
/// what a recorder asks before it records.
pub fn recording_conflict(
    settings: &Settings,
    shortcut: &ShortcutId,
    chord: KeyChord,
    platform: Os,
) -> Option<Refusal> {
    let mut recorded = settings.clone();
    shortcut.set(&mut recorded, Some(chord));
    conflict_for(&recorded, shortcut, platform)
}

/// The first conflict if `layout` used `options` instead of its stored options. The
/// editor uses this to explain a conflict before saving.
pub fn conflict_with_layout(
    settings: &Settings,
    layout: &Layout,
    options: &LayoutOptions,
    platform: Os,
) -> Option<String> {
    layout_conflict(settings, Some((layout, options)), platform).map(|conflict| conflict.message(platform))
}

fn find_conflict(settings: &Settings, platform: Os) -> Option<Conflict> {
    global_conflict(settings, platform)
        .or_else(|| workspace_conflict(settings, platform))
        .or_else(|| layout_conflict(settings, None, platform))
}

/// Who holds a chord already.
#[derive(Debug, Clone)]
enum Owner {
    /// One of Snappy's shortcuts.
    Shortcut(Box<Bound>),
    /// A standard app command, which Snappy leaves to apps.
    AppCommand,
    /// The OS, which never passes the chord on.
    System(&'static str),
}

impl Owner {
    fn name(&self) -> &str {
        match self {
            Owner::Shortcut(bound) => &bound.title,
            Owner::AppCommand => "a standard app command",
            Owner::System(_) => "the system",
        }
    }
}

/// Two owners for one chord, named in messages by their chords as stored.
#[derive(Debug, Clone)]
enum Conflict {
    /// `claimant` wants its chord, which `owner` holds.
    Taken { claimant: Box<Bound>, owner: Owner },
    /// A clash within the layouts, explained in full.
    Other(String),
}

impl Conflict {
    fn message(&self, platform: Os) -> String {
        match self {
            Conflict::Taken {
                owner: Owner::System(message),
                ..
            } => (*message).to_owned(),
            Conflict::Taken { claimant, owner } => format!(
                "{} is already assigned to {}.",
                claimant.chord.long_label(platform),
                owner.name()
            ),
            Conflict::Other(message) => message.clone(),
        }
    }

    /// The Snappy shortcut holding the chord. Standard app commands, the system, and clashes
    /// within layouts have none.
    fn holder(&self) -> Option<&Bound> {
        match self {
            Conflict::Taken {
                owner: Owner::Shortcut(owner),
                ..
            } => Some(owner),
            Conflict::Taken { .. } | Conflict::Other(_) => None,
        }
    }

    /// The same conflict with `changed` as the claimant, when it is the owner.
    fn seen_from(self, changed: &ShortcutId) -> Conflict {
        match self {
            Conflict::Taken {
                claimant,
                owner: Owner::Shortcut(owner),
            } if owner.id == *changed => Conflict::Taken {
                claimant: owner,
                owner: Owner::Shortcut(claimant),
            },
            other => other,
        }
    }

    /// The message after a Snappy key change, told from the side the change moved: only
    /// Snappy key chords move, so when the claimant's chord is concrete and the owner's
    /// holds the Snappy key, the owner is the one that landed on the other.
    fn after_key_change(&self, platform: Os) -> String {
        match self {
            Conflict::Taken {
                claimant,
                owner: Owner::System(message),
            } => format!(
                "{} ({}) wouldn't work. {message}",
                claimant.chord.long_label(platform),
                claimant.title
            ),
            Conflict::Taken {
                claimant,
                owner: Owner::Shortcut(owner),
            } if !claimant.chord.mods.has_snappy_key() && owner.chord.mods.has_snappy_key() => format!(
                "{} ({}) would clash with {}.",
                owner.chord.long_label(platform),
                owner.title,
                claimant.title
            ),
            Conflict::Taken { claimant, owner } => format!(
                "{} ({}) would clash with {}.",
                claimant.chord.long_label(platform),
                claimant.title,
                owner.name()
            ),
            Conflict::Other(message) => message.clone(),
        }
    }
}

fn reserved_owners(platform: Os) -> BTreeMap<Combo, Owner> {
    let app_commands = reserved_app_commands(platform)
        .into_iter()
        .map(|combo| (combo, Owner::AppCommand));
    let kept = chords_the_system_keeps(platform)
        .into_iter()
        .map(|kept| (kept.combo, Owner::System(kept.message)));
    app_commands.chain(kept).collect()
}

/// Navigation and window-history chords must differ from each other and the reserved
/// chords.
fn global_conflict(settings: &Settings, platform: Os) -> Option<Conflict> {
    let mut owners = reserved_owners(platform);
    claim_all(&mut owners, bound_global(settings), settings.snappy_key)
}

/// The reserved chords and the global chords, by owner.
fn global_owners(settings: &Settings, platform: Os) -> BTreeMap<Combo, Owner> {
    let mut owners = reserved_owners(platform);
    for bound in bound_global(settings) {
        owners.insert(
            bound.chord.combo(settings.snappy_key),
            Owner::Shortcut(Box::new(bound)),
        );
    }
    owners
}

/// Workspace chords must differ from each other, the reserved chords, and the global
/// chords, whether or not workspaces are on.
fn workspace_conflict(settings: &Settings, platform: Os) -> Option<Conflict> {
    let mut owners = global_owners(settings, platform);
    claim_all(
        &mut owners,
        bound_workspace(&settings.workspaces),
        settings.snappy_key,
    )
}

/// Give each chord to its owner, reporting the first chord someone already owns.
fn claim_all(owners: &mut BTreeMap<Combo, Owner>, claims: Vec<Bound>, key: SnappyKey) -> Option<Conflict> {
    for claimant in claims {
        let combo = claimant.chord.combo(key);
        if let Some(owner) = owners.get(&combo) {
            return Some(Conflict::Taken {
                claimant: Box::new(claimant),
                owner: owner.clone(),
            });
        }
        owners.insert(combo, Owner::Shortcut(Box::new(claimant)));
    }
    None
}

/// Zone and selection chords of enabled layouts, with `replacement` standing in for one
/// layout's stored options. Each layout's zones are checked, then its selection shortcut,
/// layout by layout; zones and selections of different layouts can't share a chord either.
fn layout_conflict(
    settings: &Settings,
    replacement: Option<(&Layout, &LayoutOptions)>,
    platform: Os,
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
fn layout_owners(settings: &Settings, platform: Os) -> BTreeMap<Combo, Owner> {
    let mut taken = global_owners(settings, platform);
    if settings.workspaces.enabled {
        for bound in bound_workspace(&settings.workspaces) {
            taken
                .entry(bound.chord.combo(settings.snappy_key))
                .or_insert_with(|| Owner::Shortcut(Box::new(bound)));
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
    platform: Os,
) -> Option<Conflict> {
    let mut in_layout = BTreeSet::new();
    for zone in bound_zones(layout, options) {
        let combo = zone.chord.combo(key);
        if !in_layout.insert(combo.clone()) {
            return Some(Conflict::Other(format!(
                "{} uses {} for more than one zone.",
                layout.name,
                zone.chord.long_label(platform)
            )));
        }
        if let Some(owner) = taken.get(&combo) {
            return Some(Conflict::Taken {
                claimant: Box::new(zone),
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
    platform: Os,
) -> Option<Conflict> {
    let select = bound_select(layout, options)?;
    let combo = select.chord.combo(key);
    if let Some(owner) = selections.get(&combo) {
        return Some(Conflict::Other(format!(
            "{} already selects {owner}.",
            select.chord.long_label(platform)
        )));
    }
    if let Some(owner) = taken.get(&combo) {
        return Some(Conflict::Taken {
            claimant: Box::new(select),
            owner: owner.clone(),
        });
    }
    selections.insert(combo, &layout.name);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Modifier, SnappyKey};
    use crate::layout::preset;
    use crate::settings::{SettingsError, WorkspaceShortcut};
    use crate::shortcuts::desired_hotkeys;
    use crate::shortcuts::test_support::*;

    #[test]
    fn recording_a_chord_another_shortcut_holds_names_that_shortcut() {
        let settings = Settings::defaults(MAC);
        assert_eq!(
            recording_conflict(&settings, &left(), snappy("ArrowRight"), MAC),
            Some(Refusal {
                message: "Snappy + → is already assigned to Move window right.".into(),
                shortcut: named(right(), "Move window right"),
            })
        );
        assert_eq!(recording_conflict(&settings, &left(), snappy("KeyM"), MAC), None);
    }

    #[test]
    fn a_conflict_for_the_changed_shortcut_names_the_other_one() {
        let mut settings = Settings::defaults(MAC);
        settings.navigation.left = Some(snappy("ArrowRight"));
        assert_eq!(
            first_conflict(&settings, MAC).as_deref(),
            Some("Snappy + → is already assigned to Move window left.")
        );
        assert_eq!(
            conflict_for(&settings, &left(), MAC),
            Some(Refusal {
                message: "Snappy + → is already assigned to Move window right.".into(),
                shortcut: named(right(), "Move window right"),
            })
        );
        assert_eq!(
            conflict_for(&settings, &right(), MAC),
            Some(Refusal {
                message: "Snappy + → is already assigned to Move window left.".into(),
                shortcut: named(left(), "Move window left"),
            })
        );
    }

    #[test]
    fn both_sides_are_known_by_identity_even_when_two_workspaces_share_a_name() {
        let mut settings = Settings::defaults(MAC);
        for item in &mut settings.workspaces.items {
            item.name = "Work".to_owned();
        }
        let by_hand = KeyChord::new("Digit1", [Modifier::Ctrl, Modifier::Alt, Modifier::Meta]);
        settings.workspaces.items[1].switch_shortcut = Some(by_hand);
        let switch = |workspace: &str| ShortcutId::workspace(workspace, WorkspaceShortcut::Switch);
        assert_eq!(
            conflict_for(&settings, &switch("workspace-2"), MAC),
            Some(Refusal {
                message: "Control ⌃ + Option ⌥ + Command ⌘ + 1 is already assigned to Switch to Work.".into(),
                shortcut: named(switch("workspace-1"), "Switch to Work"),
            })
        );
        assert_eq!(
            conflict_for(&settings, &switch("workspace-1"), MAC),
            Some(Refusal {
                message: "Snappy + 1 is already assigned to Switch to Work.".into(),
                shortcut: named(switch("workspace-2"), "Switch to Work"),
            })
        );
    }

    #[test]
    fn a_conflict_names_the_zone_of_its_own_layout_when_two_layouts_share_a_name() {
        let mut settings = Settings::defaults(MAC);
        let mut copy = preset("halves").unwrap().clone();
        copy.id = "my-halves".to_owned();
        settings.layouts.custom.push(copy);
        let zone = |layout: &str| ShortcutId::Zone {
            layout: layout.to_owned(),
            zone: "halves-0".to_owned(),
        };
        zone("halves").set(&mut settings, Some(snappy("KeyM")));
        zone("my-halves").set(&mut settings, Some(snappy("KeyN")));
        for (code, layout) in [("KeyM", "halves"), ("KeyN", "my-halves")] {
            let refusal = recording_conflict(&settings, &left(), snappy(code), MAC).unwrap();
            assert_eq!(
                refusal.shortcut,
                named(zone(layout), "Zone 1 of Halves"),
                "{code}"
            );
        }
    }

    #[test]
    fn a_recorded_conflict_with_an_app_command_names_no_holder() {
        let mut settings = Settings::defaults(MAC);
        settings.navigation.left = Some(KeyChord::new("KeyQ", [Modifier::Meta]));
        assert_eq!(
            conflict_for(&settings, &left(), MAC),
            Some(Refusal {
                message: "Command ⌘ + Q is already assigned to a standard app command.".into(),
                shortcut: None,
            })
        );
    }

    #[test]
    fn defaults_have_no_conflicts() {
        for platform in [Os::MacOs, Os::Windows, Os::Linux] {
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
            "Control ⌃ + Option ⌥ + L already selects Halves."
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
            Some("Control ⌃ + Option ⌥ + Command ⌘ + 1 is already assigned to Switch to Workspace 1."),
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
            "Command ⌘ + Q is already assigned to a standard app command."
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
            Some("Control ⌃ + 1 is already assigned to Switch to Workspace 1.")
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
        let refusal = "macOS reserves Control ⌃ + Option ⌥ + Command ⌘ + Delete; it never reaches apps.";
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

        for platform in [Os::Windows, Os::Linux] {
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
            Some("Command ⌘ + Q is already assigned to a standard app command.")
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
        for platform in [Os::MacOs, Os::Windows, Os::Linux] {
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
    fn a_snappy_key_change_onto_another_binding_is_refused_naming_both() {
        let mut settings = Settings::defaults(MAC);
        settings.history.undo = Some(KeyChord::new("KeyJ", CONTROL_OPTION));
        assert_eq!(first_conflict(&settings, MAC), None);
        assert_eq!(
            settings.with_snappy_key(modifiers(&CONTROL_OPTION), MAC),
            Err(SettingsError::Conflict(
                "Snappy + J (Focus left) would clash with Undo window move.".to_owned()
            )),
            "the Snappy key chord moved onto the concrete one"
        );

        let mut settings = Settings::defaults(MAC);
        settings.navigation.left = Some(KeyChord::new("KeyU", CONTROL_OPTION));
        settings.history.undo = Some(snappy("KeyU"));
        assert_eq!(
            settings.with_snappy_key(modifiers(&CONTROL_OPTION), MAC),
            Err(SettingsError::Conflict(
                "Snappy + U (Undo window move) would clash with Move window left.".to_owned()
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
                "Snappy + Z (Undo window move) would clash with a standard app command.".to_owned()
            ))
        );
        assert!(
            settings
                .with_snappy_key(modifiers(&[Modifier::Shift, Modifier::Meta]), Os::Windows)
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
                "Snappy + ⌫ (Clear selection) wouldn't work. macOS reserves Control ⌃ + Option ⌥ + Command ⌘ + Delete; it never reaches apps.".to_owned()
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
                "Control ⌃ + Option ⌥ + J is already assigned to Focus left.".to_owned()
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
}
