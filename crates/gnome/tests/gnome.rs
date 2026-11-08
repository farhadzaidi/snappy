//! The GNOME adapter against a real GNOME Shell with Snappy's extension and the live tests'
//! helper loaded. Runs only when `SNAPPY_GNOME_TEST=1`, which `tools/live/gnome/test.sh`
//! sets inside a throwaway GNOME Shell; elsewhere every test prints "skipped". Fixture windows
//! come from `snappy-fixture`, named by `SNAPPY_GNOME_FIXTURE`.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{KeyChord, Modifier, Mods, Platform as PlatformName};
use snappy_core::settings::Settings;
use snappy_gnome::{App, GnomePlatform, Stopper};
use snappy_linux::key_codes;
use snappy_platform::{
    Caption, DesktopGuard, HotKeys, Input, LiveControl, Overlay, OverlayScene, OverlayZone, Platform,
    PlatformEvent, Tray, TrayMenu, WindowSystem,
};
use zbus::blocking::Connection;

const WAIT: Duration = Duration::from_secs(5);
/// Where a title bar is grabbed, below the window's top edge.
const TITLE_BAR_GRIP: f64 = 14.0;
/// The modifier keys the tests press, in the order they press them.
const MODIFIERS: [Modifier; 4] = [Modifier::Ctrl, Modifier::Alt, Modifier::Shift, Modifier::Meta];

macro_rules! shell {
    () => {
        match Shell::start() {
            Some(shell) => shell,
            None => {
                println!("skipped: set SNAPPY_GNOME_TEST=1 inside the GNOME test session");
                return;
            }
        }
    };
}

/// The adapter running against the session's GNOME Shell, with the events it reports.
struct Shell {
    platform: GnomePlatform,
    events: Receiver<PlatformEvent>,
    stopper: Stopper,
    run: Option<JoinHandle<()>>,
    helper: Connection,
    fixtures: Vec<Fixture>,
}

impl Shell {
    fn start() -> Option<Shell> {
        if std::env::var("SNAPPY_GNOME_TEST").ok().as_deref() != Some("1") {
            return None;
        }
        let (sender, events) = mpsc::channel();
        let app = App::new(sender).unwrap();
        let platform = app.platform();
        let stopper = app.stopper();
        let run = std::thread::spawn(move || app.run().unwrap());
        let mut shell = Shell {
            platform,
            events,
            stopper,
            run: Some(run),
            helper: Connection::session().unwrap(),
            fixtures: Vec::new(),
        };
        assert_eq!(shell.platform.live_control(), LiveControl::Available);
        static WARMED_UP: std::sync::Once = std::sync::Once::new();
        WARMED_UP.call_once(|| shell.warm_up());
        shell.drain();
        Some(shell)
    }

    /// A freshly started GNOME Shell ignores title bar drags of a Wayland app until the
    /// session has handled some input, so a throwaway window is clicked and dragged first.
    fn warm_up(&mut self) {
        let frame = Rect::new(1000.0, 500.0, 500.0, 300.0);
        self.open("warm-up", frame);
        std::thread::sleep(Duration::from_millis(500));
        let grip = Point::new(frame.mid_x(), frame.y + TITLE_BAR_GRIP);
        self.pointer(grip);
        std::thread::sleep(Duration::from_millis(100));
        self.button(true);
        self.pointer_steps(grip, (20.0, 10.0), 6);
        self.button(false);
        std::thread::sleep(Duration::from_millis(300));
        self.fixtures.clear();
        std::thread::sleep(Duration::from_millis(300));
    }

    fn helper(&self, request: Value) -> Value {
        let reply = self
            .helper
            .call_method(
                Some("org.gnome.Shell"),
                "/io/github/farhadzaidi/SnappyTest",
                Some("io.github.farhadzaidi.SnappyTest"),
                "Call",
                &(request.to_string(),),
            )
            .unwrap();
        let (text,): (String,) = reply.body().deserialize().unwrap();
        let mut reply: serde_json::Map<String, Value> = serde_json::from_str(&text).unwrap();
        assert!(
            reply.get("error").is_none(),
            "the helper refused {request}: {reply:?}"
        );
        reply.remove("ok").unwrap_or(Value::Null)
    }

    fn drain(&self) -> Vec<PlatformEvent> {
        std::thread::sleep(Duration::from_millis(150));
        self.events.try_iter().collect()
    }

    /// Waits for an event `wanted` accepts, returning everything seen up to it.
    fn wait_for(&self, what: &str, wanted: impl Fn(&PlatformEvent) -> bool) -> Vec<PlatformEvent> {
        let deadline = Instant::now() + WAIT;
        let mut seen = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if let Ok(event) = self.events.recv_timeout(left) {
                let found = wanted(&event);
                seen.push(event);
                if found {
                    return seen;
                }
            }
        }
        panic!("never saw {what}; saw {seen:?}");
    }

    /// Presses or releases the key with this evdev keycode, as the virtual keyboard would.
    fn key(&self, keycode: u8, pressed: bool) {
        self.helper(key_request(keycode, pressed));
        std::thread::sleep(Duration::from_millis(15));
    }

    fn modifier(&self, modifier: Modifier, pressed: bool) {
        self.key(key_codes::modifier_keycode(modifier), pressed);
    }

    fn press_chord(&self, chord: &KeyChord) {
        let combo = chord.combo();
        let modifiers: Vec<u8> = MODIFIERS
            .into_iter()
            .filter(|modifier| combo.mods.contains(*modifier))
            .map(key_codes::modifier_keycode)
            .collect();
        for keycode in &modifiers {
            self.key(*keycode, true);
        }
        let key = key_codes::evdev_keycode(combo.code.as_str()).unwrap();
        self.key(key, true);
        self.key(key, false);
        for keycode in modifiers.iter().rev() {
            self.key(*keycode, false);
        }
    }

    fn pointer(&self, at: Point) {
        self.helper(json!({"op": "pointerMove", "x": at.x, "y": at.y}));
        std::thread::sleep(Duration::from_millis(30));
    }

    fn button(&self, pressed: bool) {
        self.helper(json!({"op": "button", "pressed": pressed}));
        std::thread::sleep(Duration::from_millis(30));
    }

    /// Moves the pointer `count` times by `by`, starting from `from`.
    fn pointer_steps(&self, from: Point, by: (f64, f64), count: u32) {
        for step in 1..=count {
            let step = f64::from(step);
            self.pointer(Point::new(from.x + by.0 * step, from.y + by.1 * step));
        }
    }

    /// Waits until GNOME Shell moves the window under the pressed button.
    fn wait_until_moving(&self) {
        let deadline = Instant::now() + WAIT;
        while self.helper(json!({"op": "grabbing"})) != true {
            assert!(
                Instant::now() < deadline,
                "GNOME Shell never started moving the window"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Opens a fixture window titled `Fixture <id>`, placed at `frame`'s origin.
    fn open(&mut self, id: &str, frame: Rect) -> u64 {
        let title = format!("Fixture {id}");
        let mut fixture = Fixture::launch();
        fixture.send(json!({
            "op": "open", "id": id, "minSize": null, "grid": null,
            "frame": {"x": frame.x, "y": frame.y, "width": frame.width, "height": frame.height},
        }));
        self.fixtures.push(fixture);
        let deadline = Instant::now() + WAIT;
        loop {
            let placed = self.helper(json!({"op": "windowFrame", "title": title}));
            if placed.get("x").and_then(Value::as_f64) == Some(frame.x)
                && placed.get("y").and_then(Value::as_f64) == Some(frame.y)
            {
                break;
            }
            if !placed.is_null() {
                self.helper(json!({"op": "placeWindow", "title": title, "x": frame.x, "y": frame.y}));
            }
            assert!(Instant::now() < deadline, "{title} never settled at its place");
            std::thread::sleep(Duration::from_millis(50));
        }
        std::thread::sleep(Duration::from_millis(200));
        let focused = self.helper(json!({"op": "focusWindow", "title": title}));
        // Clicked at a hand's pace too: GNOME 49 lets a window that was only activated ignore
        // drags on its title bar until it has been clicked.
        let center = Point::new(
            focused["x"].as_f64().unwrap() + focused["width"].as_f64().unwrap() / 2.0,
            focused["y"].as_f64().unwrap() + focused["height"].as_f64().unwrap() / 2.0,
        );
        self.pointer(center);
        std::thread::sleep(Duration::from_millis(100));
        self.button(true);
        std::thread::sleep(Duration::from_millis(50));
        self.button(false);
        let deadline = Instant::now() + WAIT;
        while self.helper(json!({"op": "focused", "title": title})) != true {
            assert!(Instant::now() < deadline, "{title} never got focus");
            std::thread::sleep(Duration::from_millis(10));
        }
        self.platform
            .visible_windows(None)
            .into_iter()
            .find(|info| info.frame.x == frame.x && info.frame.y == frame.y)
            .map(|info| info.id.0)
            .expect("the adapter lists the new window")
    }

    fn wait_frame(&mut self, window: u64, what: &str, done: impl Fn(Rect) -> bool) -> Rect {
        let deadline = Instant::now() + WAIT;
        loop {
            let frame = self.platform.frame(snappy_platform::WindowId(window)).unwrap();
            if done(frame) {
                return frame;
            }
            assert!(
                Instant::now() < deadline,
                "the window never reached {what}; it is at {frame:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Shell {
    /// Lets go of every key and button, so a failed test leaves no modifier held for the next.
    fn drop(&mut self) {
        let _ = self.helper.call_method(
            Some("org.gnome.Shell"),
            "/io/github/farhadzaidi/SnappyTest",
            Some("io.github.farhadzaidi.SnappyTest"),
            "Call",
            &(release_all_request().to_string(),),
        );
        self.fixtures.clear();
        self.platform.unregister_all();
        self.platform.set_press_monitoring(false);
        self.platform.set_drag_monitoring(false);
        self.platform.hide();
        self.platform.clear();
        self.stopper.stop();
        if let Some(run) = self.run.take() {
            let _ = run.join();
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// One `snappy-fixture` process, quit when dropped.
struct Fixture {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Fixture {
    fn launch() -> Fixture {
        let program =
            std::env::var("SNAPPY_GNOME_FIXTURE").expect("SNAPPY_GNOME_FIXTURE names snappy-fixture");
        let mut child = Command::new(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Fixture { child, input, output }
    }

    fn send(&mut self, request: Value) {
        writeln!(self.input, "{request}").unwrap();
        self.input.flush().unwrap();
        let mut reply = String::new();
        self.output.read_line(&mut reply).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = writeln!(self.input, "{}", json!({"op": "quit"}));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn floating(x: f64) -> Rect {
    Rect::new(x, 300.0, 600.0, 400.0)
}

#[test]
fn the_default_shortcuts_are_free() {
    let mut shell = shell!();
    let settings = Settings::defaults(PlatformName::Linux);
    let layout = settings.layout("six").unwrap();
    let mut chords: Vec<KeyChord> = settings.options(layout).zone_shortcuts.into_values().collect();
    chords.extend(settings.navigation.chords().map(|(_, chord)| chord.clone()));
    chords.extend(settings.navigation.nearest_empty.clone());
    chords.extend(settings.workspaces.steps().map(|(_, chord)| chord.clone()));
    let taken: Vec<String> = chords
        .iter()
        .filter(|chord| !shell.platform.is_available(&chord.combo()))
        .map(|chord| chord.long_label(PlatformName::Linux))
        .collect();
    assert!(taken.is_empty(), "GNOME holds {taken:?}");
}

#[test]
fn chords_gnome_binds_are_reported_taken_and_snappys_own_are_free() {
    let mut shell = shell!();
    let super_a = KeyChord::new("KeyA", [Modifier::Meta]).combo();
    let ctrl_super_one = KeyChord::new("Digit1", [Modifier::Ctrl, Modifier::Meta]).combo();
    assert!(
        !shell.platform.is_available(&super_a),
        "Super+A shows the app grid"
    );
    assert!(
        !shell.platform.is_available(&ctrl_super_one),
        "Super+Ctrl+1 opens a new window"
    );
    let own = KeyChord::snappy("Digit1", []).combo();
    assert!(shell.platform.register(&[(4, own.clone())]).is_empty());
    assert!(shell.platform.is_available(&own));
    assert_eq!(
        shell.platform.register(&[(5, super_a)]),
        vec![5],
        "a chord GNOME binds is refused"
    );
}

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

#[test]
fn windows_are_listed_moved_resized_and_found_under_the_pointer() {
    let mut shell = shell!();
    let id = shell.open("move", floating(200.0));
    let window = snappy_platform::WindowId(id);
    assert_eq!(shell.platform.focused_window().unwrap().id, window);
    assert_eq!(
        shell.platform.window_at(Point::new(300.0, 400.0)).unwrap().id,
        window
    );
    let info = shell
        .platform
        .all_windows()
        .into_iter()
        .find(|info| info.id == window)
        .unwrap();
    assert!(info.pid > 0);
    assert!(info.recovery_key.is_some());

    shell
        .platform
        .set_position(window, Point::new(100.0, 150.0))
        .unwrap();
    shell.wait_frame(id, "its new position", |frame| {
        frame.x == 100.0 && frame.y == 150.0
    });
    shell.platform.set_size(window, Size::new(700.0, 500.0)).unwrap();
    let frame = shell.wait_frame(id, "its new size", |frame| {
        frame.width == 700.0 && frame.height == 500.0
    });
    assert_eq!((frame.x, frame.y), (100.0, 150.0), "resizing keeps the corner");
    assert!(!shell.platform.is_closed(window));

    shell.fixtures.clear();
    let deadline = Instant::now() + WAIT;
    while !shell.platform.is_closed(window) {
        assert!(Instant::now() < deadline, "the window never closed");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(shell.platform.frame(window).is_err());
}

#[test]
fn a_maximized_window_is_restored_before_it_moves() {
    let mut shell = shell!();
    let id = shell.open("maximized", floating(300.0));
    shell.helper(json!({"op": "maximize", "title": "Fixture maximized"}));
    shell.wait_frame(id, "maximized", |frame| frame.width > 1800.0);
    let window = snappy_platform::WindowId(id);
    shell.platform.set_size(window, Size::new(640.0, 480.0)).unwrap();
    shell
        .platform
        .set_position(window, Point::new(50.0, 100.0))
        .unwrap();
    shell.wait_frame(id, "a restored frame", |frame| {
        frame == Rect::new(50.0, 100.0, 640.0, 480.0)
    });
}

#[test]
fn windows_appearing_and_closing_report_their_process() {
    let mut shell = shell!();
    shell.platform.all_windows();
    shell.drain();
    shell.open("lifecycle", floating(400.0));
    let events = shell.wait_for("the window appearing", |event| {
        matches!(event, PlatformEvent::WindowCreated { .. })
    });
    let pid = events
        .iter()
        .find_map(|event| match event {
            PlatformEvent::WindowCreated { pid } => Some(*pid),
            _ => None,
        })
        .unwrap();
    assert!(events.contains(&PlatformEvent::AppLaunched { pid }));
    shell.fixtures.clear();
    shell.wait_for("the app quitting", |event| {
        *event == PlatformEvent::AppTerminated { pid }
    });
}

#[test]
fn a_shortcut_press_arrives_as_its_hotkey_once() {
    let mut shell = shell!();
    let chord = KeyChord::snappy("Digit2", []);
    assert!(shell.platform.register(&[(7, chord.combo())]).is_empty());
    shell.drain();
    shell.press_chord(&chord);
    shell.wait_for("the shortcut", |event| *event == PlatformEvent::HotKey(7));
    let more: Vec<PlatformEvent> = shell
        .drain()
        .into_iter()
        .filter(|event| matches!(event, PlatformEvent::HotKey(_)))
        .collect();
    assert!(more.is_empty(), "one press is one shortcut: {more:?}");

    shell.platform.unregister_all();
    shell.press_chord(&chord);
    assert!(
        !shell.drain().contains(&PlatformEvent::HotKey(7)),
        "released shortcuts do nothing"
    );
}

#[test]
fn a_drag_reports_its_window_and_the_modifiers_held_at_the_release() {
    let mut shell = shell!();
    let id = shell.open("drag", floating(500.0));
    shell.platform.set_press_monitoring(true);
    shell.platform.set_drag_monitoring(true);
    shell.drain();

    let frame = floating(500.0);
    let grip = Point::new(frame.mid_x(), frame.y + TITLE_BAR_GRIP);
    shell.pointer(grip);
    std::thread::sleep(Duration::from_millis(100));
    shell.modifier(Modifier::Shift, true);
    shell.button(true);
    shell.pointer(Point::new(grip.x - 10.0, grip.y + 7.0));
    shell.wait_until_moving();
    shell.pointer_steps(grip, (-30.0, 20.0), 8);
    // Let go of Shift right behind the button, as fast as input allows.
    shell.helper(json!({"op": "button", "pressed": false}));
    shell.helper(key_request(key_codes::modifier_keycode(Modifier::Shift), false));

    let events = shell.wait_for("the release", |event| {
        matches!(event, PlatformEvent::PointerUp { .. })
    });
    let shift = Mods::from_iter([Modifier::Shift]);
    let down = events.iter().find_map(|event| match event {
        PlatformEvent::PointerDown { mods, window, .. } => Some((*mods, *window)),
        _ => None,
    });
    assert_eq!(
        down,
        Some((shift, Some(snappy_platform::WindowId(id)))),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, PlatformEvent::PointerDragged { mods, .. } if *mods == shift)),
        "{events:?}"
    );
    let Some(PlatformEvent::PointerUp { mods, .. }) = events.last() else {
        panic!("{events:?}");
    };
    assert_eq!(*mods, shift, "Shift was down when the button came up");
}

#[test]
fn without_press_monitoring_drags_are_not_reported() {
    let mut shell = shell!();
    shell.open("unwatched", floating(600.0));
    shell.platform.set_press_monitoring(false);
    shell.platform.set_drag_monitoring(false);
    shell.drain();
    shell.pointer(Point::new(900.0, 314.0));
    shell.button(true);
    shell.pointer(Point::new(950.0, 360.0));
    shell.button(false);
    let pointer: Vec<PlatformEvent> = shell
        .drain()
        .into_iter()
        .filter(|event| {
            matches!(
                event,
                PlatformEvent::PointerDown { .. } | PlatformEvent::PointerUp { .. }
            )
        })
        .collect();
    assert!(pointer.is_empty(), "{pointer:?}");
}

#[test]
fn parking_hides_a_window_from_the_screen_and_the_switcher_and_showing_returns_it() {
    let mut shell = shell!();
    let id = shell.open("parked", floating(250.0));
    let window = snappy_platform::WindowId(id);
    let displays = shell.platform.displays();
    shell.platform.bind(&displays);
    assert!(shell.platform.on_managed_desktop());

    shell
        .platform
        .hide_for_workspace(window, Point::default())
        .unwrap();
    assert!(shell.platform.state(window).unwrap().on_parking_desktop);
    assert!(
        shell
            .platform
            .visible_windows(None)
            .iter()
            .all(|info| info.id != window)
    );
    assert!(shell.platform.all_windows().iter().any(|info| info.id == window));
    let switcher = shell.helper(json!({"op": "tabList"}));
    assert!(
        !switcher
            .as_array()
            .unwrap()
            .iter()
            .any(|title| title == "Fixture parked"),
        "Alt+Tab offers a parked window: {switcher}"
    );
    assert!(
        shell.platform.on_managed_desktop(),
        "parking doesn't leave the workspace"
    );

    shell
        .platform
        .show_for_workspace(window, Point::new(120.0, 140.0))
        .unwrap();
    assert!(!shell.platform.state(window).unwrap().on_parking_desktop);
    shell.wait_frame(id, "the saved position", |frame| {
        frame.x == 120.0 && frame.y == 140.0
    });
    assert!(
        shell
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == window)
    );
    let switcher = shell.helper(json!({"op": "tabList"}));
    assert!(
        switcher
            .as_array()
            .unwrap()
            .iter()
            .any(|title| title == "Fixture parked")
    );
}

#[test]
fn with_dynamic_workspaces_the_parking_workspace_sits_before_gnomes_empty_last_one() {
    let mut shell = shell!();
    let before = shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap();
    let id = shell.open("dynamic", floating(260.0));
    let window = snappy_platform::WindowId(id);
    let displays = shell.platform.displays();
    shell.platform.bind(&displays);
    shell
        .platform
        .hide_for_workspace(window, Point::default())
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap(),
        before + 1,
        "GNOME keeps one empty workspace after the parking one"
    );
    shell
        .platform
        .show_for_workspace(window, floating(260.0).origin())
        .unwrap();
    shell.platform.clear();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap(),
        before
    );
}

#[test]
fn with_a_fixed_number_of_workspaces_parking_adds_one_and_clearing_removes_it() {
    let mut shell = shell!();
    shell.helper(json!({"op": "setDynamicWorkspaces", "on": false}));
    std::thread::sleep(Duration::from_millis(300));
    let before = shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap();
    let id = shell.open("fixed", floating(270.0));
    let window = snappy_platform::WindowId(id);
    let displays = shell.platform.displays();
    shell.platform.bind(&displays);
    shell
        .platform
        .hide_for_workspace(window, Point::default())
        .unwrap();
    assert_eq!(
        shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap(),
        before + 1
    );
    shell
        .platform
        .show_for_workspace(window, floating(270.0).origin())
        .unwrap();
    shell.platform.clear();
    std::thread::sleep(Duration::from_millis(300));
    let after = shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap();
    shell.helper(json!({"op": "setDynamicWorkspaces", "on": true}));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(after, before);
}

#[test]
fn a_window_on_every_workspace_is_refused_with_the_reason() {
    let mut shell = shell!();
    let id = shell.open("sticky", floating(280.0));
    let window = snappy_platform::WindowId(id);
    shell.helper(json!({"op": "stick", "title": "Fixture sticky", "on": true}));
    let displays = shell.platform.displays();
    shell.platform.bind(&displays);
    let error = shell
        .platform
        .hide_for_workspace(window, Point::default())
        .unwrap_err();
    shell.helper(json!({"op": "stick", "title": "Fixture sticky", "on": false}));
    assert!(error.to_string().contains("every workspace"), "{error}");
}

#[test]
fn switching_gnome_workspaces_pauses_and_coming_back_resumes() {
    let mut shell = shell!();
    let displays = shell.platform.displays();
    shell.platform.bind(&displays);
    shell.drain();
    shell.press_chord(&KeyChord::new("PageDown", [Modifier::Meta]));
    shell.wait_for("the workspace change", |event| {
        *event == PlatformEvent::NativeDesktopChanged
    });
    assert!(!shell.platform.on_managed_desktop());
    shell.press_chord(&KeyChord::new("PageUp", [Modifier::Meta]));
    shell.wait_for("the way back", |event| {
        *event == PlatformEvent::NativeDesktopChanged
    });
    assert!(shell.platform.on_managed_desktop());
}

fn scene(work_area: Rect) -> OverlayScene {
    OverlayScene {
        work_area,
        zones: vec![OverlayZone {
            frame: Rect::new(work_area.x + 12.0, work_area.y + 12.0, 600.0, 400.0),
            number: 1,
            highlighted: true,
            caption: Some(Caption::Swap),
            swap_source: false,
        }],
    }
}

#[test]
fn the_overlay_shows_above_windows_without_taking_focus_and_hides() {
    let mut shell = shell!();
    shell.open("under", floating(300.0));
    let display = shell.platform.displays().remove(0);
    shell.platform.show(&display.id, &scene(display.work_area));
    std::thread::sleep(Duration::from_millis(200));
    let shown = shell.helper(json!({"op": "screenWindows"}));
    let front = &shown.as_array().unwrap()[0];
    assert_eq!(front["overlay"], true, "{shown}");
    assert_eq!(front["frame"]["y"].as_f64(), Some(display.work_area.y));
    let focused = shown
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["focused"] == true)
        .map(|window| window["frame"]["x"].as_f64());
    assert_eq!(focused, Some(Some(300.0)), "the window keeps focus");

    shell.platform.hide();
    std::thread::sleep(Duration::from_millis(200));
    let shown = shell.helper(json!({"op": "screenWindows"}));
    assert!(
        shown
            .as_array()
            .unwrap()
            .iter()
            .all(|window| window["overlay"] == false)
    );
}

#[test]
fn the_menu_appears_in_the_top_bar_and_its_choices_come_back() {
    let mut shell = shell!();
    shell.platform.update(&TrayMenu {
        live: true,
        tooltip: "Snappy".into(),
        ..TrayMenu::default()
    });
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(shell.helper(json!({"op": "indicator"})), true);
    shell.drain();
    assert_eq!(
        shell.helper(json!({"op": "activateMenuItem", "title": "Open Snappy…"})),
        true
    );
    shell.wait_for("the menu choice", |event| {
        *event == PlatformEvent::OpenSettingsRequested
    });
}

#[test]
fn the_extension_lets_go_of_shortcuts_when_snappy_goes_away() {
    let mut shell = shell!();
    let combo = KeyChord::snappy("Digit3", []).combo();
    let accelerator = snappy_gnome::reference::accelerator(&combo).unwrap();
    {
        let other = Connection::session().unwrap();
        let request = json!({"op": "register", "keys": [{"id": 1, "accelerator": accelerator}]});
        other
            .call_method(
                Some("org.gnome.Shell"),
                "/io/github/farhadzaidi/Snappy/Shell",
                Some("io.github.farhadzaidi.Snappy.Shell"),
                "Call",
                &(request.to_string(),),
            )
            .unwrap();
        assert!(
            !shell.platform.is_available(&combo),
            "the other connection holds it"
        );
    }
    let deadline = Instant::now() + WAIT;
    while !shell.platform.is_available(&combo) {
        assert!(Instant::now() < deadline, "the shortcut stayed grabbed");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn turning_the_extension_off_and_on_reports_why_and_restores_what_it_held() {
    let mut shell = shell!();
    let chord = KeyChord::snappy("Digit4", []);
    assert!(shell.platform.register(&[(9, chord.combo())]).is_empty());
    shell.drain();

    let toggle = |command: &str| {
        let status = Command::new("gnome-extensions")
            .args([command, "snappy@farhadzaidi.github.io"])
            .status()
            .unwrap();
        assert!(status.success(), "gnome-extensions {command}");
    };
    toggle("disable");
    shell.wait_for("live control changing", |event| {
        *event == PlatformEvent::LiveControlChanged
    });
    let reason = shell.platform.live_control();
    assert!(
        matches!(&reason, LiveControl::Unavailable { reason: message, .. } if message.contains("turned off")),
        "{reason:?}"
    );

    toggle("enable");
    let deadline = Instant::now() + WAIT;
    while !shell.platform.live_control().is_available() {
        assert!(Instant::now() < deadline, "the extension never answered again");
        std::thread::sleep(Duration::from_millis(50));
    }
    shell.drain();
    shell.press_chord(&chord);
    shell.wait_for("the shortcut, grabbed again", |event| {
        *event == PlatformEvent::HotKey(9)
    });
}

/// Drags `id`'s title bar from `(500, 164)` by 390 × 330 with Shift held, past another window's
/// edges, and returns where it ends.
fn shift_drag_past_an_edge(shell: &mut Shell, id: u64) -> Rect {
    shell.helper(json!({"op": "placeWindow", "title": "Fixture follower", "x": 200, "y": 150}));
    std::thread::sleep(Duration::from_millis(300));
    shell.pointer(Point::new(500.0, 164.0));
    std::thread::sleep(Duration::from_millis(100));
    shell.modifier(Modifier::Shift, true);
    shell.button(true);
    shell.pointer(Point::new(510.0, 170.0));
    shell.wait_until_moving();
    shell.pointer_steps(Point::new(510.0, 170.0), (13.0, 11.0), 30);
    std::thread::sleep(Duration::from_millis(100));
    shell.button(false);
    shell.modifier(Modifier::Shift, false);
    std::thread::sleep(Duration::from_millis(300));
    shell.platform.frame(snappy_platform::WindowId(id)).unwrap()
}

#[test]
fn with_the_drag_key_held_a_moved_window_follows_the_pointer_not_gnomes_edges() {
    let mut shell = shell!();
    shell.open("edges", Rect::new(1000.0, 500.0, 600.0, 400.0));
    let id = shell.open("follower", Rect::new(200.0, 150.0, 600.0, 400.0));
    shell.platform.set_press_monitoring(true);

    // Where the pointer takes the window: it started moving with the pointer at (510, 170)
    // and the pointer ends at (900, 500).
    let followed = Point::new(200.0 + 390.0, 150.0 + 330.0);
    let near = |frame: Rect| (frame.x - followed.x).abs() <= 12.0 && (frame.y - followed.y).abs() <= 12.0;

    shell.platform.set_drag_modifier(Modifier::Shift);
    let frame = shift_drag_past_an_edge(&mut shell, id);
    assert!(
        near(frame),
        "the window followed the pointer to about {followed:?}, not {frame:?}"
    );

    // With another drag key, GNOME's own Shift-snapping is left alone and pulls the window onto
    // the other window's edges, so the drag above really did undo it.
    shell.platform.set_drag_modifier(Modifier::Ctrl);
    let frame = shift_drag_past_an_edge(&mut shell, id);
    assert!(!near(frame), "GNOME snapped the window to an edge: {frame:?}");
}

/// The helper's request to press or release a key, named by its evdev keycode.
fn key_request(keycode: u8, pressed: bool) -> Value {
    json!({"op": "key", "keycode": keycode - key_codes::EVDEV_OFFSET, "pressed": pressed})
}

/// The helper's request to let go of the button and every modifier the tests press.
fn release_all_request() -> Value {
    let keycodes: Vec<u8> = MODIFIERS
        .into_iter()
        .map(|modifier| key_codes::modifier_keycode(modifier) - key_codes::EVDEV_OFFSET)
        .collect();
    json!({"op": "releaseAll", "keycodes": keycodes})
}
