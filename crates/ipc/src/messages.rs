//! The messages each side sends, and the protocol version they share.

use serde::{Deserialize, Serialize};
use snappy_core::shortcuts::Refusal;

use crate::app_state::AppState;
use crate::command::Command;

pub const PROTOCOL_VERSION: u32 = 8;

/// Daemon to app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ToApp {
    Hello {
        protocol_version: u32,
    },
    /// Everything the app shows. Sent after every change.
    State(Box<AppState>),
    /// The answer to a command: why it was refused, if it was.
    Reply {
        id: u64,
        error: Option<Refusal>,
    },
    ProbeResult {
        available: bool,
    },
    /// The answer to `ExportSettings`: the settings as a settings file's text.
    ExportedSettings {
        contents: String,
    },
    Focus,
}

/// App to daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ToDaemon {
    Hello { protocol_version: u32 },
    Command { id: u64, command: Box<Command> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use snappy_core::settings::WorkspaceShortcut;
    use snappy_core::shortcuts::{NamedShortcut, ShortcutId};

    /// The app's stand-in daemon, for development and its tests, speaks this version.
    #[test]
    fn the_fake_daemon_speaks_this_protocol_version() {
        let fake = include_str!("../../../apps/app/ui/src/bridge/fake/fakeDaemon.ts");
        assert!(fake.contains(&format!("const PROTOCOL_VERSION = {PROTOCOL_VERSION};")));
    }

    #[test]
    fn a_refusal_reply_carries_its_message_and_the_shortcut_it_names_by_id() {
        let reply = ToApp::Reply {
            id: 3,
            error: Some(Refusal {
                message: "Snappy + 3 for Switch to Workspace 3 is already used by macOS or another app."
                    .into(),
                shortcut: Some(NamedShortcut {
                    id: ShortcutId::workspace("workspace-3", WorkspaceShortcut::Switch),
                    title: "Switch to Workspace 3".into(),
                }),
            }),
        };
        assert_eq!(
            serde_json::to_value(&reply).unwrap(),
            serde_json::json!({
                "type": "reply",
                "id": 3,
                "error": {
                    "message": "Snappy + 3 for Switch to Workspace 3 is already used by macOS or another app.",
                    "shortcut": {
                        "id": { "kind": "workspace", "workspace": "workspace-3", "shortcut": "switchShortcut" },
                        "title": "Switch to Workspace 3",
                    },
                },
            })
        );
        let done = ToApp::Reply { id: 4, error: None };
        assert_eq!(
            serde_json::to_string(&done).unwrap(),
            r#"{"type":"reply","id":4,"error":null}"#
        );
    }

    #[test]
    fn exported_settings_carry_the_files_text() {
        let exported = ToApp::ExportedSettings {
            contents: "{}".into(),
        };
        assert_eq!(
            serde_json::to_string(&exported).unwrap(),
            r#"{"type":"exportedSettings","contents":"{}"}"#
        );
    }
}
