//! Which modifier keys are down, from KWin's `org_kde_kwin_keystate` Wayland protocol, the
//! one KDE's keyboard indicator uses. It reports only modifier and lock states, never which
//! other keys are pressed. Snappy listens only while a drag runs, and while Caps Lock is the
//! Snappy key, to tell its presses apart; each watch is a connection of its own, closed when
//! it is no longer needed.

use std::io;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::time::Instant;

use snappy_core::keys::{Modifier, Mods};
use snappy_linux::wayland::Connection;

use crate::wire::keystate::{
    FETCH_STATES, INTERFACE, KEY_ALT, KEY_CONTROL, KEY_META, KEY_NUM_LOCK, KEY_SHIFT, STATE_CHANGED,
    UNLOCKED, VERSION,
};

fn modifier(key: u32) -> Option<Modifier> {
    match key {
        KEY_ALT => Some(Modifier::Alt),
        KEY_CONTROL => Some(Modifier::Ctrl),
        KEY_SHIFT => Some(Modifier::Shift),
        KEY_META => Some(Modifier::Meta),
        _ => None,
    }
}

/// Folds `stateChanged` events into the held modifiers. KWin sends every key's state after
/// any change, Num Lock last, so a report is complete when Num Lock's state arrives.
#[derive(Debug, Default)]
pub struct Reports {
    held: Mods,
}

impl Reports {
    /// The modifiers held once a report is complete. Pressed, latched, and locked all count:
    /// a sticky modifier applies to the next action as a held one does.
    pub fn state_changed(&mut self, key: u32, state: u32) -> Option<Mods> {
        if let Some(modifier) = modifier(key) {
            self.held = if state == UNLOCKED {
                self.held.without(Mods::from_iter([modifier]))
            } else {
                self.held.with(modifier)
            };
        }
        (key == KEY_NUM_LOCK).then_some(self.held)
    }
}

/// A running watch; dropping it closes the connection and ends its thread.
pub struct Watch {
    stream: UnixStream,
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

/// Connects to KWin and reports the held modifiers, with when they were read, until the
/// watch is dropped.
pub fn watch(report: impl Fn(Mods, Instant) + Send + 'static) -> io::Result<Watch> {
    let mut connection = Connection::open()?;
    let (keystate, version) = connection.bind(INTERFACE, VERSION)?;
    if version < VERSION {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "KWin is too old to report modifier keys",
        ));
    }
    connection.send(keystate, FETCH_STATES, &[])?;
    let stream = connection.shutdown_handle()?;
    std::thread::Builder::new()
        .name("snappy-kwin-keystate".into())
        .spawn(move || {
            let mut reports = Reports::default();
            while let Ok(message) = connection.read() {
                if message.object != keystate || message.opcode != STATE_CHANGED {
                    continue;
                }
                let mut reader = message.reader();
                if let (Some(key), Some(state)) = (reader.uint(), reader.uint())
                    && let Some(held) = reports.state_changed(key, state)
                {
                    report(held, Instant::now());
                }
            }
        })?;
    Ok(Watch { stream })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::keystate::{KEY_ALT_GR, KEY_CAPS_LOCK, KEY_SCROLL_LOCK, LATCHED, LOCKED, PRESSED};

    /// KWin's order: Scroll Lock, the modifiers, AltGr, Caps Lock, Num Lock.
    fn report(reports: &mut Reports, alt: u32, control: u32, shift: u32, meta: u32) -> Vec<Option<Mods>> {
        [
            (KEY_SCROLL_LOCK, UNLOCKED),
            (KEY_ALT, alt),
            (KEY_SHIFT, shift),
            (KEY_CONTROL, control),
            (KEY_META, meta),
            (KEY_ALT_GR, UNLOCKED),
            (KEY_CAPS_LOCK, LOCKED),
            (KEY_NUM_LOCK, LOCKED),
        ]
        .into_iter()
        .map(|(key, state)| reports.state_changed(key, state))
        .collect()
    }

    #[test]
    fn a_report_is_complete_when_num_lock_arrives() {
        let mut reports = Reports::default();
        let states = report(&mut reports, UNLOCKED, UNLOCKED, PRESSED, UNLOCKED);
        assert!(states[..7].iter().all(Option::is_none));
        assert_eq!(states[7], Some(Mods::from_iter([Modifier::Shift])));
    }

    #[test]
    fn locks_are_not_modifiers_and_latched_keys_count_as_held() {
        let mut reports = Reports::default();
        let states = report(&mut reports, LATCHED, PRESSED, UNLOCKED, PRESSED);
        assert_eq!(
            states[7],
            Some(Mods::from_iter([Modifier::Alt, Modifier::Ctrl, Modifier::Meta]))
        );
        let states = report(&mut reports, UNLOCKED, UNLOCKED, UNLOCKED, UNLOCKED);
        assert_eq!(states[7], Some(Mods::NONE));
    }
}
