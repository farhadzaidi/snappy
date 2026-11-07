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

/// What a session needs from the platform and the store.
pub trait WorkspaceIo {
    fn frame(&mut self, window: WindowId) -> Result<Rect, WindowError>;
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
    policy: ParkingPolicy,
    /// Work areas after a display change; incoming windows are kept reachable inside them.
    work_areas: Option<Vec<Rect>>,
}

type Members = BTreeMap<WindowId, Member>;

impl Session {
    pub fn new(active: impl Into<String>, policy: ParkingPolicy) -> Self {
        Self {
            active: active.into(),
            members: Members::new(),
            last_focused: BTreeMap::new(),
            policy,
            work_areas: None,
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

    pub fn window_count(&self, workspace: &str) -> usize {
        self.members
            .values()
            .filter(|member| member.workspace == workspace)
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
    }

    pub fn note_focus(&mut self, window: WindowId) {
        if let Some(member) = self.members.get(&window) {
            self.last_focused.insert(member.workspace.clone(), window);
        }
    }

    /// Refresh the saved frames of visible windows in the active workspace.
    pub fn capture_active_frames(&mut self, io: &mut impl WorkspaceIo) {
        for (window, member) in self.members.iter_mut() {
            if member.workspace == self.active
                && member.parked.is_none()
                && let Ok(frame) = io.frame(*window)
            {
                member.frame = frame;
            }
        }
    }

    /// The parked windows, for writing the journal.
    pub fn journal(&self) -> Vec<JournalEntry> {
        journal_of(&self.members, &BTreeMap::new())
    }

    /// Show `target` and park the active workspace. Returns whether
    /// a window was focused afterwards, so the caller can take focus when none was.
    pub fn switch_to(
        &mut self,
        target: &str,
        displays: &[Rect],
        focus_incoming: bool,
        io: &mut impl WorkspaceIo,
    ) -> Result<bool, WindowError> {
        if target == self.active {
            return Ok(true);
        }
        let mut next = self.members.clone();
        for (window, member) in next.iter_mut() {
            if member.workspace == self.active {
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
                        "A workspace window is unavailable. Close its dialog or leave full screen, then retry.",
                    ));
                }
                member.parked = None;
                if let Some(areas) = &self.work_areas {
                    member.frame = reachable(member.frame, areas);
                }
            }
        }

        self.transition(next, &BTreeSet::new(), io)?;
        self.active = target.to_owned();
        Ok(!focus_incoming || self.focus_active(io))
    }

    /// Move a window to another workspace without switching.
    pub fn send(
        &mut self,
        window: WindowId,
        to: &str,
        displays: &[Rect],
        io: &mut impl WorkspaceIo,
    ) -> Result<(), WindowError> {
        let Some(mut member) = self.members.get(&window).cloned() else {
            return Ok(());
        };
        if !io.is_movable(window) {
            return Err(WindowError::unavailable(
                "Close the window's dialog or leave full screen before moving it to a workspace.",
            ));
        }
        if member.workspace == self.active && member.parked.is_none() {
            member.frame = io.frame(window)?;
        }
        let from = std::mem::replace(&mut member.workspace, to.to_owned());
        if to == self.active {
            member.parked = None;
            if let Some(areas) = &self.work_areas {
                member.frame = reachable(member.frame, areas);
            }
        } else {
            let origin = parking_origin(self.policy, member.frame, displays)
                .ok_or_else(|| WindowError::unavailable("No safe corner is available."))?;
            member.parked = Some(origin);
        }

        let mut next = self.members.clone();
        next.insert(window, member);
        self.transition(next, &BTreeSet::new(), io)?;
        if from != to && self.last_focused.get(&from) == Some(&window) {
            self.last_focused.remove(&from);
        }
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
        io.write_journal(&journal_of(&next, &BTreeMap::new()))?;
        io.place(window, origin, true)?;
        if !(lands_at(io.frame(window)?, origin) && self.hidden_as(window, true, io)) {
            return Err(WindowError::refused(
                "An application revealed a window from another workspace and refused to hide it.",
            ));
        }
        self.members = next;
        Ok(true)
    }

    /// Stop tracking a window, returning it to view first.
    pub fn release(&mut self, window: WindowId, io: &mut impl WorkspaceIo) -> Result<(), WindowError> {
        let mut next = self.members.clone();
        if let Some(member) = next.get_mut(&window) {
            member.parked = None;
        } else {
            return Ok(());
        }
        self.transition(next, &BTreeSet::new(), io)?;
        self.discard(window);
        io.write_journal(&self.journal())
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
                if member.parked.is_none() {
                    member.frame = io.frame(*window)?;
                }
            }
            if member.workspace == new_active {
                member.parked = None;
            }
        }
        self.transition(next, &BTreeSet::new(), io)?;
        self.active = new_active;
        self.last_focused.remove(id);
        Ok(())
    }

    /// Re-place every window for a new display arrangement, without revealing parked
    /// windows.
    pub fn update_displays(
        &mut self,
        displays: &[Rect],
        areas: &[Rect],
        io: &mut impl WorkspaceIo,
    ) -> Result<(), WindowError> {
        let usable = |rects: &[Rect]| !rects.is_empty() && rects.iter().all(|rect| rect.is_usable(1.0));
        if !usable(displays) || !usable(areas) {
            return Err(WindowError::InvalidFrame);
        }

        let mut next = self.members.clone();
        let mut forced = BTreeSet::new();
        for (window, member) in next.iter_mut() {
            if !io.is_movable(*window) {
                return Err(WindowError::unavailable(
                    "A window is busy during the display change. Close its dialog or leave full screen, then retry.",
                ));
            }
            if io.is_minimized(*window) {
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
                let origin = parking_origin(self.policy, actual, displays).ok_or_else(|| {
                    WindowError::unavailable("No safe screen corner is available after the display change.")
                })?;
                member.parked = Some(origin);
                origin
            };
            // The system may already have moved the window, even if its target is unchanged.
            if !lands_at(actual, target) || !self.hidden_as(*window, member.parked.is_some(), io) {
                forced.insert(*window);
            }
        }

        self.transition(next, &forced, io)?;
        self.work_areas = Some(areas.to_vec());
        Ok(())
    }

    /// Bring every parked window back into view. Returns false if any
    /// window couldn't be returned; those stay parked and journaled for a retry.
    pub fn restore_all(&mut self, areas: &[Rect], io: &mut impl WorkspaceIo) -> bool {
        let mut all_back = true;
        let parked: Vec<WindowId> = self
            .members
            .iter()
            .filter(|(_, member)| member.parked.is_some())
            .map(|(window, _)| *window)
            .collect();
        for window in parked {
            let origin = reachable(self.members[&window].frame, areas).origin();
            match self.members.get_mut(&window) {
                Some(member) if return_to(self.policy, window, origin, io) => member.parked = None,
                _ => all_back = false,
            }
        }
        io.write_journal(&self.journal()).is_ok() && all_back
    }

    fn park(&self, frame: Rect, displays: &[Rect]) -> Result<Point, WindowError> {
        parking_origin(self.policy, frame, displays).ok_or_else(|| {
            WindowError::unavailable("No safe screen corner is available for workspace windows.")
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
            .copied()
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
        let visible_here = self
            .members
            .get(&window)
            .is_some_and(|member| member.workspace == self.active && member.parked.is_none());
        visible_here && io.is_movable(window) && !io.is_minimized(window)
    }

    /// Apply `next`: journal first, windows coming into view before windows being
    /// parked, every move checked, everything reversed on a refusal.
    fn transition(
        &mut self,
        next: Members,
        forced: &BTreeSet<WindowId>,
        io: &mut impl WorkspaceIo,
    ) -> Result<(), WindowError> {
        let moving: Vec<WindowId> = next
            .iter()
            .filter(|(window, after)| {
                forced.contains(window)
                    || self
                        .members
                        .get(window)
                        .is_some_and(|before| before.parked != after.parked)
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
        io.write_journal(&journal_of(&recovery, &prior))?;

        let policy = self.policy;
        let mut order = moving;
        order.sort_by_key(|window| next[window].parked.is_some());
        let mut done: Vec<(WindowId, Point, bool)> = Vec::new();
        let outcome = order.iter().try_for_each(|&window| {
            if !io.is_movable(window) {
                return Err(WindowError::unavailable(
                    "A window became unavailable during the workspace change. Its workspace tracking has been kept.",
                ));
            }
            if io.is_minimized(window) {
                return Ok(());
            }
            let before = io.frame(window)?.origin();
            let was_hidden = self.members.get(&window).is_some_and(|member| member.parked.is_some());
            done.push((window, before, was_hidden));

            let after = &next[&window];
            let target = after.parked.unwrap_or(after.frame.origin());
            let hidden = after.parked.is_some();
            io.place(window, target, hidden)?;
            if lands_at(io.frame(window)?, target) && hidden_as(policy, window, hidden, io) {
                Ok(())
            } else {
                Err(WindowError::refused("An app refused the workspace move. Your previous workspace has been restored."))
            }
        });

        match outcome.and_then(|()| io.write_journal(&journal_of(&next, &BTreeMap::new()))) {
            Ok(()) => {
                self.members = next;
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

fn journal_of(members: &Members, prior: &BTreeMap<WindowId, Point>) -> Vec<JournalEntry> {
    members
        .iter()
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
