//! Window identity: each distinct window element gets one `WindowId` for as long as it
//! is tracked. Elements are compared with their own equality (`CFEqual` for AX elements).

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use snappy_platform::WindowId;

/// The registry the engine thread and main-thread observers share.
pub type Shared<E> = Arc<Mutex<Registry<E>>>;

/// Locks a shared registry. A panic elsewhere can't leave it inconsistent, so poisoning
/// is ignored.
pub fn lock<E>(shared: &Mutex<Registry<E>>) -> MutexGuard<'_, Registry<E>> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Clone)]
pub struct Entry<E> {
    pub element: E,
    pub pid: i32,
}

#[derive(Debug)]
pub struct Registry<E> {
    next: u64,
    ids: HashMap<E, WindowId>,
    entries: HashMap<WindowId, Entry<E>>,
}

impl<E> Default for Registry<E> {
    fn default() -> Self {
        Self {
            next: 1,
            ids: HashMap::new(),
            entries: HashMap::new(),
        }
    }
}

/// The ID for an element, and whether it was minted just now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identified {
    pub id: WindowId,
    pub new: bool,
}

impl<E: Eq + Hash + Clone> Registry<E> {
    pub fn identify(&mut self, element: E, pid: i32) -> Identified {
        if let Some(id) = self.ids.get(&element) {
            return Identified { id: *id, new: false };
        }
        let id = WindowId(self.next);
        self.next += 1;
        self.ids.insert(element.clone(), id);
        self.entries.insert(id, Entry { element, pid });
        Identified { id, new: true }
    }

    pub fn get(&self, id: WindowId) -> Option<Entry<E>> {
        self.entries.get(&id).cloned()
    }

    pub fn remove(&mut self, id: WindowId) {
        if let Some(entry) = self.entries.remove(&id) {
            self.ids.remove(&entry.element);
        }
    }

    /// Forget every window of a process.
    pub fn remove_process(&mut self, pid: i32) {
        let gone: Vec<WindowId> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.pid == pid)
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.remove(id);
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_elements_share_one_id() {
        let mut registry = Registry::default();
        let first = registry.identify("a", 1);
        let again = registry.identify("a", 1);
        assert!(first.new);
        assert_eq!(
            again,
            Identified {
                id: first.id,
                new: false
            }
        );
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn distinct_elements_get_distinct_nonzero_ids() {
        let mut registry = Registry::default();
        let a = registry.identify("a", 1).id;
        let b = registry.identify("b", 1).id;
        assert_ne!(a, b);
        assert!(a.0 > 0 && b.0 > 0);
    }

    #[test]
    fn removed_ids_are_never_reused() {
        let mut registry = Registry::default();
        let a = registry.identify("a", 1).id;
        registry.remove(a);
        assert!(registry.get(a).is_none());
        let again = registry.identify("a", 1);
        assert!(again.new);
        assert_ne!(again.id, a);
    }

    #[test]
    fn a_terminated_process_forgets_its_windows() {
        let mut registry = Registry::default();
        let a = registry.identify("a", 1).id;
        let b = registry.identify("b", 2).id;
        registry.remove_process(1);
        assert!(registry.get(a).is_none());
        assert_eq!(registry.get(b).map(|entry| entry.pid), Some(2));
    }
}
