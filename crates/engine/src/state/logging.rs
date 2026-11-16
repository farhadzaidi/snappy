//! Debug-log records, including state files that had to be moved aside.

use std::path::Path;

use snappy_platform::Platform;

use super::EngineState;
use crate::debug_log::Level;
use crate::log_event::LogEvent;
use crate::store::{ReadState, StateFile, read_state};

impl<P: Platform> EngineState<P> {
    /// Add a record to the debug log, building it only when the log is on.
    pub fn trace(&mut self, event: LogEvent, details: impl FnOnce() -> serde_json::Value) {
        self.log(Level::Info, event, details);
    }

    /// Like `trace`, for a result that differs from what was asked for.
    pub fn flag(&mut self, event: LogEvent, details: impl FnOnce() -> serde_json::Value) {
        self.log(Level::Warn, event, details);
    }

    /// Read a state file, logging one that had to be moved aside.
    pub fn read_state<T: StateFile>(&mut self, path: &Path) -> ReadState<T> {
        let read = read_state(path);
        if let ReadState::MovedAside(aside) = &read {
            let name = |path: &Path| path.file_name().map(|name| name.to_string_lossy().into_owned());
            let (file, moved_to) = (name(path), aside.as_deref().and_then(name));
            self.flag(
                LogEvent::StateFileUnreadable,
                || serde_json::json!({ "file": file, "movedTo": moved_to }),
            );
        }
        read
    }

    pub fn log(&mut self, level: Level, event: LogEvent, details: impl FnOnce() -> serde_json::Value) {
        if self.debug.is_on() {
            let now = self.platform.now();
            self.debug.record(now, level, event.as_str(), details());
        }
    }
}
