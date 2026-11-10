//! A small stroke font for the overlay's zone numbers and its "Swap" and "Stack" captions,
//! so drawing needs no font files. Glyphs are center-line paths in a box one cap height tall.

use tiny_skia::{Path, PathBuilder};

/// Path segments in glyph units: x from 0 to 1 across the glyph's advance, y from 0 at
/// cap height to 1 at the baseline. Descenders go below 1.
enum Seg {
    M(f32, f32),
    L(f32, f32),
    C(f32, f32, f32, f32, f32, f32),
}

use Seg::{C, L, M};

struct Glyph {
    /// Width as a fraction of the cap height.
    advance: f32,
    segs: &'static [Seg],
}

const DIGIT: f32 = 0.62;
/// Gap between glyphs, as a fraction of the cap height.
const TRACKING: f32 = 0.22;

fn glyph(character: char) -> Option<&'static Glyph> {
    static ZERO: Glyph = Glyph {
        advance: DIGIT,
        segs: &[
            M(0.5, 0.0),
            C(0.0, 0.0, 0.0, 1.0, 0.5, 1.0),
            C(1.0, 1.0, 1.0, 0.0, 0.5, 0.0),
        ],
    };
    static ONE: Glyph = Glyph {
        advance: DIGIT,
        segs: &[M(0.18, 0.22), L(0.6, 0.0), L(0.6, 1.0)],
    };
    static TWO: Glyph = Glyph {
        advance: DIGIT,
        segs: &[
            M(0.08, 0.25),
            C(0.08, -0.06, 0.92, -0.06, 0.92, 0.27),
            C(0.92, 0.5, 0.3, 0.7, 0.05, 1.0),
            L(0.95, 1.0),
        ],
    };
    static THREE: Glyph = Glyph {
        advance: DIGIT,
        segs: &[
            M(0.1, 0.12),
            C(0.3, -0.06, 0.9, -0.04, 0.88, 0.25),
            C(0.86, 0.45, 0.6, 0.48, 0.4, 0.48),
            C(0.65, 0.48, 0.95, 0.55, 0.92, 0.75),
            C(0.9, 1.04, 0.3, 1.06, 0.06, 0.88),
        ],
    };
    static FOUR: Glyph = Glyph {
        advance: DIGIT,
        segs: &[M(0.7, 1.0), L(0.7, 0.0), L(0.04, 0.7), L(0.98, 0.7)],
    };
    static FIVE: Glyph = Glyph {
        advance: DIGIT,
        segs: &[
            M(0.88, 0.0),
            L(0.2, 0.0),
            L(0.13, 0.46),
            C(0.35, 0.32, 0.92, 0.32, 0.92, 0.68),
            C(0.92, 1.05, 0.3, 1.06, 0.06, 0.88),
        ],
    };
    static SIX: Glyph = Glyph {
        advance: DIGIT,
        segs: &[
            M(0.84, 0.08),
            C(0.6, -0.05, 0.1, -0.02, 0.1, 0.6),
            C(0.1, 1.05, 0.9, 1.05, 0.9, 0.66),
            C(0.9, 0.3, 0.2, 0.3, 0.11, 0.6),
        ],
    };
    static SEVEN: Glyph = Glyph {
        advance: DIGIT,
        segs: &[M(0.05, 0.0), L(0.95, 0.0), L(0.38, 1.0)],
    };
    static EIGHT: Glyph = Glyph {
        advance: DIGIT,
        segs: &[
            M(0.5, 0.47),
            C(0.1, 0.47, 0.12, 0.0, 0.5, 0.0),
            C(0.88, 0.0, 0.9, 0.47, 0.5, 0.47),
            C(0.04, 0.47, 0.04, 1.0, 0.5, 1.0),
            C(0.96, 1.0, 0.96, 0.47, 0.5, 0.47),
        ],
    };
    static NINE: Glyph = Glyph {
        advance: DIGIT,
        segs: &[
            M(0.16, 0.92),
            C(0.4, 1.05, 0.9, 1.02, 0.9, 0.4),
            C(0.9, -0.05, 0.1, -0.05, 0.1, 0.34),
            C(0.1, 0.7, 0.8, 0.7, 0.89, 0.4),
        ],
    };
    static CAPITAL_S: Glyph = Glyph {
        advance: 0.64,
        segs: &[
            M(0.88, 0.15),
            C(0.7, -0.05, 0.12, -0.05, 0.12, 0.25),
            C(0.12, 0.5, 0.88, 0.45, 0.88, 0.73),
            C(0.88, 1.05, 0.25, 1.05, 0.08, 0.85),
        ],
    };
    static SMALL_W: Glyph = Glyph {
        advance: 0.82,
        segs: &[
            M(0.0, 0.35),
            L(0.24, 1.0),
            L(0.5, 0.45),
            L(0.76, 1.0),
            L(1.0, 0.35),
        ],
    };
    static SMALL_A: Glyph = Glyph {
        advance: 0.56,
        segs: &[
            M(0.12, 0.43),
            C(0.3, 0.3, 0.86, 0.28, 0.86, 0.6),
            L(0.86, 1.0),
            M(0.86, 0.62),
            C(0.4, 0.6, 0.08, 0.68, 0.1, 0.85),
            C(0.12, 1.05, 0.7, 1.05, 0.86, 0.85),
        ],
    };
    static SMALL_P: Glyph = Glyph {
        advance: 0.58,
        segs: &[
            M(0.14, 0.35),
            L(0.14, 1.3),
            M(0.14, 0.55),
            C(0.3, 0.3, 0.9, 0.3, 0.9, 0.68),
            C(0.9, 1.05, 0.3, 1.05, 0.14, 0.82),
        ],
    };
    static SMALL_T: Glyph = Glyph {
        advance: 0.42,
        segs: &[
            M(0.38, 0.1),
            L(0.38, 0.84),
            C(0.38, 1.02, 0.65, 1.04, 0.95, 0.95),
            M(0.0, 0.35),
            L(0.9, 0.35),
        ],
    };
    static SMALL_C: Glyph = Glyph {
        advance: 0.54,
        segs: &[
            M(0.9, 0.45),
            C(0.7, 0.26, 0.1, 0.28, 0.1, 0.675),
            C(0.1, 1.07, 0.7, 1.09, 0.9, 0.9),
        ],
    };
    static SMALL_K: Glyph = Glyph {
        advance: 0.56,
        segs: &[
            M(0.14, 0.0),
            L(0.14, 1.0),
            M(0.88, 0.35),
            L(0.14, 0.76),
            M(0.4, 0.61),
            L(0.92, 1.0),
        ],
    };
    Some(match character {
        '0' => &ZERO,
        '1' => &ONE,
        '2' => &TWO,
        '3' => &THREE,
        '4' => &FOUR,
        '5' => &FIVE,
        '6' => &SIX,
        '7' => &SEVEN,
        '8' => &EIGHT,
        '9' => &NINE,
        'S' => &CAPITAL_S,
        'w' => &SMALL_W,
        'a' => &SMALL_A,
        'p' => &SMALL_P,
        't' => &SMALL_T,
        'c' => &SMALL_C,
        'k' => &SMALL_K,
        _ => return None,
    })
}

/// The width of `text` set at `cap_height`; characters without a glyph are skipped.
pub fn text_width(text: &str, cap_height: f32) -> f32 {
    let glyphs: Vec<&Glyph> = text.chars().filter_map(glyph).collect();
    let advances: f32 = glyphs.iter().map(|glyph| glyph.advance).sum();
    let gaps = glyphs.len().saturating_sub(1) as f32 * TRACKING;
    (advances + gaps) * cap_height
}

/// The center-line path of `text`, centered on (`x`, `y`) by its cap height.
pub fn text_path(text: &str, x: f32, y: f32, cap_height: f32) -> Option<Path> {
    let mut builder = PathBuilder::new();
    let mut left = x - text_width(text, cap_height) / 2.0;
    let top = y - cap_height / 2.0;
    for glyph in text.chars().filter_map(glyph) {
        let width = glyph.advance * cap_height;
        let point = |gx: f32, gy: f32| (left + gx * width, top + gy * cap_height);
        for seg in glyph.segs {
            match *seg {
                M(gx, gy) => {
                    let (px, py) = point(gx, gy);
                    builder.move_to(px, py);
                }
                L(gx, gy) => {
                    let (px, py) = point(gx, gy);
                    builder.line_to(px, py);
                }
                C(x1, y1, x2, y2, gx, gy) => {
                    let (ax, ay) = point(x1, y1);
                    let (bx, by) = point(x2, y2);
                    let (px, py) = point(gx, gy);
                    builder.cubic_to(ax, ay, bx, by, px, py);
                }
            }
        }
        left += width + TRACKING * cap_height;
    }
    builder.finish()
}

#[cfg(test)]
mod tests {
    use snappy_platform::Caption;

    use super::*;

    #[test]
    fn digits_and_every_caption_have_glyphs() {
        let captions = [Caption::Swap, Caption::Stack].map(Caption::text);
        for character in captions.iter().flat_map(|text| text.chars()).chain('0'..='9') {
            assert!(glyph(character).is_some(), "no glyph for {character}");
        }
        assert!(glyph('x').is_none());
    }

    #[test]
    fn text_is_centered_on_the_requested_point() {
        let path = text_path("12", 100.0, 50.0, 20.0).unwrap();
        let bounds = path.bounds();
        assert!((bounds.left() + bounds.right() - 200.0).abs() < 2.0);
        // Bounds include curve control points, which overshoot the cap height slightly.
        assert!(bounds.top() >= 38.0 && bounds.bottom() <= 62.0);
        assert!((text_width("12", 20.0) - (0.62 * 2.0 + 0.22) * 20.0).abs() < 1e-4);
    }

    #[test]
    fn unknown_characters_draw_nothing() {
        assert!(text_path("?", 0.0, 0.0, 10.0).is_none());
        assert_eq!(text_width("?", 10.0), 0.0);
    }
}
