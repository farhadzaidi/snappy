//! The keymap the compositor gives keyboards, read once or followed as it changes.

use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;
use std::time::Duration;

use super::connection::Connection;
use super::message::{Arg, Message};

const SEAT: &str = "wl_seat";
const SEAT_CAPABILITIES: u16 = 0;
const SEAT_GET_KEYBOARD: u16 = 1;
const SEAT_KEYBOARD: u32 = 2;
pub(super) const KEYBOARD_KEYMAP: u16 = 0;
pub(super) const KEYMAP_XKB_V1: u32 = 1;
/// Far more than any keymap; a larger size is taken as a mistake.
const KEYMAP_MAX_BYTES: usize = 4 << 20;

/// The keymap the session's compositor gives keyboards, as XKB keymap text: what each key
/// does in this session, its layouts and options included. Snappy asks for a keyboard
/// without ever having a surface, so the compositor sends it no keys, and the connection
/// closes once the keymap is read.
pub fn keymap() -> io::Result<String> {
    Keymaps::open().map(|(_, keymap)| keymap)
}

/// A keyboard that hears each new keymap the compositor builds, as when the keyboard
/// settings change. It has no surface, so it is sent no keys.
pub struct Keymaps {
    connection: Connection,
    keyboard: u32,
}

impl Keymaps {
    /// The keyboard, and the keymap in use now.
    pub fn open() -> io::Result<(Keymaps, String)> {
        let mut connection = Connection::open()?;
        let (seat, _) = connection.bind(SEAT, 1)?;
        let has_keyboard = connection.roundtrip()?.iter().any(|event| {
            event.object == seat
                && event.opcode == SEAT_CAPABILITIES
                && event
                    .reader()
                    .uint()
                    .is_some_and(|capabilities| capabilities & SEAT_KEYBOARD != 0)
        });
        if !has_keyboard {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the seat has no keyboard",
            ));
        }
        let keyboard = connection.new_id();
        connection.send(seat, SEAT_GET_KEYBOARD, &[Arg::Uint(keyboard)])?;
        let events = connection.roundtrip()?;
        let mut keymaps = Keymaps { connection, keyboard };
        let event = events
            .iter()
            .find(|event| is_keymap(event, keyboard))
            .ok_or_else(|| io::Error::other("the compositor sent no keymap"))?;
        let keymap = keymaps.read(event)?;
        Ok((keymaps, keymap))
    }

    /// The next keymap the compositor sends, waiting up to `timeout` for it.
    pub fn next(&mut self, timeout: Duration) -> io::Result<String> {
        let keyboard = self.keyboard;
        let events = self
            .connection
            .read_until(timeout, |event| is_keymap(event, keyboard))?;
        let event = events
            .last()
            .ok_or_else(|| io::Error::other("the compositor sent no keymap"))?;
        self.read(event)
    }

    /// The text of a keymap event. Its descriptor travels beside it; its format and size are
    /// in the body.
    fn read(&mut self, event: &Message) -> io::Result<String> {
        let mut reader = event.reader();
        let (Some(format), Some(size)) = (reader.uint(), reader.uint()) else {
            return Err(io::Error::other("a keymap event without its size"));
        };
        let fd = self
            .connection
            .take_fd()
            .ok_or_else(|| io::Error::other("a keymap event without its file"))?;
        if format != KEYMAP_XKB_V1 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "a keymap that isn't XKB",
            ));
        }
        read_keymap(&File::from(fd), size as usize)
    }
}

fn is_keymap(event: &Message, keyboard: u32) -> bool {
    event.object == keyboard && event.opcode == KEYBOARD_KEYMAP
}

/// The keymap text in the first `size` bytes of `file`, which end in a zero. Reads at an
/// offset, since the compositor may share one file, and its position, between clients.
pub(super) fn read_keymap(file: &File, size: usize) -> io::Result<String> {
    if size > KEYMAP_MAX_BYTES {
        return Err(io::Error::other("a keymap too large to be one"));
    }
    let mut text = vec![0; size];
    file.read_exact_at(&mut text, 0)?;
    let end = text.iter().position(|byte| *byte == 0).unwrap_or(text.len());
    text.truncate(end);
    String::from_utf8(text).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keymap_is_read_up_to_its_terminating_zero_and_never_past_the_limit() {
        let path = std::env::temp_dir().join(format!("snappy-keymap-limit-{}", std::process::id()));
        std::fs::write(&path, b"abc\0junk").unwrap();
        let file = File::open(&path).unwrap();
        assert_eq!(read_keymap(&file, 8).unwrap(), "abc");
        assert!(read_keymap(&file, KEYMAP_MAX_BYTES + 1).is_err());
        assert!(read_keymap(&file, 64).is_err(), "shorter than it says");
        std::fs::remove_file(&path).unwrap();
    }
}
