//! The changes the app asks for, as rules on the settings themselves. The daemon
//! validates and saves what they produce; the app's fake daemon runs the same rules.

use super::{LayoutEditError, LayoutOptions, MAX_WORKSPACES, Settings, Workspace};
use crate::editor::is_separated;
use crate::keys::Os;
use crate::layout::{self, DEFAULT_LAYOUT_ID, Layout};
use crate::shortcuts::conflict_with_layout;

impl Settings {
    /// Use an enabled layout on a display, and make it the default. While workspaces are on,
    /// the choice belongs to the active workspace only. Returns the layout, or `None` when
    /// it is unknown or turned off and nothing changed.
    pub fn assign_layout(&mut self, display: &str, layout_id: &str) -> Option<Layout> {
        let layout = self
            .layout(layout_id)
            .filter(|layout| self.is_enabled(layout))?
            .clone();
        self.assign(display, layout_id);
        Some(layout)
    }

    fn assign(&mut self, display: &str, layout_id: &str) {
        if self.workspaces.enabled {
            let active = self.workspaces.active_id.clone();
            if let Some(item) = self.workspaces.item_mut(&active) {
                item.displays.insert(display.to_owned(), layout_id.to_owned());
            }
        } else {
            self.layouts
                .displays
                .insert(display.to_owned(), layout_id.to_owned());
            self.layouts.default_layout_id = layout_id.to_owned();
        }
    }

    /// Store a layout from the editor with its options, and use it on `display`. Presets
    /// keep their shapes and names whatever the editor sends, so only their options change.
    /// Returns the layout as stored; on a refusal nothing changes.
    pub fn save_layout(
        &mut self,
        requested: Layout,
        options: LayoutOptions,
        display: &str,
        platform: Os,
    ) -> Result<Layout, LayoutEditError> {
        let layout = layout::preset(&requested.id).cloned().unwrap_or(requested);
        if !layout.is_valid() || !layout.zones.iter().all(|zone| zone.rect.is_valid()) {
            return Err(LayoutEditError::Invalid);
        }
        if !is_separated(&layout.zones) {
            return Err(LayoutEditError::Overlapping);
        }
        if let Some(conflict) = conflict_with_layout(self, &layout, &options, platform) {
            return Err(LayoutEditError::Conflict(conflict));
        }

        if !layout.is_preset() {
            match self
                .layouts
                .custom
                .iter_mut()
                .find(|existing| existing.id == layout.id)
            {
                Some(existing) => *existing = layout.clone(),
                None => self.layouts.custom.push(layout.clone()),
            }
        }
        self.layouts.options.insert(layout.id.clone(), options);
        self.assign(display, &layout.id);
        Ok(layout)
    }

    /// Delete a custom layout with its options, and every display's and workspace's choice
    /// of it, so nothing points at a layout that is gone. Presets can't be deleted: returns
    /// whether `id` names a layout that can be.
    pub fn remove_layout(&mut self, id: &str) -> bool {
        if layout::is_preset_id(id) {
            return false;
        }
        self.layouts.custom.retain(|layout| layout.id != id);
        self.layouts.options.remove(id);
        self.layouts.displays.retain(|_, layout| layout != id);
        for item in &mut self.workspaces.items {
            item.displays.retain(|_, layout| layout != id);
        }
        if self.layouts.default_layout_id == id {
            self.layouts.default_layout_id = DEFAULT_LAYOUT_ID.to_owned();
        }
        true
    }

    /// Turn a layout on or off, refused when turning it on would clash with another
    /// binding. Returns the layout.
    pub fn set_layout_enabled(
        &mut self,
        id: &str,
        enabled: bool,
        platform: Os,
    ) -> Result<Layout, LayoutEditError> {
        let layout = self.layout(id).cloned().ok_or(LayoutEditError::Unknown)?;
        let mut options = self.options(&layout);
        options.enabled = enabled;
        if let Some(conflict) = conflict_with_layout(self, &layout, &options, platform) {
            return Err(LayoutEditError::Conflict(conflict));
        }
        self.layouts.options.insert(id.to_owned(), options);
        Ok(layout)
    }

    /// Turn workspaces on. The first time, when no workspace has a layout of its own yet,
    /// every workspace starts from what each display uses now, the connected `displays`
    /// included.
    pub fn enable_workspaces(&mut self, displays: &[String]) {
        if self.workspaces.items.iter().all(|item| item.displays.is_empty()) {
            let mut seeded = self.layouts.displays.clone();
            for display in displays {
                seeded
                    .entry(display.clone())
                    .or_insert_with(|| self.layouts.default_layout_id.clone());
            }
            for item in &mut self.workspaces.items {
                item.displays = seeded.clone();
            }
        }
        self.workspaces.enabled = true;
    }

    /// Add a workspace with the lowest number no workspace is named for, switched to with
    /// the Snappy key and that number when it is a free single digit, and using the active
    /// workspace's layouts. Returns its ID, or `None` when there are as many as allowed.
    pub fn add_workspace(&mut self) -> Option<String> {
        let items = &self.workspaces.items;
        if items.len() >= MAX_WORKSPACES {
            return None;
        }
        let number = (1..=MAX_WORKSPACES)
            .find(|n| !items.iter().any(|item| item.name == Workspace::default_name(*n)))
            .unwrap_or(items.len() + 1);
        let id = self.unused_workspace_id(number);
        let mut workspace = self.new_workspace(id.clone(), number);
        workspace.displays = self
            .workspaces
            .active()
            .map(|active| active.displays.clone())
            .unwrap_or_default();
        self.workspaces.items.push(workspace);
        Some(id)
    }

    /// The ID a workspace numbered `number` gets, with a suffix when another has it.
    fn unused_workspace_id(&self, number: usize) -> String {
        let base = Workspace::default_id(number);
        let mut id = base.clone();
        let mut suffix = 1;
        while self.workspaces.item(&id).is_some() {
            suffix += 1;
            id = format!("{base}-{suffix}");
        }
        id
    }

    /// Replace the workspace with the same ID. An unknown ID changes nothing.
    pub fn update_workspace(&mut self, workspace: Workspace) {
        if let Some(item) = self.workspaces.item_mut(&workspace.id) {
            *item = workspace;
        }
    }

    /// Move the workspaces at the indices `from`, in that order, so they land before the
    /// one at `to` (or last, for the count). A move naming an index twice or past the end
    /// changes nothing.
    pub fn reorder_workspaces(&mut self, from: &[usize], to: usize) {
        let items = &mut self.workspaces.items;
        let count = items.len();
        let repeated = from
            .iter()
            .enumerate()
            .any(|(i, index)| from[..i].contains(index));
        if from.is_empty() || repeated || from.iter().any(|&index| index >= count) || to > count {
            return;
        }
        let moving: Vec<Workspace> = from.iter().map(|&index| items[index].clone()).collect();
        let before = from.iter().filter(|&&index| index < to).count();
        let mut staying: Vec<Workspace> = items
            .iter()
            .enumerate()
            .filter(|(index, _)| !from.contains(index))
            .map(|(_, item)| item.clone())
            .collect();
        let insert_at = (to - before).min(staying.len());
        staying.splice(insert_at..insert_at, moving);
        *items = staying;
    }

    /// Delete a workspace. One always stays, and deleting the active one activates the
    /// first other. Returns the workspace its windows go to, or `None` when it can't be
    /// deleted and nothing changed.
    pub fn remove_workspace(&mut self, id: &str) -> Option<String> {
        let workspaces = &mut self.workspaces;
        if workspaces.items.len() < 2 || workspaces.item(id).is_none() {
            return None;
        }
        let destination = if workspaces.active_id == id {
            workspaces
                .items
                .iter()
                .find(|item| item.id != id)
                .map(|item| item.id.clone())?
        } else {
            workspaces.active_id.clone()
        };
        workspaces.items.retain(|item| item.id != id);
        workspaces.active_id = destination.clone();
        Some(destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyChord;
    use crate::layout::{UnitRect, Zone};
    use crate::settings::test_support::*;

    fn custom(id: &str) -> Layout {
        let zone = |id: &str, x: f64| Zone {
            id: id.to_owned(),
            rect: UnitRect {
                x,
                y: 0.0,
                width: 0.5,
                height: 1.0,
            },
        };
        Layout {
            id: id.to_owned(),
            name: "Coding".to_owned(),
            zones: vec![zone("a", 0.0), zone("b", 0.5)],
        }
    }

    fn ids(settings: &Settings) -> Vec<&str> {
        settings
            .workspaces
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect()
    }

    #[test]
    fn assigning_a_layout_sets_the_display_and_the_default() {
        let mut settings = Settings::defaults(MAC);
        assert!(settings.assign_layout("display-1", "halves").is_some());
        assert_eq!(settings.layouts.displays["display-1"], "halves");
        assert_eq!(settings.layouts.default_layout_id, "halves");
    }

    #[test]
    fn assigning_while_workspaces_are_on_changes_only_the_active_workspace() {
        let mut settings = Settings::defaults(MAC);
        settings.enable_workspaces(&["display-1".to_owned()]);
        settings.assign_layout("display-2", "rows");
        assert_eq!(settings.workspaces.items[0].displays["display-2"], "rows");
        assert!(!settings.workspaces.items[1].displays.contains_key("display-2"));
        assert!(!settings.layouts.displays.contains_key("display-2"));
    }

    #[test]
    fn unknown_or_disabled_layouts_are_not_assigned() {
        let mut settings = Settings::defaults(MAC);
        let before = settings.clone();
        assert!(settings.assign_layout("display-1", "missing").is_none());
        settings.set_layout_enabled("rows", false, MAC).unwrap();
        let disabled = settings.clone();
        assert!(settings.assign_layout("display-1", "rows").is_none());
        assert_eq!(settings, disabled);
        assert_ne!(settings, before);
    }

    #[test]
    fn saving_a_custom_layout_stores_and_assigns_it() {
        let mut settings = Settings::defaults(MAC);
        let saved = settings
            .save_layout(custom("custom-1"), LayoutOptions::empty(), "display-1", MAC)
            .unwrap();
        assert_eq!(saved.name, "Coding");
        assert_eq!(settings.layouts.custom, vec![custom("custom-1")]);
        assert_eq!(settings.layouts.displays["display-1"], "custom-1");

        let mut renamed = custom("custom-1");
        renamed.name = "Writing".to_owned();
        settings
            .save_layout(renamed.clone(), LayoutOptions::empty(), "display-1", MAC)
            .unwrap();
        assert_eq!(settings.layouts.custom, vec![renamed]);
    }

    #[test]
    fn saving_a_preset_keeps_its_shape_and_changes_only_its_options() {
        let mut settings = Settings::defaults(MAC);
        let mut changed = layout::preset("halves").unwrap().clone();
        changed.name = "Mine".to_owned();
        let mut options = LayoutOptions::empty();
        options.select_shortcut = Some(KeyChord::snappy("KeyH", []));
        let saved = settings
            .save_layout(changed, options.clone(), "display-1", MAC)
            .unwrap();
        assert_eq!(saved.name, layout::preset("halves").unwrap().name);
        assert!(settings.layouts.custom.is_empty());
        assert_eq!(settings.layouts.options["halves"], options);
    }

    #[test]
    fn saving_refuses_overlapping_or_clashing_layouts_and_changes_nothing() {
        let mut settings = Settings::defaults(MAC);
        let before = settings.clone();
        let mut overlapping = custom("custom-1");
        overlapping.zones[1].rect.x = 0.25;
        assert_eq!(
            settings.save_layout(overlapping, LayoutOptions::empty(), "display-1", MAC),
            Err(LayoutEditError::Overlapping)
        );
        let mut clashing = LayoutOptions::empty();
        clashing
            .zone_shortcuts
            .insert("a".to_owned(), KeyChord::snappy("ArrowLeft", []));
        assert!(matches!(
            settings.save_layout(custom("custom-1"), clashing, "display-1", MAC),
            Err(LayoutEditError::Conflict(_))
        ));
        assert_eq!(settings, before);
    }

    #[test]
    fn removing_a_custom_layout_clears_every_choice_of_it() {
        let mut settings = Settings::defaults(MAC);
        settings
            .save_layout(custom("custom-1"), LayoutOptions::empty(), "display-1", MAC)
            .unwrap();
        settings.enable_workspaces(&[]);
        settings.assign_layout("display-2", "custom-1");

        assert!(settings.remove_layout("custom-1"));
        assert!(settings.layouts.custom.is_empty());
        assert!(!settings.layouts.options.contains_key("custom-1"));
        assert!(!settings.layouts.displays.contains_key("display-1"));
        assert!(
            settings
                .workspaces
                .items
                .iter()
                .all(|item| item.displays.values().all(|layout| layout != "custom-1"))
        );
        assert_eq!(settings.layouts.default_layout_id, DEFAULT_LAYOUT_ID);
    }

    #[test]
    fn presets_cannot_be_removed() {
        let mut settings = Settings::defaults(MAC);
        settings.assign_layout("display-1", "halves");
        let before = settings.clone();
        assert!(!settings.remove_layout("halves"));
        assert_eq!(settings, before);
    }

    #[test]
    fn turning_a_layout_off_and_on_keeps_its_other_options() {
        let mut settings = Settings::defaults(MAC);
        let layout = settings.set_layout_enabled("rows", false, MAC).unwrap();
        assert_eq!(layout.id, "rows");
        assert!(!settings.layouts.options["rows"].enabled);
        settings.set_layout_enabled("rows", true, MAC).unwrap();
        assert!(settings.layouts.options["rows"].enabled);
        assert_eq!(
            settings.set_layout_enabled("missing", true, MAC),
            Err(LayoutEditError::Unknown)
        );
    }

    #[test]
    fn enabling_workspaces_the_first_time_seeds_each_with_the_current_layouts() {
        let mut settings = Settings::defaults(MAC);
        settings.assign_layout("display-1", "halves");
        settings.enable_workspaces(&["display-2".to_owned()]);
        assert!(settings.workspaces.enabled);
        for item in &settings.workspaces.items {
            assert_eq!(item.displays["display-1"], "halves");
            assert_eq!(item.displays["display-2"], "halves");
        }
    }

    #[test]
    fn enabling_workspaces_again_keeps_their_own_layouts() {
        let mut settings = Settings::defaults(MAC);
        settings.enable_workspaces(&["display-1".to_owned()]);
        settings.assign_layout("display-1", "rows");
        settings.workspaces.enabled = false;
        settings.enable_workspaces(&["display-1".to_owned()]);
        assert_eq!(settings.workspaces.items[0].displays["display-1"], "rows");
        assert_ne!(settings.workspaces.items[1].displays["display-1"], "rows");
    }

    #[test]
    fn a_new_workspace_takes_the_lowest_unused_number_and_the_active_layouts() {
        let mut settings = Settings::defaults(MAC);
        settings.enable_workspaces(&["display-1".to_owned()]);
        settings.assign_layout("display-1", "rows");
        settings.remove_workspace("workspace-2");

        let id = settings.add_workspace().unwrap();
        let added = settings.workspaces.items.last().unwrap();
        assert_eq!(added.id, id);
        assert_eq!(added.name, "Workspace 2");
        assert_eq!(added.id, "workspace-2");
        assert_eq!(added.displays["display-1"], "rows");
    }

    #[test]
    fn a_new_workspace_id_gets_a_suffix_when_its_number_id_is_taken() {
        let mut settings = Settings::defaults(MAC);
        settings.workspaces.items[1].name = "Notes".to_owned();
        assert_eq!(settings.add_workspace().as_deref(), Some("workspace-2-2"));
        assert_eq!(settings.workspaces.items.last().unwrap().name, "Workspace 2");
    }

    #[test]
    fn no_workspace_is_added_past_the_limit() {
        let mut settings = Settings::defaults(MAC);
        while settings.workspaces.items.len() < MAX_WORKSPACES {
            settings.add_workspace().unwrap();
        }
        let full = settings.clone();
        assert_eq!(settings.add_workspace(), None);
        assert_eq!(settings, full);
    }

    #[test]
    fn updating_replaces_the_workspace_with_the_same_id() {
        let mut settings = Settings::defaults(MAC);
        let mut renamed = settings.workspaces.items[1].clone();
        renamed.name = "Notes".to_owned();
        settings.update_workspace(renamed.clone());
        assert_eq!(settings.workspaces.items[1], renamed);
    }

    #[test]
    fn reordering_moves_workspaces_before_the_target() {
        let mut settings = Settings::defaults(MAC);
        settings.reorder_workspaces(&[0], 3);
        assert_eq!(ids(&settings), ["workspace-2", "workspace-3", "workspace-1"]);
        settings.reorder_workspaces(&[2], 0);
        assert_eq!(ids(&settings), ["workspace-1", "workspace-2", "workspace-3"]);
        settings.reorder_workspaces(&[2, 0], 1);
        assert_eq!(ids(&settings), ["workspace-3", "workspace-1", "workspace-2"]);
    }

    #[test]
    fn reordering_with_bad_indices_changes_nothing() {
        let mut settings = Settings::defaults(MAC);
        let before = settings.clone();
        settings.reorder_workspaces(&[], 1);
        settings.reorder_workspaces(&[3], 0);
        settings.reorder_workspaces(&[0], 4);
        settings.reorder_workspaces(&[0, 0], 2);
        assert_eq!(settings, before);
    }

    #[test]
    fn removing_the_active_workspace_activates_the_first_other() {
        let mut settings = Settings::defaults(MAC);
        assert_eq!(
            settings.remove_workspace("workspace-1").as_deref(),
            Some("workspace-2")
        );
        assert_eq!(settings.workspaces.active_id, "workspace-2");
        assert_eq!(
            settings.remove_workspace("workspace-3").as_deref(),
            Some("workspace-2")
        );
        assert_eq!(ids(&settings), ["workspace-2"]);
    }

    #[test]
    fn the_last_or_an_unknown_workspace_is_not_removed() {
        let mut settings = Settings::defaults(MAC);
        assert_eq!(settings.remove_workspace("missing"), None);
        settings.remove_workspace("workspace-2");
        settings.remove_workspace("workspace-3");
        let last = settings.clone();
        assert_eq!(settings.remove_workspace("workspace-1"), None);
        assert_eq!(settings, last);
    }
}
