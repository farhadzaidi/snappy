//! The tray glyph: the app icon as a monochrome mark, a window holding the lit half zone
//! beside two dimmer quarter zones. `assets/icon/snappy-glyph.svg` draws the same shapes for
//! macOS and GNOME; the trays that take pixels draw them here at the size they need.

use snappy_core::geometry::{Point, Rect};

/// The glyph is drawn on a square this many units wide.
const GRID: f64 = 16.0;

enum Shape {
    /// A rounded outline `width` wide, inside the rectangle's edge.
    Outline { rect: Rect, radius: f64, width: f64 },
    /// A filled rounded rectangle, `opacity` of the way to solid.
    Fill { rect: Rect, radius: f64, opacity: f64 },
}

const SHAPES: [Shape; 4] = [
    // The window.
    Shape::Outline {
        rect: Rect::new(0.0, 1.0, 16.0, 14.0),
        radius: 3.5,
        width: 1.0,
    },
    // The lit half zone.
    Shape::Fill {
        rect: Rect::new(2.0, 3.0, 5.0, 10.0),
        radius: 1.0,
        opacity: 1.0,
    },
    // The quarter zones.
    Shape::Fill {
        rect: Rect::new(9.0, 3.0, 5.0, 4.0),
        radius: 1.0,
        opacity: 0.4,
    },
    Shape::Fill {
        rect: Rect::new(9.0, 9.0, 5.0, 4.0),
        radius: 1.0,
        opacity: 0.4,
    },
];

/// The glyph's coverage on a `size`-pixel square, row by row from the top, from 0 to 255.
pub fn mask(size: usize) -> Vec<u8> {
    let scale = size as f64 / GRID;
    let mut coverage = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let center = Point::new((x as f64 + 0.5) / scale, (y as f64 + 0.5) / scale);
            // The shapes don't overlap, so their coverages add.
            let amount: f64 = SHAPES.iter().map(|shape| shape.coverage(center, scale)).sum();
            coverage.push((amount.min(1.0) * 255.0).round() as u8);
        }
    }
    coverage
}

impl Shape {
    /// How much of the pixel centered at `center`, in grid units, the shape covers, from 0
    /// to 1, with `scale` pixels to a unit.
    fn coverage(&self, center: Point, scale: f64) -> f64 {
        match *self {
            Shape::Outline { rect, radius, width } => {
                let distance = rect.rounded_distance(center, radius);
                edge_coverage(distance, scale) - edge_coverage(distance + width, scale)
            }
            Shape::Fill {
                rect,
                radius,
                opacity,
            } => edge_coverage(rect.rounded_distance(center, radius), scale) * opacity,
        }
    }
}

/// Coverage of a pixel whose center is `distance` units outside an edge, negative inside,
/// blended over one pixel whatever the size.
fn edge_coverage(distance: f64, scale: f64) -> f64 {
    (0.5 - distance * scale).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = include_str!("../../../assets/icon/snappy-glyph.svg");

    fn at(mask: &[u8], size: usize, x: usize, y: usize) -> u8 {
        mask[y * size + x]
    }

    /// An SVG path tracing a rounded rectangle clockwise from its top left.
    fn rounded_path(rect: Rect, radius: f64) -> String {
        let (left, top, right, bottom) = (rect.min_x(), rect.min_y(), rect.max_x(), rect.max_y());
        let arc = format!("A{radius},{radius} 0 0 1");
        format!(
            "M{},{top} H{} {arc} {right},{} V{} {arc} {},{bottom} H{} {arc} {left},{} V{} {arc} {},{top} Z",
            left + radius,
            right - radius,
            top + radius,
            bottom - radius,
            right - radius,
            left + radius,
            bottom - radius,
            top + radius,
            left + radius,
        )
    }

    #[test]
    fn the_svg_draws_the_same_shapes() {
        for shape in &SHAPES {
            let markup = match *shape {
                Shape::Outline { rect, radius, width } => format!(
                    r#"<path fill-rule="evenodd" d="{} {}"/>"#,
                    rounded_path(rect, radius),
                    rounded_path(rect.inset(width, width), radius - width),
                ),
                Shape::Fill {
                    rect,
                    radius,
                    opacity,
                } => {
                    let opacity = if opacity < 1.0 {
                        format!(r#" opacity="{opacity}""#)
                    } else {
                        String::new()
                    };
                    format!(
                        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{radius}"{opacity}/>"#,
                        rect.x, rect.y, rect.width, rect.height,
                    )
                }
            };
            assert!(SVG.contains(&markup), "the SVG lacks {markup}");
        }
        assert_eq!(
            SVG.matches("<path").count() + SVG.matches("<rect").count(),
            SHAPES.len()
        );
    }

    #[test]
    fn the_half_zone_is_solid_and_the_quarter_zones_dimmer() {
        let mask = mask(16);
        assert_eq!(mask.len(), 16 * 16);
        assert_eq!(at(&mask, 16, 4, 8), 255);
        assert_eq!(at(&mask, 16, 11, 5), 102);
        assert_eq!(at(&mask, 16, 11, 10), 102);
    }

    #[test]
    fn the_window_outline_is_solid_and_the_gaps_clear() {
        let mask = mask(16);
        assert_eq!(at(&mask, 16, 8, 1), 255);
        assert_eq!(at(&mask, 16, 0, 8), 255);
        // Between the frame and the half zone, and between the zones.
        assert_eq!(at(&mask, 16, 1, 8), 0);
        assert_eq!(at(&mask, 16, 8, 8), 0);
        assert_eq!(at(&mask, 16, 11, 7), 0);
        // Outside the window.
        assert_eq!(at(&mask, 16, 0, 0), 0);
        assert_eq!(at(&mask, 16, 8, 15), 0);
    }

    #[test]
    fn the_glyph_scales_to_any_size() {
        let mask = mask(44);
        assert_eq!(mask.len(), 44 * 44);
        assert_eq!(at(&mask, 44, 12, 22), 255);
        assert_eq!(at(&mask, 44, 0, 0), 0);
    }
}
