//! Following windows, refusals, and parking.

use std::sync::PoisonError;

use serde_json::{Value, json};
use snappy_core::geometry::Point;
use snappy_gnome::protocol::PROTOCOL_VERSION;
use snappy_platform::{DesktopGuard, WindowId, WindowSystem};

use crate::support::{Adapter, fake, record};

#[test]
fn windows_are_followed_once_and_refusals_read_as_messages() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().windows = vec![record(7, 0, 0.0), record(8, 0, 700.0)];
    let mut adapter = Adapter::start();
    assert_eq!(
        adapter.platform.window_at(Point::new(800.0, 200.0)).unwrap().id,
        WindowId(8)
    );
    adapter.platform.visible_windows(None);
    adapter.platform.visible_windows(None);
    let followed: Vec<Value> = fake
        .requests("follow")
        .iter()
        .map(|request| request["ids"].clone())
        .collect();
    assert_eq!(
        followed,
        vec![json!([8]), json!([7])],
        "each window is followed once"
    );

    let error = adapter
        .platform
        .set_position(WindowId(99), Point::new(0.0, 0.0))
        .unwrap_err();
    assert!(error.to_string().contains("no longer available"), "{error}");
    let displays = adapter.platform.displays();
    adapter.platform.bind(&displays);
    fake.state().park_refusal = Some("allWorkspaces".into());
    let error = adapter
        .platform
        .hide_for_workspace(WindowId(7), Point::default())
        .unwrap_err();
    assert!(error.to_string().contains("every workspace"), "{error}");
}

#[test]
fn lowering_asks_the_extension_and_a_closed_window_reads_as_gone() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().windows = vec![record(7, 0, 0.0)];
    let mut adapter = Adapter::start();
    adapter.platform.lower(WindowId(7)).unwrap();
    assert_eq!(fake.requests("lower"), vec![json!({"op": "lower", "id": 7})]);
    let error = adapter.platform.lower(WindowId(99)).unwrap_err();
    assert!(error.to_string().contains("no longer available"), "{error}");
}

#[test]
fn showing_a_parked_window_puts_it_back_in_place_before_it_returns() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut parked = record(5, 0, 300.0);
    parked["parked"] = json!(true);
    fake.state().windows = vec![parked];
    let mut adapter = Adapter::start();
    let origin = Point::new(300.0, 100.0);
    adapter.platform.show_for_workspace(WindowId(5), origin).unwrap();
    let order: Vec<String> = fake
        .state()
        .requests
        .iter()
        .filter_map(|request| request["op"].as_str().map(str::to_owned))
        .filter(|op| op == "setPosition" || op == "unpark")
        .collect();
    assert_eq!(order, vec!["unpark"], "already in place, so it only comes back");

    fake.state().requests.clear();
    let elsewhere = Point::new(40.0, 50.0);
    adapter
        .platform
        .show_for_workspace(WindowId(5), elsewhere)
        .unwrap();
    let order: Vec<String> = fake
        .state()
        .requests
        .iter()
        .filter_map(|request| request["op"].as_str().map(str::to_owned))
        .filter(|op| op == "setPosition" || op == "unpark")
        .collect();
    assert_eq!(
        order,
        vec!["setPosition", "unpark"],
        "moved while hidden, then shown"
    );
}
