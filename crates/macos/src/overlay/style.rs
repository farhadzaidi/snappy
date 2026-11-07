//! How a zone looks and where its parts sit.
//! Rectangles here are relative to the overlay's top-left corner, y down.

use snappy_core::geometry::Rect;
use snappy_platform::OverlayZone;

pub const CORNER_RADIUS: f64 = 16.0;
pub const ZONE_INSET: f64 = 1.0;
pub const NUMBER_SHADOW_OPACITY: f32 = 0.7;
pub const NUMBER_SHADOW_BLUR: f64 = 8.0;
pub const CAPTION_FONT_SIZE: f64 = 13.0;

const NUMBER_HEIGHT_FRACTION: f64 = 0.14;
const NUMBER_MIN_SIZE: f64 = 12.0;
const NUMBER_MAX_SIZE: f64 = 24.0;
const NUMBER_RAISE_FOR_CAPTION: f64 = 8.0;
const CAPTION_BELOW_CENTER: f64 = 14.0;
const CAPTION_HEIGHT: f64 = 20.0;
const TEXT_SIDE_MARGIN: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tint {
    /// The system accent color (`controlAccentColor`).
    Accent,
    /// `systemMint`.
    Mint,
    White,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    pub tint: Tint,
    pub alpha: f64,
}

impl Paint {
    const fn new(tint: Tint, alpha: f64) -> Self {
        Self { tint, alpha }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneStyle {
    pub fill: Paint,
    pub border: Paint,
    pub border_width: f64,
}

pub fn zone_style(zone: &OverlayZone) -> ZoneStyle {
    if zone.highlighted {
        return ZoneStyle {
            fill: Paint::new(Tint::Mint, 0.30),
            border: Paint::new(Tint::Mint, 1.0),
            border_width: 4.0,
        };
    }
    let fill = Paint::new(Tint::Accent, 0.12);
    if zone.swap_source {
        return ZoneStyle {
            fill,
            border: Paint::new(Tint::Mint, 1.0),
            border_width: 3.0,
        };
    }
    ZoneStyle {
        fill,
        border: Paint::new(Tint::White, 0.60),
        border_width: 1.5,
    }
}

/// Where a zone's shape, number, and caption go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZonePlacement {
    pub shape: Rect,
    pub number: Rect,
    pub number_size: f64,
    pub caption: Option<Rect>,
}

pub fn number_font_size(zone_height: f64) -> f64 {
    (zone_height * NUMBER_HEIGHT_FRACTION).clamp(NUMBER_MIN_SIZE, NUMBER_MAX_SIZE)
}

/// Places a zone whose global frame lies in an overlay covering `work_area`.
pub fn place_zone(zone: &OverlayZone, work_area: Rect) -> ZonePlacement {
    let local = Rect::new(
        zone.frame.x - work_area.x,
        zone.frame.y - work_area.y,
        zone.frame.width,
        zone.frame.height,
    );
    let shape = local.inset(ZONE_INSET, ZONE_INSET);
    let number_size = number_font_size(shape.height);
    let text_x = shape.x + TEXT_SIDE_MARGIN;
    let text_width = (shape.width - 2.0 * TEXT_SIDE_MARGIN).max(1.0);
    let raise = if zone.caption.is_some() {
        NUMBER_RAISE_FOR_CAPTION
    } else {
        0.0
    };
    let number = Rect::new(
        text_x,
        shape.mid_y() - number_size * 0.7 - raise,
        text_width,
        number_size * 1.6,
    );
    let caption = zone.caption.map(|_| {
        Rect::new(
            text_x,
            shape.mid_y() + CAPTION_BELOW_CENTER,
            text_width,
            CAPTION_HEIGHT,
        )
    });
    ZonePlacement {
        shape,
        number,
        number_size,
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
        assert_eq!(style.fill, Paint::new(Tint::Accent, 0.12));
        assert_eq!(style.border, Paint::new(Tint::White, 0.60));
        assert_eq!(style.border_width, 1.5);
    }

    #[test]
    fn highlighted_zone_uses_mint() {
        let highlighted = OverlayZone {
            highlighted: true,
            swap_source: true,
            ..zone()
        };
        let style = zone_style(&highlighted);
        assert_eq!(style.fill, Paint::new(Tint::Mint, 0.30));
        assert_eq!(style.border, Paint::new(Tint::Mint, 1.0));
        assert_eq!(style.border_width, 4.0);
    }

    #[test]
    fn swap_source_gets_the_source_border() {
        let source = OverlayZone {
            swap_source: true,
            ..zone()
        };
        let style = zone_style(&source);
        assert_eq!(style.fill, Paint::new(Tint::Accent, 0.12));
        assert_eq!(style.border, Paint::new(Tint::Mint, 1.0));
        assert_eq!(style.border_width, 3.0);
    }

    #[test]
    fn number_size_is_clamped() {
        assert_eq!(number_font_size(50.0), 12.0);
        assert!((number_font_size(150.0) - 21.0).abs() < 1e-9);
        assert_eq!(number_font_size(1000.0), 24.0);
    }

    #[test]
    fn zones_are_placed_relative_to_the_work_area_and_inset() {
        let placement = place_zone(&zone(), Rect::new(100.0, 125.0, 1000.0, 800.0));
        assert_eq!(placement.shape, Rect::new(13.0, 13.0, 398.0, 298.0));
        assert_eq!(placement.number_size, 24.0);
        assert_eq!(placement.number.x, 21.0);
        assert_eq!(placement.number.width, 382.0);
        assert!((placement.number.y - (162.0 - 24.0 * 0.7)).abs() < 1e-9);
        assert_eq!(placement.caption, None);
    }

    #[test]
    fn a_caption_sits_below_center_and_raises_the_number() {
        let swap = OverlayZone {
            highlighted: true,
            caption: Some(Caption::Stack),
            ..zone()
        };
        let work_area = Rect::new(100.0, 125.0, 1000.0, 800.0);
        let plain = place_zone(&zone(), work_area);
        let placement = place_zone(&swap, work_area);
        assert_eq!(placement.number.y, plain.number.y - 8.0);
        assert_eq!(placement.caption, Some(Rect::new(21.0, 176.0, 382.0, 20.0)));
    }
}
