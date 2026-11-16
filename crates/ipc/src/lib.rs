//! Messages between the daemon and the settings app, and their framing: a 4-byte
//! little-endian length followed by UTF-8 JSON. The daemon starts the settings app with
//! piped standard input and output, so the pipes are the only channel between them.

mod app_state;
mod command;
mod framing;
mod messages;

pub use app_state::{AppState, SnappingState, Status, WorkspacesState};
pub use command::Command;
pub use framing::{DAEMON_ARGUMENT, FrameError, MAX_MESSAGE_BYTES, read_message, write_message};
pub use messages::{PROTOCOL_VERSION, ToDaemon, ToSettings};
