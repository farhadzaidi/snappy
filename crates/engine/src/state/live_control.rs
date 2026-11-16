//! Whether live control runs, and reconfiguring what depends on it when that changes.

use snappy_platform::Platform;

use super::EngineState;
use crate::log_event::LogEvent;
use crate::{caps_lock, selection};

impl<P: Platform> EngineState<P> {
    pub fn is_live(&self) -> bool {
        self.live
    }

    pub fn has_permission(&self) -> bool {
        self.permission_granted
    }

    /// Recompute whether live control runs, and reconfigure what depends on it.
    pub fn refresh_live(&mut self, announce: bool) {
        self.permission_granted = !self.capabilities.needs_permission || self.platform.is_granted();
        let should_run =
            self.settings.snapping.enabled && self.permission_granted && self.live_control.is_available();
        let waiting = self.settings.snapping.enabled && !self.permission_granted;
        self.platform.watch(waiting);

        if should_run != self.live {
            self.live = should_run;
            let (enabled, granted) = (self.settings.snapping.enabled, self.permission_granted);
            self.trace(LogEvent::Snapping, || {
                serde_json::json!({ "live": should_run, "enabled": enabled, "permissionGranted": granted })
            });
            if !should_run {
                self.end_drag();
                // Its shortcuts go with live control, so nothing could clear its outlines.
                selection::clear(self, "live control off");
            }
            if announce {
                let reason = self.live_control.reason().map(str::to_owned);
                match (should_run, reason) {
                    (true, _) => self.announce("Snapping is on."),
                    // The settings window already shows the reason where it stays.
                    (false, Some(reason)) => self.announce_quietly(&reason),
                    (false, None) => self.announce("Accessibility access is off. Snapping is paused."),
                }
            }
        }
        self.platform
            .set_drag_modifier(self.settings.snapping.drag_modifier.modifier());
        self.platform.set_press_monitoring(self.live);
        self.configure_hotkeys();
        caps_lock::configure(self);
        self.prepare_overlays();
        self.update_tray();
    }
}
