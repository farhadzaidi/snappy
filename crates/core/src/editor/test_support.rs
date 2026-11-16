//! Zones and checks the editor's tests share.

use crate::layout::{UnitRect, Zone};

pub fn zone(id: &str, x: f64, y: f64, width: f64, height: f64) -> Zone {
    Zone::new(id, UnitRect::new(x, y, width, height))
}

pub fn rect(zones: &[Zone], id: &str) -> UnitRect {
    zones
        .iter()
        .find(|zone| zone.id == id)
        .map(|zone| zone.rect)
        .unwrap_or_default()
}

pub fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

pub fn grid() -> Vec<Zone> {
    vec![
        zone("a", 0.0, 0.0, 0.5, 0.5),
        zone("b", 0.5, 0.0, 0.5, 0.5),
        zone("c", 0.0, 0.5, 0.5, 0.5),
        zone("d", 0.5, 0.5, 0.5, 0.5),
    ]
}
