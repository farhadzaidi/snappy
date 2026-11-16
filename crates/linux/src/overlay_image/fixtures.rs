//! The work area, zones, and marked windows the drawing tests share.

use snappy_core::geometry::Rect;
use snappy_platform::{MarkStyle, MarkedWindow, OverlayMarks, OverlayScene, OverlayZone};

pub const AREA: Rect = Rect::new(1920.0, 32.0, 1000.0, 600.0);

pub fn zone(frame: Rect, number: usize) -> OverlayZone {
    OverlayZone {
        frame,
        number,
        highlighted: false,
        caption: None,
        swap_source: false,
    }
}

pub fn scene(zones: Vec<OverlayZone>) -> OverlayScene {
    OverlayScene {
        work_area: AREA,
        zones,
    }
}

pub fn halves() -> Vec<OverlayZone> {
    vec![
        zone(Rect::new(1932.0, 44.0, 482.0, 576.0), 1),
        zone(Rect::new(2426.0, 44.0, 482.0, 576.0), 2),
    ]
}

/// A selected window and a focused one inside `AREA`.
pub fn marks() -> OverlayMarks {
    OverlayMarks {
        work_area: AREA,
        windows: vec![
            MarkedWindow {
                frame: Rect::new(2020.0, 132.0, 400.0, 300.0),
                style: MarkStyle::Selected,
            },
            MarkedWindow {
                frame: Rect::new(2520.0, 132.0, 300.0, 300.0),
                style: MarkStyle::Focused,
            },
        ],
    }
}
