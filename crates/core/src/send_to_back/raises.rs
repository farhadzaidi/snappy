//! The raises that send a window to the back where a platform can raise another app's
//! window but never lower one, as on macOS, with the order the platform shows after each.
//!
//! There a raise lands by its app: the active app's raise goes to the very front, and
//! another app's lands just under the active app's front window. So the window that takes
//! focus is focused first, which activates its app, and every later raise lands under it.
//! It's the front of its pile once the window has gone back (`SendToBack::focus`), so it
//! shows in front at once, as it ends. The others raised (`raised_over`) follow, each
//! landing just under it, behind each it's to end behind before it, so they end in the
//! order they were. The window sent back is never raised, and ends up behind them all. A
//! raised window of the focused window's own app lands in front of it instead, so that one
//! is raised again straight after it.
//!
//! An app takes its own raises in turn, but raises to different apps land in no set order,
//! so a raise waits for the one before to show only when it's another app's (`awaits`).
//! Whichever window is free to go next, a window of the app raised last goes first, so one
//! app's raises go out together and show together, and the waits are as few as they can be.

use super::overlap::overlaps;
use super::plan::{SendToBack, StackedWindow};
use super::raised_over::raised_over;
use crate::window::{WindowId, WindowSnapshot};

/// One step, and the order it leaves the windows of the plan's stack in, front to back.
#[derive(Debug, Clone, PartialEq)]
pub struct RaiseStep {
    pub request: Step,
    pub order_after: Vec<WindowSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Focus the window, which raises it and activates its app.
    Focus(WindowId),
    /// Raise the window alone.
    Raise(WindowId),
}

impl Step {
    pub fn window(self) -> WindowId {
        match self {
            Step::Focus(window) | Step::Raise(window) => window,
        }
    }
}

/// Where in the order a raise of the active app's window lands.
const ACTIVE_APP_LANDS: usize = 0;
/// Where in the order a raise of another app's window lands, just under the active app's
/// front window.
const OTHER_APP_LANDS: usize = 1;

/// The raises that send a window behind the windows it covers, by raising them over it.
#[derive(Debug, Clone, PartialEq)]
pub struct Raises {
    sent: StackedWindow,
    /// The window sent back, the window focused, and the windows raised, in their stack order
    /// before, front to back.
    stack: Vec<StackedWindow>,
    steps: Vec<RaiseStep>,
}

impl Raises {
    /// The raises that carry out `send`, with `stacked` the windows that can be raised,
    /// frontmost first.
    pub fn plan(send: &SendToBack, stacked: &[StackedWindow]) -> Raises {
        let raised = raised_over(send, stacked);
        let (sent, focus) = (send.window(), send.focus());
        let stack: Vec<StackedWindow> = stacked
            .iter()
            .filter(|listed| listed.id == sent.id || listed.id == focus.id || raised.contains(listed))
            .copied()
            .collect();
        let mut steps = Steps {
            order: stack.clone(),
            steps: Vec::new(),
        };
        steps.take(Step::Focus(focus.id), ACTIVE_APP_LANDS);
        let others = raised
            .into_iter()
            .filter(|listed| listed.id != focus.id)
            .collect();
        for window in raise_order(others, focus.pid) {
            if window.pid == focus.pid {
                steps.take(Step::Raise(window.id), ACTIVE_APP_LANDS);
                steps.take(Step::Raise(focus.id), ACTIVE_APP_LANDS);
            } else {
                steps.take(Step::Raise(window.id), OTHER_APP_LANDS);
            }
        }
        Raises {
            sent,
            stack,
            steps: steps.steps,
        }
    }

    /// The steps, in the order they go out.
    pub fn steps(&self) -> &[RaiseStep] {
        &self.steps
    }

    /// Whether `next` waits until the window list shows `step`'s order: when it raises
    /// another app's window, which could land first, or when `step` focuses a window of an
    /// app other than the window sent back's, as every raise after counts on that app
    /// being active.
    pub fn awaits(&self, step: &RaiseStep, next: &RaiseStep) -> bool {
        let app = |window: WindowId| self.app(window);
        let window = step.request.window();
        let activates = matches!(step.request, Step::Focus(_)) && app(window) != Some(self.sent.pid);
        activates || app(window) != app(next.request.window())
    }

    /// The windows in the stack of the app that owns `window`, `window` among them.
    pub fn windows_of_its_app(&self, window: WindowId) -> Vec<WindowId> {
        let app = self.app(window);
        self.stack
            .iter()
            .filter(|listed| Some(listed.pid) == app)
            .map(|listed| listed.id)
            .collect()
    }

    fn app(&self, window: WindowId) -> Option<u32> {
        self.stack
            .iter()
            .find(|listed| listed.id == window)
            .map(|listed| listed.pid)
    }
}

/// The order to raise `left`, listed front to back as they're to end, each landing in front
/// of those raised before: every window after each window behind it that it overlaps, and,
/// of the windows free to go, one of `app`, the app raised last, before the backmost.
fn raise_order(mut left: Vec<StackedWindow>, mut app: u32) -> Vec<StackedWindow> {
    let mut order = Vec::with_capacity(left.len());
    while let Some(backmost) = left.len().checked_sub(1) {
        let free = |index: usize| {
            left[index + 1..]
                .iter()
                .all(|behind| !overlaps(left[index].frame, behind.frame))
        };
        let next = (0..backmost)
            .rev()
            .find(|&index| left[index].pid == app && free(index))
            .filter(|_| left[backmost].pid != app)
            .unwrap_or(backmost);
        let window = left.remove(next);
        app = window.pid;
        order.push(window);
    }
    order
}

/// The steps so far, and the order they leave the windows in.
struct Steps {
    order: Vec<StackedWindow>,
    steps: Vec<RaiseStep>,
}

impl Steps {
    /// Take a step whose raise lands its window at `place` in the order.
    fn take(&mut self, request: Step, place: usize) {
        let Some(at) = self.order.iter().position(|listed| listed.id == request.window()) else {
            return;
        };
        let window = self.order.remove(at);
        self.order.insert(place, window);
        self.steps.push(RaiseStep {
            request,
            order_after: self.order.iter().map(|listed| listed.snapshot()).collect(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::ZONE;
    use super::*;
    use crate::geometry::Rect;

    /// Window `id` of process `pid`, cascaded in the zone: window 0 fills it, and each after
    /// is taller than the one before, from the zone's top, so a band of it shows below the
    /// ones in front.
    fn cascaded(id: u64, pid: u32) -> StackedWindow {
        let height = if id == 0 { ZONE.height } else { id as f64 * 100.0 };
        StackedWindow {
            id: WindowId(id),
            pid,
            frame: Rect::new(ZONE.x, ZONE.y, ZONE.width, height),
        }
    }

    fn stack(pids: &[u32]) -> Vec<StackedWindow> {
        let mut stacked = vec![cascaded(0, 10)];
        stacked.extend(pids.iter().zip(1..).map(|(pid, id)| cascaded(id, *pid)));
        stacked
    }

    /// The raises that send window 0, of process 10, in front, behind windows of processes
    /// `pids`, stacked front to back.
    fn raises(pids: &[u32]) -> Raises {
        let stacked = stack(pids);
        Raises::plan(&SendToBack::plan(WindowId(0), &stacked).unwrap(), &stacked)
    }

    fn requests(raises: &Raises) -> Vec<Step> {
        raises.steps().iter().map(|step| step.request).collect()
    }

    fn order(step: &RaiseStep) -> Vec<u64> {
        step.order_after.iter().map(|window| window.id.0).collect()
    }

    fn last_order(raises: &Raises) -> Vec<u64> {
        raises.steps().last().map(order).unwrap_or_default()
    }

    fn focus(id: u64) -> Step {
        Step::Focus(WindowId(id))
    }

    fn raise(id: u64) -> Step {
        Step::Raise(WindowId(id))
    }

    #[test]
    fn one_covered_window_is_only_focused() {
        let raises = raises(&[20]);
        assert_eq!(requests(&raises), [focus(1)]);
        assert_eq!(last_order(&raises), [1, 0]);
        assert_eq!(
            raises.steps()[0].order_after[0],
            cascaded(1, 20).snapshot(),
            "with its frame"
        );
    }

    #[test]
    fn the_others_are_raised_back_to_front_each_under_the_focused_window() {
        let raises = raises(&[20, 30, 40]);
        assert_eq!(requests(&raises), [focus(1), raise(3), raise(2)]);
        let orders: Vec<Vec<u64>> = raises.steps().iter().map(order).collect();
        assert_eq!(orders, [vec![1, 0, 2, 3], vec![1, 3, 0, 2], vec![1, 2, 3, 0]]);
    }

    #[test]
    fn a_covered_window_of_the_focused_windows_app_is_raised_over_it_and_that_one_raised_again() {
        let raises = raises(&[20, 30, 20]);
        assert_eq!(requests(&raises), [focus(1), raise(3), raise(1), raise(2)]);
        assert_eq!(order(&raises.steps()[1]), [3, 1, 0, 2], "for a moment in front");
        assert_eq!(last_order(&raises), [1, 2, 3, 0]);
    }

    #[test]
    fn never_the_window_sent_back_though_its_app_has_a_covered_window() {
        let raises = raises(&[20, 30, 10]);
        assert_eq!(requests(&raises), [focus(1), raise(3), raise(2)]);
    }

    #[test]
    fn windows_of_one_app_end_up_in_the_same_order() {
        assert_eq!(last_order(&raises(&[10, 10, 10])), [1, 2, 3, 0]);
    }

    /// Window 0 over three windows side by side, 1 to 3, which overlap none but it.
    fn side_by_side(pids: [u32; 3]) -> Raises {
        let mut stacked = vec![StackedWindow {
            frame: Rect::new(0.0, 0.0, 900.0, 300.0),
            ..cascaded(0, 10)
        }];
        for (id, pid) in (1..).zip(pids) {
            stacked.push(StackedWindow {
                id: WindowId(id),
                pid,
                frame: Rect::new((id - 1) as f64 * 300.0, 0.0, 300.0, 600.0),
            });
        }
        Raises::plan(&SendToBack::plan(WindowId(0), &stacked).unwrap(), &stacked)
    }

    #[test]
    fn windows_that_overlap_no_other_raised_go_out_by_app_the_focused_ones_first() {
        let raises = side_by_side([20, 20, 30]);
        assert_eq!(requests(&raises), [focus(1), raise(2), raise(1), raise(3)]);
        let raises = side_by_side([20, 30, 40]);
        assert_eq!(requests(&raises), [focus(1), raise(3), raise(2)], "back to front");
    }

    #[test]
    fn a_window_waits_for_every_window_behind_it_that_it_overlaps_whatever_its_app() {
        let raises = raises(&[20, 20, 30]);
        assert_eq!(requests(&raises), [focus(1), raise(3), raise(2), raise(1)]);
    }

    /// Which steps the next waits for, of the raises that send window 0 behind windows of
    /// processes `pids`.
    fn awaited(raises: &Raises) -> Vec<bool> {
        raises
            .steps()
            .windows(2)
            .map(|pair| raises.awaits(&pair[0], &pair[1]))
            .collect()
    }

    #[test]
    fn a_raise_of_another_apps_window_waits_for_the_raise_before() {
        // Focus 1, raise 3 under it, then 2 under it.
        assert_eq!(awaited(&raises(&[20, 30, 40])), [true, true]);
    }

    #[test]
    fn raises_of_one_apps_windows_go_one_straight_after_another() {
        // Focus 1, then raise 3, 1, 2, and 1, all of process 10, as is window 0.
        assert_eq!(awaited(&raises(&[10, 10, 10])), [false, false, false, false]);
    }

    #[test]
    fn a_focus_that_activates_an_app_is_awaited_even_by_that_apps_next_raise() {
        // Focus 1, then raise 3 and 1 again, all of process 20, then 2 of process 30.
        assert_eq!(awaited(&raises(&[20, 30, 20])), [true, false, true]);
    }

    #[test]
    fn a_window_that_cant_be_raised_takes_its_apps_other_windows_out_with_it() {
        let raises = raises(&[20, 30, 20]);
        assert_eq!(raises.windows_of_its_app(WindowId(3)), [WindowId(1), WindowId(3)]);
    }

    #[test]
    fn the_floating_windows_second_press_raises_it_under_the_windows_that_stay_in_front_of_it() {
        // 2, of process 20, goes behind the floating window 1, of process 20 too, which 3,
        // of process 30, and 4, of process 20, stay in front of.
        let stacked = [
            (2, 20, Rect::new(600.0, 450.0, 600.0, 450.0)),
            (3, 30, Rect::new(0.0, 450.0, 600.0, 450.0)),
            (4, 20, Rect::new(0.0, 0.0, 600.0, 450.0)),
            (1, 20, Rect::new(300.0, 200.0, 600.0, 500.0)),
        ]
        .map(|(id, pid, frame)| StackedWindow {
            id: WindowId(id),
            pid,
            frame,
        });
        let raises = Raises::plan(&SendToBack::plan(WindowId(2), &stacked).unwrap(), &stacked);
        assert_eq!(requests(&raises), [focus(3), raise(1), raise(4)]);
        assert_eq!(last_order(&raises), [3, 4, 1, 2]);
        assert_eq!(awaited(&raises), [true, false], "focusing 3 activates its app");
    }
}
