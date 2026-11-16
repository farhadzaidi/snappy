//! Display scales: buffers are drawn at a whole scale, so scenes and marks are scaled to
//! buffer pixels before drawing.

use snappy_core::geometry::Rect;
use snappy_platform::{MarkedWindow, OverlayMarks, OverlayScene, OverlayZone};

/// Whole buffer scales only: a fractional scale draws at the next whole one, and KWin
/// shrinks the image to fit.
pub fn buffer_scale(scale: f64) -> u32 {
    if scale.is_finite() && scale > 1.0 {
        scale.ceil() as u32
    } else {
        1
    }
}

/// The scene in buffer pixels: every length times `factor`.
pub fn scaled(scene: &OverlayScene, factor: f64) -> OverlayScene {
    OverlayScene {
        work_area: scaled_rect(scene.work_area, factor),
        zones: scene
            .zones
            .iter()
            .map(|zone| OverlayZone {
                frame: scaled_rect(zone.frame, factor),
                ..zone.clone()
            })
            .collect(),
    }
}

/// The marks in buffer pixels: every length times `factor`.
pub fn scaled_marks(marks: &OverlayMarks, factor: f64) -> OverlayMarks {
    OverlayMarks {
        work_area: scaled_rect(marks.work_area, factor),
        windows: marks
            .windows
            .iter()
            .map(|mark| MarkedWindow {
                frame: scaled_rect(mark.frame, factor),
                style: mark.style,
            })
            .collect(),
    }
}

fn scaled_rect(rect: Rect, factor: f64) -> Rect {
    Rect::new(
        rect.x * factor,
        rect.y * factor,
        rect.width * factor,
        rect.height * factor,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_scales_draw_at_the_next_whole_one() {
        assert_eq!(buffer_scale(1.0), 1);
        assert_eq!(buffer_scale(1.25), 2);
        assert_eq!(buffer_scale(2.0), 2);
        assert_eq!(buffer_scale(f64::NAN), 1);
        assert_eq!(buffer_scale(0.0), 1);
    }

    #[test]
    fn scenes_are_drawn_in_buffer_pixels() {
        let scene = OverlayScene {
            work_area: Rect::new(0.0, 40.0, 1920.0, 1000.0),
            zones: vec![OverlayZone {
                frame: Rect::new(10.0, 50.0, 900.0, 980.0),
                number: 1,
                highlighted: false,
                caption: None,
                swap_source: false,
            }],
        };
        let doubled = scaled(&scene, 2.0);
        assert_eq!(doubled.work_area, Rect::new(0.0, 80.0, 3840.0, 2000.0));
        assert_eq!(doubled.zones[0].frame, Rect::new(20.0, 100.0, 1800.0, 1960.0));
    }
}
