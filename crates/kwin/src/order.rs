//! Putting modifier changes in order with the script's pointer events. They come from two
//! connections, Wayland and D-Bus, and the D-Bus path takes longer: a modifier let go right
//! after the button can arrive before the release does. While a drag runs, each modifier
//! change waits a moment before it counts; a release arriving meanwhile came first, and is
//! reported with the modifiers held before the change.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use snappy_core::keys::Mods;

/// Longer than the D-Bus path ever lags the Wayland one, short enough that the overlay's
/// response to a modifier still feels immediate. A modifier let go less than this before the
/// button counts as held for the drop.
pub const REORDER_WINDOW: Duration = Duration::from_millis(30);

#[derive(Debug, Default)]
pub struct HeldModifiers {
    current: Mods,
    /// Changes reported during a drag, with when they arrived, not yet counted.
    waiting: VecDeque<(Instant, Mods)>,
    dragging: bool,
}

impl HeldModifiers {
    pub fn current(&self) -> Mods {
        self.current
    }

    /// A report arriving at `at`. Returns the modifiers when they count straight away and
    /// changed.
    pub fn report(&mut self, mods: Mods, at: Instant) -> Option<Mods> {
        if self.dragging {
            self.waiting.push_back((at, mods));
            return None;
        }
        self.apply(mods)
    }

    /// Counts the waiting changes that are old enough, returning each that changed something.
    pub fn due(&mut self, now: Instant) -> Vec<Mods> {
        let mut changes = Vec::new();
        while let Some(&(at, mods)) = self.waiting.front() {
            if now.duration_since(at) < REORDER_WINDOW {
                break;
            }
            self.waiting.pop_front();
            changes.extend(self.apply(mods));
        }
        changes
    }

    /// When the oldest waiting change will count.
    pub fn next_due(&self) -> Option<Instant> {
        self.waiting.front().map(|(at, _)| *at + REORDER_WINDOW)
    }

    pub fn drag_started(&mut self) {
        self.dragging = true;
    }

    /// The button came up: the modifiers held then. Changes still waiting happened after it.
    /// Reports stop with the drag, so nothing is known about the next one until it starts.
    pub fn drag_ended(&mut self) -> Mods {
        let held = self.current;
        self.dragging = false;
        self.waiting.clear();
        self.current = Mods::NONE;
        held
    }

    fn apply(&mut self, mods: Mods) -> Option<Mods> {
        (std::mem::replace(&mut self.current, mods) != mods).then_some(mods)
    }
}

#[cfg(test)]
mod tests {
    use snappy_core::keys::Modifier;

    use super::*;

    fn shift() -> Mods {
        Mods::from_iter([Modifier::Shift])
    }

    #[test]
    fn outside_a_drag_changes_count_at_once_and_repeats_are_dropped() {
        let mut held = HeldModifiers::default();
        let now = Instant::now();
        assert_eq!(held.report(shift(), now), Some(shift()));
        assert_eq!(held.report(shift(), now), None);
        assert_eq!(held.current(), shift());
    }

    #[test]
    fn during_a_drag_a_change_counts_once_its_window_has_passed() {
        let mut held = HeldModifiers::default();
        let start = Instant::now();
        held.drag_started();
        assert_eq!(held.report(shift(), start), None);
        assert_eq!(held.next_due(), Some(start + REORDER_WINDOW));
        assert!(held.due(start + REORDER_WINDOW / 2).is_empty());
        assert_eq!(held.due(start + REORDER_WINDOW), vec![shift()]);
        assert_eq!(held.current(), shift());
        assert_eq!(held.next_due(), None);
    }

    #[test]
    fn a_modifier_let_go_just_before_the_release_arrives_still_counts_as_held() {
        let mut held = HeldModifiers::default();
        let start = Instant::now();
        held.drag_started();
        held.report(shift(), start);
        held.due(start + REORDER_WINDOW);
        held.report(Mods::NONE, start + REORDER_WINDOW * 2);
        assert_eq!(held.drag_ended(), shift());
        assert_eq!(held.current(), Mods::NONE);
        assert_eq!(held.next_due(), None, "what came after the release is dropped");
    }
}
