//! The KWin adapter against a real, headless KWin. Runs only when `SNAPPY_KWIN_TEST=1`, which
//! `tools/live/kwin/test.sh` sets inside a sealed-off KWin session; elsewhere every test
//! prints "skipped". Fixture windows come from `snappy-fixture`, named by
//! `SNAPPY_KWIN_FIXTURE`. Input goes through KWin's `org_kde_kwin_fake_input`, which KWin
//! handles like a device's.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{KeyChord, Modifier, Mods, Platform as PlatformName};
use snappy_core::settings::Settings;
use snappy_kwin::{App, KWinPlatform, Stopper};
use snappy_linux::key_codes;
use snappy_linux::wayland::{Arg, Connection};
use snappy_platform::{
    Caption, DesktopGuard, HotKeys, Input, LiveControl, Overlay, OverlayScene, OverlayZone, Platform,
    PlatformEvent, WindowInfo, WindowSystem,
};
use std::os::fd::AsFd;
use zbus::zvariant::OwnedValue;

const WAIT: Duration = Duration::from_secs(5);
/// Where a title bar is grabbed, below the window's top edge.
const TITLE_BAR_GRIP: f64 = 14.0;

macro_rules! kwin {
    () => {
        match Session::start() {
            Some(session) => session,
            None => {
                println!("skipped: set SNAPPY_KWIN_TEST=1 inside the KWin test session");
                return;
            }
        }
    };
}

/// The session's one fake input device. KWin gives the seat a pointer and a keyboard when it
/// appears, and apps that were already running when that happens can trip over it, so it is
/// created before any fixture window and kept for the whole run.
fn input() -> MutexGuard<'static, FakeInput> {
    static INPUT: OnceLock<Mutex<FakeInput>> = OnceLock::new();
    INPUT
        .get_or_init(|| Mutex::new(FakeInput::open()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// KWin's fake input device.
struct FakeInput {
    connection: Connection,
    device: u32,
}

const AUTHENTICATE: u16 = 0;
const BUTTON: u16 = 2;
const POINTER_MOTION_ABSOLUTE: u16 = 9;
const KEYBOARD_KEY: u16 = 10;
const BUTTON_LEFT: u32 = 0x110;
const PRESSED: u32 = 1;
const RELEASED: u32 = 0;

impl FakeInput {
    fn open() -> FakeInput {
        let mut connection = Connection::open().unwrap();
        let (device, version) = connection.bind("org_kde_kwin_fake_input", 4).unwrap();
        assert!(version >= 4, "KWin's fake input can't press keys");
        connection
            .send(
                device,
                AUTHENTICATE,
                &[Arg::Str("Snappy tests"), Arg::Str("Drive the test session")],
            )
            .unwrap();
        let mut input = FakeInput { connection, device };
        input.sync();
        input
    }

    /// Waits until KWin has handled everything sent so far.
    fn sync(&mut self) {
        self.connection.roundtrip().unwrap();
    }

    fn move_to(&mut self, point: Point) {
        self.connection
            .send(
                self.device,
                POINTER_MOTION_ABSOLUTE,
                &[Arg::Fixed(point.x), Arg::Fixed(point.y)],
            )
            .unwrap();
        self.sync();
    }

    fn button(&mut self, down: bool) {
        let state = if down { PRESSED } else { RELEASED };
        self.connection
            .send(self.device, BUTTON, &[Arg::Uint(BUTTON_LEFT), Arg::Uint(state)])
            .unwrap();
        self.sync();
    }

    /// A key by its evdev keycode.
    fn key(&mut self, keycode: u8, down: bool) {
        let code = u32::from(keycode - key_codes::EVDEV_OFFSET);
        let state = if down { PRESSED } else { RELEASED };
        self.connection
            .send(self.device, KEYBOARD_KEY, &[Arg::Uint(code), Arg::Uint(state)])
            .unwrap();
        self.sync();
    }

    fn modifier(&mut self, modifier: Modifier, down: bool) {
        self.key(key_codes::modifier_keycode(modifier), down);
    }

    fn press_chord(&mut self, chord: &KeyChord) {
        let combo = chord.combo();
        let modifiers: Vec<Modifier> = combo.mods.iter().collect();
        for modifier in &modifiers {
            self.modifier(*modifier, true);
        }
        let key = key_codes::evdev_keycode(combo.code.as_str()).unwrap();
        self.key(key, true);
        self.key(key, false);
        for modifier in modifiers.iter().rev() {
            self.modifier(*modifier, false);
        }
    }
}

/// The fixture app, with one window open.
struct Fixture {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Fixture {
    fn open(id: &str, size: Size) -> Fixture {
        let program =
            std::env::var("SNAPPY_KWIN_FIXTURE").expect("SNAPPY_KWIN_FIXTURE names the fixture app");
        let mut child = Command::new(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut fixture = Fixture {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
        };
        let request = serde_json::json!({
            "op": "open", "id": id,
            "frame": {"x": 0.0, "y": 0.0, "width": size.width, "height": size.height},
            "minSize": null, "grid": null,
        });
        writeln!(fixture.input, "{request}").unwrap();
        fixture.input.flush().unwrap();
        let mut reply = String::new();
        fixture.output.read_line(&mut reply).unwrap();
        fixture
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = writeln!(self.input, r#"{{"op":"quit"}}"#);
        let _ = self.input.flush();
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A panel along a display's top or bottom edge, reserving its thickness the way Plasma's
/// panels do: a layer-shell surface with the dock namespace and an exclusive zone.
struct Panel {
    connection: Connection,
    surface: u32,
    layer: u32,
    _pixels: std::fs::File,
}

#[derive(Clone, Copy)]
enum Edge {
    Top,
    Bottom,
}

const ANCHOR_TOP: u32 = 1;
const ANCHOR_BOTTOM: u32 = 2;
const ANCHOR_LEFT_RIGHT: u32 = 4 | 8;

impl Panel {
    fn open(output: &str, edge: Edge, thickness: u32) -> Panel {
        let mut connection = Connection::open().unwrap();
        let (compositor, _) = connection.bind("wl_compositor", 4).unwrap();
        let (shm, _) = connection.bind("wl_shm", 1).unwrap();
        let (layer_shell, _) = connection.bind("zwlr_layer_shell_v1", 3).unwrap();
        let output = output_named(&mut connection, output);

        let surface = connection.new_id();
        connection.send(compositor, 0, &[Arg::Uint(surface)]).unwrap();
        let layer = connection.new_id();
        connection
            .send(
                layer_shell,
                0,
                &[
                    Arg::Uint(layer),
                    Arg::Uint(surface),
                    Arg::Uint(output),
                    Arg::Uint(2),
                    Arg::Str("dock"),
                ],
            )
            .unwrap();
        let anchor = match edge {
            Edge::Top => ANCHOR_TOP,
            Edge::Bottom => ANCHOR_BOTTOM,
        } | ANCHOR_LEFT_RIGHT;
        connection.send(layer, 1, &[Arg::Uint(anchor)]).unwrap();
        connection
            .send(layer, 0, &[Arg::Uint(0), Arg::Uint(thickness)])
            .unwrap();
        connection.send(layer, 2, &[Arg::Int(thickness as i32)]).unwrap();
        connection.send(surface, 6, &[]).unwrap();
        let configure = connection
            .read_until(WAIT, |message| message.object == layer && message.opcode == 0)
            .expect("KWin configures the panel")
            .pop()
            .unwrap();
        let mut reader = configure.reader();
        let (serial, width, height) = (
            reader.uint().unwrap(),
            reader.uint().unwrap(),
            reader.uint().unwrap(),
        );
        connection.send(layer, 6, &[Arg::Uint(serial)]).unwrap();

        let stride = width * 4;
        let pixels = scratch_file();
        pixels.set_len(u64::from(stride * height)).unwrap();
        let pool = connection.new_id();
        connection
            .send_with_fd(
                shm,
                0,
                &[Arg::Uint(pool), Arg::Int((stride * height) as i32)],
                pixels.as_fd(),
            )
            .unwrap();
        let buffer = connection.new_id();
        connection
            .send(
                pool,
                0,
                &[
                    Arg::Uint(buffer),
                    Arg::Int(0),
                    Arg::Int(width as i32),
                    Arg::Int(height as i32),
                    Arg::Int(stride as i32),
                    Arg::Uint(0),
                ],
            )
            .unwrap();
        connection
            .send(surface, 1, &[Arg::Uint(buffer), Arg::Int(0), Arg::Int(0)])
            .unwrap();
        connection.send(surface, 6, &[]).unwrap();
        connection.roundtrip().unwrap();
        Panel {
            connection,
            surface,
            layer,
            _pixels: pixels,
        }
    }

    /// Changes only the space the panel reserves, as switching it to auto-hide does, which
    /// KWin tells scripts nothing about.
    fn reserve(&mut self, thickness: i32) {
        self.connection
            .send(self.layer, 2, &[Arg::Int(thickness)])
            .unwrap();
        self.connection.send(self.surface, 6, &[]).unwrap();
        self.connection.roundtrip().unwrap();
    }
}

fn output_named(connection: &mut Connection, name: &str) -> u32 {
    let globals: Vec<_> = connection
        .globals()
        .iter()
        .filter(|global| global.interface == "wl_output")
        .cloned()
        .collect();
    let ids: Vec<u32> = globals
        .iter()
        .map(|global| connection.bind_global(global, 4).unwrap().0)
        .collect();
    connection
        .roundtrip()
        .unwrap()
        .into_iter()
        .find(|message| {
            ids.contains(&message.object)
                && message.opcode == 4
                && message.reader().string().as_deref() == Some(name)
        })
        .map(|message| message.object)
        .unwrap_or_else(|| panic!("no output named {name}"))
}

/// A file only this process can reach, for pixels KWin reads by descriptor.
fn scratch_file() -> std::fs::File {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let folder = std::env::var_os("XDG_RUNTIME_DIR").unwrap();
    let path = std::path::PathBuf::from(folder).join(format!("snappy-test-{}-{next}", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    file
}

const PROBE_NAME: &str = "io.github.farhadzaidi.SnappyTest";
const PROBE_PATH: &str = "/io/github/farhadzaidi/SnappyTest";

/// The on-screen display surfaces KWin shows, Snappy's overlay among them, by process: asked
/// of KWin by a throwaway script in the test session, never through Snappy.
/// An on-screen display window as KWin knows it: whose, where, and its KWin UUID, which
/// changes only when the window closes and another opens.
#[derive(Debug, Clone, PartialEq)]
struct OsdWindow {
    pid: u32,
    frame: Rect,
    uuid: String,
}

fn on_screen_displays() -> Vec<OsdWindow> {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::var_os("SNAPPY_KWIN_OUT").expect("SNAPPY_KWIN_OUT is set in the test session");
    let path = std::path::PathBuf::from(out).join(format!("probe-{}-{next}.js", std::process::id()));
    std::fs::write(
        &path,
        format!(
            r#"const found = workspace.windowList().filter(w => w.onScreenDisplay).map(w => {{
                const g = w.frameGeometry;
                return [w.pid, g.x, g.y, g.width, g.height, String(w.internalId)];
            }});
            callDBus("{PROBE_NAME}", "{PROBE_PATH}", "{PROBE_NAME}", "Report", JSON.stringify(found));"#
        ),
    )
    .unwrap();
    let bus = zbus::blocking::connection::Builder::session()
        .unwrap()
        .name(PROBE_NAME)
        .unwrap()
        .build()
        .unwrap();
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::MethodCall)
        .path(PROBE_PATH)
        .unwrap()
        .build();
    let calls = zbus::blocking::MessageIterator::for_match_rule(rule, &bus, None).unwrap();
    let script = format!("snappy-probe-{next}");
    let reply = bus
        .call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "loadScript",
            &(path.to_string_lossy().into_owned(), script.as_str()),
        )
        .unwrap();
    let id: i32 = reply.body().deserialize().unwrap();
    bus.call_method(
        Some("org.kde.KWin"),
        format!("/Scripting/Script{id}").as_str(),
        Some("org.kde.kwin.Script"),
        "run",
        &(),
    )
    .unwrap();
    let message = calls.into_iter().next().unwrap().unwrap();
    let json: String = message.body().deserialize().unwrap();
    bus.reply(&message.header(), &()).unwrap();
    let _ = bus.call_method(
        Some("org.kde.KWin"),
        "/Scripting",
        Some("org.kde.kwin.Scripting"),
        "unloadScript",
        &(script.as_str(),),
    );
    std::fs::remove_file(&path).unwrap();
    let found: Vec<(u32, f64, f64, f64, f64, String)> = serde_json::from_str(&json).unwrap();
    found
        .into_iter()
        .map(|(pid, x, y, width, height, uuid)| OsdWindow {
            pid,
            frame: Rect::new(x, y, width, height),
            uuid,
        })
        .collect()
}

/// Waits until `pid` has an on-screen display window at `frame`, and returns it.
fn wait_for_overlay(pid: u32, frame: Rect) -> OsdWindow {
    let deadline = Instant::now() + WAIT;
    loop {
        let found: Vec<OsdWindow> = on_screen_displays()
            .into_iter()
            .filter(|window| window.pid == pid)
            .collect();
        if let Some(window) = found.iter().find(|window| window.frame == frame) {
            return window.clone();
        }
        assert!(
            Instant::now() < deadline,
            "the overlay of {pid} is at {found:?}, not {frame:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `pid`'s on-screen display windows, in the order KWin lists them.
fn overlay_windows(pid: u32) -> Vec<OsdWindow> {
    on_screen_displays()
        .into_iter()
        .filter(|window| window.pid == pid)
        .collect()
}

/// The adapter running against the session's KWin, with the events it reports.
struct Session {
    platform: KWinPlatform,
    events: Receiver<PlatformEvent>,
    stopper: Stopper,
    run: Option<JoinHandle<()>>,
}

impl Session {
    fn start() -> Option<Session> {
        if std::env::var("SNAPPY_KWIN_TEST").ok().as_deref() != Some("1") {
            return None;
        }
        drop(input());
        let (sender, events) = mpsc::channel();
        let app = App::new(sender).unwrap();
        let platform = app.platform();
        let stopper = app.stopper();
        let run = std::thread::spawn(move || app.run().unwrap());
        let mut session = Session {
            platform,
            events,
            stopper,
            run: Some(run),
        };
        session.wait_until("the script answers", |session| {
            session.platform.live_control() == LiveControl::Available
        });
        Some(session)
    }

    fn wait_until(&mut self, what: &str, mut done: impl FnMut(&mut Session) -> bool) {
        let deadline = Instant::now() + WAIT;
        while !done(self) {
            assert!(Instant::now() < deadline, "timed out waiting until {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The next event, skipping ones the test doesn't look at.
    #[track_caller]
    fn next(&self, wanted: impl Fn(&PlatformEvent) -> bool) -> PlatformEvent {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let event = self.events.recv_timeout(left).expect("an event");
            if wanted(&event) {
                return event;
            }
            println!("passed over {event:?}");
        }
    }

    fn drain(&self) {
        while self.events.try_recv().is_ok() {}
    }

    /// The fixture's window, once KWin manages it.
    fn window_of(&mut self, fixture: &Fixture) -> WindowInfo {
        let pid = fixture.pid();
        let mut found = None;
        self.wait_until("the fixture's window appears", |session| {
            found = session
                .platform
                .all_windows()
                .into_iter()
                .find(|window| window.pid == pid);
            found.is_some()
        });
        found.unwrap()
    }

    fn place(&mut self, window: &WindowInfo, origin: Point) -> Rect {
        self.platform.set_position(window.id, origin).unwrap();
        let mut frame = Rect::default();
        self.wait_until("the window moves", |session| {
            frame = session.platform.frame(window.id).unwrap();
            (frame.x - origin.x).abs() < 1.0 && (frame.y - origin.y).abs() < 1.0
        });
        frame
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stopper.stop();
        if let Some(run) = self.run.take() {
            let _ = run.join();
        }
    }
}

#[test]
fn windows_are_listed_with_their_facts_and_move_where_asked() {
    let mut kwin = kwin!();
    let fixture = Fixture::open("facts", Size::new(600.0, 400.0));
    let window = kwin.window_of(&fixture);
    assert!(
        window.recovery_key.is_some(),
        "KWin's UUID and the process identify it"
    );

    let frame = kwin.place(&window, Point::new(200.0, 150.0));
    assert!(frame.width >= 600.0 && frame.height >= 400.0, "{frame:?}");
    kwin.platform
        .set_size(window.id, Size::new(700.0, 500.0))
        .unwrap();
    kwin.wait_until("the window resizes", |session| {
        let frame = session.platform.frame(window.id).unwrap();
        (frame.width - 700.0).abs() < 1.0 && (frame.height - 500.0).abs() < 1.0
    });

    let displays = kwin.platform.displays();
    assert_eq!(displays.len(), 2);
    assert_eq!(displays[0].frame, Rect::new(0.0, 0.0, 1920.0, 1080.0));
    assert_eq!(displays[1].frame.x, 1920.0);
    assert_eq!(
        kwin.platform.window_at(Point::new(300.0, 300.0)).unwrap().id,
        window.id
    );
    assert!(
        kwin.platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == window.id)
    );
}

#[test]
fn a_drag_reports_the_modifiers_held_when_the_button_came_up() {
    let mut kwin = kwin!();
    let fixture = Fixture::open("drag", Size::new(500.0, 300.0));
    let window = kwin.window_of(&fixture);
    let frame = kwin.place(&window, Point::new(300.0, 200.0));
    kwin.platform.set_press_monitoring(true);
    kwin.drain();

    let mut input = input();
    let grip = Point::new(frame.x + frame.width / 2.0, frame.y + TITLE_BAR_GRIP);
    input.move_to(grip);
    input.modifier(Modifier::Shift, true);
    input.button(true);
    for step in 1..=5 {
        input.move_to(Point::new(
            grip.x + 20.0 * f64::from(step),
            grip.y + 10.0 * f64::from(step),
        ));
    }
    let PlatformEvent::PointerDown { window: pressed, .. } =
        kwin.next(|event| matches!(event, PlatformEvent::PointerDown { .. }))
    else {
        unreachable!()
    };
    assert_eq!(pressed, Some(window.id));
    // The engine turns drag monitoring on when a press starts tracking a window.
    kwin.platform.set_drag_monitoring(true);
    let PlatformEvent::ModifiersChanged { mods, .. } =
        kwin.next(|event| matches!(event, PlatformEvent::ModifiersChanged { .. }))
    else {
        unreachable!()
    };
    assert_eq!(mods, Mods::from_iter([Modifier::Shift]));
    input.move_to(Point::new(grip.x + 200.0, grip.y + 120.0));
    let PlatformEvent::PointerDragged { at, mods } =
        kwin.next(|event| matches!(event, PlatformEvent::PointerDragged { .. }))
    else {
        unreachable!()
    };
    assert!(mods.contains(Modifier::Shift));
    assert!(at.x > grip.x, "{at:?}");

    // Let go of Shift right behind the button, as quickly as input allows.
    input.button(false);
    input.modifier(Modifier::Shift, false);
    let PlatformEvent::PointerUp { mods, .. } =
        kwin.next(|event| matches!(event, PlatformEvent::PointerUp { .. }))
    else {
        unreachable!()
    };
    assert_eq!(
        mods,
        Mods::from_iter([Modifier::Shift]),
        "Shift was down at the release"
    );
    kwin.platform.set_drag_monitoring(false);
}

#[test]
fn a_registered_shortcut_reports_its_id_and_kwins_own_chords_are_taken() {
    let mut kwin = kwin!();
    let chord = KeyChord::snappy("Digit1", []);
    let refused = kwin.platform.register(&[(41, chord.combo())]);
    assert!(refused.is_empty(), "{refused:?}");
    assert!(
        kwin.platform.is_available(&chord.combo()),
        "Snappy's own shortcut counts as free"
    );
    kwin.drain();

    let mut input = input();
    input.press_chord(&chord);
    assert_eq!(
        kwin.next(|event| matches!(event, PlatformEvent::HotKey(_))),
        PlatformEvent::HotKey(41)
    );

    // KWin closes windows with Alt+F4.
    let close = KeyChord::new("F4", [Modifier::Alt]);
    assert!(!kwin.platform.is_available(&close.combo()));
    assert_eq!(kwin.platform.register(&[(42, close.combo())]), vec![42]);
    kwin.platform.unregister_all();
}

#[test]
fn snappys_own_shortcuts_never_count_as_taken() {
    let mut kwin = kwin!();
    let one = KeyChord::snappy("Digit1", []).combo();
    let two = KeyChord::snappy("Digit2", []).combo();

    // A run that crashed leaves its actions with KDE's shortcut service.
    let bus = zbus::blocking::Connection::session().unwrap();
    let left_behind = vec!["snappy", "shortcut-7", "Snappy", "Snappy shortcut 7"];
    let key = snappy_kwin::reference::key_code(&one).unwrap();
    bus.call_method(
        Some("org.kde.kglobalaccel"),
        "/kglobalaccel",
        Some("org.kde.KGlobalAccel"),
        "doRegister",
        &(left_behind.clone(),),
    )
    .unwrap();
    bus.call_method(
        Some("org.kde.kglobalaccel"),
        "/kglobalaccel",
        Some("org.kde.KGlobalAccel"),
        "setShortcutKeys",
        &(left_behind, vec![(vec![key],)], 6u32),
    )
    .unwrap();
    assert!(
        kwin.platform.is_available(&one),
        "a key Snappy itself holds is free"
    );

    assert_eq!(
        kwin.platform.register(&[(1, one.clone()), (2, two.clone())]),
        Vec::<u32>::new()
    );
    // The engine registers everything again whenever shortcuts or displays change.
    assert_eq!(
        kwin.platform.register(&[(1, one.clone()), (2, two.clone())]),
        Vec::<u32>::new()
    );
    // Shortcuts trade keys when the user swaps them.
    assert_eq!(
        kwin.platform.register(&[(1, two.clone()), (2, one.clone())]),
        Vec::<u32>::new()
    );
    assert!(kwin.platform.is_available(&one) && kwin.platform.is_available(&two));
    kwin.drain();

    let mut input = input();
    input.press_chord(&KeyChord::snappy("Digit1", []));
    assert_eq!(
        kwin.next(|event| matches!(event, PlatformEvent::HotKey(_))),
        PlatformEvent::HotKey(2)
    );
    drop(input);
    kwin.platform.unregister_all();
}

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

#[test]
fn work_areas_follow_panels_as_they_come_change_and_go() {
    let mut kwin = kwin!();
    let output = kwin.platform.displays()[0].id.clone();
    let area = |kwin: &mut Session| kwin.platform.displays()[0].work_area;
    assert_eq!(area(&mut kwin), Rect::new(0.0, 0.0, 1920.0, 1080.0));
    kwin.drain();

    let mut top = Panel::open(&output, Edge::Top, 40);
    kwin.next(|event| matches!(event, PlatformEvent::DisplaysChanged));
    assert_eq!(area(&mut kwin), Rect::new(0.0, 40.0, 1920.0, 1040.0));

    let bottom = Panel::open(&output, Edge::Bottom, 51);
    kwin.next(|event| matches!(event, PlatformEvent::DisplaysChanged));
    assert_eq!(area(&mut kwin), Rect::new(0.0, 40.0, 1920.0, 989.0));

    // Reserving less space moves no panel, so KWin says nothing; the next drag finds out
    // before its press is reported.
    top.reserve(0);
    let fixture = Fixture::open("panels", Size::new(500.0, 300.0));
    let window = kwin.window_of(&fixture);
    let frame = kwin.place(&window, Point::new(300.0, 300.0));
    kwin.platform.set_press_monitoring(true);
    kwin.drain();
    let mut input = input();
    let grip = Point::new(frame.x + frame.width / 2.0, frame.y + TITLE_BAR_GRIP);
    input.move_to(grip);
    input.button(true);
    for step in 1..=5 {
        input.move_to(Point::new(grip.x + 20.0 * f64::from(step), grip.y));
    }
    let first = kwin.next(|event| {
        matches!(
            event,
            PlatformEvent::DisplaysChanged | PlatformEvent::PointerDown { .. }
        )
    });
    input.button(false);
    assert_eq!(
        first,
        PlatformEvent::DisplaysChanged,
        "the change is heard before the press"
    );
    assert_eq!(area(&mut kwin), Rect::new(0.0, 0.0, 1920.0, 1029.0));
    drop(input);

    drop(bottom);
    drop(top);
    kwin.next(|event| matches!(event, PlatformEvent::DisplaysChanged));
    kwin.wait_until("the panels' space is free again", |session| {
        session.platform.displays()[0].work_area == Rect::new(0.0, 0.0, 1920.0, 1080.0)
    });
}

#[test]
fn parking_hides_a_window_on_snappys_desktop_and_brings_it_back() {
    let mut kwin = kwin!();
    let fixture = Fixture::open("park", Size::new(400.0, 300.0));
    let window = kwin.window_of(&fixture);
    let frame = kwin.place(&window, Point::new(100.0, 100.0));
    let displays = kwin.platform.displays();
    kwin.platform.bind(&displays);
    assert!(kwin.platform.on_managed_desktop());

    kwin.platform
        .hide_for_workspace(window.id, Point::default())
        .unwrap();
    assert!(kwin.platform.state(window.id).unwrap().on_parking_desktop);
    assert!(
        !kwin
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == window.id)
    );
    assert!(
        kwin.platform.on_managed_desktop(),
        "parking doesn't switch desktops"
    );

    kwin.platform
        .show_for_workspace(window.id, frame.origin())
        .unwrap();
    assert!(!kwin.platform.state(window.id).unwrap().on_parking_desktop);
    assert!(
        kwin.platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == window.id)
    );
    kwin.platform.clear();
}

/// KWin's own view of a window, found by its title, so the end-to-end test checks frames
/// without going through Snappy.
fn kwin_frame(bus: &zbus::blocking::Connection, title: &str) -> Option<Rect> {
    let info = kwin_info(bus, title)?;
    let number = |key: &str| -> Option<f64> {
        let value = info.get(key)?;
        f64::try_from(value)
            .ok()
            .or_else(|| i32::try_from(value).ok().map(f64::from))
    };
    Some(Rect::new(
        number("x")?,
        number("y")?,
        number("width")?,
        number("height")?,
    ))
}

/// Whether KWin keeps the window off the task manager, as Snappy's parking does.
fn kwin_skips_taskbar(bus: &zbus::blocking::Connection, title: &str) -> Option<bool> {
    bool::try_from(kwin_info(bus, title)?.get("skipTaskbar")?).ok()
}

/// KWin's facts about the window with this title, from its D-Bus interface.
fn kwin_info(bus: &zbus::blocking::Connection, title: &str) -> Option<HashMap<String, OwnedValue>> {
    type Matches = Vec<(String, String, String, i32, f64, HashMap<String, OwnedValue>)>;
    let reply = bus
        .call_method(
            Some("org.kde.KWin"),
            "/WindowsRunner",
            Some("org.kde.krunner1"),
            "Match",
            &(title,),
        )
        .ok()?;
    let matches: Matches = reply.body().deserialize().ok()?;
    let (id, ..) = matches.into_iter().find(|(_, text, ..)| text == title)?;
    let uuid = id
        .split_once('_')
        .map_or(id.as_str(), |(_, uuid)| uuid)
        .to_owned();
    let reply = bus
        .call_method(
            Some("org.kde.KWin"),
            "/KWin",
            Some("org.kde.KWin"),
            "getWindowInfo",
            &(uuid,),
        )
        .ok()?;
    reply.body().deserialize().ok()
}

/// The real daemon, with scratch settings: snapping on and the halves layout everywhere.
struct Daemon {
    child: Child,
}

impl Daemon {
    /// Starts the daemon on settings that are the defaults with snapping on, the halves
    /// layout everywhere, and whatever `change` makes.
    fn start(home: &std::path::Path, change: impl FnOnce(&mut Settings)) -> Option<Daemon> {
        let program = std::env::var("SNAPPY_KWIN_DAEMON").ok()?;
        let mut settings = Settings::defaults(PlatformName::Linux);
        settings.snapping.enabled = true;
        settings.layouts.default_layout_id = "halves".to_owned();
        change(&mut settings);
        let settings = settings.validate(PlatformName::Linux).unwrap();
        std::fs::create_dir_all(home).unwrap();
        std::fs::write(home.join("settings.json"), settings.to_json().unwrap()).unwrap();
        let log = std::fs::File::create(home.join("daemon.log")).unwrap();
        let child = Command::new(program)
            .arg("--background")
            .env("SNAPPY_HOME", home)
            .env("SNAPPY_DEBUG", "1")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        Some(Daemon { child })
    }

    /// Asks the daemon to quit as `killall` would, and waits for it.
    fn quit(&mut self) {
        let pid = self.child.id().to_string();
        Command::new("kill").args(["-TERM", &pid]).status().unwrap();
        let deadline = Instant::now() + WAIT;
        while matches!(self.child.try_wait(), Ok(None)) {
            assert!(Instant::now() < deadline, "the daemon didn't quit");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn wait_for_frame(
    bus: &zbus::blocking::Connection,
    title: &str,
    what: &str,
    done: impl Fn(Rect) -> bool,
) -> Rect {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(frame) = kwin_frame(bus, title).filter(|frame| done(*frame)) {
            return frame;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting until {what}; the window is at {:?}",
            kwin_frame(bus, title)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn the_daemon_snaps_inside_the_panels_with_its_overlay_showing_and_by_shortcut() {
    if std::env::var("SNAPPY_KWIN_TEST").ok().as_deref() != Some("1") {
        println!("skipped: set SNAPPY_KWIN_TEST=1 inside the KWin test session");
        return;
    }
    let Some(home) =
        std::env::var_os("SNAPPY_KWIN_OUT").map(|out| std::path::PathBuf::from(out).join("daemon"))
    else {
        println!("skipped: SNAPPY_KWIN_OUT names where the daemon's files go");
        return;
    };
    drop(input());
    let bus = zbus::blocking::Connection::session().unwrap();
    let Some(mut daemon) = Daemon::start(&home, |_| {}) else {
        println!("skipped: set SNAPPY_KWIN_DAEMON to the daemon binary");
        return;
    };
    let _fixture = Fixture::open("e2e", Size::new(600.0, 400.0));
    let title = "Fixture e2e";
    let frame = wait_for_frame(&bus, title, "the window appears", |frame| frame.width > 1.0);
    // Give the daemon time to load its script and take its shortcuts, then add panels as a
    // user would after Snappy started: 40 points at the top and 51 at the bottom of the
    // first display, which leave 0, 40, 1920 × 989 for windows.
    std::thread::sleep(Duration::from_millis(1500));
    // The daemon prepares its overlay at start, before any drag, at the whole display.
    let prepared = wait_for_overlay(daemon.child.id(), Rect::new(0.0, 0.0, 1920.0, 1080.0));
    let _top = Panel::open("Virtual-0", Edge::Top, 40);
    let _bottom = Panel::open("Virtual-0", Edge::Bottom, 51);
    let work_area = Rect::new(0.0, 40.0, 1920.0, 989.0);
    // The daemon reads displays again once a change has settled.
    std::thread::sleep(Duration::from_millis(600));

    let mut input = input();
    let grip = Point::new(frame.x + frame.width / 2.0, frame.y + TITLE_BAR_GRIP);
    input.move_to(grip);
    input.button(true);
    for step in 1..=4 {
        input.move_to(Point::new(
            grip.x - 25.0 * f64::from(step),
            grip.y + 10.0 * f64::from(step),
        ));
    }
    input.modifier(Modifier::Shift, true);
    let target = Point::new(400.0, 540.0);
    for step in 1..=10 {
        let t = f64::from(step) / 10.0;
        input.move_to(Point::new(
            grip.x + (target.x - grip.x) * t,
            grip.y + (target.y - grip.y) * t,
        ));
        std::thread::sleep(Duration::from_millis(16));
    }
    std::thread::sleep(Duration::from_millis(100));
    // The overlay covers the new work area while the drag key is held: the same window,
    // moved and resized, never closed and opened again for KWin's effects to animate.
    let during = wait_for_overlay(daemon.child.id(), work_area);
    assert_eq!(during.uuid, prepared.uuid);
    input.button(false);
    input.modifier(Modifier::Shift, false);

    let inside =
        |frame: Rect| frame.y >= work_area.y && frame.y + frame.height <= work_area.y + work_area.height;
    let left = wait_for_frame(
        &bus,
        title,
        "the window fills the left half between the panels",
        |frame| {
            frame.x < 40.0
                && frame.width > 900.0
                && frame.width < 961.0
                && frame.height > 900.0
                && inside(frame)
        },
    );

    // The halves layout's second zone, by its default shortcut, moves the focused window.
    input.press_chord(&KeyChord::snappy("Digit2", []));
    let right = wait_for_frame(
        &bus,
        title,
        "the window fills the right half between the panels",
        |frame| frame.x >= 960.0 && inside(frame),
    );
    assert_eq!(right.width, left.width);
    drop(input);
    // Each display keeps its own surface; hiding leaves this one where it was.
    assert!(
        overlay_windows(daemon.child.id()).contains(&during),
        "hiding keeps the window"
    );

    daemon.quit();
    let output = std::fs::read_to_string(home.join("daemon.log")).unwrap();
    assert!(
        !output.contains("zone overlay"),
        "the overlay had trouble during the drag; the daemon said:\n{output}"
    );
    let mut scripting = None;
    for _ in 0..20 {
        let reply = bus.call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "isScriptLoaded",
            &("snappy",),
        );
        scripting = reply
            .ok()
            .and_then(|reply| reply.body().deserialize::<bool>().ok());
        if scripting == Some(false) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(scripting, Some(false), "quitting unloads the script");
}

#[test]
fn the_daemons_workspaces_hide_and_bring_back_windows_with_the_default_shortcuts() {
    if std::env::var("SNAPPY_KWIN_TEST").ok().as_deref() != Some("1") {
        println!("skipped: set SNAPPY_KWIN_TEST=1 inside the KWin test session");
        return;
    }
    let Some(home) =
        std::env::var_os("SNAPPY_KWIN_OUT").map(|out| std::path::PathBuf::from(out).join("workspaces"))
    else {
        println!("skipped: SNAPPY_KWIN_OUT names where the daemon's files go");
        return;
    };
    drop(input());
    let bus = zbus::blocking::Connection::session().unwrap();
    // Only "Enable workspaces": every shortcut is the default.
    let Some(mut daemon) = Daemon::start(&home, |settings| settings.workspaces.enabled = true) else {
        println!("skipped: set SNAPPY_KWIN_DAEMON to the daemon binary");
        return;
    };
    let _fixture = Fixture::open("workspaces", Size::new(500.0, 300.0));
    let title = "Fixture workspaces";
    let frame = wait_for_frame(&bus, title, "the window appears", |frame| frame.width > 1.0);
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(kwin_skips_taskbar(&bus, title), Some(false));

    let mut input = input();
    // Snappy + ' goes to the next workspace, which has no windows yet.
    input.press_chord(&KeyChord::snappy("Quote", []));
    let deadline = Instant::now() + WAIT;
    while kwin_skips_taskbar(&bus, title) != Some(true) {
        assert!(
            Instant::now() < deadline,
            "the window wasn't hidden on Snappy's desktop"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // Snappy + ; comes back, with the window where it was.
    input.press_chord(&KeyChord::snappy("Semicolon", []));
    let back = wait_for_frame(&bus, title, "the window comes back", |_| {
        kwin_skips_taskbar(&bus, title) == Some(false)
    });
    assert_eq!(back, frame);
    drop(input);

    daemon.quit();
    let output = std::fs::read_to_string(home.join("daemon.log")).unwrap();
    assert!(
        !output.contains("snappy:"),
        "the daemon reported trouble:\n{output}"
    );
}
