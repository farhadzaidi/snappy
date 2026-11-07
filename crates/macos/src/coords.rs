//! Conversions between Cocoa's global coordinates (origin at the primary display's
//! bottom-left, y up) and Snappy's (origin at its top-left, y down).
//!
//! Both directions are the same reflection about half the primary display's height.

use snappy_core::geometry::{Point, Rect};

/// Converts a point between Cocoa and top-left coordinates.
pub fn flip_point(point: Point, primary_height: f64) -> Point {
    Point::new(point.x, primary_height - point.y)
}

/// Converts a rectangle between Cocoa and top-left coordinates.
pub fn flip_rect(rect: Rect, primary_height: f64) -> Rect {
    Rect::new(
        rect.x,
        primary_height - rect.y - rect.height,
        rect.width,
        rect.height,
    )
}

/// The y of a child laid out `top` points below its container's top edge, in the
/// container's bottom-up layer coordinates.
pub fn layer_y(top: f64, height: f64, container_height: f64) -> f64 {
    container_height - top - height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_display_frame_maps_onto_itself() {
        let frame = Rect::new(0.0, 0.0, 1440.0, 900.0);
        assert_eq!(flip_rect(frame, 900.0), frame);
    }

    #[test]
    fn menu_bar_offset_work_area_moves_down() {
        let cocoa_visible = Rect::new(0.0, 0.0, 1440.0, 875.0);
        assert_eq!(
            flip_rect(cocoa_visible, 900.0),
            Rect::new(0.0, 25.0, 1440.0, 875.0)
        );
    }

    #[test]
    fn display_above_the_primary_has_negative_y() {
        let cocoa = Rect::new(0.0, 900.0, 1920.0, 1080.0);
        assert_eq!(flip_rect(cocoa, 900.0), Rect::new(0.0, -1080.0, 1920.0, 1080.0));
    }

    #[test]
    fn conversion_round_trips() {
        let rect = Rect::new(-300.0, 120.5, 640.0, 480.0);
        assert_eq!(flip_rect(flip_rect(rect, 1117.0), 1117.0), rect);
        let point = Point::new(10.0, 20.0);
        assert_eq!(flip_point(flip_point(point, 900.0), 900.0), point);
    }

    #[test]
    fn pointer_near_the_bottom_is_near_max_y() {
        assert_eq!(flip_point(Point::new(5.0, 1.0), 900.0), Point::new(5.0, 899.0));
    }

    #[test]
    fn layer_y_measures_from_the_bottom() {
        assert_eq!(layer_y(0.0, 100.0, 800.0), 700.0);
        assert_eq!(layer_y(700.0, 100.0, 800.0), 0.0);
    }
}
