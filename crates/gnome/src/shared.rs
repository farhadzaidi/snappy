//! What the engine thread's platform handle and the event thread share: the bus, whether
//! the extension answers, and what the extension has been asked to keep, so it can be asked
//! again when the extension starts over.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, MutexGuard, PoisonError};

use snappy_platform::{LiveControl, PlatformEvent};

use crate::bus::Bus;
use crate::protocol::{Accelerator, MenuEntry, OverlayScene, Request};

pub struct Shared {
    pub bus: Bus,
    /// The extension's files are in place, so GNOME Shell loads it at the next login.
    pub installed: bool,
    state: Mutex<State>,
}

#[derive(Default)]
pub struct State {
    pub live_control: Option<LiveControl>,
    /// The extension started since it was last told what to keep.
    pub replay_needed: bool,
    pub press_monitoring: bool,
    pub drag_monitoring: bool,
    pub registered: Vec<Accelerator>,
    pub menu: Option<(String, Vec<MenuEntry>)>,
    /// What each numbered menu choice stands for.
    pub menu_actions: Vec<PlatformEvent>,
    pub followed: BTreeSet<u64>,
    /// The scene shown, by display, to skip sending one already on screen.
    pub overlay_shown: Option<(String, OverlayScene)>,
    pub overlay_prepared: HashMap<String, OverlayScene>,
    /// Display IDs to window units per point, from the last display read.
    pub scales: HashMap<String, f64>,
}

impl Shared {
    pub fn new(bus: Bus, installed: bool) -> Shared {
        Shared {
            bus,
            installed,
            state: Mutex::new(State::default()),
        }
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
            let mut requests = vec![Request::Monitoring {
                press: state.press_monitoring,
                drag: state.drag_monitoring,
            }];
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
            if let Some((tooltip, entries)) = &state.menu {
                requests.push(Request::Menu {
                    tooltip: tooltip.clone(),
                    entries: entries.clone(),
                });
            }
            requests
        };
        for request in requests {
            let _ = self.bus.send(&request);
        }
    }
}
