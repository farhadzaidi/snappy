//! The Settings page's sections of global shortcuts.

use serde::{Deserialize, Serialize};

use super::ShortcutId;
use crate::placement::navigate::Direction;
use crate::settings::{HistoryStep, NavigationShortcut, WorkspaceStep};

/// The app's sections of global shortcuts, each reset on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutSection {
    /// Moving the focused window to a neighboring or the nearest empty zone.
    MoveBetweenZones,
    FocusAndSelection,
    /// Stepping through workspaces and sending windows to the previous or next one.
    WorkspaceSteps,
    WindowHistory,
}

impl ShortcutSection {
    /// Every section, in the order the app shows them.
    pub const ALL: [ShortcutSection; 4] = [
        ShortcutSection::MoveBetweenZones,
        ShortcutSection::FocusAndSelection,
        ShortcutSection::WorkspaceSteps,
        ShortcutSection::WindowHistory,
    ];

    /// The section's name, as the app titles it.
    pub fn title(self) -> &'static str {
        match self {
            ShortcutSection::MoveBetweenZones => "Move between zones",
            ShortcutSection::FocusAndSelection => "Focus and selection",
            ShortcutSection::WorkspaceSteps => "Workspaces",
            ShortcutSection::WindowHistory => "Window history",
        }
    }

    /// The section's shortcuts, in the order the app lists them.
    pub fn shortcuts(self) -> Vec<ShortcutId> {
        let navigation = |shortcuts: &[NavigationShortcut]| shortcuts.iter().map(|&s| s.into()).collect();
        match self {
            ShortcutSection::MoveBetweenZones => navigation(&[
                NavigationShortcut::Move(Direction::Left),
                NavigationShortcut::Move(Direction::Right),
                NavigationShortcut::Move(Direction::Up),
                NavigationShortcut::Move(Direction::Down),
                NavigationShortcut::NearestEmptyZone,
            ]),
            ShortcutSection::FocusAndSelection => navigation(&[
                NavigationShortcut::Focus(Direction::Up),
                NavigationShortcut::Focus(Direction::Left),
                NavigationShortcut::Focus(Direction::Down),
                NavigationShortcut::Focus(Direction::Right),
                NavigationShortcut::SendToBack,
                NavigationShortcut::SelectWindow,
                NavigationShortcut::ClearSelection,
            ]),
            ShortcutSection::WorkspaceSteps => WorkspaceStep::ALL.map(ShortcutId::from).to_vec(),
            ShortcutSection::WindowHistory => HistoryStep::ALL.map(ShortcutId::from).to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sections_hold_every_global_shortcut_once() {
        let listed: Vec<ShortcutId> = ShortcutSection::ALL
            .into_iter()
            .flat_map(ShortcutSection::shortcuts)
            .collect();
        let global: Vec<ShortcutId> = NavigationShortcut::ALL
            .map(ShortcutId::from)
            .into_iter()
            .chain(WorkspaceStep::ALL.map(ShortcutId::from))
            .chain(HistoryStep::ALL.map(ShortcutId::from))
            .collect();
        assert_eq!(listed.len(), global.len());
        assert!(global.iter().all(|id| listed.contains(id)));
    }
}
