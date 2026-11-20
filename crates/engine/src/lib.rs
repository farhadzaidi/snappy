//! The engine: the single owner of Snappy's runtime state. It turns platform events and
//! app commands into decisions and window operations.
//!
//! The engine runs on one thread inside a Tokio `LocalSet`. Window operations are tasks on
//! that set and run one at a time; a new operation cancels the one in flight.

mod actions;
mod caps_lock;
mod commands;
mod debug_log;
mod diagnostics;
mod drag;
mod engine;
mod events;
mod focus;
mod log_event;
mod marks;
mod ops;
mod outcome;
mod selection;
mod send_to_back;
mod state;
mod store;
mod workspaces;

pub use crate::diagnostics::SwitchTiming;
pub use crate::engine::{Engine, Input};
pub use crate::log_event::LogEvent;
pub use crate::store::{Paths, file_names};
