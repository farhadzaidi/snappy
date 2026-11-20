//! What the tests share: the adapter running on the test display, and a client connection
//! that opens windows and fakes input as other apps and the user would.

use std::sync::mpsc::{Receiver, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::Modifier;
use snappy_platform::{DesktopGuard, PlatformEvent, WindowId, WindowSystem};
use snappy_x11::{App, Stopper, X11Platform};
use x11rb::connection::Connection;
use x11rb::properties::{WmSizeHints, WmSizeHintsSpecification};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt, CreateWindowAux, EventMask, MapState, PropMode,
    Window, WindowClass,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

pub const TIMEOUT: Duration = Duration::from_secs(3);
pub const QUIET: Duration = Duration::from_millis(400);

/// Evdev keycodes; the session script loads the evdev US keymap.
pub mod key {
    pub const DIGIT_1: u8 = 10;
    pub const CONTROL_L: u8 = 37;
    pub const KEY_J: u8 = 44;
    pub const KEY_B: u8 = 56;
    pub const SHIFT_L: u8 = 50;
    pub const ALT_L: u8 = 64;
    pub const NUM_LOCK: u8 = 77;
    pub const SUPER_L: u8 = 133;
    pub const CAPS_LOCK: u8 = 66;
    pub const RIGHT: u8 = 114;
}

/// The default Snappy key's modifiers.
pub const SNAPPY_KEY: [Modifier; 3] = [Modifier::Ctrl, Modifier::Alt, Modifier::Meta];

/// A running adapter plus a client connection that plays the part of other apps.
pub struct Desktop {
    pub platform: X11Platform,
    pub events: Receiver<PlatformEvent>,
    pub stopper: Stopper,
    pub run: Option<JoinHandle<()>>,
    pub client: RustConnection,
    pub root: Window,
    pub windows: Vec<Window>,
}

pub fn open_desktop() -> Option<Desktop> {
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

pub fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

pub fn id(window: Window) -> WindowId {
    WindowId(u64::from(window))
}

impl Desktop {
    pub fn atom(&self, name: &str) -> Atom {
        self.client
            .intern_atom(false, name.as_bytes())
            .unwrap()
            .reply()
            .unwrap()
            .atom
    }

    pub fn cardinals(&self, window: Window, name: &str, kind: impl Into<Atom>) -> Vec<u32> {
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
    pub fn open(&mut self, frame: Rect, setup: impl FnOnce(&Desktop, Window)) -> Window {
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

    pub fn open_plain(&mut self, frame: Rect) -> Window {
        self.open(frame, |_, _| {})
    }

    pub fn close(&mut self, window: Window) {
        self.client.destroy_window(window).unwrap();
        self.client.flush().unwrap();
        self.windows.retain(|open| *open != window);
    }

    pub fn frame_becomes(&mut self, window: Window, expected: Rect) -> Rect {
        let mut last = Rect::ZERO;
        wait_until(|| {
            last = self.platform.frame(id(window)).unwrap_or(Rect::ZERO);
            last == expected
        });
        last
    }

    pub fn fake(&self, kind: u8, detail: u8, at: Option<Point>) {
        let (x, y) = at.map_or((0, 0), |point| (point.x as i16, point.y as i16));
        let root = if at.is_some() { self.root } else { x11rb::NONE };
        self.client
            .xtest_fake_input(kind, detail, 0, root, x, y, 0)
            .unwrap();
        self.client.flush().unwrap();
        std::thread::sleep(Duration::from_millis(15));
    }

    pub fn move_pointer(&self, to: Point) {
        self.fake(x11rb::protocol::xproto::MOTION_NOTIFY_EVENT, 0, Some(to));
    }

    pub fn button(&self, pressed: bool) {
        let kind = if pressed {
            x11rb::protocol::xproto::BUTTON_PRESS_EVENT
        } else {
            x11rb::protocol::xproto::BUTTON_RELEASE_EVENT
        };
        self.fake(kind, 1, None);
    }

    pub fn key(&self, keycode: u8, pressed: bool) {
        let kind = if pressed {
            x11rb::protocol::xproto::KEY_PRESS_EVENT
        } else {
            x11rb::protocol::xproto::KEY_RELEASE_EVENT
        };
        self.fake(kind, keycode, None);
    }

    /// Releases the primary button and then `keycode` in one flush, with no pause between,
    /// so the server has let go of the key before the release reaches the adapter.
    pub fn release_button_then_key(&self, keycode: u8) {
        use x11rb::protocol::xproto::{BUTTON_RELEASE_EVENT, KEY_RELEASE_EVENT};
        for (kind, detail) in [(BUTTON_RELEASE_EVENT, 1), (KEY_RELEASE_EVENT, keycode)] {
            self.client
                .xtest_fake_input(kind, detail, 0, x11rb::NONE, 0, 0, 0)
                .unwrap();
        }
        self.client.flush().unwrap();
    }

    pub fn tap(&self, keys: &[u8]) {
        for keycode in keys {
            self.key(*keycode, true);
        }
        for keycode in keys.iter().rev() {
            self.key(*keycode, false);
        }
    }

    /// The next event matching `wanted`, skipping others.
    pub fn expect(&self, wanted: impl Fn(&PlatformEvent) -> bool) -> Option<PlatformEvent> {
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
    pub fn drain(&self) -> Vec<PlatformEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.recv_timeout(QUIET) {
            events.push(event);
        }
        events
    }

    pub fn overlay_windows(&self) -> Vec<Window> {
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

    pub fn is_viewable(&self, window: Window) -> bool {
        self.client
            .get_window_attributes(window)
            .unwrap()
            .reply()
            .is_ok_and(|attributes| attributes.map_state == MapState::VIEWABLE)
    }

    pub fn desktop_count(&self) -> u32 {
        self.cardinals(self.root, "_NET_NUMBER_OF_DESKTOPS", AtomEnum::CARDINAL)[0]
    }

    pub fn desktop_names(&self) -> Vec<String> {
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
    pub fn set_desktop_names(&self, names: &[String]) {
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

    pub fn window_desktop(&self, window: Window) -> Option<u32> {
        self.cardinals(window, "_NET_WM_DESKTOP", AtomEnum::CARDINAL)
            .first()
            .copied()
    }

    pub fn set_state(&self, window: Window, add: bool, states: [&str; 2]) {
        let data = [u32::from(add), self.atom(states[0]), self.atom(states[1]), 2, 0];
        let event = ClientMessageEvent::new(32, window, self.atom("_NET_WM_STATE"), data);
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        self.client.send_event(false, self.root, mask, event).unwrap();
        self.client.flush().unwrap();
    }
}

macro_rules! desktop {
    () => {
        match $crate::support::open_desktop() {
            Some(desktop) => desktop,
            None => return,
        }
    };
}
pub(crate) use desktop;
