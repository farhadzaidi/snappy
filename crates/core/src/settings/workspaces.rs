//! The workspaces section: each workspace with its own shortcuts, and the shortcuts that
//! step through them.

use serde::{Deserialize, Serialize};

use super::{DEFAULT_WORKSPACES, DisplayAssignments, MAX_DISPLAY_ASSIGNMENTS, MAX_WORKSPACE_ID_LENGTH};
use crate::keys::KeyChord;
use crate::layout::valid_name;

/// Workspaces numbered up to this get the Snappy key with their number to switch to them.
pub const NUMBERED_SWITCH_SHORTCUTS: usize = 9;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspaces {
    pub enabled: bool,
    pub active_id: String,
    pub previous: Option<KeyChord>,
    pub next: Option<KeyChord>,
    /// Send the focused window, or the selection, to the previous workspace and stay.
    #[serde(default)]
    pub send_previous: Option<KeyChord>,
    /// Send the focused window, or the selection, to the next workspace and stay.
    #[serde(default)]
    pub send_next: Option<KeyChord>,
    pub items: Vec<Workspace>,
}

/// The shortcuts for every workspace at once: stepping to the previous or next one, and
/// sending windows there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceStep {
    Previous,
    Next,
    SendPrevious,
    SendNext,
}

impl WorkspaceStep {
    pub const ALL: [WorkspaceStep; 4] = [
        WorkspaceStep::Previous,
        WorkspaceStep::Next,
        WorkspaceStep::SendPrevious,
        WorkspaceStep::SendNext,
    ];

    /// The name the app and conflict messages use.
    pub fn title(self) -> &'static str {
        match self {
            WorkspaceStep::Previous => "Previous workspace",
            WorkspaceStep::Next => "Next workspace",
            WorkspaceStep::SendPrevious => "Send window to previous workspace",
            WorkspaceStep::SendNext => "Send window to next workspace",
        }
    }

    /// The field that holds this step's chord in the settings file.
    pub(super) fn field(self) -> &'static str {
        match self {
            WorkspaceStep::Previous => "previous",
            WorkspaceStep::Next => "next",
            WorkspaceStep::SendPrevious => "sendPrevious",
            WorkspaceStep::SendNext => "sendNext",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub displays: DisplayAssignments,
    pub switch_shortcut: Option<KeyChord>,
    pub send_shortcut: Option<KeyChord>,
    pub send_and_switch_shortcut: Option<KeyChord>,
}

impl Workspace {
    /// "Workspace <number>", switched to with the Snappy key and its number when that is a
    /// single digit, and with no other shortcuts.
    pub fn numbered(id: impl Into<String>, number: usize) -> Self {
        Self {
            id: id.into(),
            name: Self::default_name(number),
            displays: DisplayAssignments::new(),
            switch_shortcut: numbered_switch_shortcut(number),
            send_shortcut: None,
            send_and_switch_shortcut: None,
        }
    }

    /// The name a workspace gets from its number.
    pub fn default_name(number: usize) -> String {
        format!("Workspace {number}")
    }

    /// The ID a workspace gets from its number, before anything added to keep it unique.
    pub fn default_id(number: usize) -> String {
        format!("workspace-{number}")
    }

    pub fn get(&self, shortcut: WorkspaceShortcut) -> Option<&KeyChord> {
        match shortcut {
            WorkspaceShortcut::Switch => self.switch_shortcut.as_ref(),
            WorkspaceShortcut::Send => self.send_shortcut.as_ref(),
            WorkspaceShortcut::SendAndSwitch => self.send_and_switch_shortcut.as_ref(),
        }
    }

    pub fn set(&mut self, shortcut: WorkspaceShortcut, chord: Option<KeyChord>) {
        *self.slot(shortcut) = chord;
    }

    pub(crate) fn slot(&mut self, shortcut: WorkspaceShortcut) -> &mut Option<KeyChord> {
        match shortcut {
            WorkspaceShortcut::Switch => &mut self.switch_shortcut,
            WorkspaceShortcut::Send => &mut self.send_shortcut,
            WorkspaceShortcut::SendAndSwitch => &mut self.send_and_switch_shortcut,
        }
    }

    /// Every assigned chord of this workspace's own.
    pub fn chords(&self) -> impl Iterator<Item = (WorkspaceShortcut, &KeyChord)> {
        WorkspaceShortcut::ALL
            .into_iter()
            .filter_map(|shortcut| self.get(shortcut).map(|chord| (shortcut, chord)))
    }

    pub(super) fn is_valid(&self) -> bool {
        !self.id.is_empty()
            && self.id.chars().count() <= MAX_WORKSPACE_ID_LENGTH
            && valid_name(&self.name)
            && self.displays.len() <= MAX_DISPLAY_ASSIGNMENTS
            && self.chords().all(|(_, chord)| chord.is_valid())
    }
}

/// A workspace's own shortcuts. Written as the field of `Workspace` that holds it, such as
/// `"switchShortcut"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceShortcut {
    #[serde(rename = "switchShortcut")]
    Switch,
    /// Send the focused window, or the selection, there and stay.
    #[serde(rename = "sendShortcut")]
    Send,
    /// Send the focused window there, then switch, taking the selection along.
    #[serde(rename = "sendAndSwitchShortcut")]
    SendAndSwitch,
}

impl WorkspaceShortcut {
    pub const ALL: [WorkspaceShortcut; 3] = [
        WorkspaceShortcut::Switch,
        WorkspaceShortcut::Send,
        WorkspaceShortcut::SendAndSwitch,
    ];

    /// The name the app and conflict messages use for the workspace named
    /// `workspace`.
    pub fn title(self, workspace: &str) -> String {
        match self {
            WorkspaceShortcut::Switch => format!("Switch to {workspace}"),
            WorkspaceShortcut::Send => format!("Send window to {workspace}"),
            WorkspaceShortcut::SendAndSwitch => format!("Send window to {workspace} and switch"),
        }
    }
}

/// The Snappy key with the workspace's number, for workspaces 1 to 9.
pub(crate) fn numbered_switch_shortcut(number: usize) -> Option<KeyChord> {
    (1..=NUMBERED_SWITCH_SHORTCUTS)
        .contains(&number)
        .then(|| KeyChord::snappy(&format!("Digit{number}"), []).with_label(number.to_string()))
}

impl Workspaces {
    /// Three workspaces, switched to with the Snappy key and their number, and stepped
    /// through with the Snappy key and the semicolon and quote keys, or with the period and
    /// slash keys to send the focused window away. Both pairs sit together under the right
    /// hand, on every keyboard layout's home and bottom rows.
    pub fn defaults() -> Self {
        let chord = |code: &str, label: &str| Some(KeyChord::snappy(code, []).with_label(label));
        Self {
            enabled: false,
            active_id: Workspace::default_id(1),
            previous: chord("Semicolon", ";"),
            next: chord("Quote", "'"),
            send_previous: chord("Period", "."),
            send_next: chord("Slash", "/"),
            items: (1..=DEFAULT_WORKSPACES)
                .map(|n| Workspace::numbered(Workspace::default_id(n), n))
                .collect(),
        }
    }

    pub fn step(&self, step: WorkspaceStep) -> Option<&KeyChord> {
        match step {
            WorkspaceStep::Previous => self.previous.as_ref(),
            WorkspaceStep::Next => self.next.as_ref(),
            WorkspaceStep::SendPrevious => self.send_previous.as_ref(),
            WorkspaceStep::SendNext => self.send_next.as_ref(),
        }
    }

    pub fn set_step(&mut self, step: WorkspaceStep, chord: Option<KeyChord>) {
        *self.step_slot(step) = chord;
    }

    pub(crate) fn step_slot(&mut self, step: WorkspaceStep) -> &mut Option<KeyChord> {
        match step {
            WorkspaceStep::Previous => &mut self.previous,
            WorkspaceStep::Next => &mut self.next,
            WorkspaceStep::SendPrevious => &mut self.send_previous,
            WorkspaceStep::SendNext => &mut self.send_next,
        }
    }

    /// Every assigned step chord.
    pub fn steps(&self) -> impl Iterator<Item = (WorkspaceStep, &KeyChord)> {
        WorkspaceStep::ALL
            .into_iter()
            .filter_map(|step| self.step(step).map(|chord| (step, chord)))
    }

    pub fn active(&self) -> Option<&Workspace> {
        self.items.iter().find(|item| item.id == self.active_id)
    }

    pub fn item(&self, id: &str) -> Option<&Workspace> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn item_mut(&mut self, id: &str) -> Option<&mut Workspace> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    /// The workspace `steps` away from `from` in saved order, wrapping.
    pub fn adjacent(&self, from: &str, steps: isize) -> Option<&Workspace> {
        let count = self.items.len() as isize;
        let index = self.items.iter().position(|item| item.id == from).unwrap_or(0) as isize;
        let target = (index + steps).rem_euclid(count.max(1));
        self.items.get(target as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::SnappyKey;
    use crate::settings::test_support::*;

    #[test]
    fn stepping_through_workspaces_wraps_around() {
        let workspaces = Workspaces::defaults();
        assert_eq!(workspaces.adjacent("workspace-1", -1).unwrap().id, "workspace-3");
        assert_eq!(workspaces.adjacent("workspace-3", 1).unwrap().id, "workspace-1");
    }

    #[test]
    fn workspaces_one_to_nine_switch_with_their_digit_and_later_ones_have_no_shortcuts() {
        let fourth = Workspace::numbered("workspace-4", 4);
        assert_eq!(fourth.name, "Workspace 4");
        assert_eq!(
            fourth
                .switch_shortcut
                .as_ref()
                .map(|chord| chord.combo(SnappyKey::DEFAULT)),
            snappy("Digit4")
        );
        assert_eq!(
            fourth.switch_shortcut.as_ref().unwrap().long_label(MAC),
            "Snappy + 4"
        );
        assert_eq!(fourth.send_shortcut, None);
        assert_eq!(fourth.send_and_switch_shortcut, None);
        assert!(Workspace::numbered("workspace-9", 9).switch_shortcut.is_some());
        assert_eq!(Workspace::numbered("workspace-10", 10).chords().count(), 0);
    }
}
