//! A small software rasterizer over premultiplied BGRA pixels, the format layered windows
//! take. It draws antialiased rounded rectangles and coverage masks, which is all a zone
//! overlay needs.

use snappy_core::geometry::Rect;

/// A straight-alpha color with components from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgb(hex: u32, a: f32) -> Color {
        Color {
            r: ((hex >> 16) & 0xff) as f32 / 255.0,
            g: ((hex >> 8) & 0xff) as f32 / 255.0,
            b: (hex & 0xff) as f32 / 255.0,
            a,
        }
    }
}

/// An 8-bit coverage mask, such as rendered text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub coverage: Vec<u8>,
}

impl Mask {
    /// The mask blurred by about `radius` pixels: three box blurs, which approach a
    /// Gaussian, after padding the edges so the blur has room.
    pub fn blurred(&self, radius: usize) -> Mask {
        let pad = radius * 3;
        let (width, height) = (self.width + 2 * pad, self.height + 2 * pad);
        let mut values = vec![0f32; width * height];
        for y in 0..self.height {
            for x in 0..self.width {
                values[(y + pad) * width + x + pad] = f32::from(self.coverage[y * self.width + x]);
            }
        }
        for _ in 0..3 {
            box_blur(&mut values, width, height, radius, true);
            box_blur(&mut values, width, height, radius, false);
        }
        Mask {
            width,
            height,
            coverage: values
                .iter()
                .map(|value| value.round().clamp(0.0, 255.0) as u8)
                .collect(),
        }
    }
}

fn box_blur(values: &mut [f32], width: usize, height: usize, radius: usize, horizontal: bool) {
    if radius == 0 {
        return;
    }
    let (lines, length) = if horizontal {
        (height, width)
    } else {
        (width, height)
    };
    let index = |line: usize, position: usize| {
        if horizontal {
            line * width + position
        } else {
            position * width + line
        }
    };
    let span = (2 * radius + 1) as f32;
    let mut line_values = vec![0f32; length];
    for line in 0..lines {
        for (position, value) in line_values.iter_mut().enumerate() {
            *value = values[index(line, position)];
        }
        let mut sum: f32 = line_values.iter().take(radius + 1).sum();
        for position in 0..length {
            values[index(line, position)] = sum / span;
            if let Some(entering) = line_values.get(position + radius + 1) {
                sum += entering;
            }
            if let Some(leaving) = position.checked_sub(radius).and_then(|at| line_values.get(at)) {
                sum -= leaving;
            }
        }
    }
}

pub struct Canvas<'a> {
    pub width: usize,
    pub height: usize,
    pixels: &'a mut [u32],
}

/// Signed distance from `(x, y)` to the edge of a rounded rectangle; negative inside.
fn rounded_distance(x: f64, y: f64, rect: Rect, radius: f64) -> f64 {
    let half_w = rect.width / 2.0;
    let half_h = rect.height / 2.0;
    let radius = radius.min(half_w).min(half_h).max(0.0);
    let dx = (x - rect.mid_x()).abs() - (half_w - radius);
    let dy = (y - rect.mid_y()).abs() - (half_h - radius);
    let outside = dx.max(0.0).hypot(dy.max(0.0));
    outside + dx.max(dy).min(0.0) - radius
}

fn coverage(distance: f64) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0) as f32
}

impl<'a> Canvas<'a> {
    /// A canvas over `pixels`, row by row from the top, cleared to transparent.
    pub fn new(width: usize, height: usize, pixels: &'a mut [u32]) -> Canvas<'a> {
        let len = (width * height).min(pixels.len());
        pixels[..len].fill(0);
        Canvas {
            width,
            height,
            pixels,
        }
    }

    #[cfg(test)]
    pub fn pixel(&self, x: usize, y: usize) -> u32 {
        self.pixels[y * self.width + x]
    }

    /// Composites `color` over one pixel with `amount` coverage.
    fn blend(&mut self, x: usize, y: usize, color: Color, amount: f32) {
        let alpha = color.a * amount;
        if alpha <= 0.0 {
            return;
        }
        let index = y * self.width + x;
        let below = self.pixels[index];
        let keep = 1.0 - alpha;
        let channel = |shift: u32, value: f32| {
            let under = ((below >> shift) & 0xff) as f32;
            ((value * alpha * 255.0 + under * keep).round().clamp(0.0, 255.0) as u32) << shift
        };
        self.pixels[index] =
            channel(24, 1.0) | channel(16, color.r) | channel(8, color.g) | channel(0, color.b);
    }

    /// The pixel rows and columns `rect`, grown by `margin`, touches.
    fn span(&self, rect: Rect, margin: f64) -> (usize, usize, usize, usize) {
        let clamp = |value: f64, limit: usize| value.max(0.0).min(limit as f64) as usize;
        (
            clamp((rect.min_x() - margin).floor(), self.width),
            clamp((rect.max_x() + margin).ceil(), self.width),
            clamp((rect.min_y() - margin).floor(), self.height),
            clamp((rect.max_y() + margin).ceil(), self.height),
        )
    }

    /// Fills a rounded rectangle.
    pub fn fill_rounded(&mut self, rect: Rect, radius: f64, color: Color) {
        let (x0, x1, y0, y1) = self.span(rect, 1.0);
        // Pixels this far inside are fully covered without measuring.
        let solid = rect.inset(radius.max(1.0), 1.0);
        let solid_tall = rect.inset(1.0, radius.max(1.0));
        for y in y0..y1 {
            let cy = y as f64 + 0.5;
            for x in x0..x1 {
                let cx = x as f64 + 0.5;
                let inside = |area: Rect| {
                    area.width > 0.0
                        && area.height > 0.0
                        && area.contains(snappy_core::geometry::Point::new(cx, cy))
                };
                let amount = if inside(solid) || inside(solid_tall) {
                    1.0
                } else {
                    coverage(rounded_distance(cx, cy, rect, radius))
                };
                self.blend(x, y, color, amount);
            }
        }
    }

    /// Strokes a rounded rectangle's outline `width` wide, inside its edge.
    pub fn stroke_rounded(&mut self, rect: Rect, radius: f64, width: f64, color: Color) {
        let (x0, x1, y0, y1) = self.span(rect, 1.0);
        let interior = rect.inset(width + 1.0, width + 1.0);
        let skip = interior.inset(radius.min(interior.width / 2.0), 0.0);
        let skip_tall = interior.inset(0.0, radius.min(interior.height / 2.0));
        for y in y0..y1 {
            let cy = y as f64 + 0.5;
            for x in x0..x1 {
                let cx = x as f64 + 0.5;
                let point = snappy_core::geometry::Point::new(cx, cy);
                if (skip.width > 0.0 && skip.height > 0.0 && skip.contains(point))
                    || (skip_tall.width > 0.0 && skip_tall.height > 0.0 && skip_tall.contains(point))
                {
                    continue;
                }
                let distance = rounded_distance(cx, cy, rect, radius);
                let amount = coverage(distance) - coverage(distance + width);
                self.blend(x, y, color, amount);
            }
        }
    }

    /// Paints `color` through `mask` with its top left at (`left`, `top`).
    pub fn draw_mask(&mut self, mask: &Mask, left: i64, top: i64, color: Color) {
        for row in 0..mask.height {
            let Some(y) = usize::try_from(top + row as i64)
                .ok()
                .filter(|y| *y < self.height)
            else {
                continue;
            };
            for column in 0..mask.width {
                let Some(x) = usize::try_from(left + column as i64)
                    .ok()
                    .filter(|x| *x < self.width)
                else {
                    continue;
                };
                let amount = f32::from(mask.coverage[row * mask.width + column]) / 255.0;
                self.blend(x, y, color, amount);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Color = Color::rgb(0xffffff, 1.0);

    fn alpha(pixel: u32) -> u32 {
        pixel >> 24
    }

    #[test]
    fn fills_cover_the_inside_and_round_the_corners() {
        let mut pixels = vec![0u32; 100 * 100];
        let mut canvas = Canvas::new(100, 100, &mut pixels);
        canvas.fill_rounded(Rect::new(10.0, 10.0, 80.0, 80.0), 16.0, WHITE);
        assert_eq!(canvas.pixel(50, 50), 0xffff_ffff);
        assert_eq!(alpha(canvas.pixel(5, 50)), 0);
        // The corner pixel lies outside the 16-pixel rounding.
        assert_eq!(alpha(canvas.pixel(10, 10)), 0);
        assert_eq!(alpha(canvas.pixel(30, 10)), 255);
    }

    #[test]
    fn strokes_stay_inside_the_edge() {
        let mut pixels = vec![0u32; 100 * 100];
        let mut canvas = Canvas::new(100, 100, &mut pixels);
        canvas.stroke_rounded(Rect::new(10.0, 10.0, 80.0, 80.0), 16.0, 4.0, WHITE);
        assert_eq!(alpha(canvas.pixel(50, 11)), 255);
        assert_eq!(alpha(canvas.pixel(50, 13)), 255);
        assert_eq!(alpha(canvas.pixel(50, 15)), 0);
        assert_eq!(alpha(canvas.pixel(50, 50)), 0);
        assert_eq!(alpha(canvas.pixel(50, 9)), 0);
    }

    #[test]
    fn blending_is_premultiplied() {
        let mut pixels = vec![0u32; 4];
        let mut canvas = Canvas::new(2, 2, &mut pixels);
        canvas.fill_rounded(Rect::new(0.0, 0.0, 2.0, 2.0), 0.0, Color::rgb(0xff0000, 0.5));
        assert_eq!(canvas.pixel(0, 0), 0x8080_0000);
    }

    #[test]
    fn masks_paint_their_coverage() {
        let mask = Mask {
            width: 2,
            height: 1,
            coverage: vec![255, 0],
        };
        let mut pixels = vec![0u32; 9];
        let mut canvas = Canvas::new(3, 3, &mut pixels);
        canvas.draw_mask(&mask, 1, 1, WHITE);
        assert_eq!(canvas.pixel(1, 1), 0xffff_ffff);
        assert_eq!(canvas.pixel(2, 1), 0);
        canvas.draw_mask(&mask, -5, -5, WHITE);
    }

    #[test]
    fn blurring_spreads_coverage_and_grows_the_mask() {
        let mask = Mask {
            width: 1,
            height: 1,
            coverage: vec![255],
        };
        let blurred = mask.blurred(2);
        assert_eq!((blurred.width, blurred.height), (13, 13));
        let center = blurred.coverage[6 * 13 + 6];
        assert!(center > 0 && center < 255);
        assert!(blurred.coverage[6 * 13 + 5] > 0);
    }
}
