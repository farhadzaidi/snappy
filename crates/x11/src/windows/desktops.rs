//! The native desktop Snappy parks workspace windows on: one desktop added after the
//! user's own and named "Snappy". Its number is kept in a root window property, so a
//! daemon restarted after a crash finds it and the windows left on it.

use snappy_core::error::WindowError;
use snappy_platform::WindowId;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, PropMode};
use x11rb::wrapper::ConnectionExt as _;

use super::{FROM_PAGER, Windows};
use crate::messages::{NO_PARKING_DESKTOP, WINDOW_GONE, unavailable};
use crate::x;

/// The parking desktop's name in pagers.
pub const NAME: &str = "Snappy";

impl Windows {
    pub fn desktop_count(&self) -> Option<u32> {
        x::cardinal(&*self.conn, self.screen.root, self.atoms._NET_NUMBER_OF_DESKTOPS)
    }

    fn desktop_names(&self) -> Vec<String> {
        x::bytes(
            &*self.conn,
            self.screen.root,
            self.atoms._NET_DESKTOP_NAMES,
            self.atoms.UTF8_STRING,
        )
        .map(|bytes| parse_names(&bytes))
        .unwrap_or_default()
    }

    /// The parking desktop a running or crashed Snappy added in this X session, or a
    /// trailing "Snappy" desktop the window manager kept from an earlier one.
    pub fn find_parking_desktop(&self) -> Option<u32> {
        let recorded = x::cardinal(&*self.conn, self.screen.root, self.atoms._SNAPPY_PARKING_DESKTOP);
        existing(recorded, self.desktop_count()?, &self.desktop_names())
    }

    /// Ask the window manager for one more desktop and name it. Fails when the window
    /// manager has no desktops or doesn't add one.
    pub fn add_parking_desktop(&self) -> Result<u32, WindowError> {
        let count = self
            .desktop_count()
            .ok_or_else(|| unavailable(NO_PARKING_DESKTOP))?;
        self.client_message(
            self.screen.root,
            self.atoms._NET_NUMBER_OF_DESKTOPS,
            [count + 1, 0, 0, 0, 0],
        )?;
        self.conn.flush().map_err(|_| unavailable(NO_PARKING_DESKTOP))?;
        if !self.wait(|| self.desktop_count() == Some(count + 1)) {
            return Err(unavailable(NO_PARKING_DESKTOP));
        }

        if let Some(names) = with_name(self.desktop_names(), count) {
            self.write_names(&names);
        }
        let _ = self.conn.change_property32(
            PropMode::REPLACE,
            self.screen.root,
            self.atoms._SNAPPY_PARKING_DESKTOP,
            AtomEnum::CARDINAL,
            &[count],
        );
        let _ = self.conn.flush();
        Ok(count)
    }

    /// Remove the parking desktop when it is still the last one and no window is on it.
    /// Returns whether it is gone.
    pub fn remove_parking_desktop(&self, desktop: u32) -> bool {
        let occupied = self
            .all_records()
            .iter()
            .any(|record| record.desktop == Some(desktop));
        if occupied || self.desktop_count() != Some(desktop + 1) {
            return false;
        }

        let _ = self.client_message(
            self.screen.root,
            self.atoms._NET_NUMBER_OF_DESKTOPS,
            [desktop, 0, 0, 0, 0],
        );
        let _ = self
            .conn
            .delete_property(self.screen.root, self.atoms._SNAPPY_PARKING_DESKTOP);
        let _ = self.conn.flush();
        let removed = self.wait(|| self.desktop_count() == Some(desktop));

        // Trimmed only once the desktop is gone: some window managers name every desktop
        // that has no name yet.
        let mut names = self.desktop_names();
        if names.get(desktop as usize).map(String::as_str) == Some(NAME) {
            names.truncate(desktop as usize);
            self.write_names(&names);
            let _ = self.conn.flush();
        }
        removed
    }

    fn write_names(&self, names: &[String]) {
        let _ = self.conn.change_property8(
            PropMode::REPLACE,
            self.screen.root,
            self.atoms._NET_DESKTOP_NAMES,
            self.atoms.UTF8_STRING,
            &encode_names(names),
        );
    }

    /// Move a window to `desktop` as a pager would, and wait until the window manager
    /// has. The frame stays where it is.
    pub fn move_to_desktop(&self, id: WindowId, desktop: u32) -> Result<(), WindowError> {
        let record = self.record(id)?;
        if record.desktop == Some(desktop) {
            return Ok(());
        }
        self.client_message(
            record.window,
            self.atoms._NET_WM_DESKTOP,
            [desktop, FROM_PAGER, 0, 0, 0],
        )?;
        self.conn.flush().map_err(|_| unavailable(WINDOW_GONE))?;
        self.wait(|| {
            self.record(id)
                .is_ok_and(|record| record.desktop == Some(desktop))
        });
        Ok(())
    }
}

/// `_NET_DESKTOP_NAMES`: UTF-8 names, each ending in a null byte.
fn parse_names(bytes: &[u8]) -> Vec<String> {
    let mut names: Vec<String> = bytes
        .split(|byte| *byte == 0)
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect();
    if bytes.last() == Some(&0) || bytes.is_empty() {
        names.pop();
    }
    names
}

fn encode_names(names: &[String]) -> Vec<u8> {
    names
        .iter()
        .flat_map(|name| name.bytes().chain(std::iter::once(0)))
        .collect()
}

/// `names` with the parking desktop at `desktop` named, or `None` when an earlier desktop
/// has no name, which writing the list would leave blank in pagers.
fn with_name(mut names: Vec<String>, desktop: u32) -> Option<Vec<String>> {
    let index = desktop as usize;
    if names.len() < index {
        return None;
    }
    names.truncate(index);
    names.push(NAME.to_owned());
    Some(names)
}

/// The recorded parking desktop while it still exists, else a last desktop named "Snappy".
fn existing(recorded: Option<u32>, count: u32, names: &[String]) -> Option<u32> {
    let last = count.checked_sub(1)?;
    recorded
        .filter(|desktop| *desktop < count)
        .or_else(|| (names.get(last as usize).map(String::as_str) == Some(NAME) && last > 0).then_some(last))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn desktop_names_round_trip_as_null_terminated_strings() {
        let bytes = b"Workspace 1\0Workspace 2\0";
        assert_eq!(parse_names(bytes), names(&["Workspace 1", "Workspace 2"]));
        assert_eq!(encode_names(&parse_names(bytes)), bytes.to_vec());
        assert_eq!(parse_names(b"One\0Two"), names(&["One", "Two"]));
        assert!(parse_names(b"").is_empty());
    }

    #[test]
    fn the_parking_desktop_is_named_after_the_users_desktops() {
        assert_eq!(
            with_name(names(&["One", "Two"]), 2),
            Some(names(&["One", "Two", NAME]))
        );
        assert_eq!(
            with_name(names(&["One", "Two", "Workspace 3"]), 2),
            Some(names(&["One", "Two", NAME])),
            "a default name the window manager gave it is replaced"
        );
        assert_eq!(with_name(names(&["One"]), 2), None);
    }

    #[test]
    fn a_restarted_daemon_finds_the_parking_desktop() {
        assert_eq!(existing(Some(4), 5, &[]), Some(4));
        assert_eq!(existing(Some(5), 5, &[]), None, "the desktop was removed");
        assert_eq!(existing(None, 3, &names(&["One", "Two", NAME])), Some(2));
        assert_eq!(existing(None, 3, &names(&["One", "Two", "Three"])), None);
        assert_eq!(
            existing(None, 1, &names(&[NAME])),
            None,
            "the only desktop is the user's"
        );
    }
}
