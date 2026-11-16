//! What the settings app asks the daemon to do.

use serde::{Deserialize, Serialize};
use snappy_core::keys::{KeyChord, Mods};
use snappy_core::layout::Layout;
use snappy_core::placement::swap::{SwapModifier, WhenTaken};
use snappy_core::settings::Reset;
use snappy_core::settings::{
    DragModifier, HistoryStep, LayoutOptions, NavigationShortcut, Workspace, WorkspaceStep,
};
use snappy_platform::Remedy;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Command {
    SetSnapping {
        enabled: bool,
    },
    SetDragModifier {
        modifier: DragModifier,
    },
    SetSwapModifier {
        modifier: SwapModifier,
    },
    /// Swap with, or stack on, a window already in the destination zone.
    SetWhenTaken {
        choice: WhenTaken,
    },
    SetShortcutsEnabled {
        enabled: bool,
    },
    /// Make the Snappy key stand for these modifiers. Refused, with the reason, unless
    /// every shortcut still works with them.
    SetSnappyKey {
        modifiers: Mods,
    },
    /// Hold Caps Lock as the Snappy key, or give it back its usual behavior. Turning it on
    /// is refused, with the reason, when the system already remaps Caps Lock.
    SetCapsLockAsSnappyKey {
        enabled: bool,
    },
    /// A shortcut of the navigation section: moving the focused window between zones or
    /// into the nearest empty zone, moving focus, or changing the selection.
    SetNavigationShortcut {
        shortcut: NavigationShortcut,
        chord: Option<KeyChord>,
    },
    ApplyLayout {
        display: String,
        layout: String,
    },
    SaveLayout {
        layout: Layout,
        options: LayoutOptions,
        display: String,
    },
    DeleteLayout {
        layout: String,
    },
    SetLayoutEnabled {
        layout: String,
        enabled: bool,
    },
    ShowPreview {
        display: String,
        layout: String,
    },
    HidePreview,
    BeginRecording,
    EndRecording,
    ProbeShortcut {
        chord: KeyChord,
    },
    SetWorkspacesEnabled {
        enabled: bool,
    },
    AddWorkspace,
    UpdateWorkspace {
        workspace: Workspace,
    },
    MoveWorkspaces {
        from: Vec<usize>,
        to: usize,
    },
    DeleteWorkspace {
        workspace: String,
    },
    SetWorkspaceStepShortcut {
        step: WorkspaceStep,
        chord: Option<KeyChord>,
    },
    SetHistoryShortcut {
        step: HistoryStep,
        chord: Option<KeyChord>,
    },
    SetLoginItem {
        enabled: bool,
    },
    SetDebugLog {
        enabled: bool,
    },
    OpenPermissionSettings,
    /// Do what the platform offered to make live control available, such as logging out.
    ApplyRemedy {
        remedy: Remedy,
    },
    /// Put a section of shortcuts, a workspace's shortcuts, every setting, or everything
    /// back to the defaults (`Settings::reset`). Resetting everything turns workspaces off
    /// first, bringing their windows back.
    Reset {
        reset: Reset,
    },
    ImportSettings {
        path: String,
    },
    ExportSettings {
        path: String,
    },
    DismissStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_use_readable_json() {
        let json = serde_json::to_string(&Command::SetSnapping { enabled: true }).unwrap();
        assert_eq!(json, r#"{"kind":"setSnapping","enabled":true}"#);
        let key: Command =
            serde_json::from_str(r#"{"kind":"setSnappyKey","modifiers":["alt","ctrl"]}"#).unwrap();
        let modifiers = [
            snappy_core::keys::Modifier::Ctrl,
            snappy_core::keys::Modifier::Alt,
        ];
        assert_eq!(
            key,
            Command::SetSnappyKey {
                modifiers: modifiers.into_iter().collect()
            }
        );
    }

    #[test]
    fn the_navigation_command_names_its_shortcut_by_the_settings_field() {
        let focus: Command =
            serde_json::from_str(r#"{"kind":"setNavigationShortcut","shortcut":"focusUp","chord":null}"#)
                .unwrap();
        assert_eq!(
            focus,
            Command::SetNavigationShortcut {
                shortcut: NavigationShortcut::Focus(snappy_core::placement::navigate::Direction::Up),
                chord: None,
            }
        );
        let arrow = Command::SetNavigationShortcut {
            shortcut: snappy_core::placement::navigate::Direction::Left.into(),
            chord: None,
        };
        assert_eq!(
            serde_json::to_string(&arrow).unwrap(),
            r#"{"kind":"setNavigationShortcut","shortcut":"left","chord":null}"#
        );
        let nearest: Command = serde_json::from_str(
            r#"{"kind":"setNavigationShortcut","shortcut":"nearestEmpty","chord":null}"#,
        )
        .unwrap();
        assert!(matches!(
            nearest,
            Command::SetNavigationShortcut {
                shortcut: NavigationShortcut::NearestEmptyZone,
                ..
            }
        ));
        assert!(
            serde_json::from_str::<Command>(
                r#"{"kind":"setNavigationShortcut","shortcut":"sideways","chord":null}"#
            )
            .is_err()
        );
    }

    #[test]
    fn caps_lock_as_the_snappy_key_has_a_command_of_its_own() {
        let command: Command =
            serde_json::from_str(r#"{"kind":"setCapsLockAsSnappyKey","enabled":true}"#).unwrap();
        assert_eq!(command, Command::SetCapsLockAsSnappyKey { enabled: true });
    }
}
