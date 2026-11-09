//! The engine's overlay scenes as finished shapes, placed and colored by the style every
//! Linux overlay shares, for desktop shells that draw overlays themselves: GNOME Shell's
//! extension and KWin's script.

use serde::{Deserialize, Serialize};
use snappy_core::geometry::Rect;
use snappy_platform::OverlayScene as EngineScene;

use crate::overlay_style::{Paint, SHADOW_BLUR, SHADOW_OPACITY, place, zone_style};

/// A frame in the desktop shell's coordinates, as its script reports it.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl From<Frame> for Rect {
    fn from(frame: Frame) -> Rect {
        Rect::new(frame.x, frame.y, frame.width, frame.height)
    }
}

impl From<Rect> for Frame {
    fn from(rect: Rect) -> Frame {
        Frame {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

/// One display's overlay, every length in the shell's pixels and relative to the work area.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayScene {
    pub work_area: Frame,
    pub zones: Vec<OverlayZone>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayZone {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
    /// Red, green, and blue from 0 to 255, then opacity from 0 to 1.
    pub fill: [f64; 4],
    pub border: [f64; 4],
    pub border_width: f64,
    pub number: OverlayText,
    pub caption: Option<OverlayText>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayText {
    pub text: String,
    pub center_y: f64,
    pub size: f64,
    pub shadow_blur: f64,
    pub shadow_opacity: f64,
}

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
pub fn scene(scene: &EngineScene, scale: f64) -> OverlayScene {
    let text = |text: String, center_y: f64, size: f64| OverlayText {
        text,
        center_y,
        size,
        shadow_blur: SHADOW_BLUR * scale,
        shadow_opacity: SHADOW_OPACITY,
    };
    OverlayScene {
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
    use snappy_platform::{Caption, OverlayZone as EngineZone};

    use super::*;

    fn engine_scene() -> EngineScene {
        EngineScene {
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
