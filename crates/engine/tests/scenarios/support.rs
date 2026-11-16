//! What the scenarios share: engines set up for a test, input like the user's, and what
//! the engine did.

use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;

use snappy_core::bindings::Refusal;
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{KeyChord, Modifier, Mods};
use snappy_core::settings::{HistoryStep, LayoutOptions, Settings};
use snappy_core::workspace::ParkingPolicy;
use snappy_engine::{Engine, Input, Paths};
use snappy_fake::{FakePlatform, FakeWindow};
use snappy_ipc::{AppState, Command, ToSettings};
use snappy_platform::{MarkStyle, PlatformEvent, WindowId};

pub const LEFT_ZONE: Rect = Rect::new(12.0, 37.0, 702.0, 851.0);
pub const RIGHT_ZONE: Rect = Rect::new(726.0, 37.0, 702.0, 851.0);

/// Fresh paths for a test: nothing on disk yet.
pub fn paths(name: &str) -> Paths {
    let kept = paths_kept(name);
    if let Some(root) = kept.settings.parent() {
        let _ = std::fs::remove_dir_all(root);
    }
    kept
}

/// The same paths, keeping whatever an earlier engine in the test wrote.
pub fn paths_kept(name: &str) -> Paths {
    let root = std::env::temp_dir().join(format!("snappy-engine-{name}-{}", std::process::id()));
    Paths {
        settings: root.join("settings.json"),
        state_dir: root.join("state"),
        log_dir: root.join("logs"),
    }
}

/// Run a test on a single-threaded runtime with paused time, so settle waits are instant.
pub fn scenario<F: Future<Output = ()>>(test: impl FnOnce() -> F) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, test());
}

/// A live engine on the halves layout, where every layout's zones have the Snappy key with
/// their number as shortcuts.
pub fn live_engine(name: &str, fake: FakePlatform) -> Engine<FakePlatform> {
    let paths = paths(name);
    let mut settings = Settings::defaults(fake.platform);
    for layout in snappy_core::layout::presets() {
        let mut options = LayoutOptions::empty();
        for (index, zone) in layout.zones.iter().enumerate().take(9) {
            let chord = snappy(&format!("Digit{}", index + 1));
            options.zone_shortcuts.insert(zone.id.clone(), chord);
        }
        settings.layouts.options.insert(layout.id.clone(), options);
    }
    if let Some(folder) = paths.settings.parent() {
        std::fs::create_dir_all(folder).unwrap();
    }
    std::fs::write(&paths.settings, settings.to_json().unwrap()).unwrap();
    let engine = Engine::new(fake, paths);
    engine.start();
    command(
        &engine,
        Command::ApplyLayout {
            display: "main".into(),
            layout: "halves".into(),
        },
    );
    command(&engine, Command::SetSnapping { enabled: true });
    engine
}

pub fn command(engine: &Engine<FakePlatform>, command: Command) {
    engine.handle(Input::Command { id: 1, command });
}

/// Send a command and return the daemon's reply to it.
pub fn reply_to(engine: &Engine<FakePlatform>, sent: Command) -> Option<Refusal> {
    engine.take_outbox();
    command(engine, sent);
    engine
        .take_outbox()
        .into_iter()
        .find_map(|message| match message {
            ToSettings::Reply { error, .. } => Some(error),
            _ => None,
        })
        .unwrap()
}

/// A Snappy key of Control and Option, in place of the default.
pub fn control_option() -> Mods {
    Mods::from_iter([Modifier::Ctrl, Modifier::Alt])
}

pub fn event(engine: &Engine<FakePlatform>, event: PlatformEvent) {
    engine.handle(Input::Platform(event));
}

pub fn shift() -> Mods {
    Mods::from_iter([Modifier::Shift])
}

pub fn with_window(frame: Rect) -> FakePlatform {
    let mut fake = FakePlatform::default();
    fake.open(1, FakeWindow::new("com.example.editor", 100, frame));
    fake
}

/// Press a window's title bar, let the app follow the pointer, release, and settle.
pub async fn drag(engine: &Engine<FakePlatform>, window: WindowId, to: Point, mods: Mods) {
    release(engine, window, to, mods);
    settle(engine).await;
}

/// Drag like `drag`, but return as soon as the button is up.
pub fn release(engine: &Engine<FakePlatform>, window: WindowId, to: Point, mods: Mods) {
    let start = engine.with_platform(|p| p.frame_of(window));
    let grip = Point::new(start.x + 200.0, start.y + 15.0);
    event(
        engine,
        PlatformEvent::PointerDown {
            at: grip,
            mods,
            window: None,
        },
    );
    let moved = start.with_origin(Point::new(to.x - 200.0, to.y - 15.0));
    engine.with_platform(|p| p.windows.get_mut(&window).unwrap().frame = moved);
    event(engine, PlatformEvent::PointerDragged { at: to, mods });
    event(engine, PlatformEvent::PointerUp { at: to, mods });
}

/// The records of `event` in a test's debug log so far, oldest first.
pub fn logged(name: &str, event: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(paths_kept(name).log_dir.join("debug.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|record| record["event"] == event)
        .collect()
}

pub async fn settle(engine: &Engine<FakePlatform>) {
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        engine.idle().await;
    }
}

pub fn hotkey_id(engine: &Engine<FakePlatform>, chord: &KeyChord) -> u32 {
    let key = engine.settings().snappy_key;
    engine.with_platform(|p| {
        p.registered
            .iter()
            .find(|(_, registered)| *registered == chord.combo(key))
            .map(|(id, _)| *id)
            .unwrap()
    })
}

pub fn snappy(code: &str) -> KeyChord {
    KeyChord::snappy(code, [])
}

pub fn started(name: &str, fake: FakePlatform) -> Engine<FakePlatform> {
    let engine = Engine::new(fake, paths(name));
    engine.start();
    engine
}

/// The settings window, open on its own and enrolled in workspace 1.
pub fn with_settings_window() -> (FakePlatform, Rect) {
    let settings_frame = Rect::new(200.0, 100.0, 1080.0, 760.0);
    let mut fake = FakePlatform::default();
    fake.open(9, FakeWindow::new("local.snappy.settings", 4242, settings_frame));
    (fake, settings_frame)
}

pub fn status_text(engine: &Engine<FakePlatform>) -> String {
    engine.status().map(|status| status.message).unwrap_or_default()
}

/// Window 1 focused in the left zone, window 2 in the right zone.
pub fn two_zoned_windows() -> FakePlatform {
    let mut fake = FakePlatform::default();
    fake.open(2, FakeWindow::new("com.example.browser", 200, RIGHT_ZONE));
    fake.open(1, FakeWindow::new("com.example.editor", 100, LEFT_ZONE));
    fake
}

/// Window 1 focused and floating over the left zone, its center inside it, and window 2
/// filling `occupied`.
pub fn floating_over_the_left_zone(occupied: Rect) -> FakePlatform {
    let mut fake = FakePlatform::default();
    fake.open(2, FakeWindow::new("com.example.browser", 200, occupied));
    fake.open(1, FakeWindow::new("com.example.editor", 100, FLOATING));
    fake
}

/// A second display to the right of the main one.
pub fn side_display() -> snappy_platform::Display {
    snappy_platform::Display {
        id: "side".into(),
        name: "Side".into(),
        frame: Rect::new(1440.0, 0.0, 1920.0, 1080.0),
        work_area: Rect::new(1440.0, 25.0, 1920.0, 1055.0),
        scale: 1.0,
    }
}

/// A zone of the thirds layout on the side display.
pub fn side_third(index: usize) -> Rect {
    let thirds = snappy_core::layout::preset("thirds").unwrap();
    snappy_core::layout::zone_frame(
        thirds.zones[index].rect,
        side_display().work_area,
        snappy_core::layout::Spacing::STANDARD,
    )
}

/// Thirds on the side display, halves on the main one and as the default.
pub fn thirds_on_the_side(engine: &Engine<FakePlatform>) {
    for (display, layout) in [("side", "thirds"), ("main", "halves")] {
        command(
            engine,
            Command::ApplyLayout {
                display: display.into(),
                layout: layout.into(),
            },
        );
    }
}

pub fn frozen(engine: &Engine<FakePlatform>, window: u64, frozen: bool) {
    engine.with_platform(|p| p.windows.get_mut(&WindowId(window)).unwrap().frozen = frozen);
}

/// Size and position writes to a window so far.
pub fn writes_to(engine: &Engine<FakePlatform>, window: u64) -> usize {
    engine.with_platform(|p| {
        p.writes
            .iter()
            .filter(|(id, kind)| *id == WindowId(window) && matches!(*kind, "size" | "position"))
            .count()
    })
}

/// Give each workspace Control+n to switch, in place of its default Snappy key with n,
/// which `live_engine`'s zones use, Control+Shift+n to send, and Control+Option+n to move
/// here and switch, then turn workspaces on.
pub fn enable_workspaces(engine: &Engine<FakePlatform>) {
    for (index, mut workspace) in engine.settings().workspaces.items.into_iter().enumerate() {
        let digit = format!("Digit{}", index + 1);
        let chord = |mods: &[Modifier]| Some(KeyChord::new(&digit, mods.iter().copied()));
        workspace.switch_shortcut = chord(&[Modifier::Ctrl]);
        workspace.send_shortcut = chord(&[Modifier::Ctrl, Modifier::Shift]);
        workspace.send_and_switch_shortcut = chord(&[Modifier::Ctrl, Modifier::Alt]);
        command(engine, Command::UpdateWorkspace { workspace });
    }
    command(engine, Command::SetWorkspacesEnabled { enabled: true });
}

/// Undo or redo through its shortcut, assigning it first since neither has a default.
pub fn press_history(engine: &Engine<FakePlatform>, step: HistoryStep) {
    let chord = match step {
        HistoryStep::Undo => snappy("KeyU"),
        HistoryStep::Redo => snappy("KeyY"),
    };
    if engine.settings().history.get(step) != Some(&chord) {
        command(
            engine,
            Command::SetHistoryShortcut {
                step,
                chord: Some(chord.clone()),
            },
        );
    }
    event(engine, PlatformEvent::HotKey(hotkey_id(engine, &chord)));
}

pub fn undo(engine: &Engine<FakePlatform>) {
    press_history(engine, HistoryStep::Undo);
}

pub fn switch_to(engine: &Engine<FakePlatform>, number: u8) -> u32 {
    hotkey_id(
        engine,
        &KeyChord::new(&format!("Digit{number}"), [Modifier::Ctrl]),
    )
}

pub fn send_to(engine: &Engine<FakePlatform>, number: u8) -> u32 {
    hotkey_id(
        engine,
        &KeyChord::new(&format!("Digit{number}"), [Modifier::Ctrl, Modifier::Shift]),
    )
}

pub fn is_parked(engine: &Engine<FakePlatform>, window: u64) -> bool {
    engine.with_platform(|p| p.frame_of(WindowId(window)).x) >= 1439.0
}

/// Workspaces on, window 2 sent to workspace 2 and then shown in the right zone again by
/// its app, and window 1 focused in the left zone.
pub async fn inactive_window_in_the_right_zone(name: &str) -> Engine<FakePlatform> {
    let engine = live_engine(name, two_zoned_windows());
    enable_workspaces(&engine);
    engine.with_platform(|p| p.focused = Some(WindowId(2)));
    event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
    settle(&engine).await;
    assert!(is_parked(&engine, 2));

    engine.with_platform(|p| {
        p.windows.get_mut(&WindowId(2)).unwrap().frame = RIGHT_ZONE;
        p.focused = Some(WindowId(1));
    });
    engine
}

/// What the settings app would see now.
pub fn app_state(engine: &Engine<FakePlatform>) -> AppState {
    engine.handle(Input::SettingsAppStarted { pid: 9999 });
    engine
        .take_outbox()
        .into_iter()
        .rev()
        .find_map(|message| match message {
            ToSettings::State(state) => Some(*state),
            _ => None,
        })
        .unwrap()
}

pub fn window_counts(engine: &Engine<FakePlatform>) -> BTreeMap<String, usize> {
    app_state(engine).workspaces.window_counts
}

pub fn frame_of(engine: &Engine<FakePlatform>, window: u64) -> Rect {
    engine.with_platform(|p| p.frame_of(WindowId(window)))
}

pub fn place(engine: &Engine<FakePlatform>, window: u64, frame: Rect) {
    engine.with_platform(|p| p.windows.get_mut(&WindowId(window)).unwrap().frame = frame);
}

/// The outcomes recorded in the drop diagnostics, oldest first.
pub fn drop_outcomes(name: &str) -> Vec<String> {
    let Ok(bytes) = std::fs::read(paths_kept(name).log_dir.join("drops.json")) else {
        return Vec::new();
    };
    let records: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    records
        .iter()
        .map(|record| record["outcome"].as_str().unwrap().to_owned())
        .collect()
}

pub fn press(engine: &Engine<FakePlatform>, at: Point, mods: Mods) {
    event(
        engine,
        PlatformEvent::PointerDown {
            at,
            mods,
            window: None,
        },
    );
}

/// A click above the zones, where no window is, which cancels the operation in flight
/// without claiming any window.
pub fn click_away(engine: &Engine<FakePlatform>) {
    let at = Point::new(720.0, 10.0);
    press(engine, at, Mods::NONE);
    event(engine, PlatformEvent::PointerUp { at, mods: Mods::NONE });
}

pub const FLOATING: Rect = Rect::new(300.0, 200.0, 600.0, 400.0);

/// A live engine on the halves layout with workspaces on and every shortcut at its default.
pub fn default_workspaces(name: &str, fake: FakePlatform) -> Engine<FakePlatform> {
    let engine = started(name, fake);
    command(
        &engine,
        Command::ApplyLayout {
            display: "main".into(),
            layout: "halves".into(),
        },
    );
    command(&engine, Command::SetSnapping { enabled: true });
    command(&engine, Command::SetWorkspacesEnabled { enabled: true });
    assert!(app_state(&engine).workspaces.running);
    engine
}

pub async fn press_default(engine: &Engine<FakePlatform>, chord: &str) {
    event(engine, PlatformEvent::HotKey(hotkey_id(engine, &snappy(chord))));
    settle(engine).await;
}

/// Focus a window the way the platform reports it.
pub fn focus(engine: &Engine<FakePlatform>, window: u64) {
    let pid = engine.with_platform(|p| {
        p.focused = Some(WindowId(window));
        p.windows[&WindowId(window)].pid
    });
    event(
        engine,
        PlatformEvent::FocusChanged {
            pid,
            by_activation: false,
        },
    );
}

pub fn focused(engine: &Engine<FakePlatform>) -> Option<WindowId> {
    engine.with_platform(|p| p.focused)
}

/// The frames the main display outlines, in either style.
pub fn marks(engine: &Engine<FakePlatform>) -> Vec<Rect> {
    marked(engine).into_iter().map(|(frame, _)| frame).collect()
}

pub fn marked(engine: &Engine<FakePlatform>) -> Vec<(Rect, MarkStyle)> {
    engine.with_platform(|p| {
        p.marks
            .get("main")
            .map(|windows| windows.iter().map(|mark| (mark.frame, mark.style)).collect())
            .unwrap_or_default()
    })
}

pub fn active(engine: &Engine<FakePlatform>) -> String {
    engine.settings().workspaces.active_id
}

/// Four windows of one app, side by side.
pub fn four_windows() -> FakePlatform {
    let mut fake = FakePlatform::default();
    for id in 1..=4 {
        let frame = Rect::new(40.0 + 300.0 * (id as f64 - 1.0), 100.0, 280.0, 400.0);
        fake.open(id, FakeWindow::new("com.example.editor", 100, frame));
    }
    fake
}

pub fn relaunched(name: &str, platform: FakePlatform) -> Engine<FakePlatform> {
    let engine = Engine::new(platform, paths_kept(name));
    engine.start();
    engine
}

/// Every write and bounds-change bracket to any window so far.
pub fn all_writes(engine: &Engine<FakePlatform>) -> Vec<(WindowId, &'static str)> {
    engine.with_platform(|p| p.writes.clone())
}

/// Stop the way installing a new build does. Returns the platform as the next launch finds
/// it, with its writes cleared.
pub fn stopped_for_restart(engine: &Engine<FakePlatform>) -> FakePlatform {
    engine.with_platform(|p| p.writes.clear());
    engine.handle(Input::Platform(PlatformEvent::RestartRequested));
    assert_eq!(all_writes(engine), [], "stopping moves nothing");
    let mut platform = engine.with_platform(|p| p.clone());
    platform.writes.clear();
    platform
}

pub fn parking_on_a_desktop(mut fake: FakePlatform) -> FakePlatform {
    fake.parking = ParkingPolicy::Desktop;
    fake
}

pub fn on_parking_desktop(engine: &Engine<FakePlatform>, window: u64) -> bool {
    engine.with_platform(|p| p.windows[&WindowId(window)].state.on_parking_desktop)
}

/// Workspaces on with window 1 sent to workspace 2.
pub async fn window_one_on_the_parking_desktop(name: &str) -> Engine<FakePlatform> {
    let engine = live_engine(name, parking_on_a_desktop(two_zoned_windows()));
    enable_workspaces(&engine);
    event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
    settle(&engine).await;
    assert!(on_parking_desktop(&engine, 1));
    engine
}

pub fn press_nearest_empty(engine: &Engine<FakePlatform>) {
    event(engine, PlatformEvent::HotKey(hotkey_id(engine, &snappy("Space"))));
}

pub const TABBED: Rect = Rect::new(200.0, 150.0, 640.0, 420.0);
pub const TERMINAL: u32 = 300;

/// An editor window, 4, and a Terminal window with three native tabs, 1 to 3, the last
/// opened showing. Each tab is a window of its own, as macOS makes them.
pub fn tabbed_terminal(frame: Rect) -> FakePlatform {
    let mut fake = FakePlatform::default();
    let editor = Rect::new(900.0, 300.0, 400.0, 300.0);
    fake.open(4, FakeWindow::new("com.example.editor", 100, editor));
    fake.open(1, FakeWindow::new("com.apple.Terminal", TERMINAL, frame));
    fake.add_tab(2, WindowId(1));
    fake.add_tab(3, WindowId(2));
    fake
}

/// Click a tab, which shows and focuses it.
pub fn pick_tab(engine: &Engine<FakePlatform>, tab: u64) {
    engine.with_platform(|p| p.select_tab(WindowId(tab)));
    terminal_focused(engine);
}

pub fn terminal_focused(engine: &Engine<FakePlatform>) {
    event(
        engine,
        PlatformEvent::FocusChanged {
            pid: TERMINAL,
            by_activation: false,
        },
    );
}

/// Workspaces on with every tab of the Terminal window visited once, so each is a member,
/// and tab 3 showing again.
pub fn visited_tabs(name: &str, frame: Rect) -> Engine<FakePlatform> {
    let engine = default_workspaces(name, tabbed_terminal(frame));
    for tab in [1, 2, 3] {
        pick_tab(&engine, tab);
    }
    engine
}

/// The windows a journal or membership file lists, checking it says its version.
pub fn saved_windows(path: &std::path::Path) -> Vec<serde_json::Value> {
    let file: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(file["version"], 1);
    file["windows"].as_array().unwrap().clone()
}

/// The workspace the membership file gives each window.
pub fn remembered_workspaces(name: &str) -> BTreeMap<u64, String> {
    saved_windows(&paths_kept(name).membership())
        .iter()
        .map(|entry| {
            let window = entry["key"]["window"].as_str().unwrap().parse().unwrap();
            (window, entry["workspaceId"].as_str().unwrap().to_owned())
        })
        .collect()
}
