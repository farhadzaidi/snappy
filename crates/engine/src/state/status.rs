//! The status message the app shows.

use snappy_ipc::Status;
use snappy_platform::Platform;

use super::EngineState;
use crate::log_event::LogEvent;

impl<P: Platform> EngineState<P> {
    /// Report a message the app shows.
    pub fn announce(&mut self, message: &str) {
        self.post(message, false, true);
    }

    /// Report a routine confirmation the app doesn't show; it clears a visible
    /// error.
    pub fn announce_quietly(&mut self, message: &str) {
        self.post(message, false, false);
    }

    pub fn announce_error(&mut self, message: &str) {
        self.post(message, true, true);
    }

    pub fn status(&self) -> Option<&Status> {
        self.status.as_ref()
    }

    pub fn dismiss_status(&mut self) {
        self.status = None;
    }

    fn post(&mut self, message: &str, is_error: bool, notify: bool) {
        let details = || serde_json::json!({ "message": message });
        if is_error {
            self.flag(LogEvent::Status, details);
        } else {
            self.trace(LogEvent::Status, details);
        }
        self.status_counter += 1;
        self.status = Some(Status {
            id: self.status_counter,
            message: message.to_owned(),
            is_error,
            notify,
        });
        self.publish_state();
    }
}
