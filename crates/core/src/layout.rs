use std::collections::HashSet;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::geometry::{Point, Rect, round};

/// Smallest zone width or height, as a fraction of the work area.
pub const MINIMUM_ZONE_SIZE: f64 = 0.04;
/// Upper-edge tolerance for unit rectangles.
const UNIT_EDGE_TOLERANCE: f64 = 1.000_001;
pub const MAX_ZONES: usize = 32;
pub const MAX_ID_LENGTH: usize = 80;
pub const MAX_NAME_LENGTH: usize = 80;
/// Gap and padding between zones, in points.
pub const SPACING: f64 = 12.0;
pub const DEFAULT_LAYOUT_ID: &str = "focus";
pub const FALLBACK_LAYOUT_ID: &str = "full";

/// A rectangle in fractions (0 to 1) of a display's work area.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct UnitRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl UnitRect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    pub fn right(self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(self) -> f64 {
        self.y + self.height
    }

    pub fn area(self) -> f64 {
        self.width * self.height
    }

    pub fn is_valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.x >= 0.0
            && self.y >= 0.0
            && self.width >= MINIMUM_ZONE_SIZE
            && self.height >= MINIMUM_ZONE_SIZE
            && self.right() <= UNIT_EDGE_TOLERANCE
            && self.bottom() <= UNIT_EDGE_TOLERANCE
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub id: String,
    pub rect: UnitRect,
}

impl Zone {
    pub fn new(id: impl Into<String>, rect: UnitRect) -> Self {
        Self { id: id.into(), rect }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub id: String,
    pub name: String,
    pub zones: Vec<Zone>,
}

impl Layout {
    pub fn is_preset(&self) -> bool {
        is_preset_id(&self.id)
    }

    pub fn is_valid(&self) -> bool {
        let ids: HashSet<&str> = self.zones.iter().map(|zone| zone.id.as_str()).collect();
        valid_id(&self.id)
            && valid_name(&self.name)
            && (1..=MAX_ZONES).contains(&self.zones.len())
            && ids.len() == self.zones.len()
            && self
                .zones
                .iter()
                .all(|zone| valid_id(&zone.id) && zone.rect.is_valid())
    }

    pub fn zone(&self, id: &str) -> Option<&Zone> {
        self.zones.iter().find(|zone| zone.id == id)
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().count() <= MAX_ID_LENGTH
}

pub fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= MAX_NAME_LENGTH
}

pub fn presets() -> &'static [Layout] {
    &PRESETS
}

pub fn preset(id: &str) -> Option<&'static Layout> {
    PRESETS.iter().find(|layout| layout.id == id)
}

pub fn is_preset_id(id: &str) -> bool {
    preset(id).is_some()
}

static PRESETS: LazyLock<Vec<Layout>> = LazyLock::new(|| {
    let third = 1.0 / 3.0;
    let two_thirds = 2.0 / 3.0;
    vec![
        preset_layout("halves", "Halves", &[(0.0, 0.0, 0.5, 1.0), (0.5, 0.0, 0.5, 1.0)]),
        preset_layout(
            "thirds",
            "Three columns",
            &[
                (0.0, 0.0, third, 1.0),
                (third, 0.0, third, 1.0),
                (two_thirds, 0.0, third, 1.0),
            ],
        ),
        preset_layout(
            "focus",
            "Center stage",
            &[
                (0.0, 0.0, 0.25, 1.0),
                (0.25, 0.0, 0.5, 1.0),
                (0.75, 0.0, 0.25, 1.0),
            ],
        ),
        preset_layout(
            "quarters",
            "Four corners",
            &[
                (0.0, 0.0, 0.5, 0.5),
                (0.5, 0.0, 0.5, 0.5),
                (0.0, 0.5, 0.5, 0.5),
                (0.5, 0.5, 0.5, 0.5),
            ],
        ),
        preset_layout(
            "wide-left",
            "Main + stack",
            &[
                (0.0, 0.0, two_thirds, 1.0),
                (two_thirds, 0.0, third, 0.5),
                (two_thirds, 0.5, third, 0.5),
            ],
        ),
        preset_layout("rows", "Two rows", &[(0.0, 0.0, 1.0, 0.5), (0.0, 0.5, 1.0, 0.5)]),
        preset_layout(
            "six",
            "Six zones",
            &(0..6)
                .map(|i| (f64::from(i % 3) / 3.0, f64::from(i / 3) / 2.0, third, 0.5))
                .collect::<Vec<_>>(),
        ),
        preset_layout("full", "Full workspace", &[(0.0, 0.0, 1.0, 1.0)]),
    ]
});

fn preset_layout(id: &str, name: &str, rects: &[(f64, f64, f64, f64)]) -> Layout {
    let zones = rects
        .iter()
        .enumerate()
        .map(|(index, &(x, y, width, height))| {
            Zone::new(format!("{id}-{index}"), UnitRect::new(x, y, width, height))
        })
        .collect();
    Layout {
        id: id.to_owned(),
        name: name.to_owned(),
        zones,
    }
}

/// Gap and padding applied around zones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spacing {
    pub gap: f64,
    pub padding: f64,
}

impl Spacing {
    pub const STANDARD: Spacing = Spacing {
        gap: SPACING,
        padding: SPACING,
    };

    pub fn scaled(self, factor: f64) -> Spacing {
        Spacing {
            gap: self.gap * factor,
            padding: self.padding * factor,
        }
    }
}

/// A zone's frame inside a work area, after padding and gaps (thresholds: zone frames).
pub fn zone_frame(rect: UnitRect, work_area: Rect, spacing: Spacing) -> Rect {
    if !rect.is_valid() || !work_area.is_finite() || work_area.width <= 0.0 || work_area.height <= 0.0 {
        return Rect::ZERO;
    }
    let padding = if spacing.padding.is_finite() {
        spacing.padding
    } else {
        0.0
    };
    let padding = padding
        .max(0.0)
        .min((work_area.width.min(work_area.height) / 2.0 - 1.0).max(0.0));
    let area = work_area.inset(padding, padding);
    let gap = if spacing.gap.is_finite() { spacing.gap } else { 0.0 };
    let gap = gap.max(0.0).min(area.width.min(area.height) * 0.02);

    let x = area.min_x() + rect.x * (area.width + gap);
    let y = area.min_y() + rect.y * (area.height + gap);
    let right = area
        .max_x()
        .min(x + (rect.width * (area.width + gap) - gap).max(1.0));
    let bottom = area
        .max_y()
        .min(y + (rect.height * (area.height + gap) - gap).max(1.0));
    Rect::new(
        round(x),
        round(y),
        (round(right) - round(x)).max(1.0),
        (round(bottom) - round(y)).max(1.0),
    )
}

/// The zone under `point`: the smallest containing zone, earliest on ties.
pub fn hit_test(point: Point, layout: &Layout, work_area: Rect, spacing: Spacing) -> Option<&Zone> {
    if !work_area.contains(point) {
        return None;
    }
    let mut best: Option<&Zone> = None;
    for zone in &layout.zones {
        if !zone_frame(zone.rect, work_area, spacing).contains(point) {
            continue;
        }
        if best.is_none_or(|current| zone.rect.area() < current.rect.area()) {
            best = Some(zone);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const WORK: Rect = Rect::new(0.0, 25.0, 1440.0, 875.0);

    /// A valid zone: at least the minimum size and on the unit canvas.
    fn unit_rect() -> impl Strategy<Value = UnitRect> {
        (0.0..1.0f64, 0.0..1.0f64, 0.0..1.0f64, 0.0..1.0f64).prop_map(|(x, y, w, h)| {
            let span = 1.0 - MINIMUM_ZONE_SIZE;
            let (x, y) = (x * span, y * span);
            let width = MINIMUM_ZONE_SIZE + w * (1.0 - MINIMUM_ZONE_SIZE - x);
            let height = MINIMUM_ZONE_SIZE + h * (1.0 - MINIMUM_ZONE_SIZE - y);
            UnitRect::new(x, y, width, height)
        })
    }

    /// Work areas from small windows to large multi-display coordinates, at any origin.
    fn work_area() -> impl Strategy<Value = Rect> {
        (
            -4000.0..4000.0f64,
            -2000.0..2000.0f64,
            200.0..6000.0f64,
            150.0..3500.0f64,
        )
            .prop_map(|(x, y, width, height)| Rect::new(x, y, width, height))
    }

    proptest! {
        #[test]
        fn zone_frames_stay_inside_the_work_area(
            rect in unit_rect(),
            area in work_area(),
            scale in 0.5..3.0f64,
        ) {
            prop_assume!(rect.is_valid());
            let frame = zone_frame(rect, area, Spacing::STANDARD.scaled(scale));
            // Frames are rounded to whole points, so an edge may move half a point.
            let slack = 0.5;
            prop_assert!(frame.width >= 1.0 && frame.height >= 1.0, "{frame:?}");
            prop_assert!(frame.min_x() >= area.min_x() - slack, "{frame:?} in {area:?}");
            prop_assert!(frame.min_y() >= area.min_y() - slack, "{frame:?} in {area:?}");
            prop_assert!(frame.max_x() <= area.max_x() + slack, "{frame:?} in {area:?}");
            prop_assert!(frame.max_y() <= area.max_y() + slack, "{frame:?} in {area:?}");
        }
    }

    #[test]
    fn presets_are_valid_and_unique() {
        let ids: HashSet<&str> = presets().iter().map(|layout| layout.id.as_str()).collect();
        assert_eq!(ids.len(), 8);
        assert!(presets().iter().all(Layout::is_valid));
        assert_eq!(presets()[2].zones[1].id, "focus-1");
    }

    #[test]
    fn halves_have_exact_gap_and_padding() {
        let halves = preset("halves").unwrap();
        let left = zone_frame(halves.zones[0].rect, WORK, Spacing::STANDARD);
        let right = zone_frame(halves.zones[1].rect, WORK, Spacing::STANDARD);
        assert_eq!(left.min_x(), 12.0);
        assert_eq!(left.min_y(), 37.0);
        assert_eq!(right.min_x() - left.max_x(), 12.0);
        assert_eq!(WORK.max_x() - right.max_x(), 12.0);
        assert_eq!(WORK.max_y() - left.max_y(), 12.0);
    }

    #[test]
    fn gap_and_padding_are_twelve_points_in_every_direction() {
        assert_eq!(
            Spacing::STANDARD,
            Spacing {
                gap: 12.0,
                padding: 12.0
            }
        );

        let quarters = preset("quarters").unwrap();
        let frame = |index: usize| zone_frame(quarters.zones[index].rect, WORK, Spacing::STANDARD);
        let (top_left, top_right, bottom_left, bottom_right) = (frame(0), frame(1), frame(2), frame(3));
        assert_eq!(top_left.min_x() - WORK.min_x(), 12.0);
        assert_eq!(top_left.min_y() - WORK.min_y(), 12.0);
        assert_eq!(WORK.max_x() - bottom_right.max_x(), 12.0);
        assert_eq!(WORK.max_y() - bottom_right.max_y(), 12.0);
        assert_eq!(top_right.min_x() - top_left.max_x(), 12.0);
        assert_eq!(bottom_left.min_y() - top_left.max_y(), 12.0);
    }

    #[test]
    fn invalid_geometry_yields_zero() {
        assert_eq!(
            zone_frame(UnitRect::new(0.0, 0.0, 0.01, 1.0), WORK, Spacing::STANDARD),
            Rect::ZERO
        );
        assert_eq!(
            zone_frame(UnitRect::new(0.0, 0.0, 1.0, 1.0), Rect::ZERO, Spacing::STANDARD),
            Rect::ZERO
        );
    }

    #[test]
    fn hit_test_rejects_gaps_and_prefers_smaller_zones() {
        let halves = preset("halves").unwrap();
        assert_eq!(
            hit_test(Point::new(5.0, 300.0), halves, WORK, Spacing::STANDARD),
            None
        );
        assert_eq!(
            hit_test(Point::new(720.0, 300.0), halves, WORK, Spacing::STANDARD),
            None
        );
        assert_eq!(
            hit_test(Point::new(100.0, 10.0), halves, WORK, Spacing::STANDARD),
            None
        );
        let hit = hit_test(Point::new(100.0, 300.0), halves, WORK, Spacing::STANDARD).unwrap();
        assert_eq!(hit.id, "halves-0");

        let overlapping = Layout {
            id: "o".into(),
            name: "o".into(),
            zones: vec![
                Zone::new("big", UnitRect::new(0.0, 0.0, 1.0, 1.0)),
                Zone::new("small", UnitRect::new(0.0, 0.0, 0.5, 0.5)),
            ],
        };
        let hit = hit_test(Point::new(100.0, 100.0), &overlapping, WORK, Spacing::STANDARD).unwrap();
        assert_eq!(hit.id, "small");
    }

    #[test]
    fn the_eight_presets_have_fixed_names_and_zones_in_order() {
        type Rects = Vec<(f64, f64, f64, f64)>;
        let (third, half) = (1.0 / 3.0, 0.5);
        let expected: [(&str, &str, Rects); 8] = [
            (
                "halves",
                "Halves",
                vec![(0.0, 0.0, half, 1.0), (half, 0.0, half, 1.0)],
            ),
            (
                "thirds",
                "Three columns",
                vec![
                    (0.0, 0.0, third, 1.0),
                    (third, 0.0, third, 1.0),
                    (2.0 * third, 0.0, third, 1.0),
                ],
            ),
            (
                "focus",
                "Center stage",
                vec![
                    (0.0, 0.0, 0.25, 1.0),
                    (0.25, 0.0, half, 1.0),
                    (0.75, 0.0, 0.25, 1.0),
                ],
            ),
            (
                "quarters",
                "Four corners",
                vec![
                    (0.0, 0.0, half, half),
                    (half, 0.0, half, half),
                    (0.0, half, half, half),
                    (half, half, half, half),
                ],
            ),
            (
                "wide-left",
                "Main + stack",
                vec![
                    (0.0, 0.0, 2.0 * third, 1.0),
                    (2.0 * third, 0.0, third, half),
                    (2.0 * third, half, third, half),
                ],
            ),
            (
                "rows",
                "Two rows",
                vec![(0.0, 0.0, 1.0, half), (0.0, half, 1.0, half)],
            ),
            (
                "six",
                "Six zones",
                (0..6)
                    .map(|i| (f64::from(i % 3) / 3.0, f64::from(i / 3) / 2.0, third, half))
                    .collect(),
            ),
            ("full", "Full workspace", vec![(0.0, 0.0, 1.0, 1.0)]),
        ];

        assert_eq!(presets().len(), expected.len());
        for (layout, (id, name, rects)) in presets().iter().zip(expected) {
            assert_eq!((layout.id.as_str(), layout.name.as_str()), (id, name));
            let zones: Vec<(String, UnitRect)> = layout
                .zones
                .iter()
                .map(|zone| (zone.id.clone(), zone.rect))
                .collect();
            let wanted: Vec<(String, UnitRect)> = rects
                .into_iter()
                .enumerate()
                .map(|(index, (x, y, width, height))| {
                    (format!("{id}-{index}"), UnitRect::new(x, y, width, height))
                })
                .collect();
            assert_eq!(zones, wanted, "{id}");
        }

        let custom = Layout {
            id: "mine".into(),
            name: "Mine".into(),
            zones: vec![Zone::new("z", UnitRect::new(0.0, 0.0, 1.0, 1.0))],
        };
        let mut settings = crate::settings::Settings::defaults(crate::keys::Platform::MacOs);
        settings.layouts.custom.push(custom);
        let listed: Vec<&str> = settings.layouts().map(|layout| layout.id.as_str()).collect();
        assert_eq!(
            listed,
            [
                "halves",
                "thirds",
                "focus",
                "quarters",
                "wide-left",
                "rows",
                "six",
                "full",
                "mine"
            ]
        );
    }
}
