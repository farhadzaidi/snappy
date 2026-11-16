//! The shortcuts that have a chord, named, and the chords they hold.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::ShortcutId;
use crate::keys::{Combo, KeyChord};
use crate::layout::Layout;
use crate::settings::{LayoutOptions, LayoutShortcut, Settings, WorkspaceStep, Workspaces};

/// An assigned shortcut with its name, for conflicts and the OS's checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bound {
    pub id: ShortcutId,
    pub title: String,
    pub chord: KeyChord,
}

/// A shortcut a message names: which one it is, for the app to find it by, and its
/// title, as the message writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedShortcut {
    pub id: ShortcutId,
    pub title: String,
}

impl From<&Bound> for NamedShortcut {
    fn from(bound: &Bound) -> Self {
        Self {
            id: bound.id.clone(),
            title: bound.title.clone(),
        }
    }
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

/// Every chord a binding uses. Disabled layouts and workspaces count, even while off.
pub fn bound_combos(settings: &Settings) -> BTreeSet<Combo> {
    bound(settings)
        .iter()
        .map(|bound| bound.chord.combo(settings.snappy_key))
        .collect()
}

/// Give each of `ids` its default, except one whose chord a binding already uses, which
/// stays unassigned. They are cleared first and given out in order, each taking its chord
/// only while it is free, so this never creates a conflict.
pub fn give_defaults(settings: &mut Settings, ids: &[ShortcutId]) {
    let defaults: Vec<Option<KeyChord>> = ids.iter().map(|id| id.default(settings)).collect();
    for id in ids {
        id.set(settings, None);
    }
    let mut used = bound_combos(settings);
    let key = settings.snappy_key;
    for (id, default) in ids.iter().zip(defaults) {
        id.set(settings, default.filter(|chord| used.insert(chord.combo(key))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Os;
    use crate::placement::navigate::Direction;
    use crate::settings::NavigationShortcut;
    use crate::shortcuts::test_support::*;

    #[test]
    fn defaults_go_out_only_to_free_chords() {
        let mut settings = Settings::defaults(Os::MacOs);
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
