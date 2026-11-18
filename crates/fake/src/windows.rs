//! Windows and displays on the fake platform.

use std::collections::BTreeSet;
use std::time::Duration;

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_platform::{
    Display, HiddenTab, TabEvidence, TabGroup, WindowId, WindowInfo, WindowState, WindowSystem,
};

use crate::platform::FakePlatform;

impl WindowSystem for FakePlatform {
    fn displays(&mut self) -> Vec<Display> {
        self.displays.clone()
    }

    fn window_at(&mut self, point: Point) -> Result<WindowInfo, WindowError> {
        let id = self
            .stacking
            .iter()
            .copied()
            .find(|id| {
                self.windows
                    .get(id)
                    .is_some_and(|w| w.is_shown() && w.frame.contains(point))
            })
            .ok_or_else(|| WindowError::unavailable("No window at the pointer."))?;
        self.eligible(id)
    }

    fn focused_window(&mut self) -> Result<WindowInfo, WindowError> {
        let id = self
            .focused
            .ok_or_else(|| WindowError::unavailable("No movable window is focused."))?;
        self.eligible(id)
    }

    fn frame(&mut self, window: WindowId) -> Result<Rect, WindowError> {
        Ok(self.window(window)?.frame)
    }

    fn set_size(&mut self, window: WindowId, size: Size) -> Result<(), WindowError> {
        self.writes.push((window, "size"));
        let frame = self.window(window)?.frame;
        let on_display = self.fully_on_a_display(frame);
        let target = self.window_mut(window)?;
        if target.frozen || (target.strict_resize && !on_display) {
            return Ok(());
        }
        let accepted = Size::new(
            size.width.max(target.min_size.width),
            size.height.max(target.min_size.height),
        );
        target.frame = Rect::from_origin_size(target.frame.origin(), accepted);
        Ok(())
    }

    fn set_position(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError> {
        self.writes.push((window, "position"));
        let target = self.window_mut(window)?;
        if !target.frozen {
            target.frame = target.frame.with_origin(origin);
        }
        Ok(())
    }

    fn state(&mut self, window: WindowId) -> Result<WindowState, WindowError> {
        Ok(self.window(window)?.state)
    }

    fn is_closed(&mut self, window: WindowId) -> bool {
        !self.windows.contains_key(&window)
    }

    fn visible_windows(&mut self, app: Option<&str>) -> Vec<WindowInfo> {
        self.windows
            .iter()
            .filter(|(_, w)| w.is_shown() && !w.state.minimized && app.is_none_or(|app| w.app == app))
            .filter(|(_, w)| !w.state.full_screen)
            .filter_map(|(id, _)| self.info(*id))
            .collect()
    }

    /// Like macOS, hidden tabs are listed only while their group is minimized.
    fn all_windows(&mut self) -> Vec<WindowInfo> {
        self.window_listings += 1;
        self.windows
            .iter()
            .filter(|(_, window)| window.tab_of.is_none() || window.state.minimized)
            .filter_map(|(id, _)| self.info(*id))
            .collect()
    }

    fn windows_occupying(&mut self, frames: &[Rect]) -> Result<Vec<WindowInfo>, WindowError> {
        self.occupancy_queries += 1;
        let mut found = Vec::new();
        for (id, window) in &self.windows {
            let fills = frames
                .iter()
                .any(|frame| snappy_core::placement::swap::fills_zone(window.frame, *frame));
            if fills && window.is_shown() {
                found.extend(self.info(*id));
            }
        }
        Ok(found)
    }

    /// The fake knows its groups, so every hidden tab it reports is one it remembers.
    fn tab_groups(&mut self, windows: &[WindowId]) -> Vec<TabGroup> {
        let pids: BTreeSet<u32> = windows
            .iter()
            .filter_map(|id| self.windows.get(id).map(|window| window.pid))
            .collect();
        self.windows
            .iter()
            .filter(|(_, window)| window.tab_of.is_none() && pids.contains(&window.pid))
            .filter_map(|(id, _)| {
                let hidden: Vec<HiddenTab> = self
                    .windows
                    .iter()
                    .filter(|(_, window)| window.tab_of == Some(*id))
                    .map(|(hidden, _)| HiddenTab {
                        window: *hidden,
                        evidence: TabEvidence::Remembered,
                    })
                    .collect();
                let shown = self.info(*id)?;
                (!hidden.is_empty()).then_some(TabGroup {
                    shown,
                    tabs: hidden.len() + 1,
                    hidden,
                })
            })
            .collect()
    }

    /// Focusing raises the window, as every platform does. Focusing a hidden tab shows it.
    fn focus(&mut self, window: WindowId) {
        if self
            .windows
            .get(&window)
            .is_some_and(|target| target.tab_of.is_some())
        {
            return self.select_tab(window);
        }
        self.focused = Some(window);
        self.stacking.retain(|id| *id != window);
        self.stacking.insert(0, window);
    }

    fn begin_bounds_change(&mut self, window: WindowId) {
        self.writes.push((window, "beginBoundsChange"));
    }

    fn end_bounds_change(&mut self, window: WindowId) -> Duration {
        self.writes.push((window, "endBoundsChange"));
        self.restore_delay
    }

    fn hide_for_workspace(&mut self, window: WindowId, parked: Point) -> Result<(), WindowError> {
        if self.parking.moves_windows() {
            return self.set_position(window, parked);
        }
        self.set_parking_desktop(window, true)
    }

    fn show_for_workspace(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError> {
        self.set_position(window, origin)?;
        if self.parking.moves_windows() {
            return Ok(());
        }
        self.set_parking_desktop(window, false)
    }

    fn activate_self(&mut self) {
        self.self_activations += 1;
        self.focused = None;
    }

    fn pointer(&mut self) -> Point {
        self.pointer
    }
}
