//! Messages between the daemon and the settings app, and their framing: a 4-byte
//! little-endian length followed by UTF-8 JSON. The daemon starts the settings app with
//! piped standard input and output, so the pipes are the only channel between them.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use snappy_core::keys::KeyChord;
use snappy_core::layout::Layout;
use snappy_core::navigate::Direction;
use snappy_core::settings::{DragModifier, HistoryStep, LayoutOptions, Settings, Workspace, WorkspaceStep};
use snappy_core::swap::{SwapModifier, WhenTaken};
use snappy_platform::Display;
use thiserror::Error;

pub const PROTOCOL_VERSION: u32 = 1;
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
    /// The answer to a command.
    Reply {
        id: u64,
        error: Option<String>,
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
    SetNavigationShortcut {
        direction: Direction,
        chord: Option<KeyChord>,
    },
    /// The shortcut that moves the focused window into the nearest empty zone.
    SetNearestEmptyShortcut {
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
    /// "macos", "windows", or "linux": decides modifier names and permission controls.
    pub platform: String,
    pub snapping: SnappingState,
    pub workspaces: WorkspacesState,
    pub login_item_enabled: bool,
    /// Where the debug log is written while it's on.
    pub debug_log_path: String,
    pub status: Option<Status>,
    /// The ID of a workspace newly added by `AddWorkspace`, so the page can select it.
    pub added_workspace: Option<String>,
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
