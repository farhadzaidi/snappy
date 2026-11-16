use std::collections::{BTreeMap, BTreeSet};

use super::parking::{ParkingPolicy, parking_origin, reachable};
use crate::error::WindowError;
use crate::geometry::{Point, Rect};
use crate::window::WindowId;

/// A workspace move landed when the window ends up this close to its target.
const MOVE_TOLERANCE: f64 = 3.0;

/// One window's place in the workspaces.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub workspace: String,
    /// Where the window sits when its workspace is active.
    pub frame: Rect,
    /// Where the window waits while its workspace is inactive.
    pub parked: Option<Point>,
}

/// A write-ahead record of a parked window, enough to bring it back after a crash.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalEntry {
    pub window: WindowId,
    pub workspace: String,
    pub frame: Rect,
    pub parked: Point,
    /// While a parked window moves to a new parking position, where it was before.
    pub prior_parked: Option<Point>,
}

/// The tabs of one window, where the app makes each tab a window of its own and shows only
/// the selected one, as macOS does. A group moves, hides, and changes workspace as one
/// window: its shown tab. The hidden tabs keep stale frames and never move themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tabs {
    pub shown: WindowId,
    pub hidden: Vec<WindowId>,
}

/// What a session needs from the platform and the store.
pub trait WorkspaceIo {
    fn frame(&mut self, window: WindowId) -> Result<Rect, WindowError>;
    /// Closed for good, as opposed to unavailable for now.
    fn is_closed(&mut self, window: WindowId) -> bool;
    /// Present, not full screen, and without an open sheet.
    fn is_movable(&mut self, window: WindowId) -> bool;
    fn is_minimized(&mut self, window: WindowId) -> bool;
    /// On the platform's parking desktop. Only desktop parking asks.
    fn is_on_parking_desktop(&mut self, window: WindowId) -> bool;
    /// Move a window without resizing it; `hidden` says whether it is being parked.
    fn place(&mut self, window: WindowId, origin: Point, hidden: bool) -> Result<(), WindowError>;
    fn focus(&mut self, window: WindowId);
    /// Persist the parked windows before and after every change.
    fn write_journal(&mut self, entries: &[JournalEntry]) -> Result<(), WindowError>;
}

/// Window groups and which one is visible. Every change is all-or-nothing: the journal is
/// written first, each move is checked, and a refused move reverses the ones before it.
#[derive(Debug, Clone)]
pub struct Session {
    active: String,
    members: BTreeMap<WindowId, Member>,
    last_focused: BTreeMap<String, WindowId>,
    /// Member windows in the order they were focused, the most recent last.
    recent: Vec<WindowId>,
    policy: ParkingPolicy,
    /// Work areas after a display change; incoming windows are kept reachable inside them.
    work_areas: Option<Vec<Rect>>,
    /// Members hidden as tabs, and the shown tab each sits behind.
    tabs: BTreeMap<WindowId, WindowId>,
}

type Members = BTreeMap<WindowId, Member>;

impl Session {
    pub fn new(active: impl Into<String>, policy: ParkingPolicy) -> Self {
        Self {
            active: active.into(),
            members: Members::new(),
            last_focused: BTreeMap::new(),
            recent: Vec::new(),
            policy,
            work_areas: None,
            tabs: BTreeMap::new(),
        }
    }

    pub fn active(&self) -> &str {
        &self.active
    }

    pub fn members(&self) -> &Members {
        &self.members
    }

    pub fn member(&self, window: WindowId) -> Option<&Member> {
        self.members.get(&window)
    }

    /// Windows in `workspace`, counting a tab group once.
    pub fn window_count(&self, workspace: &str) -> usize {
        self.members
            .iter()
            .filter(|(window, member)| member.workspace == workspace && !self.is_hidden_tab(**window))
            .count()
    }

    /// True when `window` is tracked in a workspace that isn't showing.
    pub fn is_inactive(&self, window: WindowId) -> bool {
        self.members
            .get(&window)
            .is_some_and(|member| member.workspace != self.active)
    }

    /// Start tracking a window. Already-tracked windows keep their state.
    pub fn enroll(&mut self, window: WindowId, member: Member) {
        self.members.entry(window).or_insert(member);
    }

    pub fn discard(&mut self, window: WindowId) {
        self.members.remove(&window);
        self.last_focused.retain(|_, focused| *focused != window);
        self.recent.retain(|focused| *focused != window);
        self.tabs
            .retain(|hidden, shown| *hidden != window && *shown != window);
    }

    /// Whether `window` is a tab hidden behind another member.
    pub fn is_hidden_tab(&self, window: WindowId) -> bool {
        self.tabs.contains_key(&window)
    }

    /// The window that stands for `window`'s tab group: the shown tab it sits behind, else
    /// `window` itself.
    pub fn tab_root(&self, window: WindowId) -> WindowId {
        self.tabs.get(&window).copied().unwrap_or(window)
    }

    /// The windows that stand for `windows`' tab groups, each once, in the order of the
    /// first window of each group.
    pub fn tab_roots(&self, windows: &[WindowId]) -> Vec<WindowId> {
        let mut roots: Vec<WindowId> = Vec::new();
        for window in windows {
            let root = self.tab_root(*window);
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
        roots
    }

    /// Whether `window` is tracked in the active workspace and not parked.
    pub fn shows(&self, window: WindowId) -> bool {
        self.members
            .get(&window)
            .is_some_and(|member| member.workspace == self.active && member.parked.is_none())
    }

    /// The members hidden as tabs behind `shown`.
    pub fn tabs_behind(&self, shown: WindowId) -> Vec<WindowId> {
        self.tabs
            .iter()
            .filter(|(_, in_front)| **in_front == shown)
            .map(|(hidden, _)| *hidden)
            .collect()
    }

    /// Replace what is known about the tab groups of the `refreshed` members with
    /// `groups`, then put every hidden tab where its shown tab is: in its workspace, with
    /// its frame and parking, so a group never spans workspaces. Returns the hidden tabs
    /// that changed workspace.
    pub fn update_tabs(&mut self, groups: &[Tabs], refreshed: &[WindowId]) -> Vec<WindowId> {
        self.tabs
            .retain(|hidden, shown| !refreshed.contains(hidden) && !refreshed.contains(shown));
        for group in groups {
            if !self.members.contains_key(&group.shown) {
                continue;
            }
            self.tabs.remove(&group.shown);
            for hidden in &group.hidden {
                if *hidden != group.shown && self.members.contains_key(hidden) {
                    self.tabs.insert(*hidden, group.shown);
                }
            }
        }

        let mut moved = Vec::new();
        for (hidden, shown) in self.tabs.clone() {
            let Some(shown_tab) = self.members.get(&shown).cloned() else {
                continue;
            };
            if let Some(member) = self.members.get_mut(&hidden)
                && *member != shown_tab
            {
                if member.workspace != shown_tab.workspace {
                    moved.push(hidden);
                }
                *member = shown_tab;
            }
        }
        self.forget_focus_elsewhere();
        moved
    }

    /// Give every hidden tab in `next` its shown tab's place.
    fn follow_tabs(&self, next: &mut Members) {
        for (hidden, shown) in &self.tabs {
            if let Some(shown_tab) = next.get(shown).cloned()
                && next.contains_key(hidden)
            {
                next.insert(*hidden, shown_tab);
            }
        }
    }

    /// A workspace's last focused window must still belong to it.
    fn forget_focus_elsewhere(&mut self) {
        let members = &self.members;
        self.last_focused.retain(|workspace, window| {
            members
                .get(window)
                .is_some_and(|member| member.workspace == *workspace)
        });
    }

    pub fn note_focus(&mut self, window: WindowId) {
        if let Some(member) = self.members.get(&window) {
            self.last_focused.insert(member.workspace.clone(), window);
            self.recent.retain(|focused| *focused != window);
            self.recent.push(window);
        }
    }

    /// Member windows in the order they were last focused, the most recent first.
    pub fn recently_focused(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.recent.iter().rev().copied()
    }

    /// Focus the active workspace's most recently focused window that can take focus,
    /// which is its frontmost, else its first one that can. Returns the window focused, or
    /// `None` when the workspace has none left.
    pub fn focus_recent(&mut self, io: &mut impl WorkspaceIo) -> Option<WindowId> {
        let recent = self
            .recent
            .iter()
            .rev()
            .map(|window| self.tab_root(*window))
            .find(|window| self.can_focus(*window, io));
        let window = recent.or_else(|| {
            self.members
                .keys()
                .copied()
                .find(|window| self.can_focus(*window, io))
        })?;
        io.focus(window);
        self.note_focus(window);
        Some(window)
    }

    /// Refresh the saved frames of visible windows in the active workspace.
    pub fn capture_active_frames(&mut self, io: &mut impl WorkspaceIo) {
        for (window, member) in self.members.iter_mut() {
            if member.workspace == self.active
                && member.parked.is_none()
                && !self.tabs.contains_key(window)
                && let Ok(frame) = io.frame(*window)
            {
                member.frame = frame;
            }
        }
    }

    /// The parked windows, for writing the journal.
    pub fn journal(&self) -> Vec<JournalEntry> {
        journal_of(&self.members, &self.tabs, &BTreeMap::new())
    }

    /// Show `target` and park the active workspace, except that the active workspace's visible
    /// windows in `along` join `target` where they are instead of being parked, and the last of
    /// them that can take focus gets it. A tab taken along takes its whole group. Returns
    /// whether a window was focused afterwards, so the caller can take focus when none was.
    pub fn switch_taking_along(
        &mut self,
        target: &str,
        along: &[WindowId],
        displays: &[Rect],
        focus_incoming: bool,
        io: &mut impl WorkspaceIo,
    ) -> Result<bool, WindowError> {
        if target == self.active {
            return Ok(true);
        }
        let along = self.tab_roots(along);
        let mut next = self.members.clone();
        let mut arriving = Vec::new();
        for (window, member) in next.iter_mut() {
            if self.tabs.contains_key(window) {
                continue;
            }
            if member.workspace == self.active && member.parked.is_none() && along.contains(window) {
                member.workspace = target.to_owned();
                member.frame = io.frame(*window)?;
                arriving.push(*window);
            } else if member.workspace == self.active {
                if !io.is_movable(*window) {
                    return Err(WindowError::unavailable(
                        "Close the window's dialog or leave full screen before switching workspaces.",
                    ));
                }
                let frame = io.frame(*window)?;
                if member.parked.is_none() {
                    member.frame = frame;
                }
                member.parked = Some(self.park(frame, displays)?);
            } else if member.workspace == target {
                if !io.is_movable(*window) {
                    return Err(WindowError::unavailable(
                        "A window in that workspace is full screen or has a dialog open; leave full screen or close it first.",
                    ));
                }
                member.parked = None;
                if let Some(areas) = &self.work_areas {
                    member.frame = reachable(member.frame, areas);
                }
            }
        }

        self.follow_tabs(&mut next);
        self.transition(next, &BTreeSet::new(), io)?;
        self.active = target.to_owned();
        self.forget_focus_elsewhere();
        if !focus_incoming {
            return Ok(true);
        }
        let last_along = along
            .iter()
            .rev()
            .copied()
            .find(|window| arriving.contains(window) && self.can_focus(*window, io));
        match last_along {
            Some(window) => {
                io.focus(window);
                self.note_focus(window);
                Ok(true)
            }
            None => Ok(self.focus_active(io)),
        }
    }

    /// Send a window to another workspace without switching. A tab takes its whole group.
    pub fn send(
        &mut self,
        window: WindowId,
        to: &str,
        displays: &[Rect],
        io: &mut impl WorkspaceIo,
    ) -> Result<(), WindowError> {
        let window = self.tab_root(window);
        let Some(mut member) = self.members.get(&window).cloned() else {
            return Ok(());
        };
        if !io.is_movable(window) {
            return Err(WindowError::unavailable(
                "Close the window's dialog or leave full screen before sending it to a workspace.",
            ));
        }
        if member.workspace == self.active && member.parked.is_none() {
            member.frame = io.frame(window)?;
        }
        member.workspace = to.to_owned();
        if to == self.active {
            member.parked = None;
            if let Some(areas) = &self.work_areas {
                member.frame = reachable(member.frame, areas);
            }
        } else {
            member.parked = Some(self.park(member.frame, displays)?);
        }

        let mut next = self.members.clone();
        next.insert(window, member);
        self.follow_tabs(&mut next);
        self.transition(next, &BTreeSet::new(), io)?;
        self.forget_focus_elsewhere();
        Ok(())
    }

    /// Park again a window of an inactive workspace that its app brought back on screen.
    /// Returns whether the window had to move.
    pub fn contain(
        &mut self,
        window: WindowId,
        displays: &[Rect],
        io: &mut impl WorkspaceIo,
    ) -> Result<bool, WindowError> {
        let Some(member) = self.members.get(&window) else {
            return Ok(false);
        };
        // A hidden tab isn't on screen; its group comes back through its shown tab.
        if self.tabs.contains_key(&window) {
            return Ok(false);
        }
        if member.workspace == self.active || member.parked.is_none() || !io.is_movable(window) {
            return Ok(false);
        }
        let actual = io.frame(window)?;
        let origin = self.park(actual, displays)?;
        if lands_at(actual, origin) && self.hidden_as(window, true, io) {
            return Ok(false);
        }

        let mut next = self.members.clone();
        if let Some(member) = next.get_mut(&window) {
            member.parked = Some(origin);
        }
        io.write_journal(&journal_of(&next, &self.tabs, &BTreeMap::new()))?;
        io.place(window, origin, true)?;
        if !(lands_at(io.frame(window)?, origin) && self.hidden_as(window, true, io)) {
            return Err(WindowError::refused(
                "A window from another workspace wouldn't stay hidden.",
            ));
        }
        self.members = next;
        Ok(true)
    }

    /// Move a deleted workspace's windows to `destination`.
    pub fn remove_workspace(
        &mut self,
        id: &str,
        destination: &str,
        io: &mut impl WorkspaceIo,
    ) -> Result<(), WindowError> {
        let new_active = if self.active == id {
            destination.to_owned()
        } else {
            self.active.clone()
        };
        let mut next = self.members.clone();
        for (window, member) in next.iter_mut() {
            if member.workspace == id {
                member.workspace = destination.to_owned();
                if member.parked.is_none() && !self.tabs.contains_key(window) {
                    member.frame = io.frame(*window)?;
                }
            }
            if member.workspace == new_active {
                member.parked = None;
            }
        }
        self.follow_tabs(&mut next);
        self.transition(next, &BTreeSet::new(), io)?;
        self.active = new_active;
        self.last_focused.remove(id);
        Ok(())
    }

    /// Re-place every window for a new display arrangement, without revealing parked
    /// windows. Windows that can't move right now, minimized, full screen, or showing a
    /// dialog, stay where they are rather than holding up the rest. Returns how many
    /// windows it moved.
    pub fn update_displays(
        &mut self,
        displays: &[Rect],
        areas: &[Rect],
        io: &mut impl WorkspaceIo,
    ) -> Result<usize, WindowError> {
        let usable = |rects: &[Rect]| !rects.is_empty() && rects.iter().all(|rect| rect.is_usable(1.0));
        if !usable(displays) || !usable(areas) {
            return Err(WindowError::InvalidFrame);
        }

        let mut next = self.members.clone();
        let mut forced = BTreeSet::new();
        for (window, member) in next.iter_mut() {
            if self.tabs.contains_key(window) {
                continue;
            }
            if !io.is_movable(*window) || io.is_minimized(*window) {
                continue;
            }
            let actual = io.frame(*window)?;
            let target = if member.workspace == self.active {
                let visible = if member.parked.is_some() {
                    actual.with_origin(member.frame.origin())
                } else {
                    actual
                };
                member.frame = reachable(visible, areas);
                member.parked = None;
                member.frame.origin()
            } else {
                let origin = self.park(actual, displays)?;
                member.parked = Some(origin);
                origin
            };
            // The system may already have moved the window, even if its target is unchanged.
            if !lands_at(actual, target) || !self.hidden_as(*window, member.parked.is_some(), io) {
                forced.insert(*window);
            }
        }

        self.follow_tabs(&mut next);
        self.transition(next, &forced, io)?;
        self.work_areas = Some(areas.to_vec());
        // A window that closed meanwhile left the session without moving.
        Ok(forced
            .iter()
            .filter(|window| self.members.contains_key(window))
            .count())
    }

    /// Bring every parked window back into view. Returns false if any
    /// window couldn't be returned; those stay parked and journaled for a retry.
    pub fn restore_all(&mut self, areas: &[Rect], io: &mut impl WorkspaceIo) -> bool {
        let mut all_back = true;
        let parked: Vec<WindowId> = self
            .members
            .iter()
            .filter(|(window, member)| member.parked.is_some() && !self.tabs.contains_key(window))
            .map(|(window, _)| *window)
            .collect();
        for window in parked {
            let origin = reachable(self.members[&window].frame, areas).origin();
            match self.members.get_mut(&window) {
                Some(member) if return_to(self.policy, window, origin, io) => member.parked = None,
                _ => all_back = false,
            }
        }
        for (hidden, shown) in &self.tabs {
            let back = self
                .members
                .get(shown)
                .is_some_and(|shown_tab| shown_tab.parked.is_none());
            if let (true, Some(member)) = (back, self.members.get_mut(hidden)) {
                member.parked = None;
            }
        }
        io.write_journal(&self.journal()).is_ok() && all_back
    }

    fn park(&self, frame: Rect, displays: &[Rect]) -> Result<Point, WindowError> {
        parking_origin(self.policy, frame, displays).ok_or_else(|| {
            WindowError::unavailable("No screen corner is free to hide windows in with these displays.")
        })
    }

    fn hidden_as(&self, window: WindowId, hidden: bool, io: &mut impl WorkspaceIo) -> bool {
        hidden_as(self.policy, window, hidden, io)
    }

    /// Focus the active workspace's last focused window, or its first usable one.
    fn focus_active(&self, io: &mut impl WorkspaceIo) -> bool {
        let remembered = self
            .last_focused
            .get(&self.active)
            .map(|window| self.tab_root(*window))
            .filter(|window| self.can_focus(*window, io));
        let candidate = remembered.or_else(|| {
            self.members
                .keys()
                .copied()
                .find(|window| self.can_focus(*window, io))
        });
        if let Some(window) = candidate {
            io.focus(window);
        }
        candidate.is_some()
    }

    fn can_focus(&self, window: WindowId, io: &mut impl WorkspaceIo) -> bool {
        // Focusing a hidden tab would make its app show that tab instead.
        !self.tabs.contains_key(&window)
            && self.shows(window)
            && io.is_movable(window)
            && !io.is_minimized(window)
    }

    /// Move `window` where `after` puts it and check it got there, noting where it was in
    /// `done` so a later refusal can put it back. A minimized window stays put.
    fn move_member(
        &self,
        window: WindowId,
        after: &Member,
        done: &mut Vec<(WindowId, Point, bool)>,
        io: &mut impl WorkspaceIo,
    ) -> Result<(), WindowError> {
        if !io.is_movable(window) {
            return Err(WindowError::unavailable(
                "A window couldn't move just then, so nothing changed; try again.",
            ));
        }
        if io.is_minimized(window) {
            return Ok(());
        }
        let before = io.frame(window)?.origin();
        let was_hidden = self
            .members
            .get(&window)
            .is_some_and(|member| member.parked.is_some());
        done.push((window, before, was_hidden));

        let target = after.parked.unwrap_or(after.frame.origin());
        let hidden = after.parked.is_some();
        io.place(window, target, hidden)?;
        if lands_at(io.frame(window)?, target) && hidden_as(self.policy, window, hidden, io) {
            Ok(())
        } else {
            Err(WindowError::refused(
                "An app kept its window from moving, so nothing changed.",
            ))
        }
    }

    /// Apply `next`: journal first, windows coming into view before windows being
    /// parked, every move checked, everything reversed on a refusal. A window that closes
    /// during the change leaves the session.
    fn transition(
        &mut self,
        mut next: Members,
        forced: &BTreeSet<WindowId>,
        io: &mut impl WorkspaceIo,
    ) -> Result<(), WindowError> {
        let moving: Vec<WindowId> = next
            .iter()
            .filter(|(window, after)| {
                let changed = forced.contains(window)
                    || self
                        .members
                        .get(window)
                        .is_some_and(|before| before.parked != after.parked);
                // A hidden tab moves with its shown tab; moving it alone would do nothing
                // on screen until its app shows it.
                changed && !self.tabs.contains_key(window)
            })
            .map(|(window, _)| *window)
            .collect();

        let mut prior = BTreeMap::new();
        for window in &moving {
            let was_parked = self
                .members
                .get(window)
                .is_some_and(|before| before.parked.is_some());
            if was_parked && next[window].parked.is_some() && !io.is_minimized(*window) {
                prior.insert(*window, io.frame(*window)?.origin());
            }
        }
        // Windows coming back stay journaled as parked until the move has landed.
        let mut recovery = next.clone();
        for window in &moving {
            if next[window].parked.is_none()
                && let Some(before) = self.members.get(window).filter(|before| before.parked.is_some())
            {
                recovery.insert(*window, before.clone());
            }
        }
        io.write_journal(&journal_of(&recovery, &self.tabs, &prior))?;

        let policy = self.policy;
        let mut order = moving;
        order.sort_by_key(|window| next[window].parked.is_some());
        let mut done: Vec<(WindowId, Point, bool)> = Vec::new();
        let mut closed = Vec::new();
        let mut outcome = Ok(());
        for window in order {
            match self.move_member(window, &next[&window], &mut done, io) {
                // A window closed meanwhile has nothing left to move; it leaves the
                // session rather than undoing the change for every other window.
                Err(_) if io.is_closed(window) => {
                    done.retain(|(moved, ..)| *moved != window);
                    next.remove(&window);
                    closed.push(window);
                }
                Err(error) => {
                    outcome = Err(error);
                    break;
                }
                Ok(()) => {}
            }
        }

        match outcome.and_then(|()| io.write_journal(&journal_of(&next, &self.tabs, &BTreeMap::new()))) {
            Ok(()) => {
                self.members = next;
                for window in closed {
                    self.discard(window);
                }
                Ok(())
            }
            Err(error) => {
                let rolled_back = done.iter().rev().all(|&(window, origin, hidden)| {
                    io.place(window, origin, hidden).is_ok() && settled(policy, window, origin, hidden, io)
                });
                if rolled_back {
                    let _ = io.write_journal(&self.journal());
                }
                Err(error)
            }
        }
    }
}

/// Move a parked window back to `origin` and confirm it got there.
fn return_to(policy: ParkingPolicy, window: WindowId, origin: Point, io: &mut impl WorkspaceIo) -> bool {
    if !io.is_movable(window) {
        return false;
    }
    if !settled(policy, window, origin, false, io) && io.place(window, origin, false).is_err() {
        return false;
    }
    settled(policy, window, origin, false, io)
}

/// Whether `window` sits at `origin`, hidden or shown as `hidden` says.
fn settled(
    policy: ParkingPolicy,
    window: WindowId,
    origin: Point,
    hidden: bool,
    io: &mut impl WorkspaceIo,
) -> bool {
    io.frame(window).is_ok_and(|frame| lands_at(frame, origin)) && hidden_as(policy, window, hidden, io)
}

/// Whether `window` is hidden or shown as `hidden` asks. Where parking moves windows,
/// only the frame tells, so this holds.
fn hidden_as(policy: ParkingPolicy, window: WindowId, hidden: bool, io: &mut impl WorkspaceIo) -> bool {
    policy.moves_windows() || io.is_on_parking_desktop(window) == hidden
}

fn lands_at(frame: Rect, origin: Point) -> bool {
    (frame.x - origin.x).abs() < MOVE_TOLERANCE && (frame.y - origin.y).abs() < MOVE_TOLERANCE
}

/// Parked windows to journal. Hidden tabs come back with their shown tab, so only the
/// shown one is journaled.
fn journal_of(
    members: &Members,
    tabs: &BTreeMap<WindowId, WindowId>,
    prior: &BTreeMap<WindowId, Point>,
) -> Vec<JournalEntry> {
    members
        .iter()
        .filter(|(window, _)| !tabs.contains_key(window))
        .filter_map(|(window, member)| {
            member.parked.map(|parked| JournalEntry {
                window: *window,
                workspace: member.workspace.clone(),
                frame: member.frame,
                parked,
                prior_parked: prior.get(window).copied(),
            })
        })
        .collect()
}
