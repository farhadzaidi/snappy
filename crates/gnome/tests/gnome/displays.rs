//! Displays with stable IDs, names, and work areas.

use snappy_core::geometry::Rect;
use snappy_platform::WindowSystem;

use crate::support::shell;

#[test]
fn displays_have_stable_ids_names_and_work_areas() {
    let mut shell = shell!();
    let displays = shell.platform.displays();
    assert_eq!(displays.len(), 2, "{displays:?}");
    let primary = &displays[0];
    assert_eq!(primary.frame, Rect::new(0.0, 0.0, 1920.0, 1080.0));
    assert!(
        primary.work_area.y > 0.0 && primary.work_area.max_y() == 1080.0,
        "the top bar leaves the work area: {primary:?}"
    );
    assert_eq!(primary.scale, 1.0);
    assert_eq!(displays[1].frame.x, 1920.0);
    assert!(!primary.name.is_empty());
    assert_ne!(primary.id, displays[1].id);
    assert_eq!(shell.platform.displays(), displays, "the same read twice");
}
