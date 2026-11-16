//! Where a zone's layers and a marked window's outline sit, from the overlays' shared style.
//! Rectangles here are relative to the overlay's top-left corner, y down.

use snappy_core::geometry::Rect;
use snappy_platform::OverlayZone;
use snappy_platform::overlay_style::{MARK_BORDER_WIDTH, place_zone};

/// Text layers stop short of the zone's sides.
const TEXT_SIDE_MARGIN: f64 = 8.0;
const CAPTION_HEIGHT: f64 = 20.0;
/// Close to the corners of macOS's own windows.
pub const MARK_CORNER_RADIUS: f64 = 10.0;

/// Where a zone's shape layer and its two text layers go. A text layer draws from the top
/// of its frame, so the number's frame starts above its center.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerFrames {
    pub shape: Rect,
    pub number: Rect,
    pub number_size: f64,
    pub caption: Option<Rect>,
}

/// Lays out a zone whose global frame lies in an overlay covering `work_area`.
pub fn layer_frames(zone: &OverlayZone, work_area: Rect) -> LayerFrames {
    let placement = place_zone(zone, work_area, 1.0);
    let shape = placement.shape;
    let number_size = placement.number_size;
    let text_x = shape.x + TEXT_SIDE_MARGIN;
    let text_width = (shape.width - 2.0 * TEXT_SIDE_MARGIN).max(1.0);
    let number = Rect::new(
        text_x,
        placement.number_center.y - number_size * 0.7,
        text_width,
        number_size * 1.6,
    );
    let caption = placement
        .caption
        .map(|caption| Rect::new(text_x, caption.top, text_width, CAPTION_HEIGHT));
    LayerFrames {
        shape,
        number,
        number_size,
        caption,
    }
}

/// The line along which a window's outline is stroked, relative to an overlay covering
/// `work_area`: half the border inside the frame, so the whole border is inside it.
pub fn mark_path(frame: Rect, work_area: Rect) -> Rect {
    let half = MARK_BORDER_WIDTH / 2.0;
    Rect::new(
        frame.x - work_area.x,
        frame.y - work_area.y,
        frame.width,
        frame.height,
    )
    .inset(half, half)
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
    fn a_marked_windows_outline_lies_inside_its_frame_relative_to_the_overlay() {
        let path = mark_path(
            Rect::new(300.0, 200.0, 600.0, 400.0),
            Rect::new(100.0, 25.0, 1000.0, 800.0),
        );
        assert_eq!(path, Rect::new(201.5, 176.5, 597.0, 397.0));
    }

    #[test]
    fn zones_are_placed_relative_to_the_work_area_and_inset() {
        let frames = layer_frames(&zone(), Rect::new(100.0, 125.0, 1000.0, 800.0));
        assert_eq!(frames.shape, Rect::new(13.0, 13.0, 398.0, 298.0));
        assert_eq!(frames.number_size, 24.0);
        assert_eq!(frames.number.x, 21.0);
        assert_eq!(frames.number.width, 382.0);
        assert!((frames.number.y - (162.0 - 24.0 * 0.7)).abs() < 1e-9);
        assert_eq!(frames.caption, None);
    }

    #[test]
    fn a_caption_sits_below_center_and_raises_the_number() {
        let swap = OverlayZone {
            highlighted: true,
            caption: Some(Caption::Stack),
            ..zone()
        };
        let work_area = Rect::new(100.0, 125.0, 1000.0, 800.0);
        let plain = layer_frames(&zone(), work_area);
        let frames = layer_frames(&swap, work_area);
        assert!((frames.number.y - (plain.number.y - 8.0)).abs() < 1e-9);
        assert_eq!(frames.caption, Some(Rect::new(21.0, 176.0, 382.0, 20.0)));
    }
}
