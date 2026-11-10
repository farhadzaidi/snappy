//! Turning changes of the managed window list into window and app lifecycle events.

use std::collections::{BTreeMap, BTreeSet};

use snappy_platform::{PlatformEvent, WindowId};

/// The managed clients last seen, with their process IDs.
#[derive(Debug, Default)]
pub struct ClientTracker {
    clients: BTreeMap<u64, Option<u32>>,
}

impl ClientTracker {
    /// Starts from the current list without reporting it.
    pub fn new(clients: impl IntoIterator<Item = (u64, Option<u32>)>) -> ClientTracker {
        ClientTracker {
            clients: clients.into_iter().collect(),
        }
    }

    /// The windows known now.
    pub fn windows(&self) -> Vec<u64> {
        self.clients.keys().copied().collect()
    }

    /// Windows in `list` not seen before; their process IDs are needed for `update`.
    pub fn added(&self, list: &[u64]) -> Vec<u64> {
        list.iter()
            .copied()
            .filter(|window| !self.clients.contains_key(window))
            .collect()
    }

    /// Records the new list. `pids` names the process of each added window.
    pub fn update(&mut self, list: &[u64], pids: &BTreeMap<u64, Option<u32>>) -> Vec<PlatformEvent> {
        let before = self.running();
        let current: BTreeSet<u64> = list.iter().copied().collect();
        let removed: Vec<u64> = self
            .clients
            .keys()
            .copied()
            .filter(|window| !current.contains(window))
            .collect();

        let mut events: Vec<PlatformEvent> = removed
            .iter()
            .map(|window| PlatformEvent::WindowDestroyed(WindowId(*window)))
            .collect();
        for window in &removed {
            self.clients.remove(window);
        }

        let added = self.added(list);
        for window in &added {
            let pid = pids.get(window).copied().flatten();
            self.clients.insert(*window, pid);
        }

        let after = self.running();
        events.extend(
            after
                .difference(&before)
                .map(|pid| PlatformEvent::AppLaunched { pid: *pid }),
        );
        events.extend(
            added
                .iter()
                .filter_map(|window| self.clients.get(window).copied().flatten())
                .map(|pid| PlatformEvent::WindowCreated { pid }),
        );
        events.extend(
            before
                .difference(&after)
                .map(|pid| PlatformEvent::AppTerminated { pid: *pid }),
        );
        events
    }

    fn running(&self) -> BTreeSet<u32> {
        self.clients.values().filter_map(|pid| *pid).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pids(entries: &[(u64, Option<u32>)]) -> BTreeMap<u64, Option<u32>> {
        entries.iter().copied().collect()
    }

    #[test]
    fn a_new_process_launches_then_creates_its_window() {
        let mut tracker = ClientTracker::new([(1, Some(100))]);
        let events = tracker.update(&[1, 2], &pids(&[(2, Some(200))]));
        assert_eq!(
            events,
            vec![
                PlatformEvent::AppLaunched { pid: 200 },
                PlatformEvent::WindowCreated { pid: 200 },
            ]
        );
    }

    #[test]
    fn a_second_window_of_a_running_process_is_only_created() {
        let mut tracker = ClientTracker::new([(1, Some(100))]);
        let events = tracker.update(&[1, 2], &pids(&[(2, Some(100))]));
        assert_eq!(events, vec![PlatformEvent::WindowCreated { pid: 100 }]);
    }

    #[test]
    fn the_last_window_closing_terminates_its_process() {
        let mut tracker = ClientTracker::new([(1, Some(100)), (2, Some(100)), (3, Some(300))]);
        assert_eq!(
            tracker.update(&[2, 3], &BTreeMap::new()),
            vec![PlatformEvent::WindowDestroyed(WindowId(1))]
        );
        assert_eq!(
            tracker.update(&[3], &BTreeMap::new()),
            vec![
                PlatformEvent::WindowDestroyed(WindowId(2)),
                PlatformEvent::AppTerminated { pid: 100 },
            ]
        );
    }

    #[test]
    fn windows_without_a_process_id_only_report_destruction() {
        let mut tracker = ClientTracker::new([]);
        assert!(tracker.update(&[5], &pids(&[(5, None)])).is_empty());
        assert_eq!(
            tracker.update(&[], &BTreeMap::new()),
            vec![PlatformEvent::WindowDestroyed(WindowId(5))]
        );
    }

    #[test]
    fn added_lists_only_unseen_windows() {
        let tracker = ClientTracker::new([(1, Some(100))]);
        assert_eq!(tracker.added(&[1, 4, 5]), vec![4, 5]);
    }
}
