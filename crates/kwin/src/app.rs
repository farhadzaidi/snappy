//! Setting up the adapter, and the event loop that turns what KWin, its script, and KDE's
//! shortcut service report into engine events. The loop may run on any thread, and owns the
//! tray icon.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Instant;

use snappy_core::geometry::Point;
use snappy_linux::adapter::Adapter;
use snappy_linux::clients::ClientTracker;
use snappy_linux::dbus;
use snappy_linux::journal;
use snappy_linux::names::{DBUS_NAME, NAME_OWNER_CHANGED};
use snappy_linux::tray::TrayIcon;
use snappy_platform::{CapsLockInput, LiveControl, PlatformEvent, WindowId};
use zbus::MatchRule;
use zbus::message::Type;

use crate::bus::{
    Bus, COMPONENT_INTERFACE, CONFIG_CHANGED, CONFIG_NOTIFY_INTERFACE, GLOBAL_ACCEL_NAME, KWIN_NAME,
    KXKBRC_PATH, SCREEN_SAVER_INTERFACE, Served,
};
use crate::error::KWinError;
use crate::input::order::HeldModifiers;
use crate::messages::{KWIN_NOT_RUNNING, SCRIPT_NOT_LOADED, SCRIPT_OTHER_VERSION, SCRIPT_STARTING};
use crate::overlay::{Command, Overlays};
use crate::platform::KWinPlatform;
use crate::protocol::{Event, PROTOCOL_VERSION, Request, WindowRecord};
use crate::shared::{Incoming, Shared};
use crate::{caps_lock, hotkeys, script};

pub struct App {
    shared: Arc<Shared>,
    events: Sender<PlatformEvent>,
    incoming: Receiver<Incoming>,
    stop: Sender<Incoming>,
    tray: Option<TrayIcon>,
}

/// Asks a running `App::run` to return. Callable from any thread.
#[derive(Clone)]
pub struct Stopper {
    stop: Sender<Incoming>,
}

impl Stopper {
    pub fn stop(&self) {
        let _ = self.stop.send(Incoming::Stop);
    }
}

impl App {
    /// Connects to the session bus, starts listening, and loads the script into KWin.
    pub fn new(events: Sender<PlatformEvent>) -> Result<App, KWinError> {
        caps_lock::prepare_xkb_folder();
        let bus = Bus::session()?;
        let (sender, incoming) = mpsc::channel();
        let shared = Arc::new(Shared::new(
            bus,
            Overlays::start()?,
            sender.clone(),
            LiveControl::unavailable(SCRIPT_STARTING),
        ));

        let script_events = sender.clone();
        shared.bus.serve(move |served| match served {
            Served::Event(event) => script_events.send(Incoming::Event(event)).is_ok(),
            Served::Ignored => true,
        })?;
        let pressed = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(GLOBAL_ACCEL_NAME)?
            .path(hotkeys::COMPONENT_PATH)?
            .interface(COMPONENT_INTERFACE)?
            .member("globalShortcutPressed")?
            .build();
        listen(&shared.bus, pressed, sender.clone(), |message| {
            let (_, action, _): (String, String, i64) = message.body().deserialize().ok()?;
            Some(Incoming::Shortcut(action))
        })?;
        let kwin_owner = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(DBUS_NAME)?
            .interface(DBUS_NAME)?
            .member(NAME_OWNER_CHANGED)?
            .arg(0, KWIN_NAME)?
            .build();
        listen(&shared.bus, kwin_owner, sender.clone(), |_| {
            Some(Incoming::KWinChanged)
        })?;
        listen_for_caps_lock(&shared.bus, &sender)?;

        load_script(&shared);
        let tray = TrayIcon::start(events.clone());
        Ok(App {
            shared,
            events,
            incoming,
            stop: sender,
            tray,
        })
    }

    /// The engine thread's handle.
    pub fn platform(&self) -> KWinPlatform {
        KWinPlatform::new(Arc::clone(&self.shared))
    }

    pub fn stopper(&self) -> Stopper {
        Stopper {
            stop: self.stop.clone(),
        }
    }

    /// Handles events until `Stopper::stop`, then takes the overlay down, releases Snappy's
    /// shortcuts, and unloads the script. Windows parked on Snappy's desktop stay
    /// there; quitting returns them first. Fails when the session bus goes away.
    pub fn run(mut self) -> Result<(), KWinError> {
        let mut windows = WindowList::default();
        let mut held = HeldModifiers::default();
        let mut pointer = Point::default();
        windows.refresh(&self.shared.bus);
        let result = loop {
            let next = match held.next_due() {
                Some(due) => match self
                    .incoming
                    .recv_timeout(due.saturating_duration_since(Instant::now()))
                {
                    Ok(incoming) => Some(incoming),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break Ok(()),
                },
                None => match self.incoming.recv() {
                    Ok(incoming) => Some(incoming),
                    Err(_) => break Ok(()),
                },
            };
            for mods in held.due(Instant::now()) {
                self.emit(PlatformEvent::ModifiersChanged { at: pointer, mods });
            }
            let Some(incoming) = next else { continue };
            match incoming {
                Incoming::Stop => break Ok(()),
                Incoming::BusLost => break Err(KWinError::BusLost),
                Incoming::KWinChanged => self.kwin_changed(),
                Incoming::Tray(menu) => {
                    if let Some(tray) = &self.tray {
                        tray.update(menu);
                    }
                }
                Incoming::Shortcut(action) if action == hotkeys::CAPS_LOCK_TAP => {
                    if caps_lock::released_alone(&self.shared) {
                        self.emit(PlatformEvent::CapsLock(CapsLockInput::Released));
                    }
                }
                Incoming::Shortcut(action) => {
                    if let Some(id) = hotkeys::shortcut_id(&action) {
                        self.emit(PlatformEvent::HotKey(id));
                    }
                }
                Incoming::CapsLockModifiers(mods) => {
                    if caps_lock::modifiers_reported(&self.shared, mods) {
                        self.emit(PlatformEvent::CapsLock(CapsLockInput::Pressed));
                    }
                }
                Incoming::ScreenLocked(locked) => {
                    if caps_lock::screen_locked(&self.shared, locked) {
                        self.emit(PlatformEvent::CapsLockLost);
                    }
                }
                Incoming::KeyboardSettingsChanged => {
                    if caps_lock::settings_changed(&self.shared) {
                        self.emit(PlatformEvent::CapsLockLost);
                    }
                }
                Incoming::Modifiers(mods, at) => {
                    if let Some(mods) = held.report(mods, at) {
                        self.emit(PlatformEvent::ModifiersChanged { at: pointer, mods });
                    }
                }
                Incoming::Event(Event::Ready { version }) => self.script_started(version, &mut windows),
                Incoming::Event(event) => self.event(event, &mut windows, &mut held, &mut pointer),
            }
        };
        self.release();
        result
    }

    fn release(&mut self) {
        caps_lock::restore(&self.shared);
        let grabs = std::mem::take(&mut self.shared.state().registered);
        hotkeys::unregister(&self.shared.bus, &grabs);
        self.shared.follow_modifiers(false);
        self.shared.overlays.send(Command::Stop);
        script::stop(&self.shared.bus);
        self.shared.bus.release_name();
        if let Some(tray) = self.tray.take() {
            tray.stop();
        }
    }

    fn emit(&self, event: PlatformEvent) {
        let _ = self.events.send(event);
    }

    fn set_live_control(&self, live_control: LiveControl) {
        if self.shared.set_live_control(live_control) {
            self.emit(PlatformEvent::LiveControlChanged);
        }
    }

    /// A freshly started script holds none of Snappy's monitoring or followed windows, and
    /// windows may have come and gone meanwhile, so it is told everything again and the
    /// differences are reported before the engine hears that it can control windows.
    fn script_started(&self, version: u32, windows: &mut WindowList) {
        if version != PROTOCOL_VERSION {
            journal::warn(format_args!(
                "KWin runs version {version} of Snappy's script, not {PROTOCOL_VERSION}"
            ));
            self.set_live_control(LiveControl::unavailable(SCRIPT_OTHER_VERSION));
            return;
        }
        self.shared.replay();
        for event in windows.refresh(&self.shared.bus) {
            self.emit(event);
        }
        self.shared.set_live_control(LiveControl::Available);
        self.emit(PlatformEvent::LiveControlChanged);
    }

    /// KWin started over, which drops every script, or went away.
    /// Reported once, with where loading the script left things.
    fn kwin_changed(&self) {
        let before = self.shared.live_control();
        self.shared
            .set_live_control(LiveControl::unavailable(SCRIPT_STARTING));
        load_script(&self.shared);
        if self.shared.live_control() != before {
            self.emit(PlatformEvent::LiveControlChanged);
        }
    }

    fn event(&self, event: Event, windows: &mut WindowList, held: &mut HeldModifiers, pointer: &mut Point) {
        let translated = match event {
            Event::PointerDown { x, y, window, frame } => {
                *pointer = Point::new(x, y);
                held.drag_started();
                vec![PlatformEvent::PointerDown {
                    at: *pointer,
                    mods: held.current(),
                    window: Some(WindowId(window)),
                    frame: Some(frame.into()),
                }]
            }
            Event::PointerDragged { x, y } => {
                *pointer = Point::new(x, y);
                vec![PlatformEvent::PointerDragged {
                    at: *pointer,
                    mods: held.current(),
                }]
            }
            Event::PointerUp { x, y } => {
                *pointer = Point::new(x, y);
                vec![PlatformEvent::PointerUp {
                    at: *pointer,
                    mods: held.drag_ended(),
                }]
            }
            Event::WindowAdded { id, pid } => windows.added(id, pid),
            Event::WindowRemoved { id } => {
                self.shared.state().followed.remove(&id);
                windows.removed(id)
            }
            Event::WindowMoved { id } => vec![PlatformEvent::WindowMoved(WindowId(id))],
            Event::Focus { pid } => vec![PlatformEvent::FocusChanged {
                pid,
                by_activation: true,
            }],
            Event::Displays => vec![PlatformEvent::DisplaysChanged],
            Event::Desktop => vec![PlatformEvent::NativeDesktopChanged],
            Event::Ready { .. } | Event::Unknown => Vec::new(),
        };
        for event in translated {
            self.emit(event);
        }
    }
}

impl Adapter for App {
    type Platform = KWinPlatform;
    type Error = KWinError;

    fn start(events: Sender<PlatformEvent>) -> Result<App, KWinError> {
        App::new(events)
    }

    fn platform(&self) -> KWinPlatform {
        App::platform(self)
    }

    fn stop_handle(&self) -> impl FnOnce() + Send + 'static {
        let stopper = self.stopper();
        move || stopper.stop()
    }

    fn run(self) -> Result<(), KWinError> {
        App::run(self)
    }
}

/// Loads the script and says why live control is off when it can't be. The script reports
/// `ready` once it runs.
fn load_script(shared: &Shared) {
    if !shared.bus.kwin_running() {
        shared.set_live_control(LiveControl::unavailable(KWIN_NOT_RUNNING));
        return;
    }
    if let Err(error) = script::start(&shared.bus) {
        journal::warn(format_args!("KWin didn't load Snappy's script: {error:?}"));
        shared.set_live_control(LiveControl::unavailable(SCRIPT_NOT_LOADED));
    }
}

/// The managed windows with their processes, to tell apps launching and quitting.
#[derive(Default)]
struct WindowList {
    tracker: ClientTracker,
    /// The list was read once; until then there's nothing to compare with.
    seeded: bool,
}

impl WindowList {
    /// Reads the list again. The first read is taken as it is; later ones report what came
    /// and went since, as after the script started over. Nothing changes when the script
    /// doesn't answer.
    fn refresh(&mut self, bus: &Bus) -> Vec<PlatformEvent> {
        let Ok(records) = bus.call::<Vec<WindowRecord>>(&Request::Windows) else {
            return Vec::new();
        };
        let pids: BTreeMap<u64, Option<u32>> = records
            .iter()
            .map(|record| (record.id, (record.pid > 0).then_some(record.pid)))
            .collect();
        if !std::mem::replace(&mut self.seeded, true) {
            self.tracker = ClientTracker::new(pids);
            return Vec::new();
        }
        let list: Vec<u64> = pids.keys().copied().collect();
        self.tracker.update(&list, &pids)
    }

    fn added(&mut self, id: u64, pid: u32) -> Vec<PlatformEvent> {
        let pid = (pid > 0).then_some(pid);
        let mut list = self.tracker.windows();
        list.push(id);
        self.tracker.update(&list, &BTreeMap::from([(id, pid)]))
    }

    fn removed(&mut self, id: u64) -> Vec<PlatformEvent> {
        let list: Vec<u64> = self
            .tracker
            .windows()
            .into_iter()
            .filter(|window| *window != id)
            .collect();
        self.tracker.update(&list, &BTreeMap::new())
    }
}

/// Listens for what Caps Lock as the Snappy key follows: the session being locked, which
/// the lock screen's own process announces, and KDE's keyboard layout settings changed by
/// another program. Snappy's own notifications of that change are left out.
fn listen_for_caps_lock(bus: &Bus, sender: &Sender<Incoming>) -> Result<(), KWinError> {
    let locked = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(SCREEN_SAVER_INTERFACE)?
        .member("ActiveChanged")?
        .build();
    listen(bus, locked, sender.clone(), |message| {
        message
            .body()
            .deserialize::<bool>()
            .ok()
            .map(Incoming::ScreenLocked)
    })?;
    let changed = MatchRule::builder()
        .msg_type(Type::Signal)
        .path(KXKBRC_PATH)?
        .interface(CONFIG_NOTIFY_INTERFACE)?
        .member(CONFIG_CHANGED)?
        .build();
    let own = bus.connection().unique_name().map(ToString::to_string);
    listen(bus, changed, sender.clone(), move |message| {
        let header = message.header();
        let sender = header.sender().map(ToString::to_string);
        if sender.is_some() && sender == own {
            return None;
        }
        let groups: std::collections::HashMap<String, Vec<Vec<u8>>> = message.body().deserialize().ok()?;
        groups
            .contains_key(caps_lock::CHANGED_GROUP)
            .then_some(Incoming::KeyboardSettingsChanged)
    })
}

/// Forwards each matching signal from a thread of its own, and reports the bus going away.
fn listen(
    bus: &Bus,
    rule: MatchRule<'static>,
    sender: Sender<Incoming>,
    translate: impl Fn(&zbus::Message) -> Option<Incoming> + Send + 'static,
) -> Result<(), KWinError> {
    dbus::forward_signals(
        bus.connection(),
        rule,
        "snappy-kwin-signals",
        sender,
        Incoming::BusLost,
        translate,
    )
}
