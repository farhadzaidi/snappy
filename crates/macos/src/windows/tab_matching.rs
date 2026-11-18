//! Telling which windows are native tabs of one window.
//!
//! macOS makes each tab a window of its own, but an app's `AXWindows` lists only the
//! selected tab of each group, unless the group is minimized, when every tab is listed as
//! minimized. The selected tab carries the tab bar: an `AXTabGroup` child whose `AXTabs`
//! are the group's tabs. Hidden tabs have no tab bar, and keep answering with the frame they
//! had when they last showed, which macOS updates only when they show or the group gains a
//! tab. Nothing ties a hidden tab to its group except titles, which Snappy never reads.
//!
//! Windows on another Space leave `AXWindows` too, and the window server lists them off
//! screen just as it lists hidden tabs; no public API tells the two apart. A window wrongly
//! taken for a hidden tab can't be focused and moves with another window, while a missed tab
//! only moves on its own, so a window counts as a hidden tab only on positive evidence:
//!
//! 1. It stays in the group it was found in while that group shows a tab with room for it.
//! 2. Otherwise Snappy must have seen it go: listed earlier in this visit to the Spaces
//!    showing now, it went out of sight where exactly one group now shows, with that
//!    group's frame or off by no more than a tab bar's height.
//!
//! A group never takes more hidden tabs than its tab bar holds; when more windows match it
//! than it has room for, they can't be told apart, and none of them joins.

use std::collections::{HashMap, HashSet};

use snappy_core::geometry::Rect;
use snappy_platform::{HiddenTab, TabEvidence, WindowId};

use super::occupancy::same_frame;
use crate::thresholds::{TAB_BAR_HEIGHT_LIMIT, WINDOW_LIST_MATCH};

/// A listed window with a tab bar of `tabs` tabs: the shown tab of its group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShownTab {
    pub id: WindowId,
    pub frame: Rect,
    pub tabs: usize,
    pub minimized: bool,
}

/// A window without a tab bar of its own that may be a hidden tab: listed minimized, as
/// every tab of a minimized group is, or else not listed at all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaybeTab {
    pub id: WindowId,
    pub frame: Rect,
    pub minimized: bool,
}

/// What was learned of one app's tab groups: each shown tab, with the hidden tabs found
/// behind it.
pub type Groups = Vec<(WindowId, Vec<HiddenTab>)>;

/// What Snappy has seen of the tab groups of one app's windows, across queries.
#[derive(Debug, Default)]
pub struct TabMemory {
    /// The group each window was last found in, by a mark the group keeps while its shown
    /// tab changes and its hidden tabs' frames go stale.
    group_of: HashMap<WindowId, u64>,
    next: u64,
    /// The visit to the Spaces in which each window was last listed.
    seen: HashMap<WindowId, u64>,
}

impl TabMemory {
    /// Note that `windows` are listed during `visit`.
    pub fn saw(&mut self, windows: &[WindowId], visit: u64) {
        for window in windows {
            self.seen.insert(*window, visit);
        }
    }

    /// Whether `window` is in a group Snappy found.
    pub fn remembers(&self, window: WindowId) -> bool {
        self.group_of.contains_key(&window)
    }

    /// Whether `window`, gone from its app's list during `visit`, could be a hidden tab at
    /// all: it was found in a group, or Snappy saw it go. Any other window may be on another
    /// Space, so it isn't worth reading.
    pub fn may_be_hidden(&self, window: WindowId, visit: u64) -> bool {
        self.remembers(window) || self.seen.get(&window) == Some(&visit)
    }

    /// One app's tab groups during `visit`: each shown tab with the hidden tabs behind it.
    /// `alone` are its listed windows without a tab bar, which belong to no group now.
    pub fn groups(
        &mut self,
        visit: u64,
        shown: &[ShownTab],
        maybe: &[MaybeTab],
        alone: &[WindowId],
    ) -> Groups {
        let listed: Vec<WindowId> = shown
            .iter()
            .map(|tab| tab.id)
            .chain(alone.iter().copied())
            .chain(maybe.iter().filter(|tab| tab.minimized).map(|tab| tab.id))
            .collect();
        self.saw(&listed, visit);
        for window in alone {
            self.group_of.remove(window);
        }
        let mut matching = Matching::new(shown, maybe);
        matching.by_memory(&self.group_of);
        matching.by_place(|tab| self.seen.get(&tab.id) == Some(&visit));
        let groups = matching.groups();
        self.remember(&groups);
        groups
    }

    /// Give each group one mark, kept from its shown tab or a hidden one where possible.
    fn remember(&mut self, groups: &Groups) {
        let mut taken = HashSet::new();
        for (shown, hidden) in groups {
            let members = || std::iter::once(*shown).chain(hidden.iter().map(|tab| tab.window));
            let kept = members()
                .filter_map(|window| self.group_of.get(&window).copied())
                .find(|group| !taken.contains(group));
            let group = kept.unwrap_or_else(|| {
                self.next += 1;
                self.next
            });
            taken.insert(group);
            for window in members() {
                self.group_of.insert(window, group);
            }
        }
    }

    /// Forget windows that are gone.
    pub fn retain(&mut self, mut keep: impl FnMut(WindowId) -> bool) {
        self.group_of.retain(|window, _| keep(*window));
        self.seen.retain(|window, _| keep(*window));
    }
}

/// Hidden tabs being matched to the shown tabs of one app, by index into each.
struct Matching<'a> {
    shown: &'a [ShownTab],
    maybe: &'a [MaybeTab],
    /// How many more hidden tabs each group can take.
    room: Vec<usize>,
    /// The group each window that may be a hidden tab sits behind, once matched, and why.
    behind: Vec<Option<(usize, TabEvidence)>>,
}

impl<'a> Matching<'a> {
    fn new(shown: &'a [ShownTab], maybe: &'a [MaybeTab]) -> Self {
        Self {
            shown,
            maybe,
            room: shown.iter().map(|tab| tab.tabs.saturating_sub(1)).collect(),
            behind: vec![None; maybe.len()],
        }
    }

    /// Windows found in a group before stay in it while its shown tab carries the group's
    /// mark. A mark two shown tabs carry, as after a tab is dragged into another group, no
    /// longer tells which group a window is in.
    fn by_memory(&mut self, group_of: &HashMap<WindowId, u64>) {
        let marks: Vec<Option<u64>> = self
            .shown
            .iter()
            .map(|tab| group_of.get(&tab.id).copied())
            .collect();
        for (group, shown) in self.shown.iter().enumerate() {
            let Some(mark) = marks[group] else { continue };
            if marks.iter().filter(|other| **other == Some(mark)).count() > 1 {
                continue;
            }
            let remembered: Vec<(usize, TabEvidence)> = (0..self.maybe.len())
                .filter(|index| {
                    let tab = &self.maybe[*index];
                    self.behind[*index].is_none()
                        && tab.minimized == shown.minimized
                        && group_of.get(&tab.id) == Some(&mark)
                })
                .map(|index| (index, TabEvidence::Remembered))
                .collect();
            self.join(group, &remembered);
        }
    }

    /// A window Snappy saw go joins the one group with room that shows where it went out of
    /// sight.
    fn by_place(&mut self, seen_going: impl Fn(&MaybeTab) -> bool) {
        let mut found: Vec<Vec<(usize, TabEvidence)>> = vec![Vec::new(); self.shown.len()];
        for (index, tab) in self.maybe.iter().enumerate() {
            if self.behind[index].is_some() || !seen_going(tab) {
                continue;
            }
            let places: Vec<(usize, TabEvidence)> = self
                .shown
                .iter()
                .enumerate()
                .filter(|(group, shown)| shown.minimized == tab.minimized && self.room[*group] > 0)
                .filter_map(|(group, shown)| place(shown.frame, tab.frame).map(|evidence| (group, evidence)))
                .collect();
            if let [(group, evidence)] = places[..] {
                found[group].push((index, evidence));
            }
        }
        for (group, candidates) in found.iter().enumerate() {
            self.join(group, candidates);
        }
    }

    /// `candidates`, by index with their evidence, join `group` when it has room for all of
    /// them. Otherwise none does: they can't be told apart, and a missed tab does less harm
    /// than a window wrongly hidden.
    fn join(&mut self, group: usize, candidates: &[(usize, TabEvidence)]) {
        if candidates.is_empty() || candidates.len() > self.room[group] {
            return;
        }
        self.room[group] -= candidates.len();
        for (index, evidence) in candidates {
            self.behind[*index] = Some((group, *evidence));
        }
    }

    /// Each shown tab with the hidden tabs matched to it.
    fn groups(self) -> Groups {
        self.shown
            .iter()
            .enumerate()
            .map(|(group, shown)| {
                let hidden = self
                    .maybe
                    .iter()
                    .zip(&self.behind)
                    .filter_map(|(tab, behind)| match behind {
                        Some((matched, evidence)) if *matched == group => Some(HiddenTab {
                            window: tab.id,
                            evidence: *evidence,
                        }),
                        _ => None,
                    })
                    .collect();
                (shown.id, hidden)
            })
            .collect()
    }
}

/// How a window that went out of sight at `hidden` sits against a group shown at `shown`:
/// in its place exactly, or with the same left edge and width and its top and bottom off by
/// no more than a tab bar's height. `None` when it's elsewhere.
fn place(shown: Rect, hidden: Rect) -> Option<TabEvidence> {
    if same_frame(shown, hidden) {
        return Some(TabEvidence::SameFrame);
    }
    let close = |a: f64, b: f64, limit: f64| (a - b).abs() <= limit;
    let near = close(shown.x, hidden.x, WINDOW_LIST_MATCH)
        && close(shown.width, hidden.width, WINDOW_LIST_MATCH)
        && close(shown.y, hidden.y, TAB_BAR_HEIGHT_LIMIT)
        && close(shown.max_y(), hidden.max_y(), TAB_BAR_HEIGHT_LIMIT);
    near.then_some(TabEvidence::NearFrame)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Terminal windows in a grid on a 2560-point-wide display, as sized by Terminal's
    /// character cells, and one in the next display's corner.
    const TOP_LEFT: Rect = Rect::new(12.0, 37.0, 1268.0, 1045.0);
    const TOP_RIGHT: Rect = Rect::new(1288.0, 37.0, 1257.0, 1045.0);
    const BOTTOM_RIGHT: Rect = Rect::new(1288.0, 1099.0, 1268.0, 1045.0);
    const NEXT_DISPLAY: Rect = Rect::new(2564.0, 1091.0, 1246.0, 1069.0);
    const ELSEWHERE: Rect = Rect::new(300.0, 300.0, 800.0, 600.0);

    /// The visit to the Spaces the tests mostly run in, and one before it.
    const NOW: u64 = 7;
    const BEFORE: u64 = 6;

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

    fn minimized(mut tab: MaybeTab) -> MaybeTab {
        tab.minimized = true;
        tab
    }

    fn windows(ids: &[u64]) -> Vec<WindowId> {
        ids.iter().copied().map(WindowId).collect()
    }

    /// Memory that saw `ids` listed during `visit`.
    fn saw(ids: &[u64], visit: u64) -> TabMemory {
        let mut memory = TabMemory::default();
        memory.saw(&windows(ids), visit);
        memory
    }

    fn ids(found: &Groups) -> Vec<(u64, Vec<u64>)> {
        found
            .iter()
            .map(|(shown, hidden)| (shown.0, hidden.iter().map(|tab| tab.window.0).collect()))
            .collect()
    }

    fn evidence(found: &Groups) -> Vec<(u64, TabEvidence)> {
        found
            .iter()
            .flat_map(|(_, hidden)| hidden.iter().map(|tab| (tab.window.0, tab.evidence)))
            .collect()
    }

    #[test]
    fn a_hidden_tab_seen_go_with_its_groups_frame_joins_that_group() {
        let mut memory = saw(&[3, 4, 5], NOW);
        let found = memory.groups(
            NOW,
            &[shown(1, TOP_LEFT, 3), shown(2, NEXT_DISPLAY, 2)],
            &[maybe(3, NEXT_DISPLAY), maybe(4, TOP_LEFT), maybe(5, TOP_LEFT)],
            &[],
        );
        assert_eq!(ids(&found), [(1, vec![4, 5]), (2, vec![3])]);
        assert!(
            evidence(&found)
                .iter()
                .all(|(_, evidence)| *evidence == TabEvidence::SameFrame)
        );
    }

    #[test]
    fn the_window_a_first_new_tab_hides_joins_though_the_tab_bar_made_the_group_taller() {
        let single = Rect::new(100.0, 311.0, 585.0, 399.0);
        let mut memory = saw(&[1], NOW);
        let found = memory.groups(
            NOW,
            &[shown(2, Rect::new(100.0, 311.0, 585.0, 427.0), 2)],
            &[maybe(1, single)],
            &[],
        );
        assert_eq!(evidence(&found), [(1, TabEvidence::NearFrame)]);
    }

    #[test]
    fn a_new_tab_that_pushes_the_group_up_keeps_every_tab() {
        let mut memory = saw(&[1], NOW);
        let low = Rect::new(100.0, 311.0, 585.0, 427.0);
        memory.groups(
            NOW,
            &[shown(2, low, 2)],
            &[maybe(1, Rect::new(100.0, 311.0, 585.0, 399.0))],
            &[],
        );

        let pushed_up = Rect::new(100.0, 284.0, 585.0, 427.0);
        let found = memory.groups(
            NOW,
            &[shown(3, pushed_up, 3)],
            &[maybe(1, low), maybe(2, low)],
            &[],
        );
        assert_eq!(ids(&found), [(3, vec![1, 2])]);
    }

    #[test]
    fn a_tab_keeps_its_group_after_the_group_moves_and_its_frame_goes_stale() {
        let mut memory = saw(&[3, 4], NOW);
        memory.groups(
            NOW,
            &[shown(1, TOP_LEFT, 2), shown(2, NEXT_DISPLAY, 2)],
            &[maybe(3, TOP_LEFT), maybe(4, NEXT_DISPLAY)],
            &[],
        );

        let moved = Rect::new(600.0, 200.0, 1268.0, 1045.0);
        let found = memory.groups(
            NOW + 1,
            &[shown(1, moved, 2), shown(2, NEXT_DISPLAY, 2)],
            &[maybe(3, TOP_LEFT), maybe(4, NEXT_DISPLAY)],
            &[],
        );
        assert_eq!(ids(&found), [(1, vec![3]), (2, vec![4])]);
        assert_eq!(
            evidence(&found),
            [(3, TabEvidence::Remembered), (4, TabEvidence::Remembered)]
        );
    }

    #[test]
    fn picking_another_tab_keeps_the_group_together() {
        let mut memory = saw(&[2, 3], NOW);
        memory.groups(
            NOW,
            &[shown(1, TOP_LEFT, 3)],
            &[maybe(2, TOP_LEFT), maybe(3, TOP_LEFT)],
            &[],
        );

        let moved = Rect::new(600.0, 200.0, 1268.0, 1045.0);
        let found = memory.groups(
            NOW,
            &[shown(3, moved, 3)],
            &[maybe(1, moved), maybe(2, TOP_LEFT)],
            &[],
        );
        assert_eq!(ids(&found), [(3, vec![1, 2])]);
    }

    #[test]
    fn a_torn_out_tab_leaves_its_group() {
        let mut memory = saw(&[2, 3], NOW);
        memory.groups(
            NOW,
            &[shown(1, TOP_LEFT, 3)],
            &[maybe(2, TOP_LEFT), maybe(3, TOP_LEFT)],
            &[],
        );

        let found = memory.groups(
            NOW,
            &[shown(1, TOP_LEFT, 2)],
            &[maybe(2, TOP_LEFT)],
            &windows(&[3]),
        );
        assert_eq!(ids(&found), [(1, vec![2])]);
        assert!(!memory.remembers(WindowId(3)));
        let merged = memory.groups(
            NOW,
            &[shown(4, NEXT_DISPLAY, 2), shown(1, TOP_LEFT, 2)],
            &[maybe(2, TOP_LEFT), maybe(3, NEXT_DISPLAY)],
            &[],
        );
        assert_eq!(
            ids(&merged),
            [(4, vec![3]), (1, vec![2])],
            "merged elsewhere, it goes by its new place"
        );
    }

    #[test]
    fn a_group_whose_tab_bar_goes_away_lets_every_tab_go() {
        let mut memory = saw(&[2], NOW);
        memory.groups(NOW, &[shown(1, TOP_LEFT, 2)], &[maybe(2, TOP_LEFT)], &[]);

        let found = memory.groups(NOW, &[], &[maybe(2, TOP_LEFT)], &windows(&[1]));
        assert!(found.is_empty());
        assert!(!memory.remembers(WindowId(1)));

        let later = memory.groups(NOW, &[shown(5, TOP_RIGHT, 2)], &[maybe(2, TOP_LEFT)], &[]);
        assert_eq!(
            ids(&later),
            [(5, vec![])],
            "a group elsewhere doesn't take it either"
        );
    }

    #[test]
    fn a_group_takes_no_more_hidden_tabs_than_its_tab_bar_holds() {
        let mut memory = saw(&[2, 3], NOW);
        let found = memory.groups(
            NOW,
            &[shown(1, TOP_LEFT, 2)],
            &[maybe(2, TOP_LEFT), maybe(3, TOP_LEFT)],
            &[],
        );
        assert_eq!(
            ids(&found),
            [(1, vec![])],
            "two windows match one place: neither is chosen"
        );
    }

    #[test]
    fn remembered_tabs_a_group_no_longer_has_room_for_stay_apart() {
        let mut memory = saw(&[2, 3], NOW);
        memory.groups(
            NOW,
            &[shown(1, TOP_LEFT, 3)],
            &[maybe(2, TOP_LEFT), maybe(3, TOP_LEFT)],
            &[],
        );

        let moved = Rect::new(600.0, 200.0, 1268.0, 1045.0);
        let found = memory.groups(
            NOW + 1,
            &[shown(1, moved, 2)],
            &[maybe(2, TOP_LEFT), maybe(3, TOP_LEFT)],
            &[],
        );
        assert_eq!(ids(&found), [(1, vec![])], "which tab closed can't be told");
    }

    #[test]
    fn a_mark_two_shown_tabs_carry_keeps_no_tab_by_memory() {
        let mut memory = saw(&[2, 3], NOW);
        memory.groups(
            NOW,
            &[shown(1, TOP_LEFT, 3)],
            &[maybe(2, TOP_LEFT), maybe(3, TOP_LEFT)],
            &[],
        );

        // Tab 2 was dragged into another window's group and shows there, while tab 1 shows
        // in the group it came from; both carry its mark.
        let found = memory.groups(
            NOW + 1,
            &[shown(1, TOP_LEFT, 2), shown(2, NEXT_DISPLAY, 2)],
            &[maybe(3, TOP_LEFT)],
            &[],
        );
        assert_eq!(ids(&found), [(1, vec![]), (2, vec![])]);
    }

    #[test]
    fn a_window_left_on_another_space_never_joins_a_group_in_its_place() {
        // A Terminal window was listed while the user was on its Space. Back on this one,
        // a group with tabs Snappy never saw shows in the same grid cell.
        let mut memory = saw(&[2], BEFORE);
        let found = memory.groups(
            NOW,
            &[shown(1, TOP_RIGHT, 5)],
            &[maybe(2, TOP_RIGHT)],
            &windows(&[3]),
        );
        assert_eq!(ids(&found), [(1, vec![])]);
        assert!(!memory.may_be_hidden(WindowId(2), NOW), "it isn't even read");
    }

    #[test]
    fn a_window_seen_go_joins_only_while_the_spaces_showing_stay_the_same() {
        let group = [shown(1, TOP_RIGHT, 2)];
        let gone = [maybe(2, TOP_RIGHT)];
        let mut same_visit = saw(&[2], NOW);
        assert!(same_visit.may_be_hidden(WindowId(2), NOW));
        assert_eq!(ids(&same_visit.groups(NOW, &group, &gone, &[])), [(1, vec![2])]);

        let mut after_a_change = saw(&[2], NOW);
        assert!(!after_a_change.may_be_hidden(WindowId(2), NOW + 1));
        assert_eq!(
            ids(&after_a_change.groups(NOW + 1, &group, &gone, &[])),
            [(1, vec![])]
        );
    }

    #[test]
    fn equal_size_windows_in_other_grid_cells_never_join_a_group() {
        // Every Terminal window in the grid has the same app-constrained size. One cell
        // holds a group with room for tabs Snappy never saw; the windows that went out of
        // sight in the other cells aren't its tabs, however many fit.
        let mut memory = saw(&[2, 3, 4], NOW);
        let found = memory.groups(
            NOW,
            &[shown(1, TOP_RIGHT, 4)],
            &[maybe(2, TOP_LEFT), maybe(3, BOTTOM_RIGHT), maybe(4, ELSEWHERE)],
            &[],
        );
        assert_eq!(ids(&found), [(1, vec![])]);
    }

    #[test]
    fn a_window_in_the_place_of_two_groups_joins_neither() {
        let mut memory = saw(&[3], NOW);
        let found = memory.groups(
            NOW,
            &[shown(1, TOP_LEFT, 2), shown(2, TOP_LEFT, 2)],
            &[maybe(3, TOP_LEFT)],
            &[],
        );
        assert_eq!(ids(&found), [(1, vec![]), (2, vec![])]);
    }

    #[test]
    fn minimized_tabs_join_only_a_minimized_group() {
        let mut memory = saw(&[4], NOW);
        let mut lead = shown(1, TOP_LEFT, 2);
        lead.minimized = true;
        let found = memory.groups(
            NOW,
            &[lead, shown(2, TOP_LEFT, 2)],
            &[minimized(maybe(3, TOP_LEFT)), maybe(4, TOP_LEFT)],
            &[],
        );
        assert_eq!(ids(&found), [(1, vec![3]), (2, vec![4])]);
    }

    #[test]
    fn forgotten_windows_are_neither_remembered_nor_seen() {
        let mut memory = saw(&[2], NOW);
        memory.groups(NOW, &[shown(1, TOP_LEFT, 2)], &[maybe(2, TOP_LEFT)], &[]);
        memory.retain(|window| window != WindowId(2));
        assert!(!memory.remembers(WindowId(2)));
        assert!(!memory.may_be_hidden(WindowId(2), NOW));
        assert!(memory.remembers(WindowId(1)));
    }
}
