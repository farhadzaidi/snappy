//! What the tests share: the adapter running against the session's GNOME Shell, fixture
//! windows, and input through the test helper.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{KeyChord, Modifier, SnappyKey};
use snappy_gnome::protocol::SHELL_BUS_NAME;
use snappy_gnome::{App, GnomePlatform, Stopper};
use snappy_linux::key_codes;
use snappy_platform::{
    CapsLock, DesktopGuard, HotKeyRequest, HotKeys, Input, LiveControl, Overlay, Platform, PlatformEvent,
    WindowSystem,
};
use zbus::blocking::Connection;

/// The test-only helper extension the GNOME test sessions load (`tools/live/gnome/test-extension`).
pub const HELPER_PATH: &str = "/io/github/farhadzaidi/SnappyTest";
pub const HELPER_INTERFACE: &str = "io.github.farhadzaidi.SnappyTest";
pub const WAIT: Duration = Duration::from_secs(5);
/// Caps Lock's evdev keycode.
pub const CAPS_LOCK: u8 = 66;
/// Where a title bar is grabbed, below the window's top edge.
pub const TITLE_BAR_GRIP: f64 = 14.0;
/// The modifier keys the tests press, in the order they press them.
pub const MODIFIERS: [Modifier; 4] = [Modifier::Ctrl, Modifier::Alt, Modifier::Shift, Modifier::Meta];

macro_rules! shell {
    () => {
        match $crate::support::Shell::start() {
            Some(shell) => shell,
            None => {
                println!("skipped: set SNAPPY_GNOME_TEST=1 inside the GNOME test session");
                return;
            }
        }
    };
}
pub(crate) use shell;

/// The adapter running against the session's GNOME Shell, with the events it reports.
pub struct Shell {
    pub platform: GnomePlatform,
    pub events: Receiver<PlatformEvent>,
    pub stopper: Stopper,
    pub run: Option<JoinHandle<()>>,
    pub helper: Connection,
    pub fixtures: Vec<Fixture>,
}

impl Shell {
    pub fn start() -> Option<Shell> {
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
    /// On GNOME 49 the drag's move can outlast the button's release and holds off every
    /// shortcut until Escape ends it, so Escape and a throwaway shortcut are pressed until the
    /// shortcut fires.
    pub fn warm_up(&mut self) {
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
        const PROBE: u32 = 9_999;
        let probe = KeyChord::snappy("F12", []).combo(SnappyKey::DEFAULT);
        assert!(
            self.platform
                .register(&[HotKeyRequest::new(PROBE, probe)])
                .is_empty()
        );
        let f12 = key_codes::evdev_keycode("F12").unwrap();
        let escape = key_codes::evdev_keycode("Escape").unwrap();
        let deadline = Instant::now() + WAIT;
        loop {
            self.key(escape, true);
            self.key(escape, false);
            for modifier in [Modifier::Ctrl, Modifier::Alt, Modifier::Meta] {
                self.modifier(modifier, true);
            }
            self.key(f12, true);
            self.key(f12, false);
            for modifier in [Modifier::Meta, Modifier::Alt, Modifier::Ctrl] {
                self.modifier(modifier, false);
            }
            if self.drain().contains(&PlatformEvent::HotKey(PROBE)) {
                break;
            }
            assert!(Instant::now() < deadline, "GNOME Shell never ran a shortcut");
        }
        self.platform.unregister_all();
        self.fixtures.clear();
        std::thread::sleep(Duration::from_millis(300));
    }

    pub fn helper(&self, request: Value) -> Value {
        let reply = self
            .helper
            .call_method(
                Some(SHELL_BUS_NAME),
                HELPER_PATH,
                Some(HELPER_INTERFACE),
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

    pub fn drain(&self) -> Vec<PlatformEvent> {
        std::thread::sleep(Duration::from_millis(150));
        self.events.try_iter().collect()
    }

    /// Waits for an event `wanted` accepts, returning everything seen up to it.
    pub fn wait_for(&self, what: &str, wanted: impl Fn(&PlatformEvent) -> bool) -> Vec<PlatformEvent> {
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
    pub fn key(&self, keycode: u8, pressed: bool) {
        self.helper(key_request(keycode, pressed));
        std::thread::sleep(Duration::from_millis(15));
    }

    pub fn modifier(&self, modifier: Modifier, pressed: bool) {
        self.key(key_codes::modifier_keycode(modifier), pressed);
    }

    /// The Caps Lock state of GNOME Shell's keymap.
    pub fn caps_lock_on(&self) -> bool {
        self.helper(json!({"op": "capsLockOn"})) == true
    }

    pub fn wait_caps_lock(&self, on: bool) {
        let deadline = Instant::now() + WAIT;
        while self.caps_lock_on() != on {
            assert!(Instant::now() < deadline, "Caps Lock never turned {on}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn press_chord(&self, chord: &KeyChord) {
        let combo = chord.combo(snappy_core::keys::SnappyKey::DEFAULT);
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

    pub fn pointer(&self, at: Point) {
        self.helper(json!({"op": "pointerMove", "x": at.x, "y": at.y}));
        std::thread::sleep(Duration::from_millis(30));
    }

    pub fn button(&self, pressed: bool) {
        self.helper(json!({"op": "button", "pressed": pressed}));
        std::thread::sleep(Duration::from_millis(30));
    }

    /// Moves the pointer `count` times by `by`, starting from `from`.
    pub fn pointer_steps(&self, from: Point, by: (f64, f64), count: u32) {
        for step in 1..=count {
            let step = f64::from(step);
            self.pointer(Point::new(from.x + by.0 * step, from.y + by.1 * step));
        }
    }

    /// Waits until GNOME Shell moves the window under the pressed button.
    pub fn wait_until_moving(&self) {
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
    pub fn open(&mut self, id: &str, frame: Rect) -> u64 {
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

    pub fn wait_frame(&mut self, window: u64, what: &str, done: impl Fn(Rect) -> bool) -> Rect {
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
            Some(SHELL_BUS_NAME),
            HELPER_PATH,
            Some(HELPER_INTERFACE),
            "Call",
            &(release_all_request().to_string(),),
        );
        self.fixtures.clear();
        self.platform.restore_caps_lock();
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
pub struct Fixture {
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

pub fn floating(x: f64) -> Rect {
    Rect::new(x, 300.0, 600.0, 400.0)
}

/// The helper's request to press or release a key, named by its evdev keycode.
pub fn key_request(keycode: u8, pressed: bool) -> Value {
    json!({"op": "key", "keycode": keycode - key_codes::EVDEV_OFFSET, "pressed": pressed})
}

/// The helper's request to let go of the button, every modifier, and Caps Lock.
pub fn release_all_request() -> Value {
    let keycodes: Vec<u8> = MODIFIERS
        .into_iter()
        .map(key_codes::modifier_keycode)
        .chain([CAPS_LOCK])
        .map(|keycode| keycode - key_codes::EVDEV_OFFSET)
        .collect();
    json!({"op": "releaseAll", "keycodes": keycodes})
}
