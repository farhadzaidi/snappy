//! The navigation section: moving windows between zones, moving focus, and selecting.

use serde::{Deserialize, Serialize};

use crate::keys::KeyChord;
use crate::placement::navigate::Direction;

/// Shortcuts for getting around: moving the focused window between zones, moving focus
/// between windows, and selecting windows.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Navigation {
    pub left: Option<KeyChord>,
    pub right: Option<KeyChord>,
    pub up: Option<KeyChord>,
    pub down: Option<KeyChord>,
    /// Move the focused window into the nearest empty zone, or the selection into empty
    /// zones.
    #[serde(default)]
    pub nearest_empty: Option<KeyChord>,
    #[serde(default)]
    pub focus_left: Option<KeyChord>,
    #[serde(default)]
    pub focus_right: Option<KeyChord>,
    #[serde(default)]
    pub focus_up: Option<KeyChord>,
    #[serde(default)]
    pub focus_down: Option<KeyChord>,
    /// Add the focused window to the selection, or take it out.
    #[serde(default)]
    pub select_window: Option<KeyChord>,
    #[serde(default)]
    pub clear_selection: Option<KeyChord>,
}

/// The name the settings app and conflict messages give the nearest-empty-zone shortcut.
pub const NEAREST_EMPTY_ZONE_TITLE: &str = "Move to the nearest empty zone";

/// Every shortcut in the navigation section, so each is listed once: for validation,
/// conflicts, registration, the defaults a file without it gets, and the command that
/// sets it. Written as the settings file field that holds it, such as `"left"` or
/// `"focusUp"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationShortcut {
    /// Move the focused window to the neighboring zone.
    Move(Direction),
    /// Move the focused window into the nearest empty zone, or the selection into empty
    /// zones.
    NearestEmptyZone,
    /// Focus the next window that way; moves no window.
    Focus(Direction),
    /// Add the focused window to the selection, or take it out.
    SelectWindow,
    ClearSelection,
}

impl NavigationShortcut {
    /// In the order conflicts are looked for.
    pub const ALL: [NavigationShortcut; 11] = [
        NavigationShortcut::Move(Direction::Left),
        NavigationShortcut::Move(Direction::Right),
        NavigationShortcut::Move(Direction::Up),
        NavigationShortcut::Move(Direction::Down),
        NavigationShortcut::NearestEmptyZone,
        NavigationShortcut::Focus(Direction::Up),
        NavigationShortcut::Focus(Direction::Left),
        NavigationShortcut::Focus(Direction::Down),
        NavigationShortcut::Focus(Direction::Right),
        NavigationShortcut::SelectWindow,
        NavigationShortcut::ClearSelection,
    ];

    /// The name the settings app and conflict messages use.
    pub fn title(self) -> String {
        match self {
            NavigationShortcut::Move(direction) => format!("Move window {}", direction.name()),
            NavigationShortcut::NearestEmptyZone => NEAREST_EMPTY_ZONE_TITLE.to_owned(),
            NavigationShortcut::Focus(direction) => format!("Focus {}", direction.name()),
            NavigationShortcut::SelectWindow => "Select window".to_owned(),
            NavigationShortcut::ClearSelection => "Clear selection".to_owned(),
        }
    }

    /// The field that holds this shortcut's chord in the settings file.
    pub(crate) fn field(self) -> &'static str {
        match self {
            NavigationShortcut::Move(direction) => direction.name(),
            NavigationShortcut::NearestEmptyZone => "nearestEmpty",
            NavigationShortcut::Focus(Direction::Up) => "focusUp",
            NavigationShortcut::Focus(Direction::Left) => "focusLeft",
            NavigationShortcut::Focus(Direction::Down) => "focusDown",
            NavigationShortcut::Focus(Direction::Right) => "focusRight",
            NavigationShortcut::SelectWindow => "selectWindow",
            NavigationShortcut::ClearSelection => "clearSelection",
        }
    }
}

impl Serialize for NavigationShortcut {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.field())
    }
}

impl<'de> Deserialize<'de> for NavigationShortcut {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let field = String::deserialize(deserializer)?;
        NavigationShortcut::ALL
            .into_iter()
            .find(|shortcut| shortcut.field() == field)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown navigation shortcut {field:?}")))
    }
}

impl From<Direction> for NavigationShortcut {
    fn from(direction: Direction) -> Self {
        NavigationShortcut::Move(direction)
    }
}

impl Navigation {
    pub fn get(&self, shortcut: impl Into<NavigationShortcut>) -> Option<&KeyChord> {
        match shortcut.into() {
            NavigationShortcut::Move(Direction::Left) => self.left.as_ref(),
            NavigationShortcut::Move(Direction::Right) => self.right.as_ref(),
            NavigationShortcut::Move(Direction::Up) => self.up.as_ref(),
            NavigationShortcut::Move(Direction::Down) => self.down.as_ref(),
            NavigationShortcut::NearestEmptyZone => self.nearest_empty.as_ref(),
            NavigationShortcut::Focus(Direction::Up) => self.focus_up.as_ref(),
            NavigationShortcut::Focus(Direction::Left) => self.focus_left.as_ref(),
            NavigationShortcut::Focus(Direction::Down) => self.focus_down.as_ref(),
            NavigationShortcut::Focus(Direction::Right) => self.focus_right.as_ref(),
            NavigationShortcut::SelectWindow => self.select_window.as_ref(),
            NavigationShortcut::ClearSelection => self.clear_selection.as_ref(),
        }
    }

    pub fn set(&mut self, shortcut: impl Into<NavigationShortcut>, chord: Option<KeyChord>) {
        *self.slot(shortcut.into()) = chord;
    }

    pub(crate) fn slot(&mut self, shortcut: NavigationShortcut) -> &mut Option<KeyChord> {
        match shortcut {
            NavigationShortcut::Move(Direction::Left) => &mut self.left,
            NavigationShortcut::Move(Direction::Right) => &mut self.right,
            NavigationShortcut::Move(Direction::Up) => &mut self.up,
            NavigationShortcut::Move(Direction::Down) => &mut self.down,
            NavigationShortcut::NearestEmptyZone => &mut self.nearest_empty,
            NavigationShortcut::Focus(Direction::Up) => &mut self.focus_up,
            NavigationShortcut::Focus(Direction::Left) => &mut self.focus_left,
            NavigationShortcut::Focus(Direction::Down) => &mut self.focus_down,
            NavigationShortcut::Focus(Direction::Right) => &mut self.focus_right,
            NavigationShortcut::SelectWindow => &mut self.select_window,
            NavigationShortcut::ClearSelection => &mut self.clear_selection,
        }
    }

    /// Every assigned chord, in `NavigationShortcut::ALL` order.
    pub fn chords(&self) -> impl Iterator<Item = (NavigationShortcut, &KeyChord)> {
        NavigationShortcut::ALL
            .into_iter()
            .filter_map(|shortcut| self.get(shortcut).map(|chord| (shortcut, chord)))
    }
}

impl Navigation {
    /// The arrows move the focused window, and I, J, K, and L, which sit like arrows under
    /// the right hand, move focus. Return selects and backslash clears the selection;
    /// Delete can't, since macOS never passes the Snappy key with Delete on to apps.
    pub fn defaults() -> Self {
        let chord = |code: &str, label: &str| Some(KeyChord::snappy(code, []).with_label(label));
        let key = |code: &str| Some(KeyChord::snappy(code, []));
        Self {
            left: chord("ArrowLeft", "←"),
            right: chord("ArrowRight", "→"),
            up: chord("ArrowUp", "↑"),
            down: chord("ArrowDown", "↓"),
            nearest_empty: key("Space"),
            focus_left: key("KeyJ"),
            focus_right: key("KeyL"),
            focus_up: key("KeyI"),
            focus_down: key("KeyK"),
            select_window: key("Enter"),
            clear_selection: chord("Backslash", "\\"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_shortcuts_are_written_as_their_settings_file_fields() {
        let written = serde_json::to_value(Navigation::defaults()).unwrap();
        for shortcut in NavigationShortcut::ALL {
            let name = serde_json::to_value(shortcut).unwrap();
            let field = name.as_str().unwrap();
            assert!(written.get(field).is_some(), "{field} is a navigation field");
            assert_eq!(
                serde_json::from_value::<NavigationShortcut>(name).unwrap(),
                shortcut
            );
        }
        assert!(serde_json::from_str::<NavigationShortcut>(r#""items""#).is_err());
    }
}
