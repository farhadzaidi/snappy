//! The default shortcuts as help lists them: one line per family of shortcuts that share
//! the Snappy key, such as the four arrows, built from the defaults themselves so help
//! can't drift from them.

use serde::Serialize;

use crate::keys::{KeyChord, Os};
use crate::placement::navigate::Direction;
use crate::settings::{
    NUMBERED_SWITCH_SHORTCUTS, NavigationShortcut, SEND_TO_BACK_DOES, Settings, Workspace, WorkspaceStep,
};

/// One line of help: what a family of default shortcuts does, and its keys as one chord
/// whose label names every key, such as "← → ↑ ↓".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GuideEntry {
    pub title: String,
    pub chord: KeyChord,
}

/// The default shortcuts on `platform`, in the order help shows them.
pub fn default_shortcuts(platform: Os) -> Vec<GuideEntry> {
    let defaults = Settings::defaults(platform);
    let navigation = |shortcut: NavigationShortcut| defaults.navigation.get(shortcut).cloned();
    let steps = |step: WorkspaceStep| defaults.workspaces.step(step).cloned();
    let numbered = |number: usize| Workspace::numbered("", number).switch_shortcut;

    let entries = [
        (
            "Move to the neighboring zone".to_owned(),
            listed(Direction::ALL.map(|direction| navigation(direction.into()))),
        ),
        (
            "Focus the window up, left, down, or right".to_owned(),
            listed(
                [Direction::Up, Direction::Left, Direction::Down, Direction::Right]
                    .map(|direction| navigation(NavigationShortcut::Focus(direction))),
            ),
        ),
        (
            SEND_TO_BACK_DOES.to_owned(),
            navigation(NavigationShortcut::SendToBack),
        ),
        (
            "Select or unselect the window".to_owned(),
            navigation(NavigationShortcut::SelectWindow),
        ),
        (
            "Clear the selection".to_owned(),
            navigation(NavigationShortcut::ClearSelection),
        ),
        (
            "Move to the nearest empty zone, or the selection into empty zones".to_owned(),
            navigation(NavigationShortcut::NearestEmptyZone),
        ),
        (
            format!("Switch to workspace 1–{NUMBERED_SWITCH_SHORTCUTS}, taking the selection along"),
            numbered(1)
                .zip(numbered(NUMBERED_SWITCH_SHORTCUTS))
                .map(|(first, last)| {
                    let range = format!("{}–{}", first.key_label(), last.key_label());
                    first.with_label(range)
                }),
        ),
        (
            "Previous or next workspace".to_owned(),
            listed([WorkspaceStep::Previous, WorkspaceStep::Next].map(steps)),
        ),
        (
            "Send the window or selection to the previous or next workspace".to_owned(),
            listed([WorkspaceStep::SendPrevious, WorkspaceStep::SendNext].map(steps)),
        ),
    ];
    entries
        .into_iter()
        .filter_map(|(title, chord)| chord.map(|chord| GuideEntry { title, chord }))
        .collect()
}

/// The first chord, labelled with every chord's key in turn. The family shares its
/// modifiers, so the first stands for all of them.
fn listed<const N: usize>(chords: [Option<KeyChord>; N]) -> Option<KeyChord> {
    let chords: Option<Vec<KeyChord>> = chords.into_iter().collect();
    let chords = chords?;
    let label = chords
        .iter()
        .map(KeyChord::key_label)
        .collect::<Vec<_>>()
        .join(" ");
    chords.into_iter().next().map(|chord| chord.with_label(label))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(platform: Os) -> Vec<(String, String)> {
        default_shortcuts(platform)
            .into_iter()
            .map(|entry| (entry.title, entry.chord.long_label(platform)))
            .collect()
    }

    #[test]
    fn help_lists_every_family_of_defaults_with_its_keys() {
        let labels = labels(Os::MacOs);
        let labels: Vec<(&str, &str)> = labels
            .iter()
            .map(|(title, keys)| (title.as_str(), keys.as_str()))
            .collect();
        assert_eq!(
            labels,
            [
                ("Move to the neighboring zone", "Snappy + ← → ↑ ↓"),
                ("Focus the window up, left, down, or right", "Snappy + I J K L"),
                ("Press to send the window to the back; hold to peek", "Snappy + B"),
                ("Select or unselect the window", "Snappy + ↩"),
                ("Clear the selection", "Snappy + \\"),
                (
                    "Move to the nearest empty zone, or the selection into empty zones",
                    "Snappy + Space"
                ),
                (
                    "Switch to workspace 1–9, taking the selection along",
                    "Snappy + 1–9"
                ),
                ("Previous or next workspace", "Snappy + ; '"),
                (
                    "Send the window or selection to the previous or next workspace",
                    "Snappy + . /"
                ),
            ]
        );
    }

    #[test]
    fn help_labels_each_key_as_the_settings_do() {
        let defaults = Settings::defaults(Os::Windows);
        let select = defaults
            .navigation
            .get(NavigationShortcut::SelectWindow)
            .map(|chord| chord.long_label(Os::Windows));
        let help = labels(Os::Windows)
            .into_iter()
            .find(|(title, _)| title == "Select or unselect the window")
            .map(|(_, keys)| keys);
        assert_eq!(help, select);
    }
}
