//! The overlays' shared style in the Linux overlays' colors and window units: sizes are
//! given in points and drawn in the display's window units, the points multiplied by its
//! scale. Rectangles are relative to the overlay's top-left corner, which is the work area's.

use snappy_core::geometry::{Point, Rect};
use snappy_platform::OverlayZone;
use snappy_platform::overlay_style::{
    MARK_BORDER_WIDTH, Rgb, Tint, ZONE_CORNER_RADIUS, place_zone, zone_style,
};

/// Close to the corners of the desktops' own windows.
pub const MARK_CORNER_RADIUS: f64 = 10.0;

/// Linux overlays use the documented default accent, not a system one: X11 has none to
/// read, and GNOME 46 has none either.
pub fn rgb(tint: Tint) -> Rgb {
    tint.rgb()
}

/// Where a marked window's outline goes, in pixels relative to the work area: its frame,
/// with the border inside it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkPlacement {
    pub shape: Rect,
    pub corner_radius: f64,
    pub border_width: f64,
}

pub fn place_mark(frame: Rect, work_area: Rect, scale: f64) -> MarkPlacement {
    let shape = Rect::new(
        frame.x - work_area.x,
        frame.y - work_area.y,
        frame.width,
        frame.height,
    );
    MarkPlacement {
        shape,
        corner_radius: fitted_radius(MARK_CORNER_RADIUS * scale, shape),
        border_width: MARK_BORDER_WIDTH * scale,
    }
}

/// Where a zone's shape, number, and caption go, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub shape: Rect,
    pub corner_radius: f64,
    pub border_width: f64,
    pub number_center: Point,
    pub number_size: f64,
    /// The caption's center and font size.
    pub caption: Option<(Point, f64)>,
}

pub fn place(zone: &OverlayZone, work_area: Rect, scale: f64) -> Placement {
    let placed = place_zone(zone, work_area, scale);
    let shape = placed.shape;
    Placement {
        shape,
        corner_radius: fitted_radius(ZONE_CORNER_RADIUS * scale, shape),
        border_width: zone_style(zone).border_width * scale,
        number_center: placed.number_center,
        number_size: placed.number_size,
        caption: placed.caption.map(|caption| {
            let center = Point::new(shape.mid_x(), caption.top + caption.size / 2.0);
            (center, caption.size)
        }),
    }
}

/// `radius`, but no more than half of `shape`'s shorter side.
fn fitted_radius(radius: f64, shape: Rect) -> f64 {
    radius.min(shape.width / 2.0).min(shape.height / 2.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use snappy_platform::Caption;

    use super::*;

    fn zone() -> OverlayZone {
        OverlayZone {
            frame: Rect::new(112.0, 137.0, 400.0, 300.0),
            number: 1,
            highlighted: false,
            caption: None,
            swap_source: false,
        }
    }

    #[test]
    fn a_marked_windows_outline_is_its_frame_in_the_work_area_with_scaled_lengths() {
        let mark = place_mark(
            Rect::new(300.0, 200.0, 600.0, 400.0),
            Rect::new(100.0, 25.0, 1000.0, 800.0),
            2.0,
        );
        assert_eq!(mark.shape, Rect::new(200.0, 175.0, 600.0, 400.0));
        assert_eq!((mark.border_width, mark.corner_radius), (6.0, 20.0));
    }

    #[test]
    fn colors_are_the_palettes_own_with_the_default_accent() {
        assert_eq!(rgb(Tint::Accent), Rgb::new(0x00, 0x7A, 0xFF));
        assert_eq!(rgb(Tint::Mint), Rgb::new(0x00, 0xC7, 0xBE));
    }

    #[test]
    fn zones_are_placed_in_the_work_area_and_inset() {
        let placement = place(&zone(), Rect::new(100.0, 125.0, 1000.0, 800.0), 1.0);
        assert_eq!(placement.shape, Rect::new(13.0, 13.0, 398.0, 298.0));
        assert_eq!(placement.number_center, Point::new(212.0, 162.0));
        assert_eq!(placement.corner_radius, 16.0);
        assert_eq!(placement.caption, None);
    }

    #[test]
    fn corners_never_round_past_half_a_small_zone() {
        let small = OverlayZone {
            frame: Rect::new(100.0, 125.0, 22.0, 102.0),
            ..zone()
        };
        let placement = place(&small, Rect::new(100.0, 125.0, 1000.0, 800.0), 1.0);
        assert_eq!(placement.corner_radius, 10.0);
    }

    #[test]
    fn a_caption_sits_below_center_and_raises_the_number() {
        let swap = OverlayZone {
            highlighted: true,
            caption: Some(Caption::Stack),
            ..zone()
        };
        let placement = place(&swap, Rect::new(100.0, 125.0, 1000.0, 800.0), 2.0);
        assert_eq!(placement.number_center.y, 162.0 - 16.0);
        assert_eq!(
            placement.caption,
            Some((Point::new(212.0, 162.0 + 28.0 + 13.0), 26.0))
        );
        assert_eq!(placement.border_width, 8.0);
    }
}
