//! The extension's events becoming engine events, in order.

use std::sync::PoisonError;

use serde_json::json;
use snappy_core::geometry::Point;
use snappy_core::keys::{Modifier, Mods};
use snappy_gnome::protocol::PROTOCOL_VERSION;
use snappy_platform::{PlatformEvent, Tray, TrayMenu, WindowId};

use crate::support::{Adapter, fake, record};

#[test]
fn the_extensions_events_arrive_in_order_as_engine_events() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().windows = vec![record(1, 100, 0.0)];
    let mut adapter = Adapter::start();
    adapter.platform.update(&TrayMenu::default());
    adapter.settle(fake);
    let generation = fake.requests("menu")[0]["generation"].clone();

    for event in [
        json!({"type": "pointerDown", "x": 10, "y": 20, "mods": 1, "window": 1}),
        json!({"type": "pointerDragged", "x": 30, "y": 40, "mods": 1 | 4}),
        json!({"type": "modifiers", "x": 30, "y": 40, "mods": 64}),
        json!({"type": "pointerUp", "x": 50, "y": 60, "mods": 0}),
        json!({"type": "hotkey", "id": 4}),
        json!({"type": "windowAdded", "id": 2, "pid": 200}),
        json!({"type": "windowMoved", "id": 2}),
        json!({"type": "focus", "pid": 200}),
        json!({"type": "windowRemoved", "id": 2}),
        json!({"type": "workspace"}),
        json!({"type": "displays"}),
        json!({"type": "somethingFromANewerExtension"}),
        json!({"type": "menu", "item": 0, "generation": generation}),
    ] {
        fake.emit(event);
    }
    let shift = Mods::from_iter([Modifier::Shift]);
    let expected = vec![
        PlatformEvent::PointerDown {
            at: Point::new(10.0, 20.0),
            mods: shift,
            window: Some(WindowId(1)),
            frame: None,
        },
        PlatformEvent::PointerDragged {
            at: Point::new(30.0, 40.0),
            mods: Mods::from_iter([Modifier::Shift, Modifier::Ctrl]),
        },
        PlatformEvent::ModifiersChanged {
            at: Point::new(30.0, 40.0),
            mods: Mods::from_iter([Modifier::Meta]),
        },
        PlatformEvent::PointerUp {
            at: Point::new(50.0, 60.0),
            mods: Mods::NONE,
        },
        PlatformEvent::HotKey(4),
        PlatformEvent::AppLaunched { pid: 200 },
        PlatformEvent::WindowCreated { pid: 200 },
        PlatformEvent::WindowMoved(WindowId(2)),
        PlatformEvent::FocusChanged {
            pid: 200,
            by_activation: true,
        },
        PlatformEvent::WindowDestroyed(WindowId(2)),
        PlatformEvent::AppTerminated { pid: 200 },
        PlatformEvent::NativeDesktopChanged,
        PlatformEvent::DisplaysChanged,
        PlatformEvent::OpenAppRequested,
    ];
    let seen: Vec<PlatformEvent> = (0..expected.len()).map(|_| adapter.next()).collect();
    assert_eq!(seen, expected);
}

#[test]
fn a_choice_from_a_replaced_menu_is_ignored() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    adapter.platform.update(&TrayMenu::default());
    let first = fake.requests("menu")[0]["generation"].clone();
    adapter.platform.update(&TrayMenu {
        live: true,
        ..TrayMenu::default()
    });
    let second = fake.requests("menu")[1]["generation"].clone();
    assert_ne!(first, second);

    fake.emit(json!({"type": "menu", "item": 0, "generation": first}));
    assert!(
        adapter.settle(fake).is_empty(),
        "the old menu's choice does nothing"
    );
    fake.emit(json!({"type": "menu", "item": 0, "generation": second}));
    assert_eq!(adapter.settle(fake), vec![PlatformEvent::OpenAppRequested]);
}
