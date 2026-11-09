//! Which modifier keys are down, from KWin's `org_kde_kwin_keystate` Wayland protocol, the
//! one KDE's keyboard indicator uses. It reports only modifier and lock states, never which
//! other keys are pressed, and Snappy listens only while a drag runs: the connection opens
//! when drag monitoring starts and closes when it stops.

use std::io;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::time::Instant;

use snappy_core::keys::{Modifier, Mods};
use snappy_linux::wayland::Connection;

const INTERFACE: &str = "org_kde_kwin_keystate";
/// Version 5 added the modifiers; earlier ones report only the locks.
const VERSION: u32 = 5;
const FETCH_STATES: u16 = 0;
const STATE_CHANGED: u16 = 0;

/// The protocol's key numbers.
const KEY_NUM_LOCK: u32 = 1;
const KEY_ALT: u32 = 3;
const KEY_CONTROL: u32 = 4;
const KEY_SHIFT: u32 = 5;
const KEY_META: u32 = 6;
/// The protocol's state for a key that is up and neither latched nor locked.
const UNLOCKED: u32 = 0;

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

    const PRESSED: u32 = 3;
    const LATCHED: u32 = 1;
    const SCROLL_LOCK: u32 = 2;
    const CAPS_LOCK: u32 = 0;
    const ALT_GR: u32 = 7;

    /// KWin's order: Scroll Lock, the modifiers, AltGr, Caps Lock, Num Lock.
    fn report(reports: &mut Reports, alt: u32, control: u32, shift: u32, meta: u32) -> Vec<Option<Mods>> {
        [
            (SCROLL_LOCK, UNLOCKED),
            (KEY_ALT, alt),
            (KEY_SHIFT, shift),
            (KEY_CONTROL, control),
            (KEY_META, meta),
            (ALT_GR, UNLOCKED),
            (CAPS_LOCK, 2),
            (KEY_NUM_LOCK, 2),
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
