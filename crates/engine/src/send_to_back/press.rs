//! The shortcut's presses: each sends the focused window to the back as it goes down, and
//! one held long enough puts it back as it's let go.

use std::rc::Rc;

use snappy_core::send_to_back::{PressKind, Sent, ShortcutPress};
use snappy_platform::Platform;
use tokio::time::Instant;

use super::sends::{self, Sends, Turn};
use crate::ops::Shared;
use crate::state::EngineState;

/// What the engine keeps for sending windows to the back.
pub(crate) struct SendingToBack {
    /// The press under way, while the shortcut is down.
    pub(super) press: Option<ShortcutPress>,
    pub(super) last: Option<LastSend>,
    /// Sends and put-backs waiting their turn in the operation queue.
    pub(super) sends: Sends,
    /// Presses and releases are timed on the runtime's clock.
    epoch: Instant,
}

/// The last window sent to the back, and what's still to come of it.
#[derive(Debug, Clone, Copy)]
pub(super) struct LastSend {
    pub sent: Sent,
    /// Focus hasn't been reported to change since, so the next press goes by it
    /// (`Sent::next_target`).
    pub focus_unreported: bool,
    /// A hold let go puts it back.
    pub can_put_back: bool,
}

impl LastSend {
    /// `sent`, just done.
    pub fn new(sent: Sent) -> Self {
        Self {
            sent,
            focus_unreported: true,
            can_put_back: true,
        }
    }
}

impl Default for SendingToBack {
    fn default() -> Self {
        Self {
            press: None,
            last: None,
            sends: Sends::default(),
            epoch: Instant::now(),
        }
    }
}

impl SendingToBack {
    /// Seconds from the clock's start to `at`.
    fn seconds(&self, at: Instant) -> f64 {
        at.saturating_duration_since(self.epoch).as_secs_f64()
    }
}

/// The shortcut went down, as the daemon received at `at`, which times the press even when
/// the engine was busy then: the focused window goes to the back. A press still under way
/// missed its release, and counts as a quick one, since how long it was held is unknown.
pub(crate) fn pressed<P: Platform + 'static>(shared: &Rc<Shared<P>>, at: Instant) {
    {
        let state = &mut shared.core.borrow_mut().sending_to_back;
        state.press = Some(ShortcutPress::new(state.seconds(at)));
    }
    sends::queue(shared, Turn::Send);
}

/// The shortcut was let go, as the daemon received at `at`: a hold puts the window its
/// press sent to the back where it was. It's the only shortcut that reports its release.
pub(crate) fn released<P: Platform + 'static>(shared: &Rc<Shared<P>>, at: Instant) {
    let kind = {
        let state = &mut shared.core.borrow_mut().sending_to_back;
        let Some(press) = state.press.take() else {
            return;
        };
        press.release(state.seconds(at))
    };
    if kind == PressKind::Held {
        sends::queue(shared, Turn::PutBack);
    }
}

/// A reported focus change: the window the last press sent back is no longer what focus is
/// reported on, or focus moved on, so the next press sends the focused window.
pub(crate) fn focus_changed<P: Platform>(core: &mut EngineState<P>) {
    if let Some(last) = &mut core.sending_to_back.last {
        last.focus_unreported = false;
    }
}
