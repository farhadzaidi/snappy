//! What the send-to-back scenarios share: the windows they start from, pressing and
//! holding the shortcut, and reading the stack and the debug log.

use std::time::Duration;

use snappy_core::geometry::Rect;
use snappy_core::keys::Os;
use snappy_core::send_to_back::{HOLD_AFTER, Restacking};
use snappy_core::workspace::ParkingPolicy;
use snappy_engine::{Engine, LogEvent};
use snappy_fake::{FakePlatform, FakeWindow, StackChange};
use snappy_platform::PlatformEvent;

use crate::support::{LEFT_ZONE, RIGHT_ZONE, event, hotkey_id, live_engine, logged, snappy};

/// A little past the moment a held press becomes a hold.
pub const PAST_HOLD: Duration = Duration::from_millis((HOLD_AFTER * 1000.0) as u64 + 50);

/// Windows 1, 2, and 3, each of its own app, stacked in the left zone, front to back, with 1
/// focused, and 4 in the right zone.
pub fn three_stacked() -> FakePlatform {
    let mut fake = FakePlatform::default();
    fake.open(4, FakeWindow::new("com.example.notes", 400, RIGHT_ZONE));
    fake.open(3, FakeWindow::new("com.example.terminal", 300, LEFT_ZONE));
    fake.open(2, FakeWindow::new("com.example.browser", 200, LEFT_ZONE));
    fake.open(1, FakeWindow::new("com.example.editor", 100, LEFT_ZONE));
    fake
}

/// The top `share` of the left zone.
pub fn top_of_left_zone(share: f64) -> Rect {
    Rect::new(
        LEFT_ZONE.x,
        LEFT_ZONE.y,
        LEFT_ZONE.width,
        LEFT_ZONE.height * share,
    )
}

/// Windows 1 to `count`, of the apps `apps` names in turn, from the top of the left zone,
/// front to back, with 1 focused: 1 fills the zone, and each after is taller than the one
/// before, so a band of each shows below the ones in front once 1 is gone.
pub fn cascade(apps: &[&str]) -> FakePlatform {
    let count = apps.len() as u64;
    let mut fake = FakePlatform::default();
    fake.open(9, FakeWindow::new("com.example.notes", 900, RIGHT_ZONE));
    for (id, app) in apps
        .iter()
        .enumerate()
        .map(|(index, app)| (index as u64 + 1, app))
        .rev()
    {
        let frame = if id == 1 {
            LEFT_ZONE
        } else {
            top_of_left_zone((id - 1) as f64 / count as f64)
        };
        let pid = 100 + apps.iter().position(|other| other == app).unwrap_or_default() as u32;
        fake.open(id, FakeWindow::new(app, pid, frame));
    }
    fake
}

/// Windows 1, 2, and 3 of three apps in a cascade.
pub fn three_cascaded() -> FakePlatform {
    cascade(&[
        "com.example.editor",
        "com.example.browser",
        "com.example.terminal",
    ])
}

/// A live engine with the debug log on.
pub fn logging_engine(name: &str, fake: FakePlatform) -> Engine<FakePlatform> {
    let engine = live_engine(name, fake);
    engine.force_debug_log();
    engine
}

pub fn send_to_back_key(engine: &Engine<FakePlatform>) -> u32 {
    hotkey_id(engine, &snappy("KeyB"))
}

/// Press the shortcut and let go at once, and wait for what the press does.
pub async fn press(engine: &Engine<FakePlatform>) {
    let id = send_to_back_key(engine);
    event(engine, PlatformEvent::HotKey(id));
    event(engine, PlatformEvent::HotKeyReleased(id));
    engine.idle().await;
}

/// Press the shortcut and keep it down past the moment it becomes a hold.
pub async fn hold(engine: &Engine<FakePlatform>) {
    event(engine, PlatformEvent::HotKey(send_to_back_key(engine)));
    tokio::time::sleep(PAST_HOLD).await;
    engine.idle().await;
}

/// Let go of the shortcut, and wait for what that does.
pub async fn let_go(engine: &Engine<FakePlatform>) {
    event(engine, PlatformEvent::HotKeyReleased(send_to_back_key(engine)));
    engine.idle().await;
}

pub fn frontmost(engine: &Engine<FakePlatform>) -> u64 {
    engine.with_platform(|p| p.stacking[0].0)
}

/// Every window, front to back.
pub fn stack(engine: &Engine<FakePlatform>) -> Vec<u64> {
    engine.with_platform(|p| p.front_to_back().iter().map(|id| id.0).collect())
}

/// Every change Snappy asked for to a window's place in the stack, in order.
pub fn stack_changes(engine: &Engine<FakePlatform>) -> Vec<(u64, StackChange)> {
    engine.with_platform(|p| {
        p.stack_changes
            .iter()
            .map(|(window, change)| (window.0, *change))
            .collect()
    })
}

/// The windows Snappy asked for `kind` of stack change, in order.
fn changed(engine: &Engine<FakePlatform>, kind: StackChange) -> Vec<u64> {
    stack_changes(engine)
        .into_iter()
        .filter(|(_, change)| *change == kind)
        .map(|(window, _)| window)
        .collect()
}

/// The windows Snappy focused, which raises each, in order.
pub fn focused_windows(engine: &Engine<FakePlatform>) -> Vec<u64> {
    changed(engine, StackChange::Focus)
}

/// The windows Snappy raised alone, in order.
pub fn raised_windows(engine: &Engine<FakePlatform>) -> Vec<u64> {
    changed(engine, StackChange::Raise)
}

/// Every window write, which moves or sizes a window.
pub fn writes(engine: &Engine<FakePlatform>) -> Vec<(u64, &'static str)> {
    engine.with_platform(|p| p.writes.iter().map(|(window, kind)| (window.0, *kind)).collect())
}

/// Each window sent to the back, with the one focused, as the debug log has them.
pub fn sent(name: &str) -> Vec<(u64, u64)> {
    logged(name, LogEvent::SentToBack)
        .iter()
        .filter_map(|record| Some((record["window"].as_u64()?, record["focus"].as_u64()?)))
        .collect()
}

/// The windows put back where they were, as the debug log has them.
pub fn put_back(name: &str) -> Vec<u64> {
    logged(name, LogEvent::PutBack)
        .iter()
        .filter_map(|record| record["window"].as_u64())
        .collect()
}

/// The fake as Windows, which lowers a window and can restack it under another.
pub fn lowering(mut fake: FakePlatform) -> FakePlatform {
    fake.platform = Os::Windows;
    fake.parking = ParkingPolicy::OffScreen;
    fake.restacking = Restacking::Lower { stack_below: true };
    fake
}

/// The fake as GNOME or KDE, which lower a window but can't restack it under another.
pub fn lowering_only(mut fake: FakePlatform) -> FakePlatform {
    fake.platform = Os::Linux;
    fake.parking = ParkingPolicy::Desktop;
    fake.restacking = Restacking::Lower { stack_below: false };
    fake
}

/// `fake` as each way of sending a window to the back has it: macOS raises the windows over
/// it, and Windows and Linux lower it.
pub fn each_way(fake: FakePlatform) -> [FakePlatform; 2] {
    [fake.clone(), lowering(fake)]
}

/// How `fake` restacks, as a scenario's name and its messages say it.
pub fn way(fake: &FakePlatform) -> &'static str {
    match fake.restacking {
        Restacking::RaiseOnly => "raising",
        Restacking::Lower { stack_below: true } => "lowering",
        Restacking::Lower { stack_below: false } => "lowering-only",
    }
}
