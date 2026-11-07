//! How a zone looks and where its parts sit. Sizes are given in points and drawn in
//! pixels, multiplied by the display's scale.
//! Rectangles are relative to the overlay's top-left corner, which is the work area's.

use snappy_core::geometry::{Point, Rect};
use snappy_platform::OverlayZone;

/// No system accent color is readable on X11, so the documented default is used.
pub const ACCENT: Rgb = Rgb(0x00, 0x7A, 0xFF);
pub const MINT: Rgb = Rgb(0x00, 0xC7, 0xBE);
pub const WHITE: Rgb = Rgb(0xFF, 0xFF, 0xFF);
pub const BLACK: Rgb = Rgb(0x00, 0x00, 0x00);

pub const CORNER_RADIUS: f64 = 16.0;
const ZONE_INSET: f64 = 1.0;
const NUMBER_HEIGHT_FRACTION: f64 = 0.14;
const NUMBER_MIN_SIZE: f64 = 12.0;
const NUMBER_MAX_SIZE: f64 = 24.0;
const NUMBER_RAISE_FOR_CAPTION: f64 = 8.0;
const CAPTION_SIZE: f64 = 13.0;
const CAPTION_BELOW_CENTER: f64 = 14.0;
pub const SHADOW_OPACITY: f64 = 0.7;
pub const SHADOW_BLUR: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    pub color: Rgb,
    pub alpha: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneStyle {
    pub fill: Paint,
    pub border: Paint,
    /// In points.
    pub border_width: f64,
}

pub fn zone_style(zone: &OverlayZone) -> ZoneStyle {
    let paint = |color, alpha| Paint { color, alpha };
    if zone.highlighted {
        return ZoneStyle {
            fill: paint(MINT, 0.30),
            border: paint(MINT, 1.0),
            border_width: 4.0,
        };
    }
    let fill = paint(ACCENT, 0.12);
    if zone.swap_source {
        return ZoneStyle {
            fill,
            border: paint(MINT, 1.0),
            border_width: 3.0,
        };
    }
    ZoneStyle {
        fill,
        border: paint(WHITE, 0.60),
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
    (height * NUMBER_HEIGHT_FRACTION).clamp(NUMBER_MIN_SIZE * scale, NUMBER_MAX_SIZE * scale)
}

pub fn place(zone: &OverlayZone, work_area: Rect, scale: f64) -> Placement {
    let local = Rect::new(
        zone.frame.x - work_area.x,
        zone.frame.y - work_area.y,
        zone.frame.width,
        zone.frame.height,
    );
    let shape = local.inset(ZONE_INSET * scale, ZONE_INSET * scale);
    let raise = if zone.caption.is_some() {
        NUMBER_RAISE_FOR_CAPTION * scale
    } else {
        0.0
    };
    let caption_size = CAPTION_SIZE * scale;
    let caption = zone.caption.map(|_| {
        let top = shape.mid_y() + CAPTION_BELOW_CENTER * scale;
        (Point::new(shape.mid_x(), top + caption_size / 2.0), caption_size)
    });
    Placement {
        shape,
        corner_radius: (CORNER_RADIUS * scale)
            .min(shape.width / 2.0)
            .min(shape.height / 2.0)
            .max(0.0),
        border_width: zone_style(zone).border_width * scale,
        number_center: Point::new(shape.mid_x(), shape.mid_y() - raise),
        number_size: number_size(shape.height, scale),
        caption,
    }
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
    fn plain_zones_use_accent_fill_and_white_border() {
        let style = zone_style(&zone());
        assert_eq!(
            style.fill,
            Paint {
                color: ACCENT,
                alpha: 0.12
            }
        );
        assert_eq!(
            style.border,
            Paint {
                color: WHITE,
                alpha: 0.60
            }
        );
        assert_eq!(style.border_width, 1.5);
    }

    #[test]
    fn highlight_wins_over_swap_source() {
        let zone = OverlayZone {
            highlighted: true,
            swap_source: true,
            ..zone()
        };
        let style = zone_style(&zone);
        assert_eq!(
            style.fill,
            Paint {
                color: MINT,
                alpha: 0.30
            }
        );
        assert_eq!(
            style.border,
            Paint {
                color: MINT,
                alpha: 1.0
            }
        );
        assert_eq!(style.border_width, 4.0);
    }

    #[test]
    fn swap_source_gets_the_source_border() {
        let zone = OverlayZone {
            swap_source: true,
            ..zone()
        };
        let style = zone_style(&zone);
        assert_eq!(style.fill.color, ACCENT);
        assert_eq!(
            style.border,
            Paint {
                color: MINT,
                alpha: 1.0
            }
        );
        assert_eq!(style.border_width, 3.0);
    }

    #[test]
    fn number_size_is_clamped_and_scaled() {
        assert_eq!(number_size(50.0, 1.0), 12.0);
        assert!((number_size(150.0, 1.0) - 21.0).abs() < 1e-9);
        assert_eq!(number_size(1000.0, 1.0), 24.0);
        assert_eq!(number_size(1000.0, 2.0), 48.0);
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
