//! Snap, swap, and undo, with the undo history they share.

use std::collections::VecDeque;
use std::future::Future;

use crate::bounds::{CancelToken, is_constrained};
use crate::error::WindowError;
use crate::geometry::{Rect, largest_overlap};
use crate::swap::{SwapSource, ZoneSlot};
use crate::window::WindowId;

/// Undo keeps this many window moves.
pub const UNDO_LIMIT: usize = 50;
/// A restored window may land this far from its original frame on each edge.
const RESTORE_TOLERANCE: f64 = 12.0;
/// Swapped windows may overlap by this much in width or height.
const ALLOWED_OVERLAP: f64 = 1.0;

/// Reading and settling windows, as the platform provides them.
pub trait WindowOps {
    fn read(&mut self, window: WindowId) -> Result<Rect, WindowError>;
    fn settle(
        &mut self,
        window: WindowId,
        frame: Rect,
        work_area: Option<Rect>,
        cancel: &CancelToken,
    ) -> impl Future<Output = Result<Rect, WindowError>>;
}

/// One window's position before a move.
#[derive(Debug, Clone, PartialEq)]
pub struct Move {
    pub window: WindowId,
    pub original: Rect,
    pub work_area: Option<Rect>,
}

/// Window moves that can be undone, and undone moves that can be redone, newest last. A
/// swap is one entry with two moves. Each entry holds the frames to move back to.
#[derive(Debug, Clone, Default)]
pub struct UndoHistory {
    entries: VecDeque<Vec<Move>>,
    undone: VecDeque<Vec<Move>>,
}

/// Which way through the history to go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replay {
    Undo,
    Redo,
}

impl UndoHistory {
    /// A new move. Nothing undone before it can be redone any more.
    pub fn record(&mut self, entry: Vec<Move>) {
        push_bounded(&mut self.entries, entry);
        self.undone.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn redo_len(&self) -> usize {
        self.undone.len()
    }

    /// The windows the next undo or redo would touch.
    pub fn next_windows(&self, replay: Replay) -> Vec<WindowId> {
        self.stack(replay)
            .back()
            .map(|entry| entry.iter().map(|m| m.window).collect())
            .unwrap_or_default()
    }

    /// Drop every entry that involves `window`.
    pub fn forget(&mut self, window: WindowId) {
        for stack in [&mut self.entries, &mut self.undone] {
            stack.retain(|entry| entry.iter().all(|m| m.window != window));
        }
    }

    fn stack(&self, replay: Replay) -> &VecDeque<Vec<Move>> {
        match replay {
            Replay::Undo => &self.entries,
            Replay::Redo => &self.undone,
        }
    }

    /// Move the newest entry of `replay`'s stack to the other one, as `reverse`.
    fn finish(&mut self, replay: Replay, reverse: Vec<Move>) {
        let (from, to) = match replay {
            Replay::Undo => (&mut self.entries, &mut self.undone),
            Replay::Redo => (&mut self.undone, &mut self.entries),
        };
        from.pop_back();
        push_bounded(to, reverse);
    }
}

fn push_bounded(stack: &mut VecDeque<Vec<Move>>, entry: Vec<Move>) {
    stack.push_back(entry);
    while stack.len() > UNDO_LIMIT {
        stack.pop_front();
    }
}

/// A swap: `dragged` started at `original` in `source` and moves to `destination`, which
/// `occupant` fills.
#[derive(Debug, Clone, Copy)]
pub struct SwapRequest<'a> {
    pub dragged: WindowId,
    pub occupant: WindowId,
    pub source: &'a SwapSource,
    pub destination: &'a ZoneSlot,
    pub original: Rect,
}

/// What a snap achieved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub requested: Rect,
    pub actual: Rect,
}

impl Placement {
    pub fn constrained(&self) -> bool {
        is_constrained(self.requested, self.actual)
    }
}

/// Snap `window` to `frame`. Undo returns it to `original`, or to where it was before the
/// snap when there is none. A failed snap puts the window back and records no history.
pub async fn snap(
    ops: &mut impl WindowOps,
    history: &mut UndoHistory,
    window: WindowId,
    frame: Rect,
    original: Option<Rect>,
    work_area: Option<Rect>,
    cancel: &CancelToken,
) -> Result<Placement, WindowError> {
    if !frame.is_usable(1.0) {
        return Err(WindowError::InvalidFrame);
    }
    let before = ops.read(window)?;
    let undo_to = Move {
        window,
        original: original.unwrap_or(before),
        work_area: None,
    };

    match ops.settle(window, frame, work_area, cancel).await {
        Ok(actual) => {
            history.record(vec![undo_to]);
            Ok(Placement {
                requested: frame,
                actual,
            })
        }
        Err(WindowError::Cancelled) => {
            // The new gesture owns the window; keep a way back if it already moved.
            if ops.read(window).is_ok_and(|now| now != before) {
                history.record(vec![undo_to]);
            }
            Err(WindowError::Cancelled)
        }
        Err(error) => {
            let _ = ops.settle(window, before, None, &CancelToken::default()).await;
            Err(error)
        }
    }
}

/// Exchange `dragged` (which started at `original` in `source`) with `occupant` in
/// `destination`. `protected` names windows recovery must not touch.
pub async fn swap(
    ops: &mut impl WindowOps,
    history: &mut UndoHistory,
    request: SwapRequest<'_>,
    cancel: &CancelToken,
    protected: &dyn Fn(WindowId) -> bool,
) -> Result<[Placement; 2], WindowError> {
    let SwapRequest {
        dragged,
        occupant,
        source,
        destination,
        original,
    } = request;
    let usable = [original, source.frame(), destination.frame]
        .iter()
        .all(|frame| frame.is_usable(1.0));
    if dragged == occupant || source.is_zone(destination) || !usable {
        return Err(WindowError::InvalidFrame);
    }
    let occupant_before = ops.read(occupant)?;
    if !source.is_filled_by(original) || !destination.is_occupied_by(occupant_before) {
        return Err(WindowError::unavailable(
            "A window left its zone before the swap. Try the drag again.",
        ));
    }
    let undo_to = vec![
        Move {
            window: dragged,
            original,
            work_area: Some(source.work_area()),
        },
        Move {
            window: occupant,
            original: occupant_before,
            work_area: Some(destination.work_area),
        },
    ];

    match exchange(ops, dragged, occupant, source, destination, cancel).await {
        Ok(placements) => {
            history.record(undo_to);
            Ok(placements)
        }
        Err(error) => {
            let stranded = recover(ops, &undo_to, protected).await;
            if !stranded.is_empty() {
                history.record(undo_to);
                if error != WindowError::Cancelled {
                    return Err(WindowError::refused(
                        "The swap could not finish. Use Undo to restore any window that could not be returned.",
                    ));
                }
            }
            Err(error)
        }
    }
}

async fn exchange(
    ops: &mut impl WindowOps,
    dragged: WindowId,
    occupant: WindowId,
    source: &SwapSource,
    destination: &ZoneSlot,
    cancel: &CancelToken,
) -> Result<[Placement; 2], WindowError> {
    cancel.check()?;
    let moved_occupant = ops
        .settle(occupant, source.frame(), Some(source.work_area()), cancel)
        .await?;
    if !source.is_filled_by(moved_occupant) {
        return Err(WindowError::refused(
            "The other window cannot fit where this window was.",
        ));
    }
    let moved_dragged = ops
        .settle(dragged, destination.frame, Some(destination.work_area), cancel)
        .await?;
    if !destination.is_occupied_by(moved_dragged) {
        return Err(WindowError::refused(
            "The dragged window cannot fit the destination zone.",
        ));
    }

    let (final_dragged, final_occupant) = (ops.read(dragged)?, ops.read(occupant)?);
    // Zones never overlap, so overlapping windows mean an app overrode its frame. A
    // floating source may overlap the destination by design.
    let overlapping = matches!(source, SwapSource::Zone(_))
        && final_dragged
            .intersection(final_occupant)
            .is_some_and(|shared| shared.width > ALLOWED_OVERLAP && shared.height > ALLOWED_OVERLAP);
    if !destination.is_occupied_by(final_dragged) || !source.is_filled_by(final_occupant) || overlapping {
        return Err(WindowError::refused(
            "The apps kept overlapping bounds, so the swap was cancelled.",
        ));
    }
    cancel.check()?;
    Ok([
        Placement {
            requested: destination.frame,
            actual: final_dragged,
        },
        Placement {
            requested: source.frame(),
            actual: final_occupant,
        },
    ])
}

/// Undo the newest entry. The entry stays on failure for a retry.
pub async fn undo(
    ops: &mut impl WindowOps,
    history: &mut UndoHistory,
    cancel: &CancelToken,
    protected: &dyn Fn(WindowId) -> bool,
) -> Result<(), WindowError> {
    replay(ops, history, Replay::Undo, cancel, protected).await
}

/// Reapply the newest undone entry, moving both windows of a swap back. The entry stays
/// on failure for a retry.
pub async fn redo(
    ops: &mut impl WindowOps,
    history: &mut UndoHistory,
    cancel: &CancelToken,
    protected: &dyn Fn(WindowId) -> bool,
) -> Result<(), WindowError> {
    replay(ops, history, Replay::Redo, cancel, protected).await
}

/// Return the windows of `which`'s newest entry to its frames, and keep where they were as
/// the entry that reverses it.
async fn replay(
    ops: &mut impl WindowOps,
    history: &mut UndoHistory,
    which: Replay,
    cancel: &CancelToken,
    protected: &dyn Fn(WindowId) -> bool,
) -> Result<(), WindowError> {
    let entry = history.stack(which).back().cloned().ok_or(match which {
        Replay::Undo => WindowError::NoHistory,
        Replay::Redo => WindowError::NoRedo,
    })?;
    let current = current_moves(ops, &entry)?;
    if let [single] = entry.as_slice() {
        ops.settle(single.window, single.original, None, cancel).await?;
        cancel.check()?;
        history.finish(which, current);
        return Ok(());
    }

    for m in &entry {
        let restored = match ops.settle(m.window, m.original, m.work_area, cancel).await {
            Ok(frame) if near(frame, m.original) => Ok(()),
            Ok(_) => Err(WindowError::refused(match which {
                Replay::Undo => "An app could not restore the swap. Undo is available to retry.",
                Replay::Redo => "An app could not redo the swap. Redo is available to retry.",
            })),
            Err(error) => Err(error),
        };
        if let Err(error) = restored {
            recover(ops, &current, protected).await;
            return Err(error);
        }
    }
    cancel.check()?;
    history.finish(which, current);
    Ok(())
}

/// Where the entry's windows are now, each with the entry's work area it overlaps most.
fn current_moves(ops: &mut impl WindowOps, entry: &[Move]) -> Result<Vec<Move>, WindowError> {
    let areas: Vec<Rect> = entry.iter().filter_map(|m| m.work_area).collect();
    entry
        .iter()
        .map(|m| {
            let frame = ops.read(m.window)?;
            let work_area = largest_overlap(frame, &areas)
                .filter(|&i| frame.intersection(areas[i]).is_some())
                .map(|i| areas[i]);
            Ok(Move {
                window: m.window,
                original: frame,
                work_area,
            })
        })
        .collect()
}

/// Move windows back to `moves`, newest first, skipping protected windows. Returns the
/// windows left elsewhere.
async fn recover(
    ops: &mut impl WindowOps,
    moves: &[Move],
    protected: &dyn Fn(WindowId) -> bool,
) -> Vec<WindowId> {
    let mut stranded = Vec::new();
    for m in moves.iter().rev() {
        if protected(m.window) {
            stranded.push(m.window);
            continue;
        }
        let returned = ops
            .settle(m.window, m.original, m.work_area, &CancelToken::default())
            .await;
        if !returned.is_ok_and(|frame| near(frame, m.original)) {
            stranded.push(m.window);
        }
    }
    stranded
}

fn near(a: Rect, b: Rect) -> bool {
    [
        (a.min_x(), b.min_x()),
        (a.min_y(), b.min_y()),
        (a.max_x(), b.max_x()),
        (a.max_y(), b.max_y()),
    ]
    .iter()
    .all(|(x, y)| (x - y).abs() <= RESTORE_TOLERANCE)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::bounds::apply_settled;
    use crate::bounds::tests::{FakeWindow, InstantClock, block_on};

    struct FakeOps {
        windows: BTreeMap<WindowId, FakeWindow>,
        failing: BTreeSet<WindowId>,
        clock: InstantClock,
    }

    impl FakeOps {
        fn new(windows: &[(u64, Rect)]) -> Self {
            Self {
                windows: windows
                    .iter()
                    .map(|(id, frame)| (WindowId(*id), FakeWindow::new(*frame)))
                    .collect(),
                failing: BTreeSet::new(),
                clock: InstantClock::default(),
            }
        }

        fn frame(&self, id: u64) -> Rect {
            self.windows[&WindowId(id)].frame
        }
    }

    impl WindowOps for FakeOps {
        fn read(&mut self, window: WindowId) -> Result<Rect, WindowError> {
            Ok(self.windows[&window].frame)
        }

        async fn settle(
            &mut self,
            window: WindowId,
            frame: Rect,
            work_area: Option<Rect>,
            cancel: &CancelToken,
        ) -> Result<Rect, WindowError> {
            if self.failing.contains(&window) {
                return Err(WindowError::refused("refused"));
            }
            let backend = self.windows.get_mut(&window).ok_or(WindowError::InvalidFrame)?;
            apply_settled(frame, work_area, backend, &self.clock, cancel, &mut Vec::new()).await
        }
    }

    const AREA: Rect = Rect::new(0.0, 25.0, 1440.0, 875.0);
    const LEFT: Rect = Rect::new(12.0, 37.0, 702.0, 851.0);
    const RIGHT: Rect = Rect::new(726.0, 37.0, 702.0, 851.0);

    fn slot(zone: &str, frame: Rect) -> ZoneSlot {
        ZoneSlot {
            display_id: "d".into(),
            zone_id: zone.into(),
            frame,
            work_area: AREA,
        }
    }

    fn unprotected(_: WindowId) -> bool {
        false
    }

    /// Window 1 dragged from its zone onto window 2's.
    fn request<'a>(source: &'a SwapSource, destination: &'a ZoneSlot) -> SwapRequest<'a> {
        SwapRequest {
            dragged: WindowId(1),
            occupant: WindowId(2),
            source,
            destination,
            original: LEFT,
        }
    }

    #[test]
    fn snap_and_undo_restore_the_exact_original() {
        let start = Rect::new(300.0, 200.0, 500.0, 400.0);
        let mut ops = FakeOps::new(&[(1, start)]);
        let mut history = UndoHistory::default();
        let cancel = CancelToken::default();
        block_on(snap(
            &mut ops,
            &mut history,
            WindowId(1),
            LEFT,
            None,
            Some(AREA),
            &cancel,
        ))
        .unwrap();
        assert_eq!(ops.frame(1), LEFT);
        block_on(undo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        assert_eq!(ops.frame(1), start);
        assert!(history.is_empty());
    }

    #[test]
    fn drags_undo_to_the_pre_drag_frame() {
        let pre_drag = Rect::new(50.0, 60.0, 500.0, 400.0);
        let mut ops = FakeOps::new(&[(1, Rect::new(900.0, 300.0, 500.0, 400.0))]);
        let mut history = UndoHistory::default();
        let cancel = CancelToken::default();
        block_on(snap(
            &mut ops,
            &mut history,
            WindowId(1),
            LEFT,
            Some(pre_drag),
            Some(AREA),
            &cancel,
        ))
        .unwrap();
        block_on(undo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        assert_eq!(ops.frame(1), pre_drag);
    }

    #[test]
    fn a_single_move_returns_unfitted_and_stays_when_the_undo_fails() {
        let partly_off_screen = Rect::new(-200.0, 0.0, 500.0, 400.0);
        let mut ops = FakeOps::new(&[(1, Rect::new(300.0, 200.0, 500.0, 400.0))]);
        let mut history = UndoHistory::default();
        let cancel = CancelToken::default();
        block_on(snap(
            &mut ops,
            &mut history,
            WindowId(1),
            LEFT,
            Some(partly_off_screen),
            Some(AREA),
            &cancel,
        ))
        .unwrap();

        ops.failing.insert(WindowId(1));
        assert!(block_on(undo(&mut ops, &mut history, &cancel, &unprotected)).is_err());
        assert_eq!(history.len(), 1, "kept for a retry");
        assert_eq!(ops.frame(1), LEFT);

        ops.failing.clear();
        block_on(undo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        assert_eq!(ops.frame(1), partly_off_screen, "never fitted into the work area");
        assert!(history.is_empty());
    }

    #[test]
    fn failed_snaps_add_no_history() {
        let mut ops = FakeOps::new(&[(1, Rect::new(300.0, 200.0, 500.0, 400.0))]);
        ops.failing.insert(WindowId(1));
        let mut history = UndoHistory::default();
        let result = block_on(snap(
            &mut ops,
            &mut history,
            WindowId(1),
            LEFT,
            None,
            None,
            &CancelToken::default(),
        ));
        assert!(result.is_err());
        assert!(history.is_empty());
    }

    #[test]
    fn undo_history_is_bounded() {
        let mut history = UndoHistory::default();
        for i in 0..60 {
            history.record(vec![Move {
                window: WindowId(i),
                original: LEFT,
                work_area: None,
            }]);
        }
        assert_eq!(history.len(), UNDO_LIMIT);
        assert_eq!(history.next_windows(Replay::Undo), [WindowId(59)]);
    }

    #[test]
    fn swaps_exchange_and_undo_both_windows_at_once() {
        let mut ops = FakeOps::new(&[(1, LEFT), (2, RIGHT)]);
        let mut history = UndoHistory::default();
        let cancel = CancelToken::default();
        let (source, destination) = (SwapSource::Zone(slot("l", LEFT)), slot("r", RIGHT));
        block_on(swap(
            &mut ops,
            &mut history,
            request(&source, &destination),
            &cancel,
            &unprotected,
        ))
        .unwrap();
        assert_eq!((ops.frame(1), ops.frame(2)), (RIGHT, LEFT));
        assert_eq!(history.len(), 1);
        block_on(undo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        assert_eq!((ops.frame(1), ops.frame(2)), (LEFT, RIGHT));
    }

    #[test]
    fn a_floating_window_and_the_occupant_trade_places() {
        let floating = Rect::new(300.0, 200.0, 500.0, 400.0);
        let mut ops = FakeOps::new(&[(1, floating), (2, RIGHT)]);
        let mut history = UndoHistory::default();
        let cancel = CancelToken::default();
        let source = SwapSource::Floating {
            frame: floating,
            work_area: AREA,
        };
        let destination = slot("r", RIGHT);
        let request = SwapRequest {
            original: floating,
            ..request(&source, &destination)
        };
        block_on(swap(&mut ops, &mut history, request, &cancel, &unprotected)).unwrap();
        assert_eq!((ops.frame(1), ops.frame(2)), (RIGHT, floating));
        assert_eq!(history.len(), 1, "one undo entry for both windows");
        block_on(undo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        assert_eq!((ops.frame(1), ops.frame(2)), (floating, RIGHT));
    }

    #[test]
    fn a_failed_second_move_returns_both_windows() {
        let mut ops = FakeOps::new(&[(1, LEFT), (2, RIGHT)]);
        ops.failing.insert(WindowId(1));
        let mut history = UndoHistory::default();
        let (source, destination) = (SwapSource::Zone(slot("l", LEFT)), slot("r", RIGHT));
        let result = block_on(swap(
            &mut ops,
            &mut history,
            request(&source, &destination),
            &CancelToken::default(),
            &unprotected,
        ));
        assert!(result.is_err());
        assert_eq!(ops.frame(2), RIGHT);
    }

    #[test]
    fn protected_windows_are_left_alone_and_undo_stays() {
        let mut ops = FakeOps::new(&[(1, LEFT), (2, RIGHT)]);
        ops.failing.insert(WindowId(1));
        let mut history = UndoHistory::default();
        let (source, destination) = (SwapSource::Zone(slot("l", LEFT)), slot("r", RIGHT));
        let protect_two = |window: WindowId| window == WindowId(2);
        let result = block_on(swap(
            &mut ops,
            &mut history,
            request(&source, &destination),
            &CancelToken::default(),
            &protect_two,
        ));
        assert_eq!(
            result.unwrap_err().to_string(),
            "The swap could not finish. Use Undo to restore any window that could not be returned."
        );
        assert_eq!(
            ops.frame(2),
            LEFT,
            "the protected window stays where the new gesture holds it"
        );
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn forgetting_a_window_drops_its_entries() {
        let mut history = UndoHistory::default();
        history.record(vec![Move {
            window: WindowId(1),
            original: LEFT,
            work_area: None,
        }]);
        history.record(vec![
            Move {
                window: WindowId(1),
                original: LEFT,
                work_area: None,
            },
            Move {
                window: WindowId(2),
                original: RIGHT,
                work_area: None,
            },
        ]);
        history.record(vec![Move {
            window: WindowId(3),
            original: LEFT,
            work_area: None,
        }]);
        history.forget(WindowId(1));
        assert_eq!(history.len(), 1);
    }

    fn snapped_left(start: Rect) -> (FakeOps, UndoHistory) {
        let mut ops = FakeOps::new(&[(1, start)]);
        let mut history = UndoHistory::default();
        block_on(snap(
            &mut ops,
            &mut history,
            WindowId(1),
            LEFT,
            None,
            Some(AREA),
            &CancelToken::default(),
        ))
        .unwrap();
        (ops, history)
    }

    #[test]
    fn redo_reapplies_an_undone_snap_and_undo_works_again_after() {
        let start = Rect::new(300.0, 200.0, 500.0, 400.0);
        let (mut ops, mut history) = snapped_left(start);
        let cancel = CancelToken::default();
        block_on(undo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        assert_eq!((history.len(), history.redo_len()), (0, 1));

        block_on(redo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        assert_eq!(ops.frame(1), LEFT);
        assert_eq!((history.len(), history.redo_len()), (1, 0));

        block_on(undo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        assert_eq!(ops.frame(1), start);
    }

    #[test]
    fn redo_reapplies_an_undone_swap_moving_both_windows_back() {
        let mut ops = FakeOps::new(&[(1, LEFT), (2, RIGHT)]);
        let mut history = UndoHistory::default();
        let cancel = CancelToken::default();
        let (source, destination) = (SwapSource::Zone(slot("l", LEFT)), slot("r", RIGHT));
        block_on(swap(
            &mut ops,
            &mut history,
            request(&source, &destination),
            &cancel,
            &unprotected,
        ))
        .unwrap();
        block_on(undo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        assert_eq!((ops.frame(1), ops.frame(2)), (LEFT, RIGHT));
        assert_eq!(history.next_windows(Replay::Redo), [WindowId(1), WindowId(2)]);

        block_on(redo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        assert_eq!((ops.frame(1), ops.frame(2)), (RIGHT, LEFT));
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn redo_stays_available_when_it_fails() {
        let (mut ops, mut history) = snapped_left(Rect::new(300.0, 200.0, 500.0, 400.0));
        let cancel = CancelToken::default();
        block_on(undo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        ops.failing.insert(WindowId(1));
        assert!(block_on(redo(&mut ops, &mut history, &cancel, &unprotected)).is_err());
        assert_eq!(history.redo_len(), 1);
    }

    #[test]
    fn a_new_move_clears_what_could_be_redone() {
        let (mut ops, mut history) = snapped_left(Rect::new(300.0, 200.0, 500.0, 400.0));
        let cancel = CancelToken::default();
        block_on(undo(&mut ops, &mut history, &cancel, &unprotected)).unwrap();
        block_on(snap(
            &mut ops,
            &mut history,
            WindowId(1),
            RIGHT,
            None,
            Some(AREA),
            &cancel,
        ))
        .unwrap();
        assert_eq!(history.redo_len(), 0);
        assert_eq!(
            block_on(redo(&mut ops, &mut history, &cancel, &unprotected)),
            Err(WindowError::NoRedo)
        );
        assert_eq!(
            WindowError::NoRedo.to_string(),
            "There is no window move to redo."
        );
    }

    #[test]
    fn forgetting_a_window_drops_its_redo_entries() {
        let (mut ops, mut history) = snapped_left(Rect::new(300.0, 200.0, 500.0, 400.0));
        block_on(undo(
            &mut ops,
            &mut history,
            &CancelToken::default(),
            &unprotected,
        ))
        .unwrap();
        history.forget(WindowId(1));
        assert_eq!(history.redo_len(), 0);
    }
}
