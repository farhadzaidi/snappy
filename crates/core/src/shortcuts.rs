//! Every shortcut by what it is: reading and setting its chord, its default, and its name.
//! A shortcut is known by its section and, for a workspace's or a layout's own, that
//! workspace's or layout's ID, never by its name, which two workspaces or layouts may share.

use serde::{Deserialize, Serialize};

use crate::bindings;
use crate::keys::KeyChord;
use crate::layout::Layout;
use crate::settings::{
    HistoryStep, LayoutOptions, LayoutShortcut, MoveHistory, Navigation, NavigationShortcut, Settings,
    Workspace, WorkspaceShortcut, WorkspaceStep, Workspaces, numbered_switch_shortcut,
};

/// One shortcut. Written as its kind and what picks it out, such as
/// `{ "kind": "navigation", "shortcut": "left" }` or
/// `{ "kind": "workspace", "workspace": "workspace-2", "shortcut": "switchShortcut" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShortcutId {
    Navigation {
        shortcut: NavigationShortcut,
    },
    History {
        step: HistoryStep,
    },
    WorkspaceStep {
        step: WorkspaceStep,
    },
    /// One workspace's own, by the workspace's ID.
    Workspace {
        workspace: String,
        shortcut: WorkspaceShortcut,
    },
    /// Applying a layout, by its ID.
    LayoutSelect {
        layout: String,
    },
    /// Moving the focused window into a zone, by its layout's ID and its own.
    Zone {
        layout: String,
        zone: String,
    },
}

impl From<NavigationShortcut> for ShortcutId {
    fn from(shortcut: NavigationShortcut) -> Self {
        ShortcutId::Navigation { shortcut }
    }
}

impl From<HistoryStep> for ShortcutId {
    fn from(step: HistoryStep) -> Self {
        ShortcutId::History { step }
    }
}

impl From<WorkspaceStep> for ShortcutId {
    fn from(step: WorkspaceStep) -> Self {
        ShortcutId::WorkspaceStep { step }
    }
}

impl ShortcutId {
    /// A workspace's own shortcut.
    pub fn workspace(workspace: &str, shortcut: WorkspaceShortcut) -> Self {
        ShortcutId::Workspace {
            workspace: workspace.to_owned(),
            shortcut,
        }
    }

    /// The shortcuts that work whatever the workspaces do: navigation, focus and selection
    /// included, then window history.
    pub fn global() -> impl Iterator<Item = ShortcutId> {
        let navigation = NavigationShortcut::ALL.into_iter().map(ShortcutId::from);
        navigation.chain(HistoryStep::ALL.into_iter().map(ShortcutId::from))
    }

    /// `workspace`'s own shortcuts.
    pub fn of_workspace(workspace: &str) -> impl Iterator<Item = ShortcutId> {
        WorkspaceShortcut::ALL
            .into_iter()
            .map(move |shortcut| ShortcutId::workspace(workspace, shortcut))
    }

    pub fn get<'a>(&self, settings: &'a Settings) -> Option<&'a KeyChord> {
        match self {
            ShortcutId::Navigation { shortcut } => settings.navigation.get(*shortcut),
            ShortcutId::History { step } => settings.history.get(*step),
            ShortcutId::WorkspaceStep { step } => settings.workspaces.step(*step),
            ShortcutId::Workspace { workspace, shortcut } => {
                settings.workspaces.item(workspace)?.get(*shortcut)
            }
            ShortcutId::LayoutSelect { layout } => {
                settings.layouts.options.get(layout)?.select_shortcut.as_ref()
            }
            ShortcutId::Zone { layout, zone } => {
                settings.layouts.options.get(layout)?.zone_shortcuts.get(zone)
            }
        }
    }

    /// Assign `chord`, or clear the shortcut. A workspace that doesn't exist is left alone.
    pub fn set(&self, settings: &mut Settings, chord: Option<KeyChord>) {
        match self {
            ShortcutId::Navigation { shortcut } => settings.navigation.set(*shortcut, chord),
            ShortcutId::History { step } => settings.history.set(*step, chord),
            ShortcutId::WorkspaceStep { step } => settings.workspaces.set_step(*step, chord),
            ShortcutId::Workspace { workspace, shortcut } => {
                if let Some(item) = settings.workspaces.item_mut(workspace) {
                    item.set(*shortcut, chord);
                }
            }
            ShortcutId::LayoutSelect { layout } => options_mut(settings, layout).select_shortcut = chord,
            ShortcutId::Zone { layout, zone } => {
                let zones = &mut options_mut(settings, layout).zone_shortcuts;
                match chord {
                    Some(chord) => zones.insert(zone.clone(), chord),
                    None => zones.remove(zone),
                };
            }
        }
    }

    /// The chord itself, to change in place.
    fn chord_mut<'a>(&self, settings: &'a mut Settings) -> Option<&'a mut KeyChord> {
        match self {
            ShortcutId::Navigation { shortcut } => settings.navigation.slot(*shortcut).as_mut(),
            ShortcutId::History { step } => settings.history.slot(*step).as_mut(),
            ShortcutId::WorkspaceStep { step } => settings.workspaces.step_slot(*step).as_mut(),
            ShortcutId::Workspace { workspace, shortcut } => {
                settings.workspaces.item_mut(workspace)?.slot(*shortcut).as_mut()
            }
            ShortcutId::LayoutSelect { layout } => {
                settings.layouts.options.get_mut(layout)?.select_shortcut.as_mut()
            }
            ShortcutId::Zone { layout, zone } => settings
                .layouts
                .options
                .get_mut(layout)?
                .zone_shortcuts
                .get_mut(zone),
        }
    }

    /// What the shortcut is on a first launch, or once reset: the Snappy key and one other
    /// key for navigation and workspace steps, the Snappy key and its position for one of
    /// the first nine workspaces to switch, and nothing for the rest.
    pub fn default(&self, settings: &Settings) -> Option<KeyChord> {
        match self {
            ShortcutId::Navigation { shortcut } => Navigation::defaults().get(*shortcut).cloned(),
            ShortcutId::History { step } => MoveHistory::default().get(*step).cloned(),
            ShortcutId::WorkspaceStep { step } => Workspaces::defaults().step(*step).cloned(),
            ShortcutId::Workspace {
                workspace,
                shortcut: WorkspaceShortcut::Switch,
            } => settings
                .workspaces
                .items
                .iter()
                .position(|item| item.id == *workspace)
                .and_then(|index| numbered_switch_shortcut(index + 1)),
            ShortcutId::Workspace { .. } | ShortcutId::LayoutSelect { .. } | ShortcutId::Zone { .. } => None,
        }
    }

    /// The name the settings app and messages give it, or none for a workspace, layout, or
    /// zone that doesn't exist.
    pub fn title(&self, settings: &Settings) -> Option<String> {
        match self {
            ShortcutId::Navigation { shortcut } => Some(shortcut.title()),
            ShortcutId::History { step } => Some(step.title().to_owned()),
            ShortcutId::WorkspaceStep { step } => Some(step.title().to_owned()),
            ShortcutId::Workspace { workspace, shortcut } => {
                Some(shortcut.title(&settings.workspaces.item(workspace)?.name))
            }
            ShortcutId::LayoutSelect { layout } => {
                Some(LayoutShortcut::Select.title(&settings.layout(layout)?.name))
            }
            ShortcutId::Zone { layout, zone } => {
                let layout = settings.layout(layout)?;
                let index = layout.zones.iter().position(|candidate| candidate.id == *zone)?;
                Some(LayoutShortcut::Zone(index).title(&layout.name))
            }
        }
    }
}

fn options_mut<'a>(settings: &'a mut Settings, layout: &str) -> &'a mut LayoutOptions {
    settings
        .layouts
        .options
        .entry(layout.to_owned())
        .or_insert_with(LayoutOptions::empty)
}

/// An assigned shortcut with its name, for conflicts and the OS's checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bound {
    pub id: ShortcutId,
    pub title: String,
    pub chord: KeyChord,
}

/// The global shortcuts that have a chord, in `ShortcutId::global` order.
pub fn bound_global(settings: &Settings) -> Vec<Bound> {
    ShortcutId::global()
        .filter_map(|id| {
            let chord = id.get(settings)?.clone();
            let title = id.title(settings)?;
            Some(Bound { id, title, chord })
        })
        .collect()
}

/// The workspace shortcuts that have a chord: the steps, then each workspace's own.
pub fn bound_workspace(workspaces: &Workspaces) -> Vec<Bound> {
    let steps = WorkspaceStep::ALL.into_iter().filter_map(|step| {
        let chord = workspaces.step(step)?.clone();
        let title = step.title().to_owned();
        Some(Bound {
            id: step.into(),
            title,
            chord,
        })
    });
    let own = workspaces.items.iter().flat_map(|item| {
        item.chords().map(|(shortcut, chord)| Bound {
            id: ShortcutId::workspace(&item.id, shortcut),
            title: shortcut.title(&item.name),
            chord: chord.clone(),
        })
    });
    steps.chain(own).collect()
}

/// `layout`'s zone shortcuts with `options`, in zone order.
pub fn bound_zones(layout: &Layout, options: &LayoutOptions) -> Vec<Bound> {
    layout
        .zones
        .iter()
        .enumerate()
        .filter_map(|(index, zone)| {
            let chord = options.zone_shortcuts.get(&zone.id)?.clone();
            Some(Bound {
                id: ShortcutId::Zone {
                    layout: layout.id.clone(),
                    zone: zone.id.clone(),
                },
                title: LayoutShortcut::Zone(index).title(&layout.name),
                chord,
            })
        })
        .collect()
}

/// `layout`'s selection shortcut with `options`, when it has one.
pub fn bound_select(layout: &Layout, options: &LayoutOptions) -> Option<Bound> {
    let chord = options.select_shortcut.clone()?;
    Some(Bound {
        id: ShortcutId::LayoutSelect {
            layout: layout.id.clone(),
        },
        title: LayoutShortcut::Select.title(&layout.name),
        chord,
    })
}

/// Every assigned shortcut: global, then workspaces, then each layout's selection and
/// zones. Disabled layouts and workspaces count, even while off.
pub fn bound(settings: &Settings) -> Vec<Bound> {
    let mut all = bound_global(settings);
    all.extend(bound_workspace(&settings.workspaces));
    for layout in settings.layouts() {
        let options = settings.options(layout);
        all.extend(bound_select(layout, &options));
        all.extend(bound_zones(layout, &options));
    }
    all
}

/// Give each of `ids` its default, except one whose chord a binding already uses, which
/// stays unassigned. They are cleared first and given out in order, each taking its chord
/// only while it is free, so this never creates a conflict.
pub fn give_defaults(settings: &mut Settings, ids: &[ShortcutId]) {
    let defaults: Vec<Option<KeyChord>> = ids.iter().map(|id| id.default(settings)).collect();
    for id in ids {
        id.set(settings, None);
    }
    let mut used = bindings::bound_combos(settings);
    let key = settings.snappy_key;
    for (id, default) in ids.iter().zip(defaults) {
        id.set(settings, default.filter(|chord| used.insert(chord.combo(key))));
    }
}

/// Change every chord a binding holds in place.
pub(crate) fn for_each_chord(settings: &mut Settings, mut visit: impl FnMut(&mut KeyChord)) {
    for id in every(settings) {
        if let Some(chord) = id.chord_mut(settings) {
            visit(chord);
        }
    }
}

/// Every shortcut that may hold a chord, assigned or not, zones by their options.
fn every(settings: &Settings) -> Vec<ShortcutId> {
    let mut ids: Vec<ShortcutId> = ShortcutId::global().collect();
    ids.extend(WorkspaceStep::ALL.map(ShortcutId::from));
    ids.extend(
        settings
            .workspaces
            .items
            .iter()
            .flat_map(|item: &Workspace| ShortcutId::of_workspace(&item.id)),
    );
    for (layout, options) in &settings.layouts.options {
        ids.push(ShortcutId::LayoutSelect {
            layout: layout.clone(),
        });
        ids.extend(options.zone_shortcuts.keys().map(|zone| ShortcutId::Zone {
            layout: layout.clone(),
            zone: zone.clone(),
        }));
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Platform;
    use crate::navigate::Direction;

    fn snappy(code: &str) -> KeyChord {
        KeyChord::snappy(code, [])
    }

    #[test]
    fn ids_are_written_by_kind_and_what_picks_them_out() {
        let id = ShortcutId::workspace("workspace-2", WorkspaceShortcut::SendAndSwitch);
        let written = serde_json::to_value(&id).unwrap();
        assert_eq!(
            written,
            serde_json::json!({ "kind": "workspace", "workspace": "workspace-2", "shortcut": "sendAndSwitchShortcut" })
        );
        assert_eq!(serde_json::from_value::<ShortcutId>(written).unwrap(), id);
        let left = ShortcutId::from(NavigationShortcut::Move(Direction::Left));
        assert_eq!(
            serde_json::to_value(&left).unwrap(),
            serde_json::json!({ "kind": "navigation", "shortcut": "left" })
        );
    }

    #[test]
    fn every_kind_reads_and_sets_its_own_chord() {
        let mut settings = Settings::defaults(Platform::MacOs);
        let ids = [
            ShortcutId::from(NavigationShortcut::ClearSelection),
            ShortcutId::from(HistoryStep::Redo),
            ShortcutId::from(WorkspaceStep::SendNext),
            ShortcutId::workspace("workspace-3", WorkspaceShortcut::Send),
            ShortcutId::LayoutSelect {
                layout: "halves".into(),
            },
            ShortcutId::Zone {
                layout: "halves".into(),
                zone: "halves-1".into(),
            },
        ];
        for (index, id) in ids.iter().enumerate() {
            let chord = snappy(&format!("F{}", index + 1));
            id.set(&mut settings, Some(chord.clone()));
            assert_eq!(id.get(&settings), Some(&chord), "{id:?}");
        }
        assert_eq!(settings.navigation.clear_selection, Some(snappy("F1")));
        assert_eq!(settings.workspaces.items[2].send_shortcut, Some(snappy("F4")));
        assert_eq!(
            settings.layouts.options["halves"].zone_shortcuts["halves-1"],
            snappy("F6")
        );
        for id in &ids {
            id.set(&mut settings, None);
            assert_eq!(id.get(&settings), None, "{id:?}");
        }
    }

    #[test]
    fn titles_name_the_workspace_and_layout_they_belong_to() {
        let settings = Settings::defaults(Platform::MacOs);
        let title = |id: ShortcutId| id.title(&settings);
        assert_eq!(
            title(ShortcutId::workspace("workspace-2", WorkspaceShortcut::Switch)).as_deref(),
            Some("Switch to Workspace 2")
        );
        assert_eq!(
            title(ShortcutId::Zone {
                layout: "halves".into(),
                zone: "halves-1".into()
            })
            .as_deref(),
            Some("Zone 2 of Halves")
        );
        assert_eq!(
            title(ShortcutId::workspace("gone", WorkspaceShortcut::Switch)),
            None
        );
    }

    #[test]
    fn a_workspace_switches_by_its_position_by_default_and_sends_with_nothing() {
        let settings = Settings::defaults(Platform::MacOs);
        assert_eq!(
            ShortcutId::workspace("workspace-2", WorkspaceShortcut::Switch).default(&settings),
            Some(snappy("Digit2").with_label("2"))
        );
        assert_eq!(
            ShortcutId::workspace("workspace-2", WorkspaceShortcut::Send).default(&settings),
            None
        );
    }

    #[test]
    fn defaults_go_out_only_to_free_chords() {
        let mut settings = Settings::defaults(Platform::MacOs);
        settings.navigation.focus_left = None;
        settings.history.undo = Some(snappy("KeyJ"));
        give_defaults(
            &mut settings,
            &[
                NavigationShortcut::Focus(Direction::Left).into(),
                NavigationShortcut::Focus(Direction::Right).into(),
            ],
        );
        assert_eq!(settings.navigation.focus_left, None, "Undo holds Snappy + J");
        assert_eq!(settings.navigation.focus_right, Some(snappy("KeyL")));
    }
}
