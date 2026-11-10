//! The adapter against a real X server and window manager. These run only when
//! `SNAPPY_X11_TEST_DISPLAY` names a disposable display, which
//! `tools/live/linux/x11-session.sh` provides; they never touch the desktop in `DISPLAY`.
//! Run them one at a time: they share the screen and the pointer.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]

use std::sync::mpsc::{Receiver, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{Combo, KeyChord, Modifier, Mods, Platform};
use snappy_core::settings::{Settings, Workspace};
use snappy_platform::{
    Caption, DesktopGuard, HotKeys, Input, Overlay, OverlayScene, OverlayZone, PlatformEvent, WindowId,
    WindowSystem,
};
use snappy_x11::{App, Stopper, X11Platform};
use x11rb::connection::Connection;
use x11rb::properties::{WmSizeHints, WmSizeHintsSpecification};
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt, CreateWindowAux, EventMask, GrabMode, MapState,
    ModMask, PropMode, Window, WindowClass,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

const TIMEOUT: Duration = Duration::from_secs(3);
const QUIET: Duration = Duration::from_millis(400);

/// Evdev keycodes; the session script loads the evdev US keymap.
mod key {
    pub const DIGIT_1: u8 = 10;
    pub const CONTROL_L: u8 = 37;
    pub const SHIFT_L: u8 = 50;
    pub const ALT_L: u8 = 64;
    pub const NUM_LOCK: u8 = 77;
    pub const SUPER_L: u8 = 133;
}

const HYPER: [Modifier; 3] = [Modifier::Ctrl, Modifier::Alt, Modifier::Meta];

/// A running adapter plus a client connection that plays the part of other apps.
struct Desktop {
    platform: X11Platform,
    events: Receiver<PlatformEvent>,
    stopper: Stopper,
    run: Option<JoinHandle<()>>,
    client: RustConnection,
    root: Window,
    windows: Vec<Window>,
}

fn desktop() -> Option<Desktop> {
    let Ok(display) = std::env::var("SNAPPY_X11_TEST_DISPLAY") else {
        eprintln!("skipped: SNAPPY_X11_TEST_DISPLAY is not set");
        return None;
    };
    let (sender, events) = channel();
    let app = App::connect(Some(&display), sender).unwrap();
    let platform = app.platform();
    let stopper = app.stopper();
    let run = std::thread::spawn(move || app.run().unwrap());
    let (client, screen) = x11rb::connect(Some(&display)).unwrap();
    let root = client.setup().roots[screen].root;
    Some(Desktop {
        platform,
        events,
        stopper,
        run: Some(run),
        client,
        root,
        windows: Vec::new(),
    })
}

impl Drop for Desktop {
    fn drop(&mut self) {
        for window in self.windows.drain(..) {
            let _ = self.client.destroy_window(window);
        }
        let _ = self.client.flush();
        self.platform.clear();
        self.stopper.stop();
        if let Some(run) = self.run.take() {
            let _ = run.join();
        }
    }
}

fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

fn id(window: Window) -> WindowId {
    WindowId(u64::from(window))
}

impl Desktop {
    fn atom(&self, name: &str) -> Atom {
        self.client
            .intern_atom(false, name.as_bytes())
            .unwrap()
            .reply()
            .unwrap()
            .atom
    }

    fn cardinals(&self, window: Window, name: &str, kind: impl Into<Atom>) -> Vec<u32> {
        let atom = self.atom(name);
        self.client
            .get_property(false, window, atom, kind, 0, 1024)
            .unwrap()
            .reply()
            .ok()
            .and_then(|reply| reply.value32().map(Iterator::collect))
            .unwrap_or_default()
    }

    /// Opens a client window at a user-chosen position and waits until the window
    /// manager manages it.
    fn open(&mut self, frame: Rect, setup: impl FnOnce(&Desktop, Window)) -> Window {
        let window = self.client.generate_id().unwrap();
        self.client
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                window,
                self.root,
                frame.x as i16,
                frame.y as i16,
                frame.width as u16,
                frame.height as u16,
                0,
                WindowClass::INPUT_OUTPUT,
                x11rb::COPY_FROM_PARENT,
                &CreateWindowAux::new().background_pixel(0x0033_6699),
            )
            .unwrap();
        self.client
            .change_property8(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                b"fixture\0Fixture\0",
            )
            .unwrap();
        let pid_atom = self.atom("_NET_WM_PID");
        self.client
            .change_property32(
                PropMode::REPLACE,
                window,
                pid_atom,
                AtomEnum::CARDINAL,
                &[std::process::id()],
            )
            .unwrap();
        let hints = WmSizeHints {
            position: Some((
                WmSizeHintsSpecification::UserSpecified,
                frame.x as i32,
                frame.y as i32,
            )),
            size: Some((
                WmSizeHintsSpecification::UserSpecified,
                frame.width as i32,
                frame.height as i32,
            )),
            ..WmSizeHints::default()
        };
        hints.set_normal_hints(&self.client, window).unwrap();
        setup(self, window);
        self.client.map_window(window).unwrap();
        self.client.flush().unwrap();
        self.windows.push(window);

        let managed = wait_until(|| {
            self.cardinals(self.root, "_NET_CLIENT_LIST", AtomEnum::WINDOW)
                .contains(&window)
                && !self
                    .cardinals(window, "_NET_FRAME_EXTENTS", AtomEnum::CARDINAL)
                    .is_empty()
        });
        assert!(managed, "the window manager never managed the test window");
        window
    }

    fn open_plain(&mut self, frame: Rect) -> Window {
        self.open(frame, |_, _| {})
    }

    fn close(&mut self, window: Window) {
        self.client.destroy_window(window).unwrap();
        self.client.flush().unwrap();
        self.windows.retain(|open| *open != window);
    }

    fn frame_becomes(&mut self, window: Window, expected: Rect) -> Rect {
        let mut last = Rect::ZERO;
        wait_until(|| {
            last = self.platform.frame(id(window)).unwrap_or(Rect::ZERO);
            last == expected
        });
        last
    }

    fn fake(&self, kind: u8, detail: u8, at: Option<Point>) {
        let (x, y) = at.map_or((0, 0), |point| (point.x as i16, point.y as i16));
        let root = if at.is_some() { self.root } else { x11rb::NONE };
        self.client
            .xtest_fake_input(kind, detail, 0, root, x, y, 0)
            .unwrap();
        self.client.flush().unwrap();
        std::thread::sleep(Duration::from_millis(15));
    }

    fn move_pointer(&self, to: Point) {
        self.fake(x11rb::protocol::xproto::MOTION_NOTIFY_EVENT, 0, Some(to));
    }

    fn button(&self, pressed: bool) {
        let kind = if pressed {
            x11rb::protocol::xproto::BUTTON_PRESS_EVENT
        } else {
            x11rb::protocol::xproto::BUTTON_RELEASE_EVENT
        };
        self.fake(kind, 1, None);
    }

    fn key(&self, keycode: u8, pressed: bool) {
        let kind = if pressed {
            x11rb::protocol::xproto::KEY_PRESS_EVENT
        } else {
            x11rb::protocol::xproto::KEY_RELEASE_EVENT
        };
        self.fake(kind, keycode, None);
    }

    /// Releases the primary button and then `keycode` in one flush, with no pause between,
    /// so the server has let go of the key before the release reaches the adapter.
    fn release_button_then_key(&self, keycode: u8) {
        use x11rb::protocol::xproto::{BUTTON_RELEASE_EVENT, KEY_RELEASE_EVENT};
        for (kind, detail) in [(BUTTON_RELEASE_EVENT, 1), (KEY_RELEASE_EVENT, keycode)] {
            self.client
                .xtest_fake_input(kind, detail, 0, x11rb::NONE, 0, 0, 0)
                .unwrap();
        }
        self.client.flush().unwrap();
    }

    fn tap(&self, keys: &[u8]) {
        for keycode in keys {
            self.key(*keycode, true);
        }
        for keycode in keys.iter().rev() {
            self.key(*keycode, false);
        }
    }

    /// The next event matching `wanted`, skipping others.
    fn expect(&self, wanted: impl Fn(&PlatformEvent) -> bool) -> Option<PlatformEvent> {
        let deadline = Instant::now() + TIMEOUT;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.events.recv_timeout(left) {
                Ok(event) if wanted(&event) => return Some(event),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    }

    /// Events arriving within a short quiet period.
    fn drain(&self) -> Vec<PlatformEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.recv_timeout(QUIET) {
            events.push(event);
        }
        events
    }

    fn overlay_windows(&self) -> Vec<Window> {
        let tree = self.client.query_tree(self.root).unwrap().reply().unwrap();
        tree.children
            .into_iter()
            .filter(|window| {
                self.client
                    .get_property(false, *window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
                    .ok()
                    .and_then(|cookie| cookie.reply().ok())
                    .is_some_and(|reply| reply.value.starts_with(b"snappy-overlay\0"))
            })
            .collect()
    }

    fn is_viewable(&self, window: Window) -> bool {
        self.client
            .get_window_attributes(window)
            .unwrap()
            .reply()
            .is_ok_and(|attributes| attributes.map_state == MapState::VIEWABLE)
    }

    fn desktop_count(&self) -> u32 {
        self.cardinals(self.root, "_NET_NUMBER_OF_DESKTOPS", AtomEnum::CARDINAL)[0]
    }

    fn desktop_names(&self) -> Vec<String> {
        let names = self.atom("_NET_DESKTOP_NAMES");
        let utf8 = self.atom("UTF8_STRING");
        let value = self
            .client
            .get_property(false, self.root, names, utf8, 0, 1024)
            .unwrap()
            .reply()
            .unwrap()
            .value;
        value
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .collect()
    }

    /// Names the desktops as a desktop session would, or removes the names.
    fn set_desktop_names(&self, names: &[String]) {
        let property = self.atom("_NET_DESKTOP_NAMES");
        if names.is_empty() {
            self.client.delete_property(self.root, property).unwrap();
        } else {
            let value: Vec<u8> = names.iter().flat_map(|name| name.bytes().chain([0])).collect();
            let utf8 = self.atom("UTF8_STRING");
            self.client
                .change_property8(PropMode::REPLACE, self.root, property, utf8, &value)
                .unwrap();
        }
        self.client.flush().unwrap();
        assert!(wait_until(|| self.desktop_names() == names));
    }

    fn window_desktop(&self, window: Window) -> Option<u32> {
        self.cardinals(window, "_NET_WM_DESKTOP", AtomEnum::CARDINAL)
            .first()
            .copied()
    }

    fn set_state(&self, window: Window, add: bool, states: [&str; 2]) {
        let data = [u32::from(add), self.atom(states[0]), self.atom(states[1]), 2, 0];
        let event = ClientMessageEvent::new(32, window, self.atom("_NET_WM_STATE"), data);
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        self.client.send_event(false, self.root, mask, event).unwrap();
        self.client.flush().unwrap();
    }
}

macro_rules! desktop {
    () => {
        match desktop() {
            Some(desktop) => desktop,
            None => return,
        }
    };
}

#[test]
fn displays_list_both_monitors_primary_first() {
    let mut desktop = desktop!();
    let displays = desktop.platform.displays();
    let frames: Vec<Rect> = displays.iter().map(|display| display.frame).collect();
    assert_eq!(
        frames,
        [
            Rect::new(0.0, 0.0, 1920.0, 1080.0),
            Rect::new(1920.0, 0.0, 1920.0, 1080.0)
        ]
    );
    assert_eq!(displays[0].id, "DUMMY0");
    assert_eq!(displays[1].id, "DUMMY1");
    assert!(displays.iter().all(|display| display.scale == 1.0));
}

#[test]
fn work_areas_exclude_panel_struts() {
    let mut desktop = desktop!();
    let before = desktop.platform.displays();
    desktop.open(Rect::new(0.0, 0.0, 1920.0, 30.0), |desktop, window| {
        let dock = desktop.atom("_NET_WM_WINDOW_TYPE_DOCK");
        let kind = desktop.atom("_NET_WM_WINDOW_TYPE");
        let strut = desktop.atom("_NET_WM_STRUT_PARTIAL");
        let conn = &desktop.client;
        conn.change_property32(PropMode::REPLACE, window, kind, AtomEnum::ATOM, &[dock])
            .unwrap();
        let top_panel = [0, 0, 30, 0, 0, 0, 0, 0, 0, 1919, 0, 0];
        conn.change_property32(PropMode::REPLACE, window, strut, AtomEnum::CARDINAL, &top_panel)
            .unwrap();
    });
    let mut areas = Vec::new();
    wait_until(|| {
        areas = desktop
            .platform
            .displays()
            .iter()
            .map(|display| display.work_area)
            .collect();
        areas[0] != before[0].work_area
    });
    assert_eq!(areas[0], Rect::new(0.0, 30.0, 1920.0, 1050.0));
    assert_eq!(areas[1], before[1].work_area);
    let events = desktop.drain();
    assert!(events.contains(&PlatformEvent::DisplaysChanged), "{events:?}");
}

#[test]
fn window_at_finds_the_topmost_managed_window() {
    let mut desktop = desktop!();
    let below = desktop.open_plain(Rect::new(100.0, 100.0, 400.0, 300.0));
    let above = desktop.open_plain(Rect::new(300.0, 250.0, 400.0, 300.0));
    let frame_below = desktop.platform.frame(id(below)).unwrap();
    let frame_above = desktop.platform.frame(id(above)).unwrap();

    let overlap = Point::new(frame_above.x + 10.0, frame_above.y + 10.0);
    assert!(frame_below.contains(overlap));
    let found = desktop.platform.window_at(overlap).unwrap();
    assert_eq!(found.id, id(above));
    assert_eq!(found.app, "Fixture");
    assert_eq!(found.pid, std::process::id());
    assert!(found.recovery_key.is_some());

    let only_below = Point::new(frame_below.x + 5.0, frame_below.y + 5.0);
    assert_eq!(desktop.platform.window_at(only_below).unwrap().id, id(below));
    assert!(desktop.platform.window_at(Point::new(3000.0, 900.0)).is_err());
}

#[test]
fn dialogs_are_refused_with_a_message() {
    let mut desktop = desktop!();
    let dialog = desktop.open(Rect::new(200.0, 200.0, 300.0, 200.0), |desktop, window| {
        let kind = desktop.atom("_NET_WM_WINDOW_TYPE");
        let dialog = desktop.atom("_NET_WM_WINDOW_TYPE_DIALOG");
        desktop
            .client
            .change_property32(PropMode::REPLACE, window, kind, AtomEnum::ATOM, &[dialog])
            .unwrap();
    });
    let frame = desktop.platform.frame(id(dialog)).unwrap();
    let error = desktop
        .platform
        .window_at(Point::new(frame.x + 20.0, frame.y + 20.0))
        .unwrap_err();
    assert_eq!(error.to_string(), "Choose a standard application window.");
    assert!(
        !desktop
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == id(dialog))
    );
}

#[test]
fn frames_include_decorations_and_follow_moves_and_resizes() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(150.0, 150.0, 500.0, 400.0));
    // Windows the engine has been told about are tracked for moves.
    assert!(
        desktop
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == id(window))
    );
    let extents = desktop.cardinals(window, "_NET_FRAME_EXTENTS", AtomEnum::CARDINAL);
    let frame = desktop.platform.frame(id(window)).unwrap();
    assert_eq!(frame.width, 500.0 + f64::from(extents[0] + extents[1]));
    assert_eq!(frame.height, 400.0 + f64::from(extents[2] + extents[3]));

    desktop
        .platform
        .set_position(id(window), Point::new(240.0, 180.0))
        .unwrap();
    let moved = Rect::new(240.0, 180.0, frame.width, frame.height);
    assert_eq!(desktop.frame_becomes(window, moved), moved);

    desktop
        .platform
        .set_size(id(window), Size::new(800.0, 600.0))
        .unwrap();
    let resized = Rect::new(240.0, 180.0, 800.0, 600.0);
    assert_eq!(desktop.frame_becomes(window, resized), resized);

    desktop
        .platform
        .set_position(id(window), Point::new(2000.0, 100.0))
        .unwrap();
    let other_display = Rect::new(2000.0, 100.0, 800.0, 600.0);
    assert_eq!(desktop.frame_becomes(window, other_display), other_display);

    let events = desktop.drain();
    assert!(
        events.contains(&PlatformEvent::WindowMoved(id(window))),
        "{events:?}"
    );
}

#[test]
fn maximized_windows_are_restored_before_moving() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(100.0, 100.0, 400.0, 300.0));
    desktop.set_state(
        window,
        true,
        ["_NET_WM_STATE_MAXIMIZED_VERT", "_NET_WM_STATE_MAXIMIZED_HORZ"],
    );
    assert!(wait_until(|| desktop
        .platform
        .frame(id(window))
        .is_ok_and(|frame| frame.width > 1000.0)));

    desktop
        .platform
        .set_position(id(window), Point::new(300.0, 200.0))
        .unwrap();
    desktop
        .platform
        .set_size(id(window), Size::new(600.0, 500.0))
        .unwrap();
    let expected = Rect::new(300.0, 200.0, 600.0, 500.0);
    assert_eq!(desktop.frame_becomes(window, expected), expected);
    let states = desktop.cardinals(window, "_NET_WM_STATE", AtomEnum::ATOM);
    assert!(!states.contains(&desktop.atom("_NET_WM_STATE_MAXIMIZED_HORZ")));
}

#[test]
fn focused_window_follows_activation() {
    let mut desktop = desktop!();
    let first = desktop.open_plain(Rect::new(100.0, 100.0, 300.0, 200.0));
    let second = desktop.open_plain(Rect::new(500.0, 100.0, 300.0, 200.0));
    for window in [first, second, first] {
        desktop.platform.focus(id(window));
        assert!(wait_until(|| desktop
            .platform
            .focused_window()
            .is_ok_and(|info| info.id == id(window))));
    }
    let focus = desktop.expect(|event| matches!(event, PlatformEvent::FocusChanged { .. }));
    assert_eq!(
        focus,
        Some(PlatformEvent::FocusChanged {
            pid: std::process::id(),
            by_activation: true
        })
    );
}

#[test]
fn window_lists_follow_creation_and_closing() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(120.0, 120.0, 300.0, 200.0));
    let created = desktop.expect(|event| matches!(event, PlatformEvent::WindowCreated { .. }));
    assert_eq!(
        created,
        Some(PlatformEvent::WindowCreated {
            pid: std::process::id()
        })
    );
    assert!(
        desktop
            .platform
            .visible_windows(Some("Fixture"))
            .iter()
            .any(|info| info.id == id(window))
    );
    assert!(desktop.platform.visible_windows(Some("Other")).is_empty());
    assert!(
        desktop
            .platform
            .all_windows()
            .iter()
            .any(|info| info.id == id(window))
    );

    let frame = desktop.platform.frame(id(window)).unwrap();
    let occupying = desktop.platform.windows_occupying(&[frame]).unwrap();
    assert_eq!(
        occupying.iter().map(|info| info.id).collect::<Vec<_>>(),
        [id(window)]
    );

    assert!(!desktop.platform.is_closed(id(window)));
    desktop.close(window);
    assert!(wait_until(|| desktop.platform.is_closed(id(window))));
    let destroyed = desktop.expect(|event| matches!(event, PlatformEvent::WindowDestroyed(_)));
    assert_eq!(destroyed, Some(PlatformEvent::WindowDestroyed(id(window))));
}

#[test]
fn minimized_windows_are_reported_and_not_visible() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(140.0, 140.0, 300.0, 200.0));
    assert!(!desktop.platform.state(id(window)).unwrap().minimized);
    desktop.set_state(window, true, ["_NET_WM_STATE_HIDDEN", "_NET_WM_STATE_HIDDEN"]);
    let event = ClientMessageEvent::new(32, window, desktop.atom("WM_CHANGE_STATE"), [3, 0, 0, 0, 0]);
    let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
    desktop
        .client
        .send_event(false, desktop.root, mask, event)
        .unwrap();
    desktop.client.flush().unwrap();
    assert!(wait_until(|| desktop
        .platform
        .state(id(window))
        .is_ok_and(|state| state.minimized)));
    assert!(
        !desktop
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == id(window))
    );
    assert!(
        desktop
            .platform
            .all_windows()
            .iter()
            .any(|info| info.id == id(window))
    );
}

#[test]
fn parked_windows_wait_on_snappys_desktop_and_return_exactly() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(300.0, 200.0, 400.0, 300.0));
    let home = desktop.platform.frame(id(window)).unwrap();
    let displays = desktop.platform.displays();
    let desktops = desktop.desktop_count();
    let names: Vec<String> = (1..=desktops).map(|number| format!("Desk {number}")).collect();
    desktop.set_desktop_names(&names);
    desktop.platform.bind(&displays);
    assert_eq!(desktop.desktop_count(), desktops + 1);
    assert_eq!(desktop.desktop_names().last().map(String::as_str), Some("Snappy"));
    assert!(desktop.platform.on_managed_desktop());

    desktop
        .platform
        .hide_for_workspace(id(window), home.origin())
        .unwrap();
    assert!(desktop.platform.state(id(window)).unwrap().on_parking_desktop);
    assert_eq!(desktop.window_desktop(window), Some(desktops));
    assert!(wait_until(|| !desktop.is_viewable(window)), "hidden at once");
    assert_eq!(
        desktop.platform.frame(id(window)).unwrap(),
        home,
        "the frame is untouched"
    );
    assert!(
        !desktop
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == id(window))
    );

    desktop
        .platform
        .show_for_workspace(id(window), home.origin())
        .unwrap();
    assert!(!desktop.platform.state(id(window)).unwrap().on_parking_desktop);
    assert_eq!(desktop.window_desktop(window), Some(0));
    assert!(wait_until(|| desktop.is_viewable(window)));
    assert_eq!(desktop.frame_becomes(window, home), home);

    desktop.platform.clear();
    assert!(wait_until(|| desktop.desktop_count() == desktops));
    assert_eq!(desktop.desktop_names(), names, "the user's names are kept");
    desktop.set_desktop_names(&[]);
}

#[test]
fn showing_a_parked_window_reapplies_its_saved_position() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(300.0, 200.0, 400.0, 300.0));
    let home = desktop.platform.frame(id(window)).unwrap();
    let displays = desktop.platform.displays();
    desktop.platform.bind(&displays);
    desktop
        .platform
        .hide_for_workspace(id(window), home.origin())
        .unwrap();

    let saved = home.with_origin(Point::new(500.0, 260.0));
    desktop
        .platform
        .show_for_workspace(id(window), saved.origin())
        .unwrap();
    assert_eq!(desktop.platform.frame(id(window)).unwrap(), saved);
    assert!(wait_until(|| desktop.is_viewable(window)));
    desktop.platform.clear();
}

#[test]
fn a_restarted_daemon_finds_windows_left_on_the_parking_desktop() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(300.0, 200.0, 400.0, 300.0));
    let home = desktop.platform.frame(id(window)).unwrap();
    let displays = desktop.platform.displays();
    let desktops = desktop.desktop_count();
    desktop.platform.bind(&displays);
    desktop
        .platform
        .hide_for_workspace(id(window), home.origin())
        .unwrap();

    let mut restarted = desktop!();
    let platform = &mut restarted.platform;
    assert!(platform.state(id(window)).unwrap().on_parking_desktop);
    platform.show_for_workspace(id(window), home.origin()).unwrap();
    assert!(wait_until(|| desktop.is_viewable(window)));
    platform.bind(&displays);
    assert_eq!(
        desktop.desktop_count(),
        desktops + 1,
        "the same desktop is reused"
    );
    platform.clear();
    assert!(wait_until(|| desktop.desktop_count() == desktops));
}

#[test]
fn desktop_guard_follows_the_native_desktop() {
    let mut desktop = desktop!();
    let displays = desktop.platform.displays();
    desktop.platform.bind(&displays);
    assert!(desktop.platform.on_managed_desktop());

    let switch_to = |desktop: &Desktop, number: u32| {
        let event = ClientMessageEvent::new(
            32,
            desktop.root,
            desktop.atom("_NET_CURRENT_DESKTOP"),
            [number, 0, 0, 0, 0],
        );
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        desktop
            .client
            .send_event(false, desktop.root, mask, event)
            .unwrap();
        desktop.client.flush().unwrap();
    };
    switch_to(&desktop, 1);
    assert!(wait_until(|| !desktop.platform.on_managed_desktop()));
    assert_eq!(
        desktop.expect(|event| *event == PlatformEvent::NativeDesktopChanged),
        Some(PlatformEvent::NativeDesktopChanged)
    );
    switch_to(&desktop, 0);
    assert!(wait_until(|| desktop.platform.on_managed_desktop()));
    desktop.platform.clear();
    assert!(!desktop.platform.on_managed_desktop());
}

#[test]
fn shortcuts_fire_with_and_without_num_lock() {
    let mut desktop = desktop!();
    let chord = KeyChord::snappy("Digit1", []).combo();
    assert!(desktop.platform.register(&[(7, chord)]).is_empty());

    let hyper_one = [key::CONTROL_L, key::ALT_L, key::SUPER_L, key::DIGIT_1];
    desktop.tap(&hyper_one);
    assert_eq!(
        desktop.expect(|event| matches!(event, PlatformEvent::HotKey(_))),
        Some(PlatformEvent::HotKey(7))
    );

    desktop.tap(&[key::NUM_LOCK]);
    desktop.tap(&hyper_one);
    let fired = desktop.expect(|event| matches!(event, PlatformEvent::HotKey(_)));
    desktop.tap(&[key::NUM_LOCK]);
    assert_eq!(fired, Some(PlatformEvent::HotKey(7)));

    desktop.platform.unregister_all();
    desktop.tap(&hyper_one);
    assert!(
        !desktop
            .drain()
            .iter()
            .any(|event| matches!(event, PlatformEvent::HotKey(_)))
    );
}

#[test]
fn chords_grabbed_elsewhere_are_unavailable() {
    let mut desktop = desktop!();
    let taken = Combo::new("KeyK", HYPER);
    let free = Combo::new("KeyJ", HYPER);
    assert!(desktop.platform.is_available(&taken));

    // Another client takes Ctrl+Alt+Super+K.
    let modifiers = ModMask::CONTROL | ModMask::M1 | ModMask::M4;
    desktop
        .client
        .grab_key(
            false,
            desktop.root,
            modifiers,
            45,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )
        .unwrap()
        .check()
        .unwrap();

    assert!(!desktop.platform.is_available(&taken));
    assert!(desktop.platform.is_available(&free));
    assert_eq!(
        desktop
            .platform
            .register(&[(1, taken.clone()), (2, free.clone())]),
        [1]
    );
    // Registered chords count as available to Snappy itself, and probing keeps them.
    assert!(desktop.platform.is_available(&free));
    assert!(desktop.platform.register(&[(2, free)]).is_empty());
    desktop.platform.unregister_all();
    desktop.client.ungrab_key(45, desktop.root, modifiers).unwrap();
    desktop.client.flush().unwrap();
    assert!(desktop.platform.is_available(&taken));
}

/// Every default chord, moving between zones, to the nearest empty zone, the workspace
/// steps, taking windows along, and switching to workspaces 1 to 9, is free under the
/// session's window manager, Xfwm4 with its default key bindings.
#[test]
fn the_default_shortcuts_are_free() {
    let mut desktop = desktop!();
    let settings = Settings::defaults(Platform::Linux);
    let mut chords: Vec<KeyChord> = settings
        .navigation
        .chords()
        .map(|(_, chord)| chord.clone())
        .collect();
    chords.extend(settings.navigation.nearest_empty.clone());
    chords.extend(settings.workspaces.steps().map(|(_, chord)| chord.clone()));
    chords.extend((1..=9).filter_map(|n| Workspace::numbered(format!("workspace-{n}"), n).switch_shortcut));
    assert_eq!(chords.len(), 5 + 5 + 9);
    let taken: Vec<String> = chords
        .iter()
        .filter(|chord| !desktop.platform.is_available(&chord.combo()))
        .map(|chord| chord.long_label(Platform::Linux))
        .collect();
    assert!(taken.is_empty(), "the window manager holds {taken:?}");
}

#[test]
fn pointer_presses_drags_and_releases_arrive_in_order() {
    let mut desktop = desktop!();
    desktop.move_pointer(Point::new(1000.0, 600.0));
    desktop.platform.set_press_monitoring(true);
    desktop.platform.set_drag_monitoring(true);
    desktop.drain();

    desktop.button(true);
    desktop.move_pointer(Point::new(1050.0, 620.0));
    desktop.key(key::SHIFT_L, true);
    desktop.move_pointer(Point::new(1100.0, 650.0));
    desktop.key(key::SHIFT_L, false);
    desktop.button(false);

    let pointer: Vec<PlatformEvent> = desktop
        .drain()
        .into_iter()
        .filter(|event| {
            matches!(
                event,
                PlatformEvent::PointerDown { .. }
                    | PlatformEvent::PointerDragged { .. }
                    | PlatformEvent::PointerUp { .. }
                    | PlatformEvent::ModifiersChanged { .. }
            )
        })
        .collect();
    let shift = [Modifier::Shift].into_iter().collect();
    assert!(
        matches!(
            pointer.first(),
            Some(PlatformEvent::PointerDown { at, mods, window: None })
                if *at == Point::new(1000.0, 600.0) && mods.is_empty()
        ),
        "{pointer:?}"
    );
    assert!(
        pointer.iter().any(|event| matches!(
            event,
            PlatformEvent::PointerDragged { at, .. } if *at == Point::new(1050.0, 620.0)
        )),
        "{pointer:?}"
    );
    assert!(
        pointer.iter().any(|event| matches!(
            event,
            PlatformEvent::ModifiersChanged { mods, .. } if *mods == shift
        )),
        "{pointer:?}"
    );
    assert!(
        pointer.iter().any(|event| matches!(
            event,
            PlatformEvent::PointerDragged { at, mods } if *at == Point::new(1100.0, 650.0) && *mods == shift
        )),
        "{pointer:?}"
    );
    assert!(
        matches!(
            pointer.last(),
            Some(PlatformEvent::PointerUp { at, .. }) if *at == Point::new(1100.0, 650.0)
        ),
        "{pointer:?}"
    );
}

#[test]
fn a_release_reports_the_drag_modifier_even_when_it_is_let_go_right_after() {
    let mut desktop = desktop!();
    desktop.move_pointer(Point::new(1000.0, 600.0));
    desktop.platform.set_press_monitoring(true);
    desktop.platform.set_drag_monitoring(true);
    desktop.drain();

    desktop.key(key::SHIFT_L, true);
    desktop.button(true);
    desktop.move_pointer(Point::new(1100.0, 650.0));
    desktop.release_button_then_key(key::SHIFT_L);

    let shift: Mods = [Modifier::Shift].into_iter().collect();
    let release = desktop.expect(|event| matches!(event, PlatformEvent::PointerUp { .. }));
    assert!(
        matches!(&release, Some(PlatformEvent::PointerUp { mods, .. }) if *mods == shift),
        "{release:?}"
    );
    let after = desktop.drain();
    assert!(
        after.iter().any(|event| matches!(
            event,
            PlatformEvent::ModifiersChanged { mods, .. } if mods.is_empty()
        )),
        "letting go of Shift is still reported: {after:?}"
    );
}

#[test]
fn keys_motion_and_presses_go_unreported_once_monitoring_is_off() {
    let mut desktop = desktop!();
    desktop.platform.set_press_monitoring(true);
    desktop.platform.set_drag_monitoring(true);
    desktop.drain();

    desktop.platform.set_drag_monitoring(false);
    desktop.drain();
    desktop.tap(&[key::SHIFT_L]);
    desktop.move_pointer(Point::new(900.0, 500.0));
    let events = desktop.drain();
    assert!(
        !events.iter().any(|event| matches!(
            event,
            PlatformEvent::ModifiersChanged { .. } | PlatformEvent::PointerDragged { .. }
        )),
        "{events:?}"
    );

    desktop.platform.set_press_monitoring(false);
    desktop.button(true);
    desktop.button(false);
    assert!(
        !desktop
            .drain()
            .iter()
            .any(|event| matches!(event, PlatformEvent::PointerDown { .. }))
    );
}

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
