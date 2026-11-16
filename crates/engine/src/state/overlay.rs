//! The zone overlay: prepared scenes, showing and hiding it, and the settings app's preview.

use snappy_core::layout::Layout;
use snappy_platform::{Caption, Display, OverlayScene, OverlayZone, Platform};

use super::{DragSession, EngineState};
use crate::log_event::LogEvent;

impl<P: Platform> EngineState<P> {
    pub fn prepare_overlays(&mut self) {
        if !self.live {
            return;
        }
        let ids = self.display_ids();
        self.platform.retain(&ids);
        for display in self.displays.clone() {
            let layout = self.settings.layout_for_display(&display.id).clone();
            let scene = self.scene(&layout, &display, None, None, None);
            self.platform.prepare(&display.id, &scene);
        }
    }

    /// What a display's overlay shows: `selected` highlighted with `caption` under its
    /// number, and the zone `swap_source` outlined.
    pub fn scene(
        &self,
        layout: &Layout,
        display: &Display,
        selected: Option<&str>,
        caption: Option<Caption>,
        swap_source: Option<&str>,
    ) -> OverlayScene {
        let frames = self.zone_frames(layout, display);
        let zones = layout
            .zones
            .iter()
            .zip(frames)
            .enumerate()
            .map(|(index, (zone, frame))| {
                let highlighted = selected == Some(zone.id.as_str());
                OverlayZone {
                    frame,
                    number: index + 1,
                    highlighted,
                    caption: caption.filter(|_| highlighted),
                    swap_source: !highlighted && swap_source == Some(zone.id.as_str()),
                }
            })
            .collect();
        OverlayScene {
            work_area: display.work_area,
            zones,
        }
    }

    /// Show the browsed layout on a display, or hide the preview.
    pub fn set_preview(&mut self, preview: Option<(String, String)>) {
        if self.preview == preview {
            return;
        }
        self.preview = preview;
        match &self.preview {
            Some((display_id, layout_id)) => {
                let display = self.displays.iter().find(|d| &d.id == display_id).cloned();
                let layout = self.settings.layout(layout_id).cloned();
                if let (Some(display), Some(layout)) = (display, layout) {
                    let scene = self.scene(&layout, &display, None, None, None);
                    self.show_overlay(&display.id, &scene);
                }
            }
            None => self.hide_overlay(),
        }
        self.publish_state();
    }

    /// Shows a display's overlay, and logs when an overlay appears or moves to another display.
    pub fn show_overlay(&mut self, display: &str, scene: &OverlayScene) {
        self.platform.show(display, scene);
        if self.overlay_on.as_deref() != Some(display) {
            self.overlay_on = Some(display.to_owned());
            self.trace(
                LogEvent::OverlayShown,
                || serde_json::json!({ "display": display }),
            );
        }
    }

    /// Hides the overlay, and logs it when one was showing.
    pub fn hide_overlay(&mut self) {
        self.platform.hide();
        if self.overlay_on.take().is_some() {
            self.trace(LogEvent::OverlayHidden, || serde_json::json!({}));
        }
    }

    /// Forget the current press; the overlay disappears.
    pub fn end_drag(&mut self) {
        self.drag = DragSession::Idle;
        if self.preview.is_none() {
            self.hide_overlay();
        }
        self.platform.set_drag_monitoring(false);
    }
}
