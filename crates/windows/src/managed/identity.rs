//! Window IDs. Windows reuses HWND values, so an ID pairs the handle with a serial the
//! adapter mints per (handle, process, process start) triple: a reused handle gets a new
//! ID, and the old ID stops resolving.

use std::collections::HashMap;

use snappy_platform::WindowId;

/// The process behind a window, which identifies it across HWND reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Owner {
    pub pid: u32,
    /// Process creation time in 100-nanosecond intervals since 1601.
    pub created: u64,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    serial: u32,
    owner: Owner,
}

/// Every window the adapter has described, by handle.
#[derive(Debug, Default)]
pub struct Registry {
    entries: HashMap<u32, Entry>,
    last_serial: u32,
}

/// The handle an ID was minted for. HWNDs carry 32 significant bits on 64-bit Windows.
pub fn handle_of(id: WindowId) -> u32 {
    id.0 as u32
}

fn serial_of(id: WindowId) -> u32 {
    (id.0 >> 32) as u32
}

fn compose(handle: u32, serial: u32) -> WindowId {
    WindowId((u64::from(serial) << 32) | u64::from(handle))
}

/// What minting an ID found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Minted {
    pub id: WindowId,
    /// No other window of this process was registered, so its events start mattering.
    pub new_process: bool,
}

/// A window that was forgotten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Removed {
    pub id: WindowId,
    pub pid: u32,
    /// No other window of its process is registered.
    pub last_of_process: bool,
}

impl Registry {
    /// The window's ID, minting one when the handle is new or now belongs to another
    /// process launch.
    pub fn mint(&mut self, handle: u32, owner: Owner) -> Minted {
        if let Some(entry) = self.entries.get(&handle)
            && entry.owner == owner
        {
            return Minted {
                id: compose(handle, entry.serial),
                new_process: false,
            };
        }
        let new_process = !self.entries.values().any(|entry| entry.owner.pid == owner.pid);
        self.last_serial = self.last_serial.wrapping_add(1).max(1);
        let serial = self.last_serial;
        self.entries.insert(handle, Entry { serial, owner });
        Minted {
            id: compose(handle, serial),
            new_process,
        }
    }

    /// The current ID of a registered handle.
    pub fn lookup(&self, handle: u32) -> Option<WindowId> {
        self.entries
            .get(&handle)
            .map(|entry| compose(handle, entry.serial))
    }

    /// The owner `id` was minted for, while it is still the handle's current ID.
    pub fn owner(&self, id: WindowId) -> Option<Owner> {
        self.entries
            .get(&handle_of(id))
            .filter(|entry| entry.serial == serial_of(id))
            .map(|entry| entry.owner)
    }

    /// Forgets a destroyed window.
    pub fn remove(&mut self, handle: u32) -> Option<Removed> {
        let entry = self.entries.remove(&handle)?;
        let last_of_process = !self
            .entries
            .values()
            .any(|other| other.owner.pid == entry.owner.pid);
        Some(Removed {
            id: compose(handle, entry.serial),
            pid: entry.owner.pid,
            last_of_process,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTEPAD: Owner = Owner { pid: 40, created: 7 };
    const RELAUNCHED: Owner = Owner { pid: 40, created: 9 };

    #[test]
    fn ids_are_stable_while_the_window_lives() {
        let mut registry = Registry::default();
        let first = registry.mint(0x1234, NOTEPAD);
        assert!(first.new_process);
        let again = registry.mint(0x1234, NOTEPAD);
        assert_eq!(first.id, again.id);
        assert!(!again.new_process);
        assert_eq!(handle_of(first.id), 0x1234);
        assert_eq!(registry.owner(first.id), Some(NOTEPAD));
        assert_eq!(registry.lookup(0x1234), Some(first.id));
    }

    #[test]
    fn a_reused_handle_gets_a_new_id() {
        let mut registry = Registry::default();
        let old = registry.mint(0x1234, NOTEPAD).id;
        let new = registry.mint(0x1234, RELAUNCHED).id;
        assert_ne!(old, new);
        assert_eq!(registry.owner(old), None);
        assert_eq!(registry.owner(new), Some(RELAUNCHED));
    }

    #[test]
    fn removing_reports_the_last_window_of_a_process() {
        let mut registry = Registry::default();
        let a = registry.mint(1, NOTEPAD).id;
        let second = registry.mint(2, NOTEPAD);
        assert!(!second.new_process);
        let first = registry.remove(1).unwrap();
        assert_eq!((first.id, first.pid, first.last_of_process), (a, 40, false));
        let last = registry.remove(2).unwrap();
        assert_eq!((last.id, last.last_of_process), (second.id, true));
        assert_eq!(registry.remove(2), None);
        assert_eq!(registry.owner(a), None);
    }
}
