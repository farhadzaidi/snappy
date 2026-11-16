//! Visible frames and window rectangles. `GetWindowRect` includes the invisible resize
//! borders DWM draws around a window; the frame users see is the DWM extended frame
//! bounds. Writes add back the per-window difference.

use snappy_core::geometry::{Point, Rect, Size, round};

/// How far the window rectangle extends beyond the visible frame on each side.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Borders {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Borders {
    pub fn between(window: Rect, visible: Rect) -> Borders {
        Borders {
            left: visible.min_x() - window.min_x(),
            top: visible.min_y() - window.min_y(),
            right: window.max_x() - visible.max_x(),
            bottom: window.max_y() - visible.max_y(),
        }
    }

    /// Where the window rectangle goes so the visible frame starts at `origin`.
    pub fn window_origin(&self, origin: Point) -> Point {
        Point::new(origin.x - self.left, origin.y - self.top)
    }

    /// The window size that gives a visible frame of `size`.
    pub fn window_size(&self, size: Size) -> Size {
        Size::new(
            size.width + self.left + self.right,
            size.height + self.top + self.bottom,
        )
    }
}

/// A coordinate as `SetWindowPos` takes it, or `None` when it isn't representable.
pub fn pixel(value: f64) -> Option<i32> {
    let value = round(value);
    (value.is_finite() && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX))
        .then_some(value as i32)
}

/// A width or height as `SetWindowPos` takes it.
pub fn extent(value: f64) -> Option<i32> {
    pixel(value).filter(|extent| *extent >= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_compensate_for_invisible_borders() {
        let window = Rect::new(93.0, 100.0, 814.0, 607.0);
        let visible = Rect::new(100.0, 100.0, 800.0, 600.0);
        let borders = Borders::between(window, visible);
        assert_eq!(
            borders,
            Borders {
                left: 7.0,
                top: 0.0,
                right: 7.0,
                bottom: 7.0
            }
        );
        assert_eq!(
            borders.window_origin(Point::new(12.0, 12.0)),
            Point::new(5.0, 12.0)
        );
        assert_eq!(
            borders.window_size(Size::new(940.0, 1020.0)),
            Size::new(954.0, 1027.0)
        );
    }

    #[test]
    fn windows_without_borders_write_their_frame_unchanged() {
        let frame = Rect::new(0.0, 0.0, 640.0, 480.0);
        let borders = Borders::between(frame, frame);
        assert_eq!(borders.window_origin(Point::new(3.0, 4.0)), Point::new(3.0, 4.0));
    }

    #[test]
    fn pixels_round_and_refuse_unrepresentable_values() {
        assert_eq!(pixel(-12.5), Some(-13));
        assert_eq!(pixel(f64::NAN), None);
        assert_eq!(pixel(1e12), None);
        assert_eq!(extent(0.4), None);
        assert_eq!(extent(800.4), Some(800));
    }
}
