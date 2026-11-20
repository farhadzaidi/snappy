//! Putting back: a press held long enough shows what's behind the focused window while it's
//! held, a peek, since the press sent it to the back, and letting go puts it back where it
//! was, focused. Putting it back takes its turn after the press's own send, in the send
//! queue.
//!
//! Anything else that acts on the windows meanwhile cancels that (`cancel_put_back`), so the
//! window stays at the back: any other operation, which goes through `Shared::submit`, the
//! events `cancels_put_back` lists, and the shortcuts registered again, as a release under
//! way may then never come.

use snappy_core::send_to_back::Restacking;
use snappy_core::shortcuts::HotKeyAction;
use snappy_platform::{Platform, PlatformEvent};

use super::press::LastSend;
use crate::log_event::LogEvent;
use crate::state::EngineState;

/// Put the window the last press sent to the back where it was: focused, which brings it to
/// the front, then back under the window that was in front of it, if one in front of it
/// overlapped it, where the platform restacks windows that way. Elsewhere it stays in
/// front, where a focused window is anyway. The window focused while it was held is
/// reported to keep focus for a moment after, so a press meanwhile sends the window put back
/// (`Sent::put_back`).
pub(super) fn put_back<P: Platform>(core: &mut EngineState<P>) {
    let Some(last) = core.sending_to_back.last.filter(|last| last.can_put_back) else {
        return;
    };
    let sent = last.sent;
    core.sending_to_back.last = Some(LastSend {
        can_put_back: false,
        ..LastSend::new(sent.put_back())
    });
    core.platform.focus(sent.window);
    let stacks_below = core.capabilities.restacking == Restacking::Lower { stack_below: true };
    let restacked = sent
        .under
        .filter(|_| stacks_below)
        .map(|under| core.platform.stack_below(sent.window, under).is_ok());
    core.trace(LogEvent::PutBack, || {
        serde_json::json!({
            "window": sent.window.0,
            "under": sent.under.map(|under| under.0),
            "restacked": restacked,
        })
    });
}

/// Something else acts on the windows: a hold under way, or let go but not yet put back,
/// leaves its window at the back.
pub(crate) fn cancel_put_back<P: Platform>(core: &mut EngineState<P>) {
    let state = &mut core.sending_to_back;
    state.press = None;
    if let Some(last) = &mut state.last {
        last.can_put_back = false;
    }
    state.sends.cancel_put_back();
}

/// Whether `event` leaves a hold's window at the back: a click, which may start a drag and
/// gives focus to what it presses, another shortcut, or a change to where windows are or to
/// Snappy's control of the desktop, after which putting the window back would pull the user
/// away.
pub(crate) fn cancels_put_back<P: Platform>(event: &PlatformEvent, core: &EngineState<P>) -> bool {
    match event {
        PlatformEvent::HotKey(id) => core
            .hotkey(*id)
            .is_some_and(|hotkey| hotkey.action != HotKeyAction::SendToBack),
        PlatformEvent::PointerDown { .. }
        | PlatformEvent::DisplaysChanged
        | PlatformEvent::NativeDesktopChanged
        | PlatformEvent::PermissionChanged
        | PlatformEvent::LiveControlChanged
        | PlatformEvent::WillSleep
        | PlatformEvent::Tray(_) => true,
        _ => false,
    }
}
