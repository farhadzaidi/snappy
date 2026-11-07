//! Telling a window move apart from a click, a content selection, or a resize.

use crate::geometry::{Point, Rect};

/// The window must travel further than this before it counts as moved.
const MOVE_THRESHOLD: f64 = 2.0;
/// A size change this large means the user is resizing.
const RESIZE_THRESHOLD: f64 = 3.0;
/// The title-bar strip: inset from the left edge (past the window controls) and the right edge.
const MOVE_AREA_LEFT_INSET: f64 = 80.0;
const MOVE_AREA_RIGHT_INSET: f64 = 20.0;
/// The title-bar strip's top inset (past the resize edge) and its maximum height.
const MOVE_AREA_TOP_INSET: f64 = 8.0;
const MOVE_AREA_MAX_HEIGHT: f64 = 44.0;

/// Why a drag can no longer snap. The names appear in drop diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    ResizeGesture,
    InvalidFrame,
    Escape,
}

impl CancelReason {
    pub fn as_str(self) -> &'static str {
        match self {
            CancelReason::ResizeGesture => "resizeGesture",
            CancelReason::InvalidFrame => "invalidFrame",
            CancelReason::Escape => "escape",
        }
    }
}

/// What one press-and-drag of a window is doing, updated as its frame changes.
#[derive(Debug, Clone, PartialEq)]
pub struct DragIntent {
    original: Rect,
    from_title_bar: bool,
    moved: bool,
    cancelled: Option<CancelReason>,
}

impl DragIntent {
    /// Start tracking a press on a window whose frame is `original`. `pointer` is where
    /// the press landed, if known.
    pub fn new(original: Rect, pointer: Option<Point>) -> Self {
        let from_title_bar = pointer.is_some_and(|point| in_move_area(point, original));
        Self {
            original,
            from_title_bar,
            moved: false,
            cancelled: None,
        }
    }

    pub fn original(&self) -> Rect {
        self.original
    }

    pub fn started_in_move_area(&self) -> bool {
        self.from_title_bar
    }

    pub fn has_moved(&self) -> bool {
        self.moved
    }

    pub fn cancellation(&self) -> Option<CancelReason> {
        self.cancelled
    }

    /// Record the window's current frame. A window back at its starting position no
    /// longer counts as moved.
    pub fn observe(&mut self, frame: Rect) {
        if self.cancelled.is_some() {
            return;
        }
        if !is_real(frame) || !is_real(self.original) {
            self.cancel(CancelReason::InvalidFrame);
            return;
        }
        let resized = (frame.width - self.original.width).abs() >= RESIZE_THRESHOLD
            || (frame.height - self.original.height).abs() >= RESIZE_THRESHOLD;
        if resized && !self.from_title_bar {
            self.cancel(CancelReason::ResizeGesture);
            return;
        }
        self.moved = (frame.x - self.original.x).abs() > MOVE_THRESHOLD
            || (frame.y - self.original.y).abs() > MOVE_THRESHOLD;
    }

    /// Cancellation lasts until the next press starts a new intent.
    pub fn cancel(&mut self, reason: CancelReason) {
        self.cancelled = Some(reason);
        self.moved = false;
    }

    pub fn permits_snap(&self, modifier_held: bool) -> bool {
        modifier_held && self.moved && self.cancelled.is_none()
    }
}

/// True when `point` is on the window's title-bar strip, where apps may legitimately
/// change the window's size during a move.
pub fn in_move_area(point: Point, frame: Rect) -> bool {
    if !is_real(frame) || !point.is_finite() {
        return false;
    }
    let bottom = frame.min_y() + MOVE_AREA_MAX_HEIGHT.min(frame.height / 3.0);
    (frame.min_x() + MOVE_AREA_LEFT_INSET..=frame.max_x() - MOVE_AREA_RIGHT_INSET).contains(&point.x)
        && (frame.min_y() + MOVE_AREA_TOP_INSET..=bottom).contains(&point.y)
}

fn is_real(frame: Rect) -> bool {
    frame.is_finite() && frame.width > 0.0 && frame.height > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Rect = Rect::new(100.0, 100.0, 800.0, 600.0);
    const TITLE_BAR: Point = Point::new(400.0, 115.0);
    const CONTENT: Point = Point::new(400.0, 400.0);

    #[test]
    fn a_press_that_never_moves_cannot_snap() {
        let mut intent = DragIntent::new(WINDOW, Some(CONTENT));
        intent.observe(WINDOW);
        assert!(!intent.permits_snap(true));
    }

    #[test]
    fn small_jitter_is_not_a_move() {
        let mut intent = DragIntent::new(WINDOW, Some(TITLE_BAR));
        intent.observe(Rect::new(102.0, 101.0, 800.0, 600.0));
        assert!(!intent.permits_snap(true));
        intent.observe(Rect::new(140.0, 101.0, 800.0, 600.0));
        assert!(intent.permits_snap(true));
    }

    #[test]
    fn the_modifier_is_required_at_drop() {
        let mut intent = DragIntent::new(WINDOW, Some(TITLE_BAR));
        intent.observe(Rect::new(300.0, 100.0, 800.0, 600.0));
        assert!(!intent.permits_snap(false));
        assert!(intent.permits_snap(true));
    }

    #[test]
    fn title_bar_moves_tolerate_size_changes() {
        let mut intent = DragIntent::new(WINDOW, Some(TITLE_BAR));
        intent.observe(Rect::new(300.0, 120.0, 700.0, 500.0));
        assert!(intent.permits_snap(true));
    }

    #[test]
    fn resizing_outside_the_title_bar_cancels_for_good() {
        let mut intent = DragIntent::new(WINDOW, Some(Point::new(102.0, 400.0)));
        intent.observe(Rect::new(90.0, 100.0, 810.0, 600.0));
        assert_eq!(intent.cancellation(), Some(CancelReason::ResizeGesture));
        intent.observe(Rect::new(300.0, 100.0, 800.0, 600.0));
        assert!(!intent.permits_snap(true));
    }

    #[test]
    fn escape_cancels_until_the_next_press() {
        let mut intent = DragIntent::new(WINDOW, Some(TITLE_BAR));
        intent.observe(Rect::new(300.0, 100.0, 800.0, 600.0));
        intent.cancel(CancelReason::Escape);
        intent.observe(Rect::new(400.0, 100.0, 800.0, 600.0));
        assert!(!intent.permits_snap(true));
    }

    #[test]
    fn returning_to_the_start_does_not_snap() {
        let mut intent = DragIntent::new(WINDOW, Some(TITLE_BAR));
        intent.observe(Rect::new(300.0, 100.0, 800.0, 600.0));
        intent.observe(WINDOW);
        assert!(!intent.permits_snap(true));
    }

    #[test]
    fn invalid_frames_cancel() {
        let mut intent = DragIntent::new(WINDOW, None);
        intent.observe(Rect::new(f64::NAN, 0.0, 10.0, 10.0));
        assert_eq!(intent.cancellation(), Some(CancelReason::InvalidFrame));
    }
}
