//! Drawing a scene: every zone's shape, border, and number, plus the hovered zone's caption.

use snappy_core::geometry::{Point, Rect};
use snappy_platform::OverlayScene;

use super::canvas::{Canvas, Mask};
use super::style::{
    CAPTION_WEIGHT, NUMBER_WEIGHT, SHADOW_BLUR, SHADOW_COLOR, TEXT_COLOR, placement, zone_style,
};

/// Rendered text, as coverage masks.
pub trait Glyphs {
    /// `text` at `size` pixels and `weight`, blurred by `blur` pixels when nonzero.
    fn text(&mut self, text: &str, size: f64, weight: u32, blur: f64) -> Option<&Mask>;
}

/// Paints `scene` onto a canvas whose top left is the scene's work area origin.
pub fn paint(canvas: &mut Canvas, scene: &OverlayScene, scale: f64, accent: u32, glyphs: &mut impl Glyphs) {
    let origin = scene.work_area.origin();
    for zone in &scene.zones {
        let frame = Rect::new(
            zone.frame.x - origin.x,
            zone.frame.y - origin.y,
            zone.frame.width,
            zone.frame.height,
        );
        let style = zone_style(zone, accent);
        let placed = placement(zone, frame, &style, scale);
        canvas.fill_rounded(placed.shape, placed.corner_radius, style.fill);
        canvas.stroke_rounded(
            placed.shape,
            placed.corner_radius,
            placed.border_width,
            style.border,
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
    if let Some(shadow) = glyphs.text(text, size, weight, SHADOW_BLUR * scale) {
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
    use snappy_platform::{Caption, OverlayZone};

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
