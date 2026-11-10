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
    /// The session bus went away; nothing more will arrive.
    BusLost,
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
        let live_control = extension::live_control(&bus, installed);
        let shared = Arc::new(Shared::new(bus, installed, live_control));

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
    /// Fails when the session bus goes away, as the session ending does.
    pub fn run(self) -> Result<(), GnomeError> {
        let mut windows = WindowList::default();
        windows.refresh(&self.shared.bus);
        for incoming in &self.incoming {
            match incoming {
                Incoming::Stop => break,
                Incoming::BusLost => return Err(GnomeError::BusLost),
                Incoming::ExtensionChanged => self.extension_changed(false, &mut windows),
                Incoming::MonitorsChanged => self.emit(PlatformEvent::DisplaysChanged),
                Incoming::NotesChanged => self.emit(PlatformEvent::NotesChanged),
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

    /// Asks again whether the extension answers. A freshly started one holds none of
    /// Snappy's shortcuts, monitoring, followed windows, or menu, and windows may have come
    /// and gone meanwhile, so it is told everything again and the differences are reported
    /// before the engine hears about it.
    fn extension_changed(&self, started: bool, windows: &mut WindowList) {
        let live_control = extension::live_control(&self.shared.bus, self.shared.installed);
        let available = live_control.is_available();
        let changed = self.shared.set_live_control(live_control);
        if !(started || changed) {
            return;
        }
        if available {
            if started {
                self.shared.state().replay_needed = true;
            }
            if self.shared.state().replay_needed {
                self.shared.replay();
            }
            for event in windows.refresh(&self.shared.bus) {
                self.emit(event);
            }
        }
        self.emit(PlatformEvent::LiveControlChanged);
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
            Event::Menu { item, generation } => {
                let state = self.shared.state();
                if generation == state.menu_generation {
                    state
                        .menu_actions
                        .get(item as usize)
                        .cloned()
                        .into_iter()
                        .collect()
                } else {
                    Vec::new()
                }
            }
            Event::Ready | Event::Unknown => Vec::new(),
        };
        for event in translated {
            self.emit(event);
        }
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
    /// and went since, as after the extension started over. Nothing changes when the
    /// extension doesn't answer.
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

/// Forwards each matching signal, in order, from a thread of its own, and reports the bus
/// going away. Blocking reads can't be interrupted, so the threads end with the process.
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
                let Ok(message) = message else { break };
                if let Some(incoming) = translate(&message)
                    && sender.send(incoming).is_err()
                {
                    return;
                }
            }
            let _ = sender.send(Incoming::BusLost);
        })?;
    Ok(())
}
