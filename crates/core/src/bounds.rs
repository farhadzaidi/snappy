//! Applying a frame to another app's window and waiting for it to settle.
//!
//! Apps often acknowledge a position or size write before their own redraw applies it,
//! and some refuse sizes outright. Every step therefore checks what actually happened
//! before taking the next one.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::WindowError;
use crate::geometry::{Point, Rect, Size};

/// Positions and sizes this close count as equal.
const CLOSE: f64 = 1.0;
/// Delays between re-reads while waiting for a position, in milliseconds.
const POSITION_WAITS: [u64; 3] = [25, 50, 100];
/// Delays between re-reads while waiting for a size, in milliseconds.
const SIZE_WAITS: [u64; 4] = [16, 32, 64, 64];
/// A result further than this from the request was constrained by the app.
pub const CONSTRAINED_THRESHOLD: f64 = 3.0;
/// Apps that size in steps, such as terminals on a character grid, can land this far from
/// a request on each axis. A size this close is the nearest the app allows.
pub const GRID_TOLERANCE: f64 = 32.0;

/// Raw access to one window's bounds.
pub trait BoundsBackend {
    fn read_frame(&mut self) -> Result<Rect, WindowError>;
    fn set_size(&mut self, size: Size) -> Result<(), WindowError>;
    fn set_position(&mut self, origin: Point) -> Result<(), WindowError>;
}

/// Time for settle waits. Tests use a clock that never really sleeps.
pub trait Clock {
    fn sleep(&self, milliseconds: u64) -> impl Future<Output = ()>;
    /// Monotonic milliseconds, for step timings.
    fn now_ms(&self) -> f64;
}

/// Shared flag a newer gesture sets to stop an operation in flight.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub fn check(&self) -> Result<(), WindowError> {
        if self.is_cancelled() {
            Err(WindowError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// One observed stage of a settle, for drop diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundsStep {
    pub stage: &'static str,
    pub frame: Rect,
    pub elapsed_ms: f64,
}

/// Apply `desired` and return the frame the window actually ended with.
pub async fn apply_settled<B: BoundsBackend, C: Clock>(
    desired: Rect,
    work_area: Option<Rect>,
    backend: &mut B,
    clock: &C,
    cancel: &CancelToken,
    steps: &mut Vec<BoundsStep>,
) -> Result<Rect, WindowError> {
    if !desired.is_usable(1.0) {
        return Err(WindowError::InvalidFrame);
    }
    let started = clock.now_ms();
    let mut settle = Settle {
        backend,
        clock,
        cancel,
        steps,
        started,
    };
    settle
        .run(desired, work_area.filter(|area| area.is_usable(1.0)))
        .await
}

struct Settle<'a, B, C> {
    backend: &'a mut B,
    clock: &'a C,
    cancel: &'a CancelToken,
    steps: &'a mut Vec<BoundsStep>,
    started: f64,
}

impl<B: BoundsBackend, C: Clock> Settle<'_, B, C> {
    async fn run(&mut self, desired: Rect, area: Option<Rect>) -> Result<Rect, WindowError> {
        let mut frame = self.read()?;
        self.record("before", frame);

        if !same_size(frame.size(), desired.size()) {
            frame = self.resize_into(frame, desired, area).await?;
        }

        let destination = fitted_origin(desired.origin(), frame.size(), area);
        if !same_origin(frame.origin(), destination) {
            frame = self.move_to(destination).await?;
            self.record("finalPosition", frame);
        }
        self.read()
    }

    async fn resize_into(
        &mut self,
        start: Rect,
        desired: Rect,
        area: Option<Rect>,
    ) -> Result<Rect, WindowError> {
        let mut frame = start;
        let already_there = area.is_none_or(|area| area.contains(start.center()));
        let fits_where_it_is = area
            .is_none_or(|area| area.contains_rect(Rect::from_origin_size(start.origin(), desired.size())));

        // A window coming from another display moves first, so its old size isn't
        // stretched across displays. So does one whose new size wouldn't fit where it is,
        // because the system clips a resize at the screen's edge.
        if !already_there || !fits_where_it_is {
            let staging = fitted_origin(desired.origin(), frame.size(), area);
            if !same_origin(frame.origin(), staging) {
                frame = self.move_to(staging).await?;
                self.record("stagedOnDestination", frame);
            }
        }

        let before_resize = frame;
        let mut baseline = frame.size();
        frame = self.resize(desired.size()).await?;
        self.record("combinedResize", frame);

        // Some apps ignore a resize until the whole window sits on the destination.
        if let Some(area) = area
            && already_there
            && !on_grid(frame.size(), desired.size())
            && same_size(frame.size(), before_resize.size())
            && !area.contains_rect(before_resize)
        {
            let staging = fitted_origin(desired.origin(), frame.size(), Some(area));
            if !same_origin(frame.origin(), staging) {
                frame = self.move_to(staging).await?;
                self.record("stagedOnDestinationAfterIgnoredResize", frame);
                baseline = frame.size();
                frame = self.resize(desired.size()).await?;
                self.record("retriedResizeOnDestination", frame);
            }
        }

        // Some apps ignore a two-axis request; ask for each axis that didn't move alone.
        let both_axes = !close(baseline.width, desired.width)
            && !close(baseline.height, desired.height)
            && !on_grid(frame.size(), desired.size());
        if both_axes && !close(frame.width, desired.width) && close(frame.width, baseline.width) {
            frame = self.resize(Size::new(desired.width, frame.height)).await?;
            self.record("widthResize", frame);
        }
        if both_axes && !close(frame.height, desired.height) && close(frame.height, baseline.height) {
            frame = self.resize(Size::new(frame.width, desired.height)).await?;
            self.record("heightResize", frame);
        }
        Ok(frame)
    }

    async fn move_to(&mut self, origin: Point) -> Result<Rect, WindowError> {
        self.cancel.check()?;
        self.backend.set_position(origin)?;
        let mut frame = self.read()?;
        for wait in POSITION_WAITS {
            if same_origin(frame.origin(), origin) {
                break;
            }
            self.clock.sleep(wait).await;
            frame = self.read()?;
        }
        Ok(frame)
    }

    /// Request `size` and wait until it lands, lands on the app's grid, or a constrained size
    /// holds still.
    async fn resize(&mut self, size: Size) -> Result<Rect, WindowError> {
        let before = self.read()?.size();
        self.cancel.check()?;
        self.backend.set_size(size)?;
        let mut frame = self.read()?;
        if same_size(frame.size(), size) || on_grid(frame.size(), size) {
            return Ok(frame);
        }
        let mut previous = frame.size();
        for wait in SIZE_WAITS {
            self.clock.sleep(wait).await;
            frame = self.read()?;
            if same_size(frame.size(), size) {
                return Ok(frame);
            }
            let acknowledged =
                |actual: f64, requested: f64, old: f64| close(actual, requested) || !close(actual, old);
            let settled_constrained = acknowledged(frame.width, size.width, before.width)
                && acknowledged(frame.height, size.height, before.height)
                && same_size(frame.size(), previous);
            if settled_constrained {
                return Ok(frame);
            }
            previous = frame.size();
        }
        Ok(frame)
    }

    fn read(&mut self) -> Result<Rect, WindowError> {
        self.cancel.check()?;
        let frame = self.backend.read_frame()?;
        if frame.is_usable(1.0) {
            Ok(frame)
        } else {
            Err(WindowError::InvalidFrame)
        }
    }

    fn record(&mut self, stage: &'static str, frame: Rect) {
        let elapsed_ms = self.clock.now_ms() - self.started;
        self.steps.push(BoundsStep {
            stage,
            frame,
            elapsed_ms,
        });
    }
}

/// `origin`, pulled inside `area` so a window of `size` fits where possible.
pub fn fitted_origin(origin: Point, size: Size, area: Option<Rect>) -> Point {
    let Some(area) = area.filter(|area| area.is_usable(1.0)) else {
        return origin;
    };
    Point::new(
        origin
            .x
            .max(area.min_x())
            .min((area.max_x() - size.width).max(area.min_x())),
        origin
            .y
            .max(area.min_y())
            .min((area.max_y() - size.height).max(area.min_y())),
    )
}

/// True when the app didn't give the window the requested frame.
pub fn is_constrained(requested: Rect, actual: Rect) -> bool {
    [
        (requested.x, actual.x),
        (requested.y, actual.y),
        (requested.width, actual.width),
        (requested.height, actual.height),
    ]
    .iter()
    .any(|(a, b)| (a - b).abs() > CONSTRAINED_THRESHOLD)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= CLOSE
}

fn same_size(a: Size, b: Size) -> bool {
    close(a.width, b.width) && close(a.height, b.height)
}

/// True when `actual` is as close to `requested` as an app sizing in steps can get.
fn on_grid(actual: Size, requested: Size) -> bool {
    (actual.width - requested.width).abs() <= GRID_TOLERANCE
        && (actual.height - requested.height).abs() <= GRID_TOLERANCE
}

fn same_origin(a: Point, b: Point) -> bool {
    close(a.x, b.x) && close(a.y, b.y)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::Cell;

    use super::*;

    /// A clock whose sleeps return at once and advance time.
    #[derive(Default)]
    pub struct InstantClock(Cell<f64>);

    impl Clock for InstantClock {
        async fn sleep(&self, milliseconds: u64) {
            self.0.set(self.0.get() + milliseconds as f64);
        }

        fn now_ms(&self) -> f64 {
            self.0.get()
        }
    }

    /// A window that can lag, refuse, and constrain like real apps.
    #[derive(Debug, Default)]
    pub struct FakeWindow {
        pub frame: Rect,
        pub screen: Rect,
        pub min_size: Size,
        /// Resizes only apply when the window is fully on `screen`.
        pub strict_resize: bool,
        /// Two-axis resizes are ignored.
        pub one_axis_only: bool,
        /// Writes land on the next read instead of immediately.
        pub deferred: bool,
        /// Sizes round to the nearest multiple of this cell, like a terminal.
        pub cell: Option<Size>,
        /// Resizes stop at `screen`'s edges, as macOS clips them.
        pub clip_to_screen: bool,
        pending_size: Option<Size>,
        pending_origin: Option<Point>,
        pub writes: Vec<&'static str>,
        pub fail_next_size: bool,
    }

    impl FakeWindow {
        pub fn new(frame: Rect) -> Self {
            Self {
                frame,
                screen: Rect::new(-10_000.0, -10_000.0, 20_000.0, 20_000.0),
                ..Self::default()
            }
        }
    }

    impl BoundsBackend for FakeWindow {
        fn read_frame(&mut self) -> Result<Rect, WindowError> {
            if let Some(size) = self.pending_size.take() {
                self.frame = Rect::from_origin_size(self.frame.origin(), size);
            }
            if let Some(origin) = self.pending_origin.take() {
                self.frame = self.frame.with_origin(origin);
            }
            Ok(self.frame)
        }

        fn set_size(&mut self, size: Size) -> Result<(), WindowError> {
            self.writes.push("size");
            if std::mem::take(&mut self.fail_next_size) {
                return Err(WindowError::refused("refused"));
            }
            let two_axis =
                (size.width - self.frame.width).abs() > 1.0 && (size.height - self.frame.height).abs() > 1.0;
            if (self.strict_resize && !self.screen.contains_rect(self.frame))
                || (self.one_axis_only && two_axis)
            {
                return Ok(());
            }
            let mut accepted = Size::new(
                size.width.max(self.min_size.width),
                size.height.max(self.min_size.height),
            );
            if self.clip_to_screen {
                accepted = Size::new(
                    accepted.width.min(self.screen.max_x() - self.frame.x),
                    accepted.height.min(self.screen.max_y() - self.frame.y),
                );
            }
            if let Some(cell) = self.cell {
                accepted = Size::new(
                    (accepted.width / cell.width).round() * cell.width,
                    (accepted.height / cell.height).round() * cell.height,
                );
            }
            if self.deferred {
                self.pending_size = Some(accepted);
            } else {
                self.frame = Rect::from_origin_size(self.frame.origin(), accepted);
            }
            Ok(())
        }

        fn set_position(&mut self, origin: Point) -> Result<(), WindowError> {
            self.writes.push("position");
            if self.deferred {
                self.pending_origin = Some(origin);
            } else {
                self.frame = self.frame.with_origin(origin);
            }
            Ok(())
        }
    }

    pub fn block_on<F: Future>(future: F) -> F::Output {
        use std::pin::pin;
        use std::task::{Context, Poll, Waker};
        let mut future = pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
                return output;
            }
        }
    }

    fn settle(window: &mut FakeWindow, desired: Rect, area: Option<Rect>) -> Result<Rect, WindowError> {
        let mut steps = Vec::new();
        block_on(apply_settled(
            desired,
            area,
            window,
            &InstantClock::default(),
            &CancelToken::default(),
            &mut steps,
        ))
    }

    const AREA: Rect = Rect::new(0.0, 25.0, 1440.0, 875.0);

    #[test]
    fn shrinking_on_the_same_display_resizes_before_moving() {
        let mut window = FakeWindow::new(Rect::new(100.0, 100.0, 1000.0, 700.0));
        let target = Rect::new(12.0, 37.0, 702.0, 600.0);
        assert_eq!(settle(&mut window, target, Some(AREA)).unwrap(), target);
        assert_eq!(window.writes, ["size", "position"]);
    }

    #[test]
    fn a_size_that_would_cross_the_screen_edge_moves_first() {
        let mut window = FakeWindow::new(Rect::new(300.0, 500.0, 400.0, 300.0));
        window.screen = AREA;
        window.clip_to_screen = true;
        let target = Rect::new(12.0, 37.0, 702.0, 851.0);
        assert_eq!(settle(&mut window, target, Some(AREA)).unwrap(), target);
        assert_eq!(window.writes, ["position", "size"]);
    }

    #[test]
    fn same_size_moves_go_straight_there() {
        let mut window = FakeWindow::new(Rect::new(100.0, 100.0, 702.0, 851.0));
        settle(&mut window, Rect::new(12.0, 37.0, 702.0, 851.0), Some(AREA)).unwrap();
        assert_eq!(window.writes, ["position"]);
    }

    #[test]
    fn strict_apps_are_staged_on_the_destination_and_retried() {
        let mut window = FakeWindow::new(Rect::new(1200.0, 100.0, 900.0, 700.0));
        window.screen = AREA;
        window.strict_resize = true;
        let target = Rect::new(726.0, 37.0, 702.0, 851.0);
        assert_eq!(settle(&mut window, target, Some(AREA)).unwrap(), target);
    }

    #[test]
    fn ignored_two_axis_requests_retry_each_axis() {
        let mut window = FakeWindow::new(Rect::new(100.0, 100.0, 1000.0, 700.0));
        window.one_axis_only = true;
        let target = Rect::new(12.0, 37.0, 702.0, 851.0);
        assert_eq!(settle(&mut window, target, Some(AREA)).unwrap(), target);
    }

    #[test]
    fn deferred_apps_settle_before_the_final_move() {
        let mut window = FakeWindow::new(Rect::new(100.0, 100.0, 1000.0, 700.0));
        window.deferred = true;
        let target = Rect::new(12.0, 37.0, 702.0, 851.0);
        assert_eq!(settle(&mut window, target, Some(AREA)).unwrap(), target);
    }

    #[test]
    fn grid_sized_apps_settle_without_waiting() {
        // Already at the grid size nearest the request, as terminals often are.
        let target = Rect::new(12.0, 37.0, 702.0, 851.0);
        let cell = Size::new(7.0, 17.0);
        for start in [
            Rect::new(300.0, 300.0, 700.0, 850.0),
            Rect::new(300.0, 300.0, 490.0, 340.0),
        ] {
            let mut window = FakeWindow::new(start);
            window.cell = Some(cell);
            let clock = InstantClock::default();
            let actual = block_on(apply_settled(
                target,
                Some(AREA),
                &mut window,
                &clock,
                &CancelToken::default(),
                &mut Vec::new(),
            ))
            .unwrap();
            assert_eq!(actual.origin(), target.origin());
            let resizes = window.writes.iter().filter(|write| **write == "size").count();
            assert_eq!(resizes, 1, "no retries: {:?}", window.writes);
            assert_eq!(clock.now_ms(), 0.0, "no settle waits");
        }
    }

    #[test]
    fn minimum_sizes_are_accepted_and_reported() {
        let mut window = FakeWindow::new(Rect::new(100.0, 100.0, 1000.0, 700.0));
        window.min_size = Size::new(800.0, 300.0);
        let target = Rect::new(12.0, 37.0, 702.0, 400.0);
        let actual = settle(&mut window, target, Some(AREA)).unwrap();
        assert_eq!(actual.width, 800.0);
        assert!(is_constrained(target, actual));
        assert!(
            window.writes.len() <= 4,
            "no endless retries: {:?}",
            window.writes
        );
    }

    #[test]
    fn unusable_frames_never_write() {
        let mut window = FakeWindow::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(
            settle(&mut window, Rect::new(0.0, 0.0, 0.5, 10.0), None),
            Err(WindowError::InvalidFrame)
        );
        assert!(window.writes.is_empty());
    }

    #[test]
    fn cancellation_stops_before_the_next_write() {
        let mut window = FakeWindow::new(Rect::new(100.0, 100.0, 1000.0, 700.0));
        let cancel = CancelToken::default();
        cancel.cancel();
        let mut steps = Vec::new();
        let result = block_on(apply_settled(
            Rect::new(12.0, 37.0, 702.0, 851.0),
            Some(AREA),
            &mut window,
            &InstantClock::default(),
            &cancel,
            &mut steps,
        ));
        assert_eq!(result, Err(WindowError::Cancelled));
        assert!(window.writes.is_empty());
    }
}
