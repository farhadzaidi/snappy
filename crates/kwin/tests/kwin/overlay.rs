//! The zone overlay's surfaces as KWin shows them.

use snappy_core::geometry::Rect;
use snappy_platform::{Caption, Overlay, OverlayScene, OverlayZone, WindowSystem};

use crate::probes::{overlay_windows, wait_for_overlay};
use crate::support::kwin;

#[test]
fn the_overlay_covers_the_work_area_and_shows_and_hides_without_closing() {
    let mut kwin = kwin!();
    let displays = kwin.platform.displays();
    let work_area = displays[0].work_area;
    let scene = |highlighted: usize, work_area: Rect| OverlayScene {
        work_area,
        zones: (0..2)
            .map(|index| OverlayZone {
                frame: Rect::new(
                    work_area.x + work_area.width / 2.0 * index as f64,
                    work_area.y,
                    work_area.width / 2.0,
                    work_area.height,
                ),
                number: index + 1,
                highlighted: index == highlighted,
                caption: (index == highlighted).then_some(Caption::Swap),
                swap_source: false,
            })
            .collect(),
    };
    let before = kwin.platform.all_windows().len();
    let pid = std::process::id();

    // Prepared ahead of a drag, the surface is there before anything shows.
    kwin.platform.prepare(&displays[0].id, &scene(0, work_area));
    let first = wait_for_overlay(pid, work_area);
    assert_eq!(
        kwin.platform.all_windows().len(),
        before,
        "the overlay is never one of the windows"
    );

    // KWin's effects animate windows that open and close. Showing, changing, and hiding
    // the overlay must do neither: the same window stays, with the same UUID.
    for cycle in 0..5 {
        kwin.platform.show(&displays[0].id, &scene(cycle % 2, work_area));
        kwin.platform
            .show(&displays[0].id, &scene((cycle + 1) % 2, work_area));
        kwin.platform.hide();
        assert_eq!(overlay_windows(pid), vec![first.clone()], "cycle {cycle}");
    }

    // A second display gets a surface of its own, and the first stays.
    let second_area = displays[1].work_area;
    kwin.platform.show(&displays[1].id, &scene(0, second_area));
    let second = wait_for_overlay(pid, second_area);
    kwin.platform.show(&displays[0].id, &scene(1, work_area));
    kwin.platform.hide();
    let mut both = vec![first.clone(), second];
    both.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    let mut found = overlay_windows(pid);
    found.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    assert_eq!(found, both);

    // A display that's gone takes its surface with it.
    kwin.platform.retain(&[displays[0].id.clone()]);
    kwin.wait_until("the second display's overlay is gone", |_| {
        overlay_windows(pid) == vec![first.clone()]
    });
    kwin.platform.retain(&[]);
    kwin.wait_until("every overlay is gone", |_| overlay_windows(pid).is_empty());
}
