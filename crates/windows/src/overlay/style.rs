//! How a zone looks and where its parts sit. Sizes are given in points and drawn in
//! pixels, multiplied by the display's scale.

use snappy_core::geometry::{Point, Rect};
use snappy_platform::OverlayZone;

use super::canvas::Color;

/// The documented accent when the system's can't be read.
pub const DEFAULT_ACCENT: u32 = 0x007AFF;
const MINT: u32 = 0x00C7BE;
const WHITE: u32 = 0xFFFFFF;

pub const CORNER_RADIUS: f64 = 16.0;
const ZONE_INSET: f64 = 1.0;
const NUMBER_HEIGHT_FRACTION: f64 = 0.14;
const NUMBER_MIN_SIZE: f64 = 12.0;
const NUMBER_MAX_SIZE: f64 = 24.0;
const NUMBER_RAISE_FOR_CAPTION: f64 = 8.0;
const CAPTION_SIZE: f64 = 13.0;
const CAPTION_BELOW_CENTER: f64 = 14.0;
pub const SHADOW_COLOR: Color = Color::rgb(0x000000, 0.7);
pub const SHADOW_BLUR: f64 = 8.0;
pub const TEXT_COLOR: Color = Color::rgb(WHITE, 1.0);
/// Font weights: medium for numbers, semibold for the caption.
pub const NUMBER_WEIGHT: u32 = 500;
pub const CAPTION_WEIGHT: u32 = 600;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneStyle {
    pub fill: Color,
    pub border: Color,
    /// In points.
    pub border_width: f64,
}

pub fn zone_style(zone: &OverlayZone, accent: u32) -> ZoneStyle {
    if zone.highlighted {
        return ZoneStyle {
            fill: Color::rgb(MINT, 0.30),
            border: Color::rgb(MINT, 1.0),
            border_width: 4.0,
        };
    }
    let fill = Color::rgb(accent, 0.12);
    if zone.swap_source {
        return ZoneStyle {
            fill,
            border: Color::rgb(MINT, 1.0),
            border_width: 3.0,
        };
    }
    ZoneStyle {
        fill,
        border: Color::rgb(WHITE, 0.60),
        border_width: 1.5,
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

/// The number's font size for a zone `height` pixels tall.
pub fn number_size(height: f64, scale: f64) -> f64 {
    (height / scale * NUMBER_HEIGHT_FRACTION).clamp(NUMBER_MIN_SIZE, NUMBER_MAX_SIZE) * scale
}

/// `frame` is the zone relative to the overlay's top left.
pub fn placement(zone: &OverlayZone, frame: Rect, style: &ZoneStyle, scale: f64) -> Placement {
    let shape = frame.inset(ZONE_INSET * scale, ZONE_INSET * scale);
    let center = shape.center();
    let caption = zone.caption.map(|_| {
        (
            Point::new(center.x, center.y + CAPTION_BELOW_CENTER * scale),
            CAPTION_SIZE * scale,
        )
    });
    let raise = if caption.is_some() {
        NUMBER_RAISE_FOR_CAPTION * scale
    } else {
        0.0
    };
    Placement {
        shape,
        corner_radius: CORNER_RADIUS * scale,
        border_width: style.border_width * scale,
        number_center: Point::new(center.x, center.y - raise),
        number_size: number_size(shape.height, scale),
        caption,
    }
}

#[cfg(test)]
mod tests {
    use snappy_platform::Caption;

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
    fn highlight_and_swap_source_styles() {
        let plain = zone_style(&zone(false, false, false), DEFAULT_ACCENT);
        assert_eq!(plain.border_width, 1.5);
        assert_eq!(plain.fill, Color::rgb(DEFAULT_ACCENT, 0.12));
        assert_eq!(
            zone_style(&zone(true, false, false), DEFAULT_ACCENT).border_width,
            4.0
        );
        assert_eq!(
            zone_style(&zone(false, false, true), DEFAULT_ACCENT).border_width,
            3.0
        );
    }

    #[test]
    fn numbers_scale_with_the_zone_within_limits() {
        assert!((number_size(100.0, 1.0) - 14.0).abs() < 1e-9);
        assert_eq!(number_size(50.0, 1.0), 12.0);
        assert_eq!(number_size(1000.0, 2.0), 48.0);
    }

    #[test]
    fn a_caption_sits_below_and_raises_the_number() {
        let zone = zone(true, true, false);
        let style = zone_style(&zone, DEFAULT_ACCENT);
        let placed = placement(&zone, zone.frame, &style, 1.0);
        assert_eq!(placed.number_center.y, 150.0 - 8.0);
        assert_eq!(placed.caption.map(|(at, size)| (at.y, size)), Some((164.0, 13.0)));
    }
}
