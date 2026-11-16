//! Overlays cut down to rectangles, for when no compositor can show transparency.

use snappy_core::geometry::{Point, Rect};
use snappy_platform::{OverlayMarks, OverlayScene};

use super::glyphs;
use super::render::CAP_HEIGHT;
use crate::overlay_style::{place, place_mark};

/// Without a compositor the overlay can't be translucent, so it is cut down to each
/// zone's border and boxes behind its number and caption. Rectangles are relative to the
/// work area.
pub fn outline(scene: &OverlayScene, scale: f64) -> Vec<Rect> {
    scene
        .zones
        .iter()
        .flat_map(|zone| {
            let placement = place(zone, scene.work_area, scale);
            let shape = placement.shape;
            let width = placement.border_width.ceil().max(2.0);
            let number = text_box(
                &zone.number.to_string(),
                placement.number_center,
                placement.number_size,
            );
            let caption = placement
                .caption
                .zip(zone.caption)
                .map(|((center, size), caption)| text_box(caption.text(), center, size));
            border_strips(shape, width)
                .into_iter()
                .chain([number])
                .chain(caption)
        })
        .collect()
}

/// The marked windows' borders, for cutting an overlay down without a compositor, as
/// `outline` does for zones. Rectangles are relative to the work area.
pub fn mark_outline(marks: &OverlayMarks, scale: f64) -> Vec<Rect> {
    marks
        .windows
        .iter()
        .flat_map(|mark| {
            let placement = place_mark(mark.frame, marks.work_area, scale);
            border_strips(placement.shape, placement.border_width.ceil().max(2.0))
        })
        .collect()
}

/// The four strips `width` wide along the inside of `shape`'s edges.
fn border_strips(shape: Rect, width: f64) -> [Rect; 4] {
    [
        Rect::new(shape.x, shape.y, shape.width, width),
        Rect::new(shape.x, shape.max_y() - width, shape.width, width),
        Rect::new(shape.x, shape.y, width, shape.height),
        Rect::new(shape.max_x() - width, shape.y, width, shape.height),
    ]
}

/// The box behind `text` set at font `size` around `center`, padded by half a cap height.
fn text_box(text: &str, center: Point, size: f64) -> Rect {
    let cap = size * CAP_HEIGHT;
    let width = f64::from(glyphs::text_width(text, cap as f32));
    let pad = cap * 0.5;
    Rect::new(
        center.x - width / 2.0 - pad,
        center.y - cap / 2.0 - pad,
        width + 2.0 * pad,
        cap + 2.0 * pad,
    )
}

#[cfg(test)]
mod tests {
    use snappy_platform::Caption;

    use super::super::fixtures::{AREA, halves, marks, scene};
    use super::*;

    #[test]
    fn outlines_cover_borders_numbers_and_captions_only() {
        let rects = outline(&scene(halves()), 1.0);
        assert_eq!(rects.len(), 10);
        assert_eq!(rects[0], Rect::new(13.0, 13.0, 480.0, 2.0));
        let number = rects[4];
        assert!(number.contains(snappy_core::geometry::Point::new(253.0, 300.0)));
        assert!(number.width < 60.0);

        let mut zones = halves();
        zones[1].highlighted = true;
        zones[1].caption = Some(Caption::Stack);
        let rects = outline(&scene(zones), 1.0);
        assert_eq!(rects.len(), 11, "the caption gets a box too");
        assert!(rects[10].contains(snappy_core::geometry::Point::new(747.0, 320.0)));
    }

    #[test]
    fn marked_windows_are_cut_down_to_their_borders() {
        let marks = marks();
        let strips = mark_outline(&marks, 1.0);
        assert_eq!(strips.len(), 8);
        assert_eq!(strips[0], Rect::new(100.0, 100.0, 400.0, 3.0));
        let none = OverlayMarks {
            work_area: AREA,
            windows: Vec::new(),
        };
        assert!(mark_outline(&none, 1.0).is_empty());
    }
}
