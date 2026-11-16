//! Checking settings as a whole, and normalizing them, before they are used or saved.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    FORMAT_VERSION, LayoutOptions, MAX_CUSTOM_LAYOUTS, MAX_LAYOUT_OPTIONS, MAX_WORKSPACES, Settings,
    SettingsError, Workspace,
};
use crate::keys::{Mods, Os, SnappyKey};
use crate::layout::DEFAULT_LAYOUT_ID;
use crate::shortcuts;

impl Settings {
    /// Check every rule and return the normalized settings, or the first problem.
    pub fn validate(self, platform: Os) -> Result<Settings, SettingsError> {
        self.checked(platform, shortcuts::first_conflict)
    }

    /// These settings with the Snappy key standing for `modifiers`, checked as a whole:
    /// every Snappy key chord moves to the new modifiers, so each must still differ from
    /// every other binding and from the chords the OS and apps keep. Chords stored with
    /// concrete modifiers keep them, even ones the new key's modifiers match, which then
    /// conflict with any Snappy key chord on the same key. Whether the OS leaves the new
    /// chords free is the caller's to probe (`shortcuts::availability_probes`).
    pub fn with_snappy_key(&self, modifiers: Mods, platform: Os) -> Result<Settings, SettingsError> {
        let mut changed = self.clone();
        changed.snappy_key = SnappyKey::new(modifiers)?;
        changed.checked(platform, shortcuts::snappy_key_conflict)
    }

    fn checked(
        mut self,
        platform: Os,
        first_conflict: fn(&Settings, Os) -> Option<String>,
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
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::keys::{KeyChord, Modifier};
    use crate::layout::{self, Layout, UnitRect, Zone};
    use crate::placement::swap::{SwapModifier, WhenTaken};
    use crate::settings::DragModifier;
    use crate::settings::test_support::*;

    fn platform() -> impl Strategy<Value = Os> {
        prop_oneof![Just(Os::MacOs), Just(Os::Windows), Just(Os::Linux)]
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
    fn edited_settings() -> impl Strategy<Value = (Settings, Os)> {
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

        let registered: Vec<_> = shortcuts::desired_hotkeys(&valid, &[], false)
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
}
