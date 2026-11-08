//! Setting up the adapter, and the event loop that turns what GNOME Shell and the extension
//! report into engine events. The loop may run on any thread.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use snappy_core::geometry::Point;
use snappy_linux::clients::ClientTracker;
use snappy_platform::{PlatformEvent, WindowId};
use zbus::MatchRule;
use zbus::blocking::MessageIterator;
use zbus::message::Type;

use crate::bus::{Bus, DISPLAY_CONFIG_NAME, DISPLAY_CONFIG_PATH, ExtensionInfo};
use crate::error::GnomeError;
use crate::extension::{self, Installed};
use crate::keys::mods;
use crate::platform::GnomePlatform;
use crate::protocol::{EXTENSION_UUID, Event, INTERFACE, OBJECT_PATH, Request, SHELL_BUS_NAME, WindowRecord};
use crate::shared::Shared;

/// What the listening threads hand the event loop.
enum Incoming {
    Event(Event),
    /// GNOME Shell reported a change to Snappy's extension, or Shell itself came or went.
    ExtensionChanged,
    MonitorsChanged,
    /// Another tool that snaps windows was turned on or off.
    NotesChanged,
    Stop,
}

pub struct App {
    shared: Arc<Shared>,
    events: Sender<PlatformEvent>,
    incoming: Receiver<Incoming>,
    stop: Sender<Incoming>,
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
    /// Connects to the session bus, installs or updates the extension, and starts listening.
    pub fn new(events: Sender<PlatformEvent>) -> Result<App, GnomeError> {
        let bus = Bus::session()?;
        let installed = install_extension();
        let shared = Arc::new(Shared::new(bus, installed));
        let live_control = extension::live_control(&shared.bus, installed);
        shared.state().live_control = Some(live_control);

        let (sender, incoming) = mpsc::channel();
        listen(
            &shared.bus,
            signal_rule(INTERFACE, "Event", Some(OBJECT_PATH))?,
            sender.clone(),
            |message| {
                let (json,): (String,) = message.body().deserialize().ok()?;
                Some(Incoming::Event(serde_json::from_str(&json).ok()?))
            },
        )?;
        listen(
            &shared.bus,
            signal_rule("org.gnome.Shell.Extensions", "ExtensionStateChanged", None)?,
            sender.clone(),
            |message| {
                let (uuid, _): (String, ExtensionInfo) = message.body().deserialize().ok()?;
                if uuid == EXTENSION_UUID {
                    Some(Incoming::ExtensionChanged)
                } else if extension::COMPETING_EXTENSIONS
                    .iter()
                    .any(|(other, _)| *other == uuid)
                {
                    Some(Incoming::NotesChanged)
                } else {
                    None
                }
            },
        )?;
        let shell_owner = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .arg(0, SHELL_BUS_NAME)?
            .build();
        listen(&shared.bus, shell_owner, sender.clone(), |_| {
            Some(Incoming::ExtensionChanged)
        })?;
        let monitors = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(DISPLAY_CONFIG_NAME)?
            .path(DISPLAY_CONFIG_PATH)?
            .interface(DISPLAY_CONFIG_NAME)?
            .member("MonitorsChanged")?
            .build();
        listen(&shared.bus, monitors, sender.clone(), |_| {
            Some(Incoming::MonitorsChanged)
        })?;

        Ok(App {
            shared,
            events,
            incoming,
            stop: sender,
        })
    }

    /// The engine thread's handle.
    pub fn platform(&self) -> GnomePlatform {
        GnomePlatform::new(Arc::clone(&self.shared))
    }

    pub fn stopper(&self) -> Stopper {
        Stopper {
            stop: self.stop.clone(),
        }
    }

    /// Handles events until `Stopper::stop`, then releases what Snappy asked the extension
    /// to keep. Hidden workspace windows stay where they are; quitting returns them first.
    pub fn run(self) -> Result<(), GnomeError> {
        let mut windows = WindowList::read(&self.shared.bus);
        for incoming in &self.incoming {
            match incoming {
                Incoming::Stop => break,
                Incoming::ExtensionChanged => self.extension_changed(false, &mut windows),
                Incoming::MonitorsChanged => self.emit(PlatformEvent::DisplaysChanged),
                Incoming::NotesChanged => self.emit(PlatformEvent::LiveControlChanged),
                Incoming::Event(Event::Ready) => self.extension_changed(true, &mut windows),
                Incoming::Event(event) => self.event(event, &mut windows),
            }
        }
        let bus = &self.shared.bus;
        let _ = bus.send(&Request::UnregisterAll);
        let _ = bus.send(&Request::Monitoring {
            press: false,
            drag: false,
            follow: 0,
        });
        let _ = bus.send(&Request::OverlayHide);
        Ok(())
    }

    fn emit(&self, event: PlatformEvent) {
        let _ = self.events.send(event);
    }

    /// Asks again whether the extension answers. A freshly started one has none of Snappy's
    /// shortcuts, monitoring, or menu yet, so the engine hears about it either way.
    fn extension_changed(&self, started: bool, windows: &mut WindowList) {
        let live_control = extension::live_control(&self.shared.bus, self.shared.installed);
        let changed = {
            let mut state = self.shared.state();
            let changed = state.live_control.as_ref() != Some(&live_control);
            if started || changed {
                state.replay_needed = live_control.is_available();
            }
            state.live_control = Some(live_control.clone());
            changed
        };
        if live_control.is_available() && (started || changed) {
            *windows = WindowList::read(&self.shared.bus);
        }
        if started || changed {
            self.emit(PlatformEvent::LiveControlChanged);
        }
    }

    fn event(&self, event: Event, windows: &mut WindowList) {
        let point = |x: f64, y: f64| Point::new(x, y);
        let translated = match event {
            Event::PointerDown {
                x,
                y,
                mods: mask,
                window,
            } => vec![PlatformEvent::PointerDown {
                at: point(x, y),
                mods: mods(mask),
                window: Some(WindowId(window)),
            }],
            Event::PointerDragged { x, y, mods: mask } => vec![PlatformEvent::PointerDragged {
                at: point(x, y),
                mods: mods(mask),
            }],
            Event::PointerUp { x, y, mods: mask } => vec![PlatformEvent::PointerUp {
                at: point(x, y),
                mods: mods(mask),
            }],
            Event::Modifiers { x, y, mods: mask } => vec![PlatformEvent::ModifiersChanged {
                at: point(x, y),
                mods: mods(mask),
            }],
            Event::Hotkey { id } => vec![PlatformEvent::HotKey(id)],
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
            Event::Workspace => vec![PlatformEvent::NativeDesktopChanged],
            Event::Menu { item } => {
                let state = self.shared.state();
                state
                    .menu_actions
                    .get(item as usize)
                    .cloned()
                    .into_iter()
                    .collect()
            }
            Event::Ready | Event::Unknown => Vec::new(),
        };
        for event in translated {
            self.emit(event);
        }
    }
}

/// The managed windows with their processes, to tell apps launching and quitting.
struct WindowList {
    pids: BTreeMap<u64, Option<u32>>,
    tracker: ClientTracker,
}

impl WindowList {
    fn read(bus: &Bus) -> WindowList {
        let records: Vec<WindowRecord> = bus.call(&Request::Windows).unwrap_or_default();
        let pids: BTreeMap<u64, Option<u32>> = records
            .iter()
            .map(|record| (record.id, (record.pid > 0).then_some(record.pid)))
            .collect();
        WindowList {
            tracker: ClientTracker::new(pids.iter().map(|(id, pid)| (*id, *pid))),
            pids,
        }
    }

    fn list(&self) -> Vec<u64> {
        self.pids.keys().copied().collect()
    }

    fn added(&mut self, id: u64, pid: u32) -> Vec<PlatformEvent> {
        let pid = (pid > 0).then_some(pid);
        self.pids.insert(id, pid);
        let list = self.list();
        self.tracker.update(&list, &BTreeMap::from([(id, pid)]))
    }

    fn removed(&mut self, id: u64) -> Vec<PlatformEvent> {
        self.pids.remove(&id);
        let list = self.list();
        self.tracker.update(&list, &BTreeMap::new())
    }
}

/// Installs or updates the extension, and turns a fresh install on for the next login.
/// Returns whether its files are in place.
fn install_extension() -> bool {
    let Some(folder) = extension::install_folder() else {
        return false;
    };
    match extension::install(&folder) {
        Ok(Installed::Fresh) => {
            if let Err(error) = extension::enable_in_settings() {
                eprintln!("snappy: could not turn on the GNOME Shell extension: {error}");
            }
            true
        }
        Ok(Installed::Updated | Installed::Current) => true,
        Err(error) => {
            eprintln!("snappy: could not install the GNOME Shell extension: {error}");
            folder.join("metadata.json").exists()
        }
    }
}

fn signal_rule(interface: &str, member: &str, path: Option<&str>) -> Result<MatchRule<'static>, zbus::Error> {
    let mut rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(SHELL_BUS_NAME)?
        .interface(interface.to_owned())?
        .member(member.to_owned())?;
    if let Some(path) = path {
        rule = rule.path(path.to_owned())?;
    }
    Ok(rule.build())
}

/// Forwards each matching signal, in order, from a thread of its own. Blocking reads can't be
/// interrupted, so the threads end with the process.
fn listen(
    bus: &Bus,
    rule: MatchRule<'static>,
    sender: Sender<Incoming>,
    translate: impl Fn(&zbus::Message) -> Option<Incoming> + Send + 'static,
) -> Result<(), GnomeError> {
    let messages = MessageIterator::for_match_rule(rule, bus.connection(), None)?;
    std::thread::Builder::new()
        .name("snappy-gnome-signals".into())
        .spawn(move || {
            for message in messages {
                let Ok(message) = message else { continue };
                if let Some(incoming) = translate(&message)
                    && sender.send(incoming).is_err()
                {
                    return;
                }
            }
        })?;
    Ok(())
}
