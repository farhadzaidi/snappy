//! The engine's overlay scenes as finished shapes for the extension, placed and colored by
//! the style every Linux overlay shares.

use snappy_linux::overlay_style::{Paint, SHADOW_BLUR, SHADOW_OPACITY, place, zone_style};
use snappy_platform::OverlayScene;

use crate::protocol::{self, Frame, OverlayText, OverlayZone};

fn rgba(paint: Paint) -> [f64; 4] {
    let color = paint.color;
    [
        f64::from(color.0),
        f64::from(color.1),
        f64::from(color.2),
        paint.alpha,
    ]
}

/// `scale` is the display's window units per point.
pub fn scene(scene: &OverlayScene, scale: f64) -> protocol::OverlayScene {
    let text = |text: String, center_y: f64, size: f64| OverlayText {
        text,
        center_y,
        size,
        shadow_blur: SHADOW_BLUR * scale,
        shadow_opacity: SHADOW_OPACITY,
    };
    protocol::OverlayScene {
        work_area: Frame::from(scene.work_area),
        zones: scene
            .zones
            .iter()
            .map(|zone| {
                let placement = place(zone, scene.work_area, scale);
                let style = zone_style(zone);
                OverlayZone {
                    x: placement.shape.x,
                    y: placement.shape.y,
                    width: placement.shape.width,
                    height: placement.shape.height,
                    radius: placement.corner_radius,
                    fill: rgba(style.fill),
                    border: rgba(style.border),
                    border_width: placement.border_width,
                    number: text(
                        zone.number.to_string(),
                        placement.number_center.y,
                        placement.number_size,
                    ),
                    caption: zone
                        .caption
                        .zip(placement.caption)
                        .map(|(caption, (center, size))| text(caption.text().to_owned(), center.y, size)),
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use snappy_core::geometry::Rect;
    use snappy_platform::{Caption, OverlayZone as EngineZone};

    use super::*;

    fn engine_scene() -> OverlayScene {
        OverlayScene {
            work_area: Rect::new(100.0, 32.0, 1000.0, 800.0),
            zones: vec![
                EngineZone {
                    frame: Rect::new(112.0, 44.0, 400.0, 300.0),
                    number: 1,
                    highlighted: true,
                    caption: Some(Caption::Stack),
                    swap_source: false,
                },
                EngineZone {
                    frame: Rect::new(524.0, 44.0, 400.0, 300.0),
                    number: 2,
                    highlighted: false,
                    caption: None,
                    swap_source: false,
                },
            ],
        }
    }

    #[test]
    fn zones_are_placed_in_the_work_area_and_styled_like_other_linux_overlays() {
        let scene = scene(&engine_scene(), 1.0);
        assert_eq!(scene.work_area.x, 100.0);
        let first = &scene.zones[0];
        assert_eq!((first.x, first.y, first.width), (13.0, 13.0, 398.0));
        assert_eq!(
            first.fill,
            [0.0, 199.0, 190.0, 0.30],
            "the highlighted zone is mint"
        );
        assert_eq!(first.border_width, 4.0);
        assert_eq!(first.number.text, "1");
        assert_eq!(
            first.caption.as_ref().map(|caption| caption.text.as_str()),
            Some("Stack")
        );
        let second = &scene.zones[1];
        assert_eq!(second.fill, [0.0, 122.0, 255.0, 0.12]);
        assert_eq!(second.caption, None);
    }

    #[test]
    fn lengths_follow_the_displays_scale() {
        let doubled = scene(&engine_scene(), 2.0);
        assert_eq!(doubled.zones[0].border_width, 8.0);
        assert_eq!(doubled.zones[0].number.shadow_blur, 16.0);
    }
}
