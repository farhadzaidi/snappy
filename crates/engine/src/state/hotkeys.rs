//! Registering the shortcuts the current state needs, and telling the user about taken ones.

use snappy_core::shortcuts::{HotKey, desired_hotkeys};
use snappy_platform::{HotKeyRequest, Platform};

use super::EngineState;
use crate::actions::action_record;
use crate::log_event::LogEvent;
use crate::send_to_back;

impl<P: Platform> EngineState<P> {
    /// Takes no shortcut as held, so the next `configure_hotkeys` registers every one again
    /// and reports any another program took meanwhile, as after the desktop shell that holds
    /// them started over.
    pub fn forget_registered_hotkeys(&mut self) {
        self.registered.clear();
    }

    /// Register the shortcuts the current state needs.
    pub fn configure_hotkeys(&mut self) {
        let allowed = self.permission_granted && !self.recording && self.live_control.is_available();
        let wanted = if allowed {
            desired_hotkeys(&self.settings, &self.display_ids(), self.live)
        } else {
            Vec::new()
        };
        if wanted == self.registered && !self.registration_failed {
            return;
        }
        // The platform forgets a shortcut held, so its release never comes.
        send_to_back::cancel_put_back(self);
        let shortcuts: Vec<HotKeyRequest> = wanted
            .iter()
            .enumerate()
            .map(|(i, hotkey)| HotKeyRequest {
                id: i as u32,
                combo: hotkey.combo.clone(),
                reports_release: hotkey.action.reports_release(),
            })
            .collect();
        let refused = if shortcuts.is_empty() {
            self.platform.unregister_all();
            Vec::new()
        } else {
            self.platform.register(&shortcuts)
        };
        self.registration_failed = !refused.is_empty();
        let platform = self.platform_name();
        self.trace(LogEvent::ShortcutsRegistered, || {
            let shortcuts: Vec<serde_json::Value> = wanted
                .iter()
                .map(|hotkey| {
                    let mut record = action_record(&hotkey.action);
                    record.insert("chord".to_owned(), hotkey.chord.long_label(platform).into());
                    serde_json::Value::Object(record)
                })
                .collect();
            serde_json::json!({ "shortcuts": shortcuts })
        });
        if self.registration_failed {
            let labels: Vec<String> = refused
                .iter()
                .filter_map(|id| wanted.get(*id as usize))
                .map(|hotkey| hotkey.chord.long_label(self.platform_name()))
                .collect();
            let all = labels.clone();
            self.trace(
                LogEvent::ShortcutsTaken,
                || serde_json::json!({ "shortcuts": all }),
            );
            self.announce_error(&taken_message(&labels));
        }
        self.registered = wanted;
    }

    /// The registered shortcut with this ID.
    pub fn hotkey(&self, id: u32) -> Option<&HotKey> {
        self.registered.get(id as usize)
    }

    /// Whether the platform refused any shortcut the last time they were registered.
    pub fn shortcuts_refused(&self) -> bool {
        self.registration_failed
    }

    /// Unregister every shortcut, as asking the OS about chords Snappy holds needs. The
    /// next `configure_hotkeys` registers them again.
    pub fn release_hotkeys(&mut self) {
        self.platform.unregister_all();
        self.registered.clear();
    }
}

/// Most chords a message about taken shortcuts names; past that it counts them, and the
/// debug log has the list.
const TAKEN_LISTED: usize = 3;

/// What the user is told when another app holds some of Snappy's shortcuts, short enough
/// to read at a glance.
fn taken_message(labels: &[String]) -> String {
    match labels {
        [one] => format!("Another app is using {one}."),
        [first, second] => format!("Another app is using {first} and {second}."),
        [rest @ .., last] if labels.len() <= TAKEN_LISTED => {
            format!("Another app is using {}, and {last}.", rest.join(", "))
        }
        _ => format!("Another app is using {} of Snappy's shortcuts.", labels.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taken_shortcuts_are_named_when_few_and_counted_when_many() {
        let labels = |count: usize| -> Vec<String> { (1..=count).map(|n| format!("Snappy + {n}")).collect() };
        assert_eq!(taken_message(&labels(1)), "Another app is using Snappy + 1.");
        assert_eq!(
            taken_message(&labels(2)),
            "Another app is using Snappy + 1 and Snappy + 2."
        );
        assert_eq!(
            taken_message(&labels(3)),
            "Another app is using Snappy + 1, Snappy + 2, and Snappy + 3."
        );
        assert_eq!(
            taken_message(&labels(10)),
            "Another app is using 10 of Snappy's shortcuts."
        );
    }
}
