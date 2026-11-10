//! Setting up the adapter: two X connections, the event thread's loop, and the overlay
//! thread. X has no main-thread rule, so `run` may be called on any thread.

use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;

use snappy_platform::PlatformEvent;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xkb::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask,
    WindowClass,
};
use x11rb::rust_connection::RustConnection;

use crate::error::X11Error;
use crate::events::EventLoop;
use crate::hotkeys;
use crate::input::{self, Monitoring};
use crate::overlay::{self, UiCommand};
use crate::platform::X11Platform;
use crate::shared::Shared;
use crate::x::{Atoms, ScreenInfo};

/// XInput 2.2: raw events keep arriving while another client grabs the pointer.
const XINPUT_VERSION: (u16, u16) = (2, 2);
const RANDR_VERSION: (u32, u32) = (1, 5);

/// Owns the event thread's side of the adapter.
pub struct App {
    shared: Arc<Shared>,
    engine_conn: Arc<RustConnection>,
    events: Sender<PlatformEvent>,
    ui: Sender<UiCommand>,
    ui_thread: JoinHandle<()>,
    live: bool,
}

/// Asks a running `App::run` to return. Callable from any thread.
#[derive(Clone)]
pub struct Stopper {
    shared: Arc<Shared>,
}

impl App {
    /// Connects to the display named by `DISPLAY` and starts the overlay thread.
    pub fn new(events: Sender<PlatformEvent>) -> Result<App, X11Error> {
        App::connect(None, events)
    }

    /// Connects to `display`, or to `DISPLAY` when `None`.
    pub fn connect(display: Option<&str>, events: Sender<PlatformEvent>) -> Result<App, X11Error> {
        let wayland = snappy_linux::session::current_is_wayland();
        let (conn, number) = x11rb::connect(display).map_err(|error| {
            if wayland {
                X11Error::WaylandWithoutX
            } else {
                X11Error::Connect(error)
            }
        })?;
        let (engine_conn, _) = x11rb::connect(display)?;

        let screen = ScreenInfo::read(&conn, number)?;
        let atoms = Atoms::new(&conn)?.reply()?;
        let control = conn.generate_id()?;
        conn.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            control,
            screen.root,
            -1,
            -1,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new().override_redirect(1),
        )?
        .check()?;
        let root_events = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE);
        conn.change_window_attributes(screen.root, &root_events)?
            .check()?;

        watch_displays(&conn, &screen);
        negotiate_randr(&engine_conn);
        let xinput = !wayland && negotiate_xinput(&conn);
        detect_key_repeat(&conn);

        let shared = Arc::new(Shared::new(conn, screen, atoms, control, xinput));
        overlay::negotiate(&shared);
        let (ui, commands) = mpsc::channel();
        let ui_thread = overlay::spawn(Arc::clone(&shared), events.clone(), commands)?;

        Ok(App {
            shared,
            engine_conn: Arc::new(engine_conn),
            events,
            ui,
            ui_thread,
            live: !wayland,
        })
    }

    /// The engine thread's handle.
    pub fn platform(&self) -> X11Platform {
        X11Platform::new(
            Arc::clone(&self.engine_conn),
            Arc::clone(&self.shared),
            self.ui.clone(),
            self.live,
        )
    }

    pub fn stopper(&self) -> Stopper {
        Stopper {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Reads events until `Stopper::stop`, then removes everything Snappy installed.
    pub fn run(self) -> Result<(), X11Error> {
        let result = EventLoop::new(Arc::clone(&self.shared), self.events.clone(), self.ui.clone()).run();
        self.teardown();
        result
    }

    fn teardown(self) {
        hotkeys::unregister_all(&self.shared);
        input::set(&self.shared, |monitoring| *monitoring = Monitoring::default());
        let _ = self.ui.send(UiCommand::Stop);
        let _ = self.ui_thread.join();
        let _ = self.shared.conn.destroy_window(self.shared.control);
        let _ = self.shared.conn.flush();
    }
}

impl Stopper {
    pub fn stop(&self) {
        let shared = &self.shared;
        let event = ClientMessageEvent::new(32, shared.control, shared.atoms._SNAPPY_STOP, [0u32; 5]);
        if let Ok(cookie) = shared
            .conn
            .send_event(false, shared.control, EventMask::NO_EVENT, event)
        {
            cookie.ignore_error();
        }
        let _ = shared.conn.flush();
    }
}

/// Display changes arrive as RandR notifications.
fn watch_displays(conn: &RustConnection, screen: &ScreenInfo) {
    if !negotiate_randr(conn) {
        return;
    }
    let changes =
        randr::NotifyMask::SCREEN_CHANGE | randr::NotifyMask::CRTC_CHANGE | randr::NotifyMask::OUTPUT_CHANGE;
    if let Ok(cookie) = conn.randr_select_input(screen.root, changes) {
        cookie.ignore_error();
    }
}

fn negotiate_randr(conn: &RustConnection) -> bool {
    if !matches!(conn.extension_information(randr::X11_EXTENSION_NAME), Ok(Some(_))) {
        return false;
    }
    conn.randr_query_version(RANDR_VERSION.0, RANDR_VERSION.1)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some()
}

fn negotiate_xinput(conn: &RustConnection) -> bool {
    if !matches!(
        conn.extension_information(xinput::X11_EXTENSION_NAME),
        Ok(Some(_))
    ) {
        return false;
    }
    conn.xinput_xi_query_version(XINPUT_VERSION.0, XINPUT_VERSION.1)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|reply| (reply.major_version, reply.minor_version) >= XINPUT_VERSION)
}

/// Held shortcut keys then repeat as presses without releases, which the event loop
/// ignores until the key is released.
fn detect_key_repeat(conn: &RustConnection) {
    if !matches!(conn.extension_information(xkb::X11_EXTENSION_NAME), Ok(Some(_))) {
        return;
    }
    let supported = conn
        .xkb_use_extension(1, 0)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|reply| reply.supported);
    if !supported {
        return;
    }
    let detectable = xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT;
    let none = xkb::BoolCtrl::from(0u32);
    if let Ok(cookie) = conn.xkb_per_client_flags(
        xkb::ID::USE_CORE_KBD.into(),
        detectable,
        detectable,
        none,
        none,
        none,
    ) {
        let _ = cookie.reply();
    }
}
