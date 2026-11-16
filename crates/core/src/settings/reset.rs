//! Putting settings back to their defaults: one section's shortcuts, one workspace's, every
//! preference and shortcut while keeping the user's layouts and workspaces, or everything.
//!
//! A reset never creates a conflict: a default whose chord another binding the reset leaves
//! alone already uses stays unassigned, as a settings file without that shortcut loads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::keys::Os;
use crate::settings::{Settings, numbered_switch_shortcut};
use crate::shortcuts::{ShortcutId, ShortcutSection, give_defaults};

/// What to put back to its defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "camelCase")]
pub enum Reset {
    /// One section's shortcuts.
    Shortcuts { section: ShortcutSection },
    /// One workspace's own shortcuts, by its ID.
    WorkspaceShortcuts { workspace: String },
    /// Every preference and shortcut, layouts' and workspaces' included. Custom layouts,
    /// workspaces, and which layout each display and workspace uses stay.
    Settings,
    /// Everything, custom layouts and workspaces too, as on a first launch. Whoever applies it
    /// turns workspaces off first, bringing their windows back.
    Everything,
}

impl Reset {
    /// What to say once `before` has been reset.
    pub fn message(&self, before: &Settings) -> String {
        match self {
            Reset::Shortcuts { section } => format!("{} shortcuts reset.", section.title()),
            Reset::WorkspaceShortcuts { workspace } => {
                let name = before
                    .workspaces
                    .item(workspace)
                    .map_or("the workspace", |item| &item.name);
                format!("Shortcuts for {name} reset.")
            }
            Reset::Settings => "Settings and shortcuts reset.".to_owned(),
            Reset::Everything => "Snappy is reset. Custom layouts and workspaces were removed.".to_owned(),
        }
    }
}

impl Settings {
    /// These settings with `reset` applied. Whether snapping is on stays as it is, and so do
    /// workspaces unless everything is reset.
    pub fn reset(&self, reset: &Reset, platform: Os) -> Settings {
        match reset {
            Reset::Shortcuts { section } => self.with_default_shortcuts(*section),
            Reset::WorkspaceShortcuts { workspace } => self.with_default_workspace_shortcuts(workspace),
            Reset::Settings => self.with_defaults_keeping_layouts_and_workspaces(platform),
            Reset::Everything => {
                let mut reset = Settings::defaults(platform);
                reset.snapping.enabled = self.snapping.enabled;
                reset
            }
        }
    }

    fn with_default_shortcuts(&self, section: ShortcutSection) -> Settings {
        let mut reset = self.clone();
        give_defaults(&mut reset, &section.shortcuts());
        reset
    }

    /// A workspace's shortcuts as a new one in its place gets them: the Snappy key and its
    /// position to switch, for 1 to 9, and nothing to send.
    fn with_default_workspace_shortcuts(&self, workspace: &str) -> Settings {
        let mut reset = self.clone();
        let shortcuts: Vec<ShortcutId> = ShortcutId::of_workspace(workspace).collect();
        give_defaults(&mut reset, &shortcuts);
        reset
    }

    fn with_defaults_keeping_layouts_and_workspaces(&self, platform: Os) -> Settings {
        let mut reset = Settings::defaults(platform);
        reset.snapping.enabled = self.snapping.enabled;
        reset.layouts.default_layout_id = self.layouts.default_layout_id.clone();
        reset.layouts.displays = self.layouts.displays.clone();
        reset.layouts.custom = self.layouts.custom.clone();
        reset.layouts.options = BTreeMap::new();
        reset.workspaces.enabled = self.workspaces.enabled;
        reset.workspaces.active_id = self.workspaces.active_id.clone();
        reset.workspaces.items = self
            .workspaces
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let mut item = item.clone();
                item.switch_shortcut = numbered_switch_shortcut(index + 1);
                item.send_shortcut = None;
                item.send_and_switch_shortcut = None;
                item
            })
            .collect();
        reset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{KeyChord, Modifier};
    use crate::layout::{Layout, preset};
    use crate::settings::{LayoutOptions, Workspace};

    const MAC: Os = Os::MacOs;

    fn snappy(code: &str) -> KeyChord {
        KeyChord::snappy(code, [])
    }

    fn valid(settings: Settings) -> Settings {
        settings.validate(MAC).unwrap()
    }

    fn reset(settings: &Settings, reset: Reset) -> Settings {
        valid(settings.reset(&reset, MAC))
    }

    fn shortcuts(section: ShortcutSection) -> Reset {
        Reset::Shortcuts { section }
    }

    #[test]
    fn a_section_gets_its_defaults_back_and_the_others_keep_theirs() {
        let mut settings = valid(Settings::defaults(MAC));
        settings.navigation.left = Some(KeyChord::new("KeyA", [Modifier::Ctrl, Modifier::Alt]));
        settings.navigation.nearest_empty = None;
        settings.navigation.focus_left = Some(snappy("KeyH"));
        let settings = valid(settings);

        let moved = reset(&settings, shortcuts(ShortcutSection::MoveBetweenZones));
        let defaults = Settings::defaults(MAC);
        assert_eq!(moved.navigation.left, defaults.navigation.left);
        assert_eq!(moved.navigation.nearest_empty, defaults.navigation.nearest_empty);
        assert_eq!(moved.navigation.focus_left, Some(snappy("KeyH")));

        let focused = reset(&settings, shortcuts(ShortcutSection::FocusAndSelection));
        assert_eq!(focused.navigation.focus_left, defaults.navigation.focus_left);
        assert_eq!(focused.navigation.left, settings.navigation.left);
    }

    #[test]
    fn a_default_another_binding_holds_stays_unassigned() {
        let mut settings = Settings::defaults(MAC);
        settings.navigation.focus_left = None;
        settings.history.undo = Some(snappy("KeyJ"));
        let settings = valid(settings);

        let focused = reset(&settings, shortcuts(ShortcutSection::FocusAndSelection));
        assert_eq!(focused.navigation.focus_left, None, "Undo keeps Snappy + J");
        assert_eq!(focused.history.undo, Some(snappy("KeyJ")));
        assert_eq!(focused.navigation.focus_right, Some(snappy("KeyL")));
    }

    #[test]
    fn a_section_whose_defaults_are_none_is_cleared() {
        let mut settings = Settings::defaults(MAC);
        settings.history.undo = Some(snappy("KeyZ"));
        settings.workspaces.next = Some(snappy("KeyN"));
        let settings = valid(settings);

        let history = reset(&settings, shortcuts(ShortcutSection::WindowHistory));
        assert_eq!(history.history.undo, None);
        let steps = reset(&settings, shortcuts(ShortcutSection::WorkspaceSteps));
        assert_eq!(steps.workspaces.next, Settings::defaults(MAC).workspaces.next);
    }

    #[test]
    fn resetting_what_is_already_default_changes_nothing() {
        let settings = valid(Settings::defaults(MAC));
        for section in [
            ShortcutSection::MoveBetweenZones,
            ShortcutSection::FocusAndSelection,
            ShortcutSection::WorkspaceSteps,
            ShortcutSection::WindowHistory,
        ] {
            assert_eq!(reset(&settings, shortcuts(section)), settings, "{section:?}");
        }
        assert_eq!(reset(&settings, Reset::Settings), settings);
        assert_eq!(reset(&settings, Reset::Everything), settings);
    }

    #[test]
    fn a_workspace_gets_its_position_to_switch_and_nothing_to_send() {
        let mut settings = Settings::defaults(MAC);
        let second = &mut settings.workspaces.items[1];
        second.switch_shortcut = Some(KeyChord::new("Digit2", [Modifier::Ctrl]));
        second.send_shortcut = Some(KeyChord::new("Digit2", [Modifier::Ctrl, Modifier::Shift]));
        let settings = valid(settings);

        let reset = reset(
            &settings,
            Reset::WorkspaceShortcuts {
                workspace: "workspace-2".to_owned(),
            },
        );
        let second = &reset.workspaces.items[1];
        assert_eq!(second.switch_shortcut, Some(snappy("Digit2").with_label("2")));
        assert_eq!(second.send_shortcut, None);
        assert_eq!(reset.workspaces.items[0], settings.workspaces.items[0]);
    }

    #[test]
    fn a_workspace_whose_number_another_binding_holds_gets_no_switch_shortcut() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[1].switch_shortcut = None;
        settings.history.redo = Some(snappy("Digit2").with_label("2"));
        let settings = valid(settings);
        let reset = reset(
            &settings,
            Reset::WorkspaceShortcuts {
                workspace: "workspace-2".to_owned(),
            },
        );
        assert_eq!(reset.workspaces.items[1].switch_shortcut, None);
    }

    fn customized() -> Settings {
        let mut settings = Settings::defaults(MAC);
        settings.snapping.enabled = false;
        settings.snappy_key =
            crate::keys::SnappyKey::new([Modifier::Ctrl, Modifier::Alt].into_iter().collect()).unwrap();
        settings.caps_lock_as_snappy_key = true;
        settings.debug_log = true;
        settings.history.undo = Some(snappy("KeyU"));
        let mut mine: Layout = preset("halves").unwrap().clone();
        mine.id = "custom-mine".to_owned();
        mine.name = "Mine".to_owned();
        let zone = mine.zones[0].id.clone();
        settings.layouts.custom.push(mine);
        settings
            .layouts
            .displays
            .insert("display-1".to_owned(), "custom-mine".to_owned());
        settings.layouts.options.insert(
            "custom-mine".to_owned(),
            LayoutOptions {
                enabled: true,
                select_shortcut: Some(snappy("KeyM")),
                zone_shortcuts: [(zone, snappy("KeyQ"))].into_iter().collect(),
            },
        );
        settings.workspaces.enabled = true;
        settings.workspaces.active_id = "workspace-2".to_owned();
        settings.workspaces.items.remove(0);
        settings
            .workspaces
            .items
            .push(Workspace::numbered("workspace-9", 9));
        settings.workspaces.items[0].name = "Code".to_owned();
        valid(settings)
    }

    #[test]
    fn resetting_settings_keeps_layouts_and_workspaces_and_resets_every_shortcut() {
        let settings = customized();
        let reset = reset(&settings, Reset::Settings);
        let defaults = Settings::defaults(MAC);

        assert_eq!(reset.snappy_key, defaults.snappy_key);
        assert!(!reset.caps_lock_as_snappy_key && !reset.debug_log);
        assert_eq!(reset.history, defaults.history);
        assert!(!reset.snapping.enabled, "snapping stays as it was");

        assert_eq!(reset.layouts.custom, settings.layouts.custom);
        assert_eq!(reset.layouts.displays, settings.layouts.displays);
        assert!(
            reset.layouts.options.values().all(|options| options.enabled
                && options.select_shortcut.is_none()
                && options.zone_shortcuts.is_empty()),
            "every layout is on, and layout and zone shortcuts go"
        );

        assert!(reset.workspaces.enabled);
        assert_eq!(reset.workspaces.active_id, "workspace-2");
        let names: Vec<&str> = reset
            .workspaces
            .items
            .iter()
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(names, ["Code", "Workspace 3", "Workspace 9"]);
        let switches: Vec<Option<KeyChord>> = reset
            .workspaces
            .items
            .iter()
            .map(|item| item.switch_shortcut.clone())
            .collect();
        assert_eq!(
            switches,
            (1..=3).map(numbered_switch_shortcut).collect::<Vec<_>>()
        );
    }

    #[test]
    fn resetting_everything_removes_custom_layouts_and_workspaces() {
        let settings = customized();
        let reset = reset(&settings, Reset::Everything);
        let mut expected = Settings::defaults(MAC);
        expected.snapping.enabled = false;
        assert_eq!(reset, valid(expected));
    }

    #[test]
    fn resets_say_what_they_did() {
        let settings = customized();
        assert_eq!(
            shortcuts(ShortcutSection::WindowHistory).message(&settings),
            "Window history shortcuts reset."
        );
        let workspace = Reset::WorkspaceShortcuts {
            workspace: "workspace-2".to_owned(),
        };
        assert_eq!(workspace.message(&settings), "Shortcuts for Code reset.");
    }

    #[test]
    fn resets_are_written_with_a_scope() {
        let reset = shortcuts(ShortcutSection::MoveBetweenZones);
        assert_eq!(
            serde_json::to_value(&reset).unwrap(),
            serde_json::json!({ "scope": "shortcuts", "section": "moveBetweenZones" })
        );
        assert_eq!(
            serde_json::to_value(Reset::Everything).unwrap(),
            serde_json::json!({ "scope": "everything" })
        );
    }
}
