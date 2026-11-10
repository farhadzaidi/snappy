//! What the engine thread's platform handle and the event loop share: the bus, the overlay,
//! whether the script answers, and what the script has been asked to keep, so it can be
//! asked again when it starts over.

use std::collections::{BTreeSet, HashMap};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use snappy_core::keys::Mods;
use snappy_linux::journal;
use snappy_platform::{LiveControl, OverlayScene, TrayMenu};

use crate::bus::Bus;
use crate::keystate::{self, Watch};
use crate::overlay::{Overlays, Target};
use crate::protocol::{Event, Request};

/// What the event loop hears about, from every thread that listens.
pub enum Incoming {
    Event(Event),
    /// The held modifiers, with when they were read, during a drag.
    Modifiers(Mods, Instant),
    /// One of Snappy's shortcuts, by the action KDE knows it as.
    Shortcut(String),
    /// KWin started, stopped, or started over.
    KWinChanged,
    Tray(TrayMenu),
    /// The session bus went away; nothing more will arrive.
    BusLost,
    Stop,
}

/// A shortcut Snappy holds with KDE's shortcut service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grab {
    pub id: u32,
    pub key: i32,
    pub action: String,
}

pub struct Shared {
    pub bus: Bus,
    pub overlays: Overlays,
    incoming: Sender<Incoming>,
    /// Whether the script answers, as last checked.
    live_control: Mutex<LiveControl>,
    state: Mutex<State>,
}

#[derive(Default)]
pub struct State {
    pub press_monitoring: bool,
    pub drag_monitoring: bool,
    pub registered: Vec<Grab>,
    pub followed: BTreeSet<u64>,
    /// The scene shown, by display, to skip drawing one already on screen.
    pub overlay_shown: Option<(String, OverlayScene)>,
    /// Where each display's overlay goes, from the last display read.
    pub targets: HashMap<String, Target>,
    /// Follows the modifier keys while drag monitoring is on.
    pub modifiers: Option<Watch>,
}

impl State {
    pub fn monitoring(&self) -> Request {
        Request::Monitoring {
            press: self.press_monitoring,
            drag: self.drag_monitoring,
        }
    }
}

impl Shared {
    pub fn new(
        bus: Bus,
        overlays: Overlays,
        incoming: Sender<Incoming>,
        live_control: LiveControl,
    ) -> Shared {
        Shared {
            bus,
            overlays,
            incoming,
            live_control: Mutex::new(live_control),
            state: Mutex::new(State::default()),
        }
    }

    pub fn live_control(&self) -> LiveControl {
        self.live_control
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Records the new answer; returns whether it differs from the last one.
    pub fn set_live_control(&self, live_control: LiveControl) -> bool {
        let mut current = self.live_control.lock().unwrap_or_else(PoisonError::into_inner);
        let changed = *current != live_control;
        *current = live_control;
        changed
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn send_incoming(&self, incoming: Incoming) {
        let _ = self.incoming.send(incoming);
    }

    /// Starts or stops following the modifier keys. A failure leaves drags without
    /// modifiers, which the daemon's output explains.
    pub fn follow_modifiers(&self, on: bool) {
        let mut state = self.state();
        if !on {
            state.modifiers = None;
            return;
        }
        if state.modifiers.is_some() {
            return;
        }
        let incoming = self.incoming.clone();
        match keystate::watch(move |mods, at| {
            let _ = incoming.send(Incoming::Modifiers(mods, at));
        }) {
            Ok(watch) => state.modifiers = Some(watch),
            Err(error) => journal::warn(format_args!("can't read modifier keys from KWin: {error}")),
        }
    }

    /// Tells a freshly started script everything it was asked to keep before.
    pub fn replay(&self) {
        let requests = {
            let state = self.state();
            let mut requests = vec![state.monitoring()];
            if !state.followed.is_empty() {
                requests.push(Request::Follow {
                    ids: state.followed.iter().copied().collect(),
                });
            }
            requests
        };
        for request in requests {
            let _ = self.bus.send(&request);
        }
    }
}
