//! Telling which windows are native tabs of one window.
//!
//! macOS makes each tab a window of its own, but an app's `AXWindows` lists only the
//! selected tab of each group, unless the group is minimized, when every tab is listed as
//! minimized. The selected tab carries the tab bar: an `AXTabGroup` child whose `AXTabs`
//! are the group's tabs. The other tabs' elements keep answering, with the frame they had
//! when they last showed, since macOS updates a tab's frame only when it shows or the group
//! changes. Nothing ties a hidden tab to its group except titles, which Snappy never reads,
//! so a window that may be a hidden tab is matched to a group by what was seen of it:
//!
//! 1. It stays in the group it was last found in while that group still shows a tab.
//! 2. Otherwise it joins the one group whose shown tab has its frame. That holds right
//!    after the user picks another tab, when the tab that hid still has the group's frame.
//! 3. Otherwise, when an app shows one group with room left and the windows not yet
//!    matched exactly fill that room, they are its tabs.
//!
//! A group never takes more hidden tabs than its tab bar holds. Windows on other Spaces,
//! which `AXWindows` leaves out as well, rarely match and stay apart.

use std::collections::{HashMap, HashSet};

use snappy_core::geometry::Rect;
use snappy_platform::WindowId;

use crate::occupancy::same_frame;

/// A listed window with a tab bar of `tabs` tabs: the shown tab of its group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShownTab {
    pub id: WindowId,
    pub frame: Rect,
    pub tabs: usize,
    pub minimized: bool,
}

/// A window that may be a hidden tab: tracked but not listed and not minimized, or listed
/// minimized without a tab bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaybeTab {
    pub id: WindowId,
    pub frame: Rect,
    pub minimized: bool,
}

/// The groups that were last found, so a hidden tab keeps its group after the group moves
/// and its own frame goes stale.
#[derive(Debug, Default)]
pub struct TabMemory {
    group_of: HashMap<WindowId, u64>,
    next: u64,
}

impl TabMemory {
    /// One app's tab groups: each shown tab with the hidden tabs behind it. `alone` are its
    /// listed windows without a tab bar, which belong to no group now.
    pub fn groups(
        &mut self,
        shown: &[ShownTab],
        maybe: &[MaybeTab],
        alone: &[WindowId],
    ) -> Vec<(WindowId, Vec<WindowId>)> {
        for window in alone {
            self.group_of.remove(window);
        }
        let mut room: Vec<usize> = shown.iter().map(|tab| tab.tabs.saturating_sub(1)).collect();
        let mut behind: Vec<Option<usize>> = vec![None; maybe.len()];
        let fits = |lead: &ShownTab, tab: &MaybeTab| lead.minimized == tab.minimized;

        for (index, tab) in maybe.iter().enumerate() {
            let Some(group) = self.group_of.get(&tab.id) else {
                continue;
            };
            let remembered = only(shown.iter().enumerate().filter(|(lead_index, lead)| {
                fits(lead, tab) && room[*lead_index] > 0 && self.group_of.get(&lead.id) == Some(group)
            }));
            if let Some(lead) = remembered {
                behind[index] = Some(lead);
                room[lead] -= 1;
            }
        }

        // More windows with a group's frame than it has room for can't be told apart.
        let framed: Vec<Option<usize>> = maybe
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let unmatched = behind[index].is_none();
                let lead = only(shown.iter().enumerate().filter(|(lead_index, lead)| {
                    fits(lead, tab) && room[*lead_index] > 0 && same_frame(lead.frame, tab.frame)
                }));
                lead.filter(|_| unmatched)
            })
            .collect();
        for (lead, space) in room.iter_mut().enumerate() {
            let matching: Vec<usize> = (0..maybe.len())
                .filter(|index| framed[*index] == Some(lead))
                .collect();
            if matching.len() <= *space {
                *space -= matching.len();
                for index in matching {
                    behind[index] = Some(lead);
                }
            }
        }

        for minimized in [false, true] {
            let left: Vec<usize> = (0..maybe.len())
                .filter(|index| behind[*index].is_none() && maybe[*index].minimized == minimized)
                .collect();
            let open = only(
                shown
                    .iter()
                    .enumerate()
                    .filter(|(lead_index, lead)| lead.minimized == minimized && room[*lead_index] > 0),
            );
            if let Some(lead) = open
                && !left.is_empty()
                && left.len() == room[lead]
            {
                for index in left {
                    behind[index] = Some(lead);
                }
                room[lead] = 0;
            }
        }

        let groups: Vec<(WindowId, Vec<WindowId>)> = shown
            .iter()
            .enumerate()
            .map(|(lead_index, lead)| {
                let hidden = maybe
                    .iter()
                    .zip(&behind)
                    .filter(|(_, group)| **group == Some(lead_index))
                    .map(|(tab, _)| tab.id)
                    .collect();
                (lead.id, hidden)
            })
            .collect();
        self.remember(&groups);
        groups
    }

    /// Give each group one mark, kept from its shown tab or a hidden one where possible.
    fn remember(&mut self, groups: &[(WindowId, Vec<WindowId>)]) {
        let mut taken = HashSet::new();
        for (shown, hidden) in groups {
            let kept = std::iter::once(shown)
                .chain(hidden)
                .filter_map(|window| self.group_of.get(window).copied())
                .find(|group| !taken.contains(group));
            let group = kept.unwrap_or_else(|| {
                self.next += 1;
                self.next
            });
            taken.insert(group);
            for window in std::iter::once(shown).chain(hidden) {
                self.group_of.insert(*window, group);
            }
        }
    }

    /// Forget windows that are gone.
    pub fn retain(&mut self, mut keep: impl FnMut(WindowId) -> bool) {
        self.group_of.retain(|window, _| keep(*window));
    }
}

/// The one index the iterator yields, if it yields exactly one.
fn only<'a, T: 'a>(mut found: impl Iterator<Item = (usize, &'a T)>) -> Option<usize> {
    let (first, _) = found.next()?;
    found.next().is_none().then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: Rect = Rect::new(10.0, 38.0, 1268.0, 1045.0);
    const RIGHT: Rect = Rect::new(2564.0, 1091.0, 1246.0, 1069.0);
    const ELSEWHERE: Rect = Rect::new(300.0, 300.0, 800.0, 600.0);

    fn shown(id: u64, frame: Rect, tabs: usize) -> ShownTab {
        ShownTab {
            id: WindowId(id),
            frame,
            tabs,
            minimized: false,
        }
    }

    fn maybe(id: u64, frame: Rect) -> MaybeTab {
        MaybeTab {
            id: WindowId(id),
            frame,
            minimized: false,
        }
    }

    fn ids(found: &[(WindowId, Vec<WindowId>)]) -> Vec<(u64, Vec<u64>)> {
        found
            .iter()
            .map(|(shown, hidden)| (shown.0, hidden.iter().map(|window| window.0).collect()))
            .collect()
    }

    #[test]
    fn a_hidden_tab_with_its_groups_frame_joins_that_group() {
        let mut memory = TabMemory::default();
        let found = memory.groups(
            &[shown(1, LEFT, 3), shown(2, RIGHT, 2)],
            &[maybe(3, RIGHT), maybe(4, LEFT), maybe(5, LEFT)],
            &[],
        );
        assert_eq!(ids(&found), [(1, vec![4, 5]), (2, vec![3])]);
    }

    #[test]
    fn a_tab_keeps_its_group_after_the_group_moves_and_its_frame_goes_stale() {
        let mut memory = TabMemory::default();
        memory.groups(
            &[shown(1, LEFT, 2), shown(2, RIGHT, 2)],
            &[maybe(3, LEFT), maybe(4, RIGHT)],
            &[],
        );

        let moved = Rect::new(600.0, 200.0, 1268.0, 1045.0);
        let found = memory.groups(
            &[shown(1, moved, 2), shown(2, RIGHT, 2)],
            &[maybe(3, LEFT), maybe(4, RIGHT)],
            &[],
        );
        assert_eq!(ids(&found), [(1, vec![3]), (2, vec![4])]);
    }

    #[test]
    fn picking_another_tab_keeps_the_group_together() {
        let mut memory = TabMemory::default();
        memory.groups(&[shown(1, LEFT, 3)], &[maybe(2, LEFT), maybe(3, LEFT)], &[]);

        let moved = Rect::new(600.0, 200.0, 1268.0, 1045.0);
        let found = memory.groups(&[shown(3, moved, 3)], &[maybe(1, moved), maybe(2, LEFT)], &[]);
        assert_eq!(ids(&found), [(3, vec![1, 2])]);
    }

    #[test]
    fn a_torn_out_tab_leaves_its_group() {
        let mut memory = TabMemory::default();
        memory.groups(&[shown(1, LEFT, 3)], &[maybe(2, LEFT), maybe(3, LEFT)], &[]);

        let found = memory.groups(&[shown(1, LEFT, 2)], &[maybe(2, LEFT)], &[WindowId(3)]);
        assert_eq!(ids(&found), [(1, vec![2])]);
        let merged = memory.groups(
            &[shown(4, RIGHT, 2), shown(1, LEFT, 2)],
            &[maybe(2, LEFT), maybe(3, RIGHT)],
            &[],
        );
        assert_eq!(
            ids(&merged),
            [(4, vec![3]), (1, vec![2])],
            "merged elsewhere, it goes by frame"
        );
    }

    #[test]
    fn a_group_takes_no_more_hidden_tabs_than_its_tab_bar_holds() {
        let mut memory = TabMemory::default();
        let found = memory.groups(&[shown(1, LEFT, 2)], &[maybe(2, LEFT), maybe(3, LEFT)], &[]);
        assert_eq!(
            ids(&found),
            [(1, vec![])],
            "two windows match one place: neither is chosen"
        );
    }

    #[test]
    fn unmatched_windows_that_exactly_fill_the_only_group_with_room_are_its_tabs() {
        let mut memory = TabMemory::default();
        let found = memory.groups(&[shown(1, LEFT, 3)], &[maybe(2, ELSEWHERE), maybe(3, RIGHT)], &[]);
        assert_eq!(ids(&found), [(1, vec![2, 3])]);
    }

    #[test]
    fn a_window_on_another_space_stays_alone() {
        let mut memory = TabMemory::default();
        let found = memory.groups(&[shown(1, LEFT, 3)], &[maybe(2, ELSEWHERE)], &[]);
        assert_eq!(ids(&found), [(1, vec![])], "the group has room for two");
        let without_room = memory.groups(&[shown(1, LEFT, 2)], &[maybe(2, LEFT), maybe(3, ELSEWHERE)], &[]);
        assert_eq!(ids(&without_room), [(1, vec![2])]);
    }

    #[test]
    fn minimized_tabs_join_only_a_minimized_group() {
        let mut memory = TabMemory::default();
        let minimized = |mut tab: MaybeTab| {
            tab.minimized = true;
            tab
        };
        let mut lead = shown(1, LEFT, 2);
        lead.minimized = true;
        let found = memory.groups(
            &[lead, shown(2, LEFT, 2)],
            &[minimized(maybe(3, LEFT)), maybe(4, LEFT)],
            &[],
        );
        assert_eq!(ids(&found), [(1, vec![3]), (2, vec![4])]);
    }
}
