//! Messages between the daemon and the settings app, and their framing: a 4-byte
//! little-endian length followed by UTF-8 JSON. The daemon starts the settings app with
//! piped standard input and output, so the pipes are the only channel between them.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use snappy_core::bindings::Refusal;
use snappy_core::keys::{KeyChord, Mods, Platform};
use snappy_core::layout::Layout;
use snappy_core::reset::Reset;
use snappy_core::settings::{
    DragModifier, HistoryStep, LayoutOptions, NavigationShortcut, Settings, Workspace, WorkspaceStep,
};
use snappy_core::swap::{SwapModifier, WhenTaken};
use snappy_platform::{Display, Remedy};
use thiserror::Error;

pub const PROTOCOL_VERSION: u32 = 5;
/// Messages larger than this are refused; settings files are capped well below it.
pub const MAX_MESSAGE_BYTES: u32 = 8 * 1024 * 1024;

/// Daemon to settings app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ToSettings {
    Hello {
        protocol_version: u32,
    },
    /// Everything the settings app shows. Sent after every change.
    State(Box<AppState>),
    /// The answer to a command: why it was refused, if it was.
    Reply {
        id: u64,
        error: Option<Refusal>,
    },
    ProbeResult {
        available: bool,
    },
    PreviewHidden,
    Focus,
}

/// Settings app to daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ToDaemon {
    Hello { protocol_version: u32 },
    Command { id: u64, command: Box<Command> },
}

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

/// What the settings app renders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub settings: Settings,
    pub displays: Vec<Display>,
    /// Decides modifier names and permission controls.
    pub platform: Platform,
    pub snapping: SnappingState,
    pub workspaces: WorkspacesState,
    /// Notes from the platform about the desktop, such as another tool that also snaps.
    #[serde(default)]
    pub platform_notes: Vec<String>,
    pub login_item_enabled: bool,
    /// Where the debug log is written while it's on.
    pub debug_log_path: String,
    pub status: Option<Status>,
    /// The ID of a workspace newly added by `AddWorkspace`, so the page can select it.
    pub added_workspace: Option<String>,
    /// Why Caps Lock isn't the Snappy key although the setting asks for it, such as the
    /// system remapping Caps Lock since it was turned on.
    #[serde(default)]
    pub caps_lock_problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnappingState {
    pub live: bool,
    pub waiting_for_permission: bool,
    pub permission_granted: bool,
    pub needs_permission: bool,
    pub preview_visible: bool,
    /// Why live control can't run at all, for example in a Wayland session.
    pub unavailable_reason: Option<String>,
    /// What Snappy can do about it when asked (`ApplyRemedy`).
    #[serde(default)]
    pub unavailable_remedy: Option<Remedy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacesState {
    pub running: bool,
    pub pause_message: Option<String>,
    /// Workspace ID to the number of windows in it.
    pub window_counts: BTreeMap<String, usize>,
}

/// A message for the toast. `notify` false means a quiet update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub id: u64,
    pub message: String,
    pub is_error: bool,
    pub notify: bool,
}

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("pipe error: {0}")]
    Io(#[from] io::Error),
    #[error("malformed message: {0}")]
    Json(#[from] serde_json::Error),
    #[error("message of {0} bytes is too large")]
    TooLarge(u32),
}

pub fn write_message<T: Serialize>(writer: &mut impl Write, message: &T) -> Result<(), FrameError> {
    let body = serde_json::to_vec(message)?;
    let length = u32::try_from(body.len()).map_err(|_| FrameError::TooLarge(u32::MAX))?;
    if length > MAX_MESSAGE_BYTES {
        return Err(FrameError::TooLarge(length));
    }
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

/// Read one message, or `None` when the other side closed the pipe.
pub fn read_message<T: DeserializeOwned>(reader: &mut impl Read) -> Result<Option<T>, FrameError> {
    let mut header = [0_u8; 4];
    match reader.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let length = u32::from_le_bytes(header);
    if length > MAX_MESSAGE_BYTES {
        return Err(FrameError::TooLarge(length));
    }
    let mut body = vec![0_u8; length as usize];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_through_the_framing() {
        let mut pipe = Vec::new();
        let command = ToDaemon::Command {
            id: 7,
            command: Box::new(Command::ApplyLayout {
                display: "d".into(),
                layout: "halves".into(),
            }),
        };
        let hello = ToDaemon::Hello {
            protocol_version: PROTOCOL_VERSION,
        };
        write_message(&mut pipe, &command).unwrap();
        write_message(&mut pipe, &hello).unwrap();
        let mut reader = pipe.as_slice();
        assert_eq!(read_message::<ToDaemon>(&mut reader).unwrap(), Some(command));
        assert_eq!(read_message::<ToDaemon>(&mut reader).unwrap(), Some(hello));
        assert_eq!(read_message::<ToDaemon>(&mut reader).unwrap(), None);
    }

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
    fn a_refusal_reply_carries_its_message_and_the_setting_it_names() {
        let reply = ToSettings::Reply {
            id: 3,
            error: Some(Refusal {
                message: "Snappy + 3 for Switch to Workspace 3 is already used by macOS or another app."
                    .into(),
                setting: Some("Switch to Workspace 3".into()),
            }),
        };
        assert_eq!(
            serde_json::to_value(&reply).unwrap(),
            serde_json::json!({
                "type": "reply",
                "id": 3,
                "error": {
                    "message": "Snappy + 3 for Switch to Workspace 3 is already used by macOS or another app.",
                    "setting": "Switch to Workspace 3",
                },
            })
        );
        let done = ToSettings::Reply { id: 4, error: None };
        assert_eq!(
            serde_json::to_string(&done).unwrap(),
            r#"{"type":"reply","id":4,"error":null}"#
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
                shortcut: NavigationShortcut::Focus(snappy_core::navigate::Direction::Up),
                chord: None,
            }
        );
        let arrow = Command::SetNavigationShortcut {
            shortcut: snappy_core::navigate::Direction::Left.into(),
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

    #[test]
    fn oversized_messages_are_refused() {
        let mut reader: &[u8] = &(MAX_MESSAGE_BYTES + 1).to_le_bytes();
        assert!(matches!(
            read_message::<ToDaemon>(&mut reader),
            Err(FrameError::TooLarge(_))
        ));
    }
}
