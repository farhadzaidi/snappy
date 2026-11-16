//! How every overlay looks, whatever draws it: the palette, each zone's and marked window's
//! colors and border, and where a zone's shape, number, and caption sit. Lengths are in
//! points; `scale` turns them into the drawing's units.

use snappy_core::geometry::{Point, Rect};

use crate::{MarkStyle, OverlayZone};

/// A color the overlays draw with. Adapters that can read the system's own version of a
/// tint use it; the rest draw `Tint::rgb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tint {
    /// The system accent color.
    Accent,
    Mint,
    Orange,
    White,
    Black,
}

impl Tint {
    /// The tint's color where no system color stands in for it. The accent is the
    /// documented default accent.
    pub const fn rgb(self) -> Rgb {
        match self {
            Tint::Accent => Rgb::new(0x00, 0x7A, 0xFF),
            Tint::Mint => Rgb::new(0x00, 0xC7, 0xBE),
            // macOS's system orange.
            Tint::Orange => Rgb::new(0xFF, 0x9F, 0x0A),
            Tint::White => Rgb::new(0xFF, 0xFF, 0xFF),
            Tint::Black => Rgb::new(0x00, 0x00, 0x00),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    /// As `0xRRGGBB`.
    pub const fn hex(self) -> u32 {
        (self.red as u32) << 16 | (self.green as u32) << 8 | self.blue as u32
    }
}

/// A tint at an opacity from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    pub tint: Tint,
    pub alpha: f64,
}

impl Paint {
    pub const fn new(tint: Tint, alpha: f64) -> Self {
        Self { tint, alpha }
    }
}

/// Every zone is lightly tinted with the accent, so the desktop shows through.
pub const ZONE_FILL: Paint = Paint::new(Tint::Accent, 0.12);
pub const ZONE_BORDER: Paint = Paint::new(Tint::White, 0.60);
pub const ZONE_BORDER_WIDTH: f64 = 1.5;

/// The zone a window would land in stands out from the rest.
pub const HIGHLIGHTED_FILL: Paint = Paint::new(Tint::Mint, 0.30);
pub const HIGHLIGHTED_BORDER: Paint = Paint::new(Tint::Mint, 1.0);
pub const HIGHLIGHTED_BORDER_WIDTH: f64 = 4.0;

/// The zone a swap starts from keeps its fill and takes the highlight's color as a border.
pub const SWAP_SOURCE_BORDER: Paint = Paint::new(Tint::Mint, 1.0);
pub const SWAP_SOURCE_BORDER_WIDTH: f64 = 3.0;

/// A selected window's outline: the swap source's border, drawn inside the window's frame.
pub const SELECTED_BORDER: Paint = SWAP_SOURCE_BORDER;
/// The brief outline of a window a focus shortcut just focused, in a color of its own so
/// it never reads as selected.
pub const FOCUSED_BORDER: Paint = Paint::new(Tint::Orange, 1.0);
pub const MARK_BORDER_WIDTH: f64 = SWAP_SOURCE_BORDER_WIDTH;

pub const ZONE_CORNER_RADIUS: f64 = 16.0;
/// How far a zone's shape sits inside its frame, so neighbors' borders don't touch.
pub const ZONE_INSET: f64 = 1.0;

/// Numbers and captions are white over a soft black shadow, readable on any desktop.
pub const TEXT: Paint = Paint::new(Tint::White, 1.0);
pub const TEXT_SHADOW: Paint = Paint::new(Tint::Black, 0.7);
pub const TEXT_SHADOW_BLUR: f64 = 8.0;

/// A zone's number grows with the zone, within limits.
const NUMBER_HEIGHT_FRACTION: f64 = 0.14;
const NUMBER_MIN_SIZE: f64 = 12.0;
const NUMBER_MAX_SIZE: f64 = 24.0;
/// A captioned zone's number moves up to make room below it.
const NUMBER_RAISE_FOR_CAPTION: f64 = 8.0;
pub const CAPTION_FONT_SIZE: f64 = 13.0;
/// How far below the zone's center the caption's top sits.
const CAPTION_BELOW_CENTER: f64 = 14.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneStyle {
    pub fill: Paint,
    pub border: Paint,
    /// In points.
    pub border_width: f64,
}

/// The highlight wins over the swap source.
pub fn zone_style(zone: &OverlayZone) -> ZoneStyle {
    if zone.highlighted {
        return ZoneStyle {
            fill: HIGHLIGHTED_FILL,
            border: HIGHLIGHTED_BORDER,
            border_width: HIGHLIGHTED_BORDER_WIDTH,
        };
    }
    if zone.swap_source {
        return ZoneStyle {
            fill: ZONE_FILL,
            border: SWAP_SOURCE_BORDER,
            border_width: SWAP_SOURCE_BORDER_WIDTH,
        };
    }
    ZoneStyle {
        fill: ZONE_FILL,
        border: ZONE_BORDER,
        border_width: ZONE_BORDER_WIDTH,
    }
}

pub fn mark_border(style: MarkStyle) -> Paint {
    match style {
        MarkStyle::Selected => SELECTED_BORDER,
        MarkStyle::Focused => FOCUSED_BORDER,
    }
}

/// The number's font size for a zone `height` units tall, at `scale` units per point.
pub fn number_size(height: f64, scale: f64) -> f64 {
    (height * NUMBER_HEIGHT_FRACTION).clamp(NUMBER_MIN_SIZE * scale, NUMBER_MAX_SIZE * scale)
}

/// Where a zone's parts sit, relative to the overlay's top-left corner, which is the work
/// area's, in the drawing's units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZonePlacement {
    pub shape: Rect,
    pub number_center: Point,
    pub number_size: f64,
    pub caption: Option<CaptionPlacement>,
}

/// The caption is centered across the zone, its top a set distance below the center.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptionPlacement {
    pub top: f64,
    pub size: f64,
}

/// Places a zone whose global frame lies in an overlay covering `work_area`, at `scale`
/// units per point.
pub fn place_zone(zone: &OverlayZone, work_area: Rect, scale: f64) -> ZonePlacement {
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
    let caption = zone.caption.map(|_| CaptionPlacement {
        top: shape.mid_y() + CAPTION_BELOW_CENTER * scale,
        size: CAPTION_FONT_SIZE * scale,
    });
    ZonePlacement {
        shape,
        number_center: Point::new(shape.mid_x(), shape.mid_y() - raise),
        number_size: number_size(shape.height, scale),
        caption,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Caption;

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
    fn highlight_wins_over_swap_source() {
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
    fn selected_windows_are_outlined_like_the_swap_source_and_focused_ones_apart() {
        let source = zone_style(&OverlayZone {
            swap_source: true,
            ..zone()
        });
        assert_eq!(mark_border(MarkStyle::Selected), source.border);
        assert_eq!(MARK_BORDER_WIDTH, source.border_width);
        let focused = mark_border(MarkStyle::Focused);
        assert_eq!(focused.tint, Tint::Orange);
        assert_ne!(focused, source.border);
    }

    #[test]
    fn the_palette_matches_its_documented_colors() {
        assert_eq!(Tint::Accent.rgb().hex(), 0x007AFF);
        assert_eq!(Tint::Mint.rgb().hex(), 0x00C7BE);
        assert_eq!(Tint::Orange.rgb().hex(), 0xFF9F0A);
        assert_eq!(Tint::White.rgb().hex(), 0xFFFFFF);
        assert_eq!(Tint::Black.rgb().hex(), 0x000000);
    }

    #[test]
    fn number_size_is_clamped_and_scaled() {
        assert_eq!(number_size(50.0, 1.0), 12.0);
        assert!((number_size(100.0, 1.0) - 14.0).abs() < 1e-9);
        assert!((number_size(150.0, 1.0) - 21.0).abs() < 1e-9);
        assert_eq!(number_size(1000.0, 1.0), 24.0);
        assert_eq!(number_size(1000.0, 2.0), 48.0);
    }

    #[test]
    fn zones_are_placed_relative_to_the_work_area_and_inset() {
        let placement = place_zone(&zone(), Rect::new(100.0, 125.0, 1000.0, 800.0), 1.0);
        assert_eq!(placement.shape, Rect::new(13.0, 13.0, 398.0, 298.0));
        assert_eq!(placement.number_center, Point::new(212.0, 162.0));
        assert_eq!(placement.number_size, 24.0);
        assert_eq!(placement.caption, None);
    }

    #[test]
    fn a_caption_sits_below_center_and_raises_the_number() {
        let captioned = OverlayZone {
            highlighted: true,
            caption: Some(Caption::Stack),
            ..zone()
        };
        let placement = place_zone(&captioned, Rect::new(100.0, 125.0, 1000.0, 800.0), 2.0);
        assert_eq!(placement.number_center.y, 162.0 - 16.0);
        assert_eq!(
            placement.caption,
            Some(CaptionPlacement {
                top: 162.0 + 28.0,
                size: 26.0
            })
        );
    }
}
