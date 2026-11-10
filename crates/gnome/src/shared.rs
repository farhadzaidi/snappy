//! What the engine thread's platform handle and the event thread share: the bus, whether
//! the extension answers, and what the extension has been asked to keep, so it can be asked
//! again when the extension starts over.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, MutexGuard, PoisonError};

use snappy_platform::{LiveControl, PlatformEvent};

use crate::bus::Bus;
use crate::protocol::{Accelerator, MenuEntry, OverlayMarks, OverlayScene, Request};

pub struct Shared {
    pub bus: Bus,
    /// The extension's files are in place, so GNOME Shell loads it at the next login.
    pub installed: bool,
    /// Whether the extension answers, as last checked.
    live_control: Mutex<LiveControl>,
    state: Mutex<State>,
}

#[derive(Default)]
pub struct State {
    /// What to send again when the extension starts over: shortcuts that couldn't be sent
    /// while it was unreachable count too.
    pub replay_needed: bool,
    pub press_monitoring: bool,
    pub drag_monitoring: bool,
    /// The drag key's mask, which keeps a dragged window under the pointer while held.
    pub follow: u32,
    pub registered: Vec<Accelerator>,
    pub menu: Option<(String, Vec<MenuEntry>)>,
    /// Counts menus sent, so choices from a replaced one are told apart.
    pub menu_generation: u32,
    /// What each numbered menu choice stands for.
    pub menu_actions: Vec<PlatformEvent>,
    pub followed: BTreeSet<u64>,
    /// The scene shown, by display, to skip sending one already on screen.
    pub overlay_shown: Option<(String, OverlayScene)>,
    pub overlay_prepared: HashMap<String, OverlayScene>,
    /// The marks each display outlines, sent again when the extension starts over.
    pub marked: HashMap<String, OverlayMarks>,
    /// Display IDs to window units per point, from the last display read.
    pub scales: HashMap<String, f64>,
}

impl State {
    /// What the extension watches. Windows follow the pointer only while Snappy is live,
    /// which is when it watches presses.
    pub fn monitoring(&self) -> Request {
        Request::Monitoring {
            press: self.press_monitoring,
            drag: self.drag_monitoring,
            follow: if self.press_monitoring { self.follow } else { 0 },
        }
    }
}

impl Shared {
    pub fn new(bus: Bus, installed: bool, live_control: LiveControl) -> Shared {
        Shared {
            bus,
            installed,
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

    /// Tells a freshly started extension everything it was asked to keep before. Whatever it
    /// had on screen is gone, so overlays are sent again on next use.
    pub fn replay(&self) {
        let requests = {
            let mut state = self.state();
            state.replay_needed = false;
            state.overlay_shown = None;
            state.overlay_prepared.clear();
            let mut requests = vec![state.monitoring()];
            if !state.registered.is_empty() {
                requests.push(Request::Register {
                    keys: state.registered.clone(),
                });
            }
            if !state.followed.is_empty() {
                requests.push(Request::Follow {
                    ids: state.followed.iter().copied().collect(),
                });
            }
            if let Some((name, entries)) = &state.menu {
                requests.push(Request::Menu {
                    name: name.clone(),
                    generation: state.menu_generation,
                    entries: entries.clone(),
                });
            }
            for (display, marks) in &state.marked {
                requests.push(Request::OverlayMark {
                    display: display.clone(),
                    marks: marks.clone(),
                });
            }
            requests
        };
        for request in requests {
            let _ = self.bus.send(&request);
        }
    }
}
