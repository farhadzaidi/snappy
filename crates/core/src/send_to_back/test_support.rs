//! Frames and windows the send to back tests share.

use super::StackedWindow;
use crate::geometry::Rect;
use crate::window::WindowId;

/// A zone, and the zone beside it.
pub const ZONE: Rect = Rect::new(12.0, 37.0, 702.0, 851.0);
pub const NEIGHBOR: Rect = Rect::new(714.0, 37.0, 702.0, 851.0);

/// Window `id` at `frame`, of an app of its own.
pub fn window(id: u64, frame: Rect) -> StackedWindow {
    StackedWindow {
        id: WindowId(id),
        pid: id as u32 * 100,
        frame,
    }
}

pub fn ids(windows: impl IntoIterator<Item = WindowId>) -> Vec<u64> {
    windows.into_iter().map(|id| id.0).collect()
}
