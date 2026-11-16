//! Overlay scenes crossing the bus.

use std::sync::PoisonError;

use snappy_core::geometry::Rect;
use snappy_gnome::protocol::PROTOCOL_VERSION;
use snappy_platform::{Caption, Overlay, OverlayScene, OverlayZone, WindowSystem};

use crate::support::{Adapter, fake};

#[test]
fn identical_overlay_scenes_cross_the_bus_once() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    let displays = adapter.platform.displays();
    let display = &displays[0];
    let scene = |caption| OverlayScene {
        work_area: display.work_area,
        zones: vec![OverlayZone {
            frame: Rect::new(12.0, 44.0, 942.0, 1024.0),
            number: 1,
            highlighted: true,
            caption,
            swap_source: false,
        }],
    };
    for _ in 0..5 {
        adapter.platform.show(&display.id, &scene(None));
    }
    adapter.platform.show(&display.id, &scene(Some(Caption::Stack)));
    adapter.platform.hide();
    adapter.platform.hide();
    assert_eq!(fake.requests("overlayShow").len(), 2);
    assert_eq!(fake.requests("overlayHide").len(), 1);
    let shown = &fake.requests("overlayShow")[1]["scene"];
    assert_eq!(shown["zones"][0]["caption"]["text"], "Stack");
    assert_eq!(shown["workArea"]["y"], 32.0);
}
