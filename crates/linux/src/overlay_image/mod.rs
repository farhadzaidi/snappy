//! Drawing an overlay scene into a premultiplied ARGB image, for adapters that draw their
//! own overlays (X11, and KDE Plasma's layer-shell surfaces), and the outline shapes used
//! when no compositor can show transparency.

use snappy_core::geometry::{Point, Rect};
use snappy_platform::{OverlayScene, OverlayZone};
use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

use crate::overlay_style::{self as style, BLACK, Placement, Rgb, WHITE, place, zone_style};

pub mod glyphs;

/// Rows drawn beyond each side of a band and then dropped, so curves are anti-aliased
/// away from the band's clip edges, exactly as in a whole image.
const BAND_MARGIN: u32 = 8;
const PIXEL_BYTES: usize = 4;
/// Cubic control-point distance that approximates a quarter circle.
const KAPPA: f32 = 0.552_284_8;
/// Cap height as a fraction of the font size.
const CAP_HEIGHT: f64 = 0.7;
/// Stroke weights as fractions of the cap height: medium numbers, a semibold caption.
const NUMBER_WEIGHT: f32 = 0.13;
const CAPTION_WEIGHT: f32 = 0.15;
/// Halos approximating the number shadow's blur: extra width in points and opacity.
const SHADOW_HALOS: [(f64, f64); 2] = [(style::SHADOW_BLUR, 0.25), (style::SHADOW_BLUR / 2.0, 0.45)];

/// `rows` rows of the scene's image starting at row `top`, or `None` when the work area
/// has no pixels there. Drawing in bands keeps a whole overlay out of memory.
pub fn render_rows(scene: &OverlayScene, scale: f64, top: u32, rows: u32) -> Option<Pixmap> {
    let (width, height) = pixel_size(scene.work_area)?;
    let rows = rows.min(height.checked_sub(top)?);
    let first = top.saturating_sub(BAND_MARGIN);
    let last = (top + rows + BAND_MARGIN).min(height);
    let mut canvas = Canvas {
        pixmap: Pixmap::new(width, last - first)?,
        transform: Transform::from_translate(0.0, -(first as f32)),
    };
    let drawn = Rect::new(0.0, f64::from(first), f64::from(width), f64::from(last - first));
    for zone in &scene.zones {
        let placement = place(zone, scene.work_area, scale);
        if placement.shape.intersection_area(drawn) > 0.0 {
            canvas.draw_zone(zone, &placement, scale);
        }
    }

    let row_bytes = width as usize * PIXEL_BYTES;
    let skip = (top - first) as usize * row_bytes;
    let mut band = Pixmap::new(width, rows)?;
    let kept = canvas.pixmap.data().get(skip..skip + rows as usize * row_bytes)?;
    band.data_mut().copy_from_slice(kept);
    Some(band)
}

/// The work area's size in whole pixels.
pub fn pixel_size(area: Rect) -> Option<(u32, u32)> {
    let width = area.width.round();
    let height = area.height.round();
    let fits = |value: f64| value >= 1.0 && value <= f64::from(i16::MAX);
    (fits(width) && fits(height)).then_some((width as u32, height as u32))
}

/// A band of the overlay image, with the transform from overlay to band coordinates.
struct Canvas {
    pixmap: Pixmap,
    transform: Transform,
}

impl Canvas {
    fn draw_zone(&mut self, zone: &OverlayZone, placement: &Placement, scale: f64) {
        let zone_style = zone_style(zone);
        if let Some(shape) = rounded_rect(placement.shape, placement.corner_radius) {
            let paint = solid(zone_style.fill.color, zone_style.fill.alpha);
            self.pixmap
                .fill_path(&shape, &paint, FillRule::Winding, self.transform, None);
        }

        // The border sits inside the shape, as an inner stroke would.
        let half = placement.border_width / 2.0;
        let border_rect = placement.shape.inset(half, half);
        if let Some(border) = rounded_rect(border_rect, (placement.corner_radius - half).max(0.0)) {
            let stroke = Stroke {
                width: placement.border_width as f32,
                ..Stroke::default()
            };
            let paint = solid(zone_style.border.color, zone_style.border.alpha);
            self.pixmap
                .stroke_path(&border, &paint, &stroke, self.transform, None);
        }

        let number = zone.number.to_string();
        let cap = (placement.number_size * CAP_HEIGHT) as f32;
        let center = placement.number_center;
        if let Some(path) = glyphs::text_path(&number, center.x as f32, center.y as f32, cap) {
            let weight = cap * NUMBER_WEIGHT;
            for (spread, opacity) in SHADOW_HALOS {
                let alpha = style::SHADOW_OPACITY * opacity;
                self.stroke_text(&path, weight + (spread * scale) as f32, BLACK, alpha);
            }
            self.stroke_text(&path, weight, WHITE, 1.0);
        }

        if let Some(((caption_center, size), caption)) = placement.caption.zip(zone.caption) {
            let cap = (size * CAP_HEIGHT) as f32;
            let (x, y) = (caption_center.x as f32, caption_center.y as f32);
            if let Some(path) = glyphs::text_path(caption.text(), x, y, cap) {
                self.stroke_text(&path, cap * CAPTION_WEIGHT, WHITE, 1.0);
            }
        }
    }

    fn stroke_text(&mut self, path: &Path, width: f32, color: Rgb, alpha: f64) {
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        self.pixmap
            .stroke_path(path, &solid(color, alpha), &stroke, self.transform, None);
    }
}

fn solid(color: Rgb, alpha: f64) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(
        color.0,
        color.1,
        color.2,
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    ));
    paint.anti_alias = true;
    paint
}

fn rounded_rect(rect: Rect, radius: f64) -> Option<Path> {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return None;
    }
    let (left, top) = (rect.min_x() as f32, rect.min_y() as f32);
    let (right, bottom) = (rect.max_x() as f32, rect.max_y() as f32);
    let r = radius.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0) as f32;
    let k = r * (1.0 - KAPPA);

    let mut builder = PathBuilder::new();
    builder.move_to(left + r, top);
    builder.line_to(right - r, top);
    builder.cubic_to(right - k, top, right, top + k, right, top + r);
    builder.line_to(right, bottom - r);
    builder.cubic_to(right, bottom - k, right - k, bottom, right - r, bottom);
    builder.line_to(left + r, bottom);
    builder.cubic_to(left + k, bottom, left, bottom - k, left, bottom - r);
    builder.line_to(left, top + r);
    builder.cubic_to(left, top + k, left + k, top, left + r, top);
    builder.close();
    builder.finish()
}

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
            [
                Rect::new(shape.x, shape.y, shape.width, width),
                Rect::new(shape.x, shape.max_y() - width, shape.width, width),
                Rect::new(shape.x, shape.y, width, shape.height),
                Rect::new(shape.max_x() - width, shape.y, width, shape.height),
                number,
            ]
            .into_iter()
            .chain(caption)
        })
        .collect()
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

/// Converts tiny-skia's premultiplied RGBA bytes to the X server's 32-bit ARGB pixels,
/// in place: B, G, R, A in memory for a least-significant-byte-first server.
pub fn to_argb(data: &mut [u8], msb_first: bool) {
    for pixel in data.as_chunks_mut::<4>().0 {
        let [r, g, b, a] = *pixel;
        *pixel = if msb_first { [a, r, g, b] } else { [b, g, r, a] };
    }
}

#[cfg(test)]
mod tests {
    use snappy_platform::Caption;

    use super::*;

    const AREA: Rect = Rect::new(1920.0, 32.0, 1000.0, 600.0);

    fn render(scene: &OverlayScene, scale: f64) -> Option<Pixmap> {
        let (_, height) = pixel_size(scene.work_area)?;
        render_rows(scene, scale, 0, height)
    }

    fn zone(frame: Rect, number: usize) -> OverlayZone {
        OverlayZone {
            frame,
            number,
            highlighted: false,
            caption: None,
            swap_source: false,
        }
    }

    fn scene(zones: Vec<OverlayZone>) -> OverlayScene {
        OverlayScene {
            work_area: AREA,
            zones,
        }
    }

    fn halves() -> Vec<OverlayZone> {
        vec![
            zone(Rect::new(1932.0, 44.0, 482.0, 576.0), 1),
            zone(Rect::new(2426.0, 44.0, 482.0, 576.0), 2),
        ]
    }

    fn rgba(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let pixel = pixmap.pixel(x, y).unwrap();
        [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
    }

    fn close(actual: [u8; 4], expected: [u8; 4], tolerance: u8) -> bool {
        actual
            .iter()
            .zip(expected)
            .all(|(a, e)| a.abs_diff(e) <= tolerance)
    }

    /// Pixels where white text shows: red is absent from every fill and shadow.
    fn white_pixels(pixmap: &Pixmap, area: Rect) -> usize {
        let mut count = 0;
        for y in area.min_y() as u32..area.max_y() as u32 {
            for x in area.min_x() as u32..area.max_x() as u32 {
                if rgba(pixmap, x, y)[0] > 150 {
                    count += 1;
                }
            }
        }
        count
    }

    #[test]
    fn zones_fill_with_the_accent_and_leave_gaps_clear() {
        let pixmap = render(&scene(halves()), 1.0).unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (1000, 600));
        // 12% of #007AFF, premultiplied.
        assert!(
            close(rgba(&pixmap, 60, 500), [0, 15, 31, 31], 2),
            "{:?}",
            rgba(&pixmap, 60, 500)
        );
        assert_eq!(rgba(&pixmap, 500, 300), [0, 0, 0, 0]);
        assert_eq!(rgba(&pixmap, 5, 5), [0, 0, 0, 0]);
    }

    #[test]
    fn borders_are_white_at_sixty_percent_inside_the_shape() {
        let pixmap = render(&scene(halves()), 1.0).unwrap();
        // The first zone's shape starts at x = 13; its 1.5 px border covers column 13.
        let border = rgba(&pixmap, 13, 300);
        assert!(border[3] > 150 && border[0] > 140, "{border:?}");
        assert_eq!(rgba(&pixmap, 11, 300), [0, 0, 0, 0]);
    }

    #[test]
    fn numbers_are_white_and_centered() {
        let pixmap = render(&scene(halves()), 1.0).unwrap();
        // The first zone's center is (253, 300).
        assert!(white_pixels(&pixmap, Rect::new(233.0, 280.0, 40.0, 40.0)) > 20);
        assert_eq!(white_pixels(&pixmap, Rect::new(100.0, 100.0, 40.0, 40.0)), 0);
    }

    #[test]
    fn highlight_uses_mint_and_both_captions_draw() {
        let mut zones = halves();
        zones[1].highlighted = true;
        let plain = render(&scene(zones.clone()), 1.0).unwrap();
        // 30% of #00C7BE, premultiplied.
        assert!(
            close(rgba(&plain, 560, 500), [0, 60, 57, 77], 2),
            "{:?}",
            rgba(&plain, 560, 500)
        );

        let below_center = Rect::new(720.0, 314.0, 80.0, 30.0);
        assert_eq!(white_pixels(&plain, below_center), 0);
        zones[1].caption = Some(Caption::Swap);
        let swap = render(&scene(zones.clone()), 1.0).unwrap();
        assert!(white_pixels(&swap, below_center) > 10);
        zones[1].caption = Some(Caption::Stack);
        let stack = render(&scene(zones), 1.0).unwrap();
        assert!(white_pixels(&stack, below_center) > 10);
        assert_ne!(swap.data(), stack.data());
    }

    #[test]
    fn bands_match_the_whole_image() {
        let mut zones = halves();
        zones[0].highlighted = true;
        zones[0].caption = Some(Caption::Stack);
        let whole = render(&scene(zones.clone()), 1.0).unwrap();
        let mut stacked = Vec::new();
        for top in (0..600).step_by(64) {
            stacked.extend_from_slice(render_rows(&scene(zones.clone()), 1.0, top, 64).unwrap().data());
        }
        // Curves shift by a rounding step where fixed-point math starts from another row.
        let largest = stacked
            .iter()
            .zip(whole.data())
            .map(|(band, full)| band.abs_diff(*full))
            .max()
            .unwrap_or(0);
        assert!(largest <= 4, "bands differ from the whole image by {largest}");
        assert!(render_rows(&scene(zones), 1.0, 600, 64).is_none());
    }

    #[test]
    fn empty_work_areas_render_nothing() {
        let empty = OverlayScene {
            work_area: Rect::new(0.0, 0.0, 0.0, 100.0),
            zones: Vec::new(),
        };
        assert!(render(&empty, 1.0).is_none());
    }

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
    fn pixels_convert_to_the_server_byte_order() {
        let mut lsb = vec![1, 2, 3, 4];
        to_argb(&mut lsb, false);
        assert_eq!(lsb, [3, 2, 1, 4]);
        let mut msb = vec![1, 2, 3, 4];
        to_argb(&mut msb, true);
        assert_eq!(msb, [4, 1, 2, 3]);
    }
}
