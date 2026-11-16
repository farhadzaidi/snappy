//! Reading and writing the settings file, including files saved by earlier versions.

use serde::Deserialize;

use super::{
    FORMAT_VERSION, Layouts, MAX_FILE_BYTES, MoveHistory, Navigation, NavigationShortcut, Settings,
    SettingsError, Snapping, WorkspaceShortcut, WorkspaceStep, Workspaces,
};
use crate::keys::{ChordMods, Combo, KeyChord, Os, SnappyKey, chords_the_system_keeps};
use crate::shortcuts::{self, ShortcutId};

impl Settings {
    /// Parse a settings file. Fields missing from the file take their defaults, except
    /// that a default shortcut is left unassigned when another binding the file has already
    /// uses its chord. Shortcuts the file has, cleared ones included, are kept as written,
    /// except chords the OS keeps for itself, which read as missing, and the chords of a
    /// file from before the Snappy key could be chosen (`read_as_before_the_snappy_key`).
    /// Defaults and kept chords compare chords with the file's Snappy key, so it is read
    /// first.
    pub fn from_json(bytes: &[u8], platform: Os) -> Result<Settings, SettingsError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Modifier, Mods};
    use crate::layout;
    use crate::placement::navigate::Direction;
    use crate::placement::swap::WhenTaken;
    use crate::settings::LayoutOptions;
    use crate::settings::test_support::*;

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
        for platform in [Os::Windows, Os::Linux] {
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
        let mut written = serde_json::to_value(Settings::defaults(Os::Windows)).unwrap();
        written["snappyKey"] = serde_json::json!(["alt", "ctrl"]);
        written["history"] = serde_json::json!({
            "undo": { "code": "KeyU", "mods": ["shift", "ctrl", "alt"] },
            "redo": { "code": "KeyR", "mods": ["snappy", "ctrl"] },
        });
        let loaded = Settings::from_json(&serde_json::to_vec(&written).unwrap(), Os::Windows).unwrap();
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
        assert_eq!(Settings::from_json(&saved, Os::Windows).unwrap(), loaded);
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
}
