//! What is on screen: the displays, their zone frames, and the windows showing on them.

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::Os;
use snappy_core::layout::{Layout, Spacing, zone_frame};
use snappy_core::window::WindowSnapshot;
use snappy_platform::{Display, Platform, WindowId};

use super::EngineState;

impl<P: Platform> EngineState<P> {
    pub fn display_ids(&self) -> Vec<String> {
        self.displays.iter().map(|display| display.id.clone()).collect()
    }

    pub fn display_frames(&self) -> Vec<Rect> {
        self.displays.iter().map(|display| display.frame).collect()
    }

    pub fn work_areas(&self) -> Vec<Rect> {
        self.displays.iter().map(|display| display.work_area).collect()
    }

    /// The display under `point`, by full frame.
    pub fn display_at(&self, point: Point) -> Option<&Display> {
        self.displays.iter().find(|display| display.frame.contains(point))
    }

    /// The display under the pointer, else the first, for shortcuts that act on a display
    /// rather than a window.
    pub fn display_under_pointer(&mut self) -> Option<&Display> {
        let pointer = self.platform.pointer();
        self.display_at(pointer).or_else(|| self.displays.first())
    }

    /// Whether `window` is on screen: on the active workspace, not minimized, and not on
    /// the parking desktop.
    pub fn is_on_screen(&mut self, window: WindowId) -> bool {
        !self.workspaces.is_inactive(window)
            && self
                .platform
                .state(window)
                .is_ok_and(|state| !state.minimized && !state.on_parking_desktop)
    }

    /// The windows filling any of `frames` that count as occupants: those of workspaces
    /// that aren't showing never do.
    pub fn occupants(&mut self, frames: &[Rect]) -> Result<Vec<WindowSnapshot>, WindowError> {
        let found = self.platform.windows_occupying(frames)?;
        Ok(found
            .iter()
            .filter(|window| !self.workspaces.is_inactive(window.id))
            .map(|window| WindowSnapshot {
                id: window.id,
                frame: window.frame,
            })
            .collect())
    }

    /// The display a window overlaps most.
    pub fn display_of(&self, frame: Rect) -> Option<&Display> {
        snappy_core::geometry::largest_overlap(frame, &self.display_frames())
            .map(|index| &self.displays[index])
    }

    /// The gap and padding for a display, in its window units: points on macOS, physical
    /// pixels elsewhere.
    pub fn spacing(&self, display: &Display) -> Spacing {
        match self.capabilities.platform {
            Os::MacOs => Spacing::STANDARD,
            Os::Windows | Os::Linux => Spacing::STANDARD.scaled(display.scale),
        }
    }

    pub fn zone_frames(&self, layout: &Layout, display: &Display) -> Vec<Rect> {
        let spacing = self.spacing(display);
        layout
            .zones
            .iter()
            .map(|zone| zone_frame(zone.rect, display.work_area, spacing))
            .collect()
    }
}
