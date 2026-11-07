//! The event thread: reads X events on the event connection and forwards them to the
//! engine as `PlatformEvent`s. It never moves windows.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::Sender;

use snappy_core::geometry::Point;
use snappy_core::keys::Mods;
use snappy_platform::{PlatformEvent, WindowId};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window};

use crate::clients::ClientTracker;
use crate::error::X11Error;
use crate::input::{HeldModifiers, PRIMARY_BUTTON};
use crate::shared::Shared;
use crate::x::{self, lock};

/// Keysyms of Shift, Control, Caps Lock, Meta, Alt, Super, and Hyper, left and right.
const MODIFIER_KEYSYMS: std::ops::RangeInclusive<u32> = 0xffe1..=0xffee;

pub struct EventLoop {
    shared: Arc<Shared>,
    events: Sender<PlatformEvent>,
    clients: ClientTracker,
    button_down: bool,
    /// Pointer motion seen but not yet reported; consecutive motion is coalesced.
    motion_pending: bool,
    /// Shortcut keys held down, so key repeat doesn't trigger them again.
    held: HashSet<u8>,
    /// Modifier keys followed through the current stretch of drag monitoring.
    modifiers: Option<HeldModifiers>,
}

enum Flow {
    Continue,
    Stop,
}

impl EventLoop {
    pub fn new(shared: Arc<Shared>, events: Sender<PlatformEvent>) -> EventLoop {
        let list = x::window_list(&shared.conn, shared.screen.root, shared.atoms._NET_CLIENT_LIST);
        let clients = ClientTracker::new(list.iter().map(|window| (*window, pid(&shared, *window))));
        EventLoop {
            shared,
            events,
            clients,
            button_down: false,
            motion_pending: false,
            held: HashSet::new(),
            modifiers: None,
        }
    }

    /// Runs until the stop request arrives or the connection fails.
    pub fn run(mut self) -> Result<(), X11Error> {
        loop {
            let event = match self.shared.conn.poll_for_event()? {
                Some(event) => event,
                None => {
                    self.report_motion();
                    self.shared.conn.wait_for_event()?
                }
            };
            if let Flow::Stop = self.handle(event) {
                return Ok(());
            }
        }
    }

    fn emit(&self, event: PlatformEvent) {
        let _ = self.events.send(event);
    }

    fn handle(&mut self, event: Event) -> Flow {
        self.follow_modifiers();
        if !matches!(event, Event::XinputRawMotion(_)) {
            self.report_motion();
        }
        match event {
            Event::XinputRawButtonPress(press) if press.detail == PRIMARY_BUTTON => self.pointer_down(),
            Event::XinputRawButtonRelease(release) if release.detail == PRIMARY_BUTTON => self.pointer_up(),
            Event::XinputRawMotion(_) => {
                self.motion_pending = self.button_down && lock(&self.shared.monitoring).drag;
            }
            Event::XinputRawKeyPress(key) => self.raw_key(key.detail, true),
            Event::XinputRawKeyRelease(key) => self.raw_key(key.detail, false),
            Event::KeyPress(key) => self.hotkey(key.detail, key.state.into()),
            Event::KeyRelease(key) => {
                self.held.remove(&key.detail);
            }
            Event::PropertyNotify(change) if change.window == self.shared.screen.root => {
                self.root_property(change.atom)
            }
            Event::ConfigureNotify(change) => self.moved(change.window),
            Event::MapNotify(map) => self.moved(map.window),
            Event::DestroyNotify(destroyed) => self.shared.untrack(destroyed.window),
            Event::RandrScreenChangeNotify(_) | Event::RandrNotify(_) => {
                self.emit(PlatformEvent::DisplaysChanged)
            }
            Event::MappingNotify(_) => self.shared.refresh_keyboard(),
            Event::ClientMessage(message)
                if message.window == self.shared.control
                    && message.type_ == self.shared.atoms._SNAPPY_STOP =>
            {
                return Flow::Stop;
            }
            _ => {}
        }
        Flow::Continue
    }

    /// Follows modifier keys while drag monitoring is on, starting again from the keys
    /// down whenever a new stretch of it begins.
    fn follow_modifiers(&mut self) {
        let monitoring = *lock(&self.shared.monitoring);
        if !monitoring.drag {
            self.modifiers = None;
            return;
        }
        if self
            .modifiers
            .as_ref()
            .is_some_and(|held| held.stretch == monitoring.drag_stretch)
        {
            return;
        }
        let keymap = self
            .shared
            .conn
            .query_keymap()
            .ok()
            .and_then(|cookie| cookie.reply().ok());
        self.modifiers = keymap.map(|reply| {
            let keyboard = lock(&self.shared.keyboard);
            HeldModifiers::new(monitoring.drag_stretch, &reply.keys, &keyboard.modifier_keys)
        });
    }

    /// The pointer position now, and the modifiers held as of the event being handled:
    /// followed ones while drag monitoring is on, else the server's current state.
    fn pointer(&self) -> Option<(Point, Mods)> {
        let reply = self
            .shared
            .conn
            .query_pointer(self.shared.screen.root)
            .ok()?
            .reply()
            .ok()?;
        let keyboard = lock(&self.shared.keyboard);
        let state = match &self.modifiers {
            Some(held) => held.mask(&keyboard.modifier_keys),
            None => reply.mask.into(),
        };
        let mods = keyboard.masks.mods(state);
        Some((Point::new(f64::from(reply.root_x), f64::from(reply.root_y)), mods))
    }

    fn monitoring_presses(&self) -> bool {
        let monitoring = *lock(&self.shared.monitoring);
        monitoring.press || monitoring.drag
    }

    fn pointer_down(&mut self) {
        if !self.monitoring_presses() {
            return;
        }
        self.button_down = true;
        if let Some((at, mods)) = self.pointer() {
            self.emit(PlatformEvent::PointerDown {
                at,
                mods,
                window: None,
            });
        }
    }

    fn pointer_up(&mut self) {
        if !std::mem::take(&mut self.button_down) {
            return;
        }
        self.motion_pending = false;
        if let Some((at, mods)) = self.pointer() {
            self.emit(PlatformEvent::PointerUp { at, mods });
        }
    }

    fn report_motion(&mut self) {
        if !std::mem::take(&mut self.motion_pending) {
            return;
        }
        if let Some((at, mods)) = self.pointer() {
            self.emit(PlatformEvent::PointerDragged { at, mods });
        }
    }

    /// Modifier changes, observed only during drags.
    fn raw_key(&mut self, keycode: u32, pressed: bool) {
        if !lock(&self.shared.monitoring).drag {
            return;
        }
        let Ok(keycode) = u8::try_from(keycode) else {
            return;
        };
        let modifier = {
            let keyboard = lock(&self.shared.keyboard);
            if let Some(held) = &mut self.modifiers {
                held.key(keycode, pressed, &keyboard.modifier_keys);
            }
            keyboard
                .keymap
                .keysyms(keycode)
                .iter()
                .any(|keysym| MODIFIER_KEYSYMS.contains(keysym))
        };
        if modifier && let Some((at, mods)) = self.pointer() {
            self.emit(PlatformEvent::ModifiersChanged { at, mods });
        }
    }

    /// A grabbed shortcut. Held keys repeat as presses without releases, which are
    /// ignored until the key is released.
    fn hotkey(&mut self, keycode: u8, state: u16) {
        let relevant = lock(&self.shared.keyboard).masks.relevant(state);
        let Some(id) = lock(&self.shared.hotkeys).lookup(keycode, relevant) else {
            return;
        };
        if self.held.insert(keycode) {
            self.emit(PlatformEvent::HotKey(id));
        }
    }

    fn moved(&self, window: Window) {
        if self.shared.is_tracked(window) {
            self.emit(PlatformEvent::WindowMoved(WindowId(u64::from(window))));
        }
    }

    fn root_property(&mut self, atom: Atom) {
        let atoms = &self.shared.atoms;
        if atom == atoms._NET_CLIENT_LIST {
            self.clients_changed();
        } else if atom == atoms._NET_ACTIVE_WINDOW {
            self.focus_changed();
        } else if atom == atoms._NET_CURRENT_DESKTOP {
            self.emit(PlatformEvent::NativeDesktopChanged);
        } else if atom == atoms._NET_WORKAREA || atom == atoms.RESOURCE_MANAGER {
            self.emit(PlatformEvent::DisplaysChanged);
        }
    }

    fn clients_changed(&mut self) {
        let shared = &self.shared;
        let list = x::window_list(&shared.conn, shared.screen.root, shared.atoms._NET_CLIENT_LIST);
        let pids: BTreeMap<Window, Option<u32>> = self
            .clients
            .added(&list)
            .into_iter()
            .map(|window| (window, pid(shared, window)))
            .collect();
        for event in self.clients.update(&list, &pids) {
            if let PlatformEvent::WindowDestroyed(id) = &event
                && let Ok(window) = Window::try_from(id.0)
            {
                self.shared.untrack(window);
            }
            self.emit(event);
        }
    }

    fn focus_changed(&self) {
        let shared = &self.shared;
        let active = x::window_list(&shared.conn, shared.screen.root, shared.atoms._NET_ACTIVE_WINDOW)
            .first()
            .copied()
            .filter(|window| *window != x11rb::NONE);
        if let Some(pid) = active.and_then(|window| pid(shared, window)) {
            self.emit(PlatformEvent::FocusChanged {
                pid,
                by_activation: true,
            });
        }
    }
}

fn pid(shared: &Shared, window: Window) -> Option<u32> {
    x::property32(&shared.conn, window, shared.atoms._NET_WM_PID, AtomEnum::CARDINAL)?
        .first()
        .copied()
        .filter(|pid| *pid > 0)
}
