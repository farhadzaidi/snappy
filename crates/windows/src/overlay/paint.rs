//! Drawing a scene: every zone's shape, border, and number, plus the hovered zone's caption.

use snappy_core::geometry::{Point, Rect};
use snappy_platform::overlay_style::{MARK_BORDER_WIDTH, TEXT_SHADOW_BLUR, mark_border, zone_style};
use snappy_platform::{OverlayMarks, OverlayScene};

use super::canvas::{Canvas, Mask};
use super::style::{
    CAPTION_WEIGHT, DEFAULT_ACCENT, MARK_CORNER_RADIUS, NUMBER_WEIGHT, SHADOW_COLOR, TEXT_COLOR, color,
    placement,
};

/// Rendered text, as coverage masks.
pub trait Glyphs {
    /// `text` at `size` pixels and `weight`, blurred by `blur` pixels when nonzero.
    fn text(&mut self, text: &str, size: f64, weight: u32, blur: f64) -> Option<&Mask>;
}

/// Paints `scene` onto a canvas whose top left is the scene's work area origin.
pub fn paint(canvas: &mut Canvas, scene: &OverlayScene, scale: f64, accent: u32, glyphs: &mut impl Glyphs) {
    for zone in &scene.zones {
        let style = zone_style(zone);
        let placed = placement(zone, scene.work_area, scale);
        canvas.fill_rounded(placed.shape, placed.corner_radius, color(style.fill, accent));
        canvas.stroke_rounded(
            placed.shape,
            placed.corner_radius,
            placed.border_width,
            color(style.border, accent),
        );
        let number = zone.number.to_string();
        label(
            canvas,
            glyphs,
            &number,
            placed.number_center,
            placed.number_size,
            NUMBER_WEIGHT,
            scale,
        );
        if let Some(((center, size), caption)) = placed.caption.zip(zone.caption) {
            label(
                canvas,
                glyphs,
                caption.text(),
                center,
                size,
                CAPTION_WEIGHT,
                scale,
            );
        }
    }
}

/// Outlines each marked window inside its frame, on a canvas whose top left is the work
/// area's origin.
pub fn paint_marks(canvas: &mut Canvas, marks: &OverlayMarks, scale: f64) {
    let origin = marks.work_area.origin();
    for mark in &marks.windows {
        let frame = mark.frame;
        let local = Rect::new(frame.x - origin.x, frame.y - origin.y, frame.width, frame.height);
        canvas.stroke_rounded(
            local,
            MARK_CORNER_RADIUS * scale,
            MARK_BORDER_WIDTH * scale,
            color(mark_border(mark.style), DEFAULT_ACCENT),
        );
    }
}

/// Text centered on `center`, over its shadow.
fn label(
    canvas: &mut Canvas,
    glyphs: &mut impl Glyphs,
    text: &str,
    center: Point,
    size: f64,
    weight: u32,
    scale: f64,
) {
    let top_left = |mask: &Mask| {
        (
            (center.x - mask.width as f64 / 2.0).round() as i64,
            (center.y - mask.height as f64 / 2.0).round() as i64,
        )
    };
    if let Some(shadow) = glyphs.text(text, size, weight, TEXT_SHADOW_BLUR * scale) {
        let (left, top) = top_left(shadow);
        canvas.draw_mask(shadow, left, top, SHADOW_COLOR);
    }
    if let Some(mask) = glyphs.text(text, size, weight, 0.0) {
        let (left, top) = top_left(mask);
        canvas.draw_mask(mask, left, top, TEXT_COLOR);
    }
}

#[cfg(test)]
mod tests {
    use snappy_platform::{Caption, MarkStyle, MarkedWindow, OverlayZone};

    use super::*;

    /// Every string renders as a solid square as tall as its size.
    struct Squares(Vec<(String, f64)>, Mask);

    impl Glyphs for Squares {
        fn text(&mut self, text: &str, size: f64, _weight: u32, blur: f64) -> Option<&Mask> {
            if blur == 0.0 {
                self.0.push((text.to_owned(), size));
            }
            let side = size as usize;
            self.1 = Mask {
                width: side,
                height: side,
                coverage: vec![255; side * side],
            };
            Some(&self.1)
        }
    }

    #[test]
    fn marked_windows_are_outlined_inside_their_frames_in_their_styles_colors() {
        let marks = OverlayMarks {
            work_area: Rect::new(100.0, 50.0, 200.0, 100.0),
            windows: vec![
                MarkedWindow {
                    frame: Rect::new(120.0, 60.0, 80.0, 80.0),
                    style: MarkStyle::Selected,
                },
                MarkedWindow {
                    frame: Rect::new(210.0, 60.0, 80.0, 80.0),
                    style: MarkStyle::Focused,
                },
            ],
        };
        let mut pixels = vec![0u32; 200 * 100];
        let mut canvas = Canvas::new(200, 100, &mut pixels);
        paint_marks(&mut canvas, &marks, 1.0);
        // The first frame starts at local (20, 10); its border covers columns 20 to 22.
        let red = |pixel: u32| (pixel >> 16) & 0xff;
        let selected = canvas.pixel(21, 50);
        assert_eq!(selected >> 24, 0xff);
        assert!(red(selected) < 0x10, "mint: {selected:08x}");
        let focused = canvas.pixel(111, 50);
        assert_eq!(focused >> 24, 0xff);
        assert!(red(focused) > 0xf0, "orange: {focused:08x}");
        assert_eq!(canvas.pixel(18, 50), 0, "nothing outside the frame");
        assert_eq!(canvas.pixel(60, 50), 0, "the window shows through");
    }

    #[test]
    fn each_zone_is_drawn_with_its_number_and_the_scenes_caption() {
        let scene = OverlayScene {
            work_area: Rect::new(100.0, 50.0, 200.0, 100.0),
            zones: vec![
                OverlayZone {
                    frame: Rect::new(100.0, 50.0, 100.0, 100.0),
                    number: 1,
                    highlighted: false,
                    caption: None,
                    swap_source: false,
                },
                OverlayZone {
                    frame: Rect::new(200.0, 50.0, 100.0, 100.0),
                    number: 2,
                    highlighted: true,
                    caption: Some(Caption::Stack),
                    swap_source: false,
                },
            ],
        };
        let mut pixels = vec![0u32; 200 * 100];
        let mut canvas = Canvas::new(200, 100, &mut pixels);
        let mut glyphs = Squares(Vec::new(), Mask::default());
        paint(&mut canvas, &scene, 1.0, 0x007AFF, &mut glyphs);
        let texts: Vec<&str> = glyphs.0.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(texts, ["1", "2", "Stack"]);
        // The number is white at the first zone's center, which is local (50, 50).
        assert_eq!(canvas.pixel(50, 50), 0xffff_ffff);
        // Away from text, the highlighted zone shows its tinted fill.
        assert!(canvas.pixel(105, 20) >> 24 > 0);
    }
}
