//! The overlays' shared style in Windows' colors and pixels: sizes are given in points and
//! drawn in pixels, multiplied by the display's scale.

use snappy_core::geometry::{Point, Rect};
use snappy_platform::OverlayZone;
use snappy_platform::overlay_style::{
    Paint, TEXT, TEXT_SHADOW, Tint, ZONE_CORNER_RADIUS, place_zone, zone_style,
};

use super::canvas::Color;

/// The documented accent when the system's can't be read.
pub const DEFAULT_ACCENT: u32 = Tint::Accent.rgb().hex();
/// Font weights: medium for numbers, semibold for the caption.
pub const NUMBER_WEIGHT: u32 = 500;
pub const CAPTION_WEIGHT: u32 = 600;
/// Close to Windows 11's own window corners.
pub const MARK_CORNER_RADIUS: f64 = 8.0;

pub const TEXT_COLOR: Color = color(TEXT, DEFAULT_ACCENT);
pub const SHADOW_COLOR: Color = color(TEXT_SHADOW, DEFAULT_ACCENT);

/// `paint` as a canvas color, with `accent` standing in for the accent tint.
pub const fn color(paint: Paint, accent: u32) -> Color {
    let hex = match paint.tint {
        Tint::Accent => accent,
        tint => tint.rgb().hex(),
    };
    Color::rgb(hex, paint.alpha as f32)
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

/// Places a zone whose global frame lies in an overlay covering `work_area`.
pub fn placement(zone: &OverlayZone, work_area: Rect, scale: f64) -> Placement {
    let placed = place_zone(zone, work_area, scale);
    let center = placed.shape.center();
    Placement {
        shape: placed.shape,
        corner_radius: ZONE_CORNER_RADIUS * scale,
        border_width: zone_style(zone).border_width * scale,
        number_center: placed.number_center,
        number_size: placed.number_size,
        // Centered on the line the other platforms hang the caption from.
        caption: placed
            .caption
            .map(|caption| (Point::new(center.x, caption.top), caption.size)),
    }
}

#[cfg(test)]
mod tests {
    use snappy_platform::overlay_style::mark_border;
    use snappy_platform::{Caption, MarkStyle};

    use super::*;

    fn zone(highlighted: bool, captioned: bool, swap_source: bool) -> OverlayZone {
        OverlayZone {
            frame: Rect::new(0.0, 0.0, 400.0, 300.0),
            number: 1,
            highlighted,
            caption: captioned.then_some(Caption::Stack),
            swap_source,
        }
    }

    #[test]
    fn the_accent_tint_takes_the_systems_accent_and_the_others_keep_their_colors() {
        let plain = zone_style(&zone(false, false, false));
        assert_eq!(color(plain.fill, 0x123456), Color::rgb(0x123456, 0.12));
        assert_eq!(color(plain.border, 0x123456), Color::rgb(0xFFFFFF, 0.60));
        let selected = color(mark_border(MarkStyle::Selected), DEFAULT_ACCENT);
        assert_eq!(selected, Color::rgb(0x00C7BE, 1.0));
        let focused = color(mark_border(MarkStyle::Focused), DEFAULT_ACCENT);
        assert_ne!(focused, selected);
        assert_ne!(focused, Color::rgb(DEFAULT_ACCENT, 1.0));
        assert_eq!(DEFAULT_ACCENT, 0x007AFF);
    }

    #[test]
    fn zones_are_placed_in_pixels_with_corners_and_borders_scaled() {
        let placed = placement(&zone(true, false, false), Rect::new(0.0, 0.0, 1000.0, 800.0), 2.0);
        assert_eq!(placed.shape, Rect::new(2.0, 2.0, 396.0, 296.0));
        assert_eq!(placed.corner_radius, 32.0);
        assert_eq!(placed.border_width, 8.0);
        assert_eq!(placed.number_size, 48.0);
    }

    #[test]
    fn a_caption_sits_below_and_raises_the_number() {
        let zone = zone(true, true, false);
        let placed = placement(&zone, Rect::new(0.0, 0.0, 1000.0, 800.0), 1.0);
        assert_eq!(placed.number_center.y, 150.0 - 8.0);
        assert_eq!(placed.caption.map(|(at, size)| (at.y, size)), Some((164.0, 13.0)));
    }
}
