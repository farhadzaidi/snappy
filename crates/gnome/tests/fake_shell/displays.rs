//! Displays from Mutter's names and the extension's work areas.

use std::sync::PoisonError;

use snappy_core::geometry::Rect;
use snappy_gnome::protocol::PROTOCOL_VERSION;
use snappy_platform::WindowSystem;

use crate::support::{Adapter, fake};

#[test]
fn displays_join_mutters_names_with_the_extensions_work_areas() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    let displays = adapter.platform.displays();
    assert_eq!(displays.len(), 1);
    assert_eq!(displays[0].name, "Dell 27\"");
    assert!(displays[0].id.starts_with("DP-1-"));
    assert_eq!(displays[0].work_area, Rect::new(0.0, 32.0, 1920.0, 1048.0));
}
