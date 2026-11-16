//! Click-through overlays, one shown at a time.

use snappy_core::geometry::{Point, Rect};
use snappy_platform::{Caption, Overlay, OverlayScene, OverlayZone, WindowSystem};
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::ConnectionExt as _;

use crate::support::{desktop, id, wait_until};

fn scene(work_area: Rect) -> OverlayScene {
    let half = (work_area.width - 36.0) / 2.0;
    OverlayScene {
        work_area,
        zones: vec![
            OverlayZone {
                frame: Rect::new(
                    work_area.x + 12.0,
                    work_area.y + 12.0,
                    half,
                    work_area.height - 24.0,
                ),
                number: 1,
                highlighted: true,
                caption: Some(Caption::Stack),
                swap_source: false,
            },
            OverlayZone {
                frame: Rect::new(
                    work_area.x + 24.0 + half,
                    work_area.y + 12.0,
                    half,
                    work_area.height - 24.0,
                ),
                number: 2,
                highlighted: false,
                caption: None,
                swap_source: false,
            },
        ],
    }
}

#[test]
fn one_click_through_overlay_shows_at_a_time() {
    let mut desktop = desktop!();
    let displays = desktop.platform.displays();
    let (left, right) = (&displays[0], &displays[1]);
    desktop.platform.prepare(&left.id, &scene(left.work_area));
    desktop.platform.prepare(&right.id, &scene(right.work_area));
    assert!(wait_until(|| desktop.overlay_windows().len() == 2));
    assert!(
        desktop
            .overlay_windows()
            .iter()
            .all(|window| !desktop.is_viewable(*window))
    );

    desktop.platform.show(&left.id, &scene(left.work_area));
    assert!(wait_until(|| desktop
        .overlay_windows()
        .iter()
        .filter(|w| desktop.is_viewable(**w))
        .count()
        == 1));
    let shown = desktop
        .overlay_windows()
        .into_iter()
        .find(|window| desktop.is_viewable(*window))
        .unwrap();
    let attributes = desktop
        .client
        .get_window_attributes(shown)
        .unwrap()
        .reply()
        .unwrap();
    assert!(attributes.override_redirect);
    let geometry = desktop.client.get_geometry(shown).unwrap().reply().unwrap();
    assert_eq!(
        (geometry.x, geometry.y, geometry.width, geometry.height),
        (0, 0, 1920, 1080)
    );

    // Without a compositor the overlay is cut down to zone outlines and number boxes.
    let compositor = desktop.atom("_NET_WM_CM_S0");
    let owner = desktop
        .client
        .get_selection_owner(compositor)
        .unwrap()
        .reply()
        .unwrap()
        .owner;
    let pieces = desktop
        .client
        .shape_get_rectangles(shown, shape::SK::BOUNDING)
        .unwrap()
        .reply()
        .unwrap()
        .rectangles
        .len();
    if owner == x11rb::NONE {
        // The server splits the outline into bands of rectangles.
        assert!(pieces > 10, "{pieces}");
    } else {
        assert_eq!(pieces, 1);
    }

    // Clicks pass through: the window under the overlay is still the one found.
    let below = desktop.open_plain(Rect::new(200.0, 200.0, 300.0, 200.0));
    desktop.platform.show(&left.id, &scene(left.work_area));
    let frame = desktop.platform.frame(id(below)).unwrap();
    let inside = Point::new(frame.x + 30.0, frame.y + 30.0);
    desktop.move_pointer(inside);
    let under = desktop
        .client
        .query_pointer(desktop.root)
        .unwrap()
        .reply()
        .unwrap()
        .child;
    assert_ne!(under, shown);

    desktop.platform.show(&right.id, &scene(right.work_area));
    assert!(wait_until(|| !desktop.is_viewable(shown)));
    assert_eq!(
        desktop
            .overlay_windows()
            .iter()
            .filter(|w| desktop.is_viewable(**w))
            .count(),
        1
    );

    desktop.platform.hide();
    assert!(wait_until(|| desktop
        .overlay_windows()
        .iter()
        .all(|w| !desktop.is_viewable(*w))));

    desktop.platform.retain(std::slice::from_ref(&left.id));
    assert!(wait_until(|| desktop.overlay_windows().len() == 1));
}
