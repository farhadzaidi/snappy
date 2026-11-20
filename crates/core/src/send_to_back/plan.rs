//! Sending a window to the back: behind every window it covers, the ones behind it that
//! overlap it, so they show without anything moving. It's the only window that moves: every
//! other keeps its place in front of or behind each window it overlaps. Focus then goes to a
//! window over where it was, one that focusing, which raises a window, puts in front of no
//! window that was in front of it, so press after press goes through the windows stacked
//! there.

use super::overlap::overlaps;
use crate::geometry::Rect;
use crate::window::{WindowId, WindowSnapshot};

/// A window in the stack, as sending one to the back sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackedWindow {
    pub id: WindowId,
    /// Its app's process. Where windows go to the back by being raised over, as on macOS, a
    /// raise lands by its app.
    pub pid: u32,
    pub frame: Rect,
}

impl StackedWindow {
    pub fn snapshot(self) -> WindowSnapshot {
        WindowSnapshot {
            id: self.id,
            frame: self.frame,
        }
    }
}

/// Sending one window behind the windows it covers.
#[derive(Debug, Clone, PartialEq)]
pub struct SendToBack {
    window: StackedWindow,
    /// The windows it covers, front to back. Never empty.
    covered: Vec<WindowId>,
    focus: StackedWindow,
    /// The window just in front of it, if a window in front of it overlaps it.
    under: Option<WindowId>,
}

impl SendToBack {
    /// Sending `window` behind what it covers. `stacked` lists the windows that can be
    /// restacked, frontmost first. `None` when it isn't listed, so what's behind it is
    /// unknown, or when it covers nothing.
    pub fn plan(window: WindowId, stacked: &[StackedWindow]) -> Option<SendToBack> {
        let sent = stacked.iter().position(|listed| listed.id == window)?;
        let frame = stacked[sent].frame;
        let covered: Vec<WindowId> = stacked[sent + 1..]
            .iter()
            .filter(|listed| overlaps(listed.frame, frame))
            .map(|listed| listed.id)
            .collect();
        if covered.is_empty() {
            return None;
        }
        let others: Vec<StackedWindow> = stacked
            .iter()
            .filter(|listed| listed.id != window)
            .copied()
            .collect();
        let overlapped = stacked[..sent].iter().any(|listed| overlaps(listed.frame, frame));
        Some(SendToBack {
            window: stacked[sent],
            covered,
            focus: focus_target(&others, frame)?,
            under: overlapped.then(|| stacked[sent - 1].id),
        })
    }

    /// The window sent to the back.
    pub fn window(&self) -> StackedWindow {
        self.window
    }

    /// The windows it goes behind, front to back.
    pub fn covered(&self) -> &[WindowId] {
        &self.covered
    }

    /// The window that takes focus: the frontmost, once the window has gone back, of the
    /// windows over its frame, unless a window in front of that one overlaps it, which
    /// focusing would raise it over. Then it's the frontmost of those, and so on up, so the
    /// window focused is the front of its pile, as the platforms that raise what they focus
    /// leave it.
    pub fn focus(&self) -> StackedWindow {
        self.focus
    }

    /// The order that shows the send took: the window focused in front of the window sent
    /// back, whatever else in the stack moved or closed meanwhile.
    pub fn focus_in_front(&self) -> [WindowSnapshot; 2] {
        [self.focus.snapshot(), self.window.snapshot()]
    }

    /// What's remembered once it's done.
    pub fn sent(&self) -> Sent {
        Sent {
            window: self.window.id,
            focus: self.focus.id,
            under: self.under,
        }
    }
}

/// A window sent to the back, the window it focused, and where putting it back leaves it:
/// focused, which brings it to the front, and then back under `under`, the window just in
/// front of it, if a window in front of it overlapped it. Otherwise the front is where it
/// was among the windows it overlaps, and restacking it under one it doesn't overlap could
/// leave it behind the window focusing raised meanwhile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sent {
    pub window: WindowId,
    pub focus: WindowId,
    pub under: Option<WindowId>,
}

impl Sent {
    /// The window the next press sends to the back: `focused`, unless that's the window
    /// this sent back, still behind the window it focused. Platforms report that focus a
    /// moment later, so a press quicker than that goes on with the window focused. Once the
    /// window sent back is in front of it again, as clicking it puts it, a press sends the
    /// focused window. `stacked` lists the windows frontmost first.
    pub fn next_target(&self, focused: WindowId, stacked: &[StackedWindow]) -> WindowId {
        if self.window != focused {
            return focused;
        }
        let place = |id: WindowId| stacked.iter().position(|listed| listed.id == id);
        match (place(self.focus), place(self.window)) {
            (Some(focus), Some(sent)) if focus < sent => self.focus,
            _ => focused,
        }
    }

    /// What's remembered once the window is put back: as if a send had sent the window
    /// focused meanwhile back and focused this one, so a press before that change of focus
    /// is reported sends this one, not the window still reported focused (`next_target`).
    pub fn put_back(self) -> Sent {
        Sent {
            window: self.focus,
            focus: self.window,
            under: None,
        }
    }
}

/// Which of `others`, the stack front to back without the window sent back, takes focus once
/// that window, at `frame`, goes behind them (`SendToBack::focus`).
fn focus_target(others: &[StackedWindow], frame: Rect) -> Option<StackedWindow> {
    let mut target = others.iter().position(|listed| overlaps(listed.frame, frame))?;
    while let Some(above) = others[..target]
        .iter()
        .position(|listed| overlaps(listed.frame, others[target].frame))
    {
        target = above;
    }
    Some(others[target])
}

#[cfg(test)]
mod tests {
    use super::super::overlap::MIN_OVERLAP;
    use super::super::test_support::{NEIGHBOR, ZONE, ids, window};
    use super::*;

    /// Windows 1, 2, and 3 stacked in one zone, front to back, with 4 in the zone beside.
    fn three_stacked() -> Vec<StackedWindow> {
        vec![
            window(1, ZONE),
            window(4, NEIGHBOR),
            window(2, ZONE),
            window(3, Rect::new(100.0, 100.0, 500.0, 400.0)),
        ]
    }

    fn covered(window: u64, stacked: &[StackedWindow]) -> Vec<u64> {
        SendToBack::plan(WindowId(window), stacked)
            .map(|plan| ids(plan.covered().iter().copied()))
            .unwrap_or_default()
    }

    fn focus(window: u64, stacked: &[StackedWindow]) -> Option<u64> {
        SendToBack::plan(WindowId(window), stacked).map(|plan| plan.focus().id.0)
    }

    #[test]
    fn a_window_goes_behind_the_windows_behind_it_that_overlap_it_and_the_first_takes_focus() {
        let stacked = three_stacked();
        let plan = SendToBack::plan(WindowId(1), &stacked).unwrap();
        assert_eq!(plan.window(), stacked[0]);
        assert_eq!(plan.covered(), [WindowId(2), WindowId(3)]);
        assert_eq!(plan.focus(), stacked[2]);
        assert_eq!(
            plan.focus_in_front(),
            [stacked[2].snapshot(), stacked[0].snapshot()]
        );
        assert_eq!(
            plan.sent(),
            Sent {
                window: WindowId(1),
                focus: WindowId(2),
                under: None,
            }
        );
    }

    #[test]
    fn windows_in_front_of_it_stay_there() {
        let mut stacked = three_stacked();
        stacked.push(window(5, ZONE));
        assert_eq!(covered(2, &stacked), [3, 5], "not 1, in front of 2");
    }

    #[test]
    fn a_window_missing_from_the_list_goes_nowhere() {
        assert_eq!(SendToBack::plan(WindowId(9), &three_stacked()), None);
    }

    #[test]
    fn windows_that_touch_or_share_only_a_sliver_are_not_covered() {
        let focused = window(1, ZONE);
        let touching = window(2, Rect::new(ZONE.max_x(), 37.0, 400.0, 400.0));
        let sliver = window(3, Rect::new(ZONE.max_x() - MIN_OVERLAP + 1.0, 37.0, 400.0, 400.0));
        let overlapping = window(4, Rect::new(ZONE.max_x() - MIN_OVERLAP, 37.0, 400.0, 400.0));
        assert_eq!(covered(1, &[focused, touching, sliver, overlapping]), [4]);
    }

    #[test]
    fn a_window_covering_nothing_has_nothing_to_go_behind() {
        let alone = [window(1, ZONE), window(4, NEIGHBOR)];
        assert_eq!(SendToBack::plan(WindowId(1), &alone), None);
        assert_eq!(
            SendToBack::plan(WindowId(3), &three_stacked()),
            None,
            "the backmost"
        );
    }

    #[test]
    fn a_window_over_its_frame_still_in_front_of_it_takes_focus() {
        let over = window(5, Rect::new(300.0, 300.0, 200.0, 200.0));
        let mut stacked = three_stacked();
        stacked.insert(0, over);
        assert_eq!(focus(1, &stacked), Some(5), "in front of 2 once 1 goes back");
    }

    /// Three windows in their own zones, top left, bottom left, and bottom middle, front to
    /// back, and a window floating over all three in front.
    const TOP_LEFT: Rect = Rect::new(0.0, 0.0, 600.0, 450.0);
    const BOTTOM_LEFT: Rect = Rect::new(0.0, 450.0, 600.0, 450.0);
    const BOTTOM_MIDDLE: Rect = Rect::new(600.0, 450.0, 600.0, 450.0);
    const FLOATING: Rect = Rect::new(300.0, 200.0, 600.0, 500.0);

    fn floating_over_three_zones() -> Vec<StackedWindow> {
        vec![
            window(1, FLOATING),
            window(2, BOTTOM_MIDDLE),
            window(3, BOTTOM_LEFT),
            window(4, TOP_LEFT),
        ]
    }

    #[test]
    fn a_floating_window_goes_behind_every_zone_it_covers_and_the_frontmost_takes_focus() {
        let stacked = floating_over_three_zones();
        assert_eq!(covered(1, &stacked), [2, 3, 4]);
        assert_eq!(focus(1, &stacked), Some(2));
    }

    #[test]
    fn focus_never_raises_a_window_over_windows_that_were_in_front_of_it() {
        // Once the floating window went back, the bottom middle zone's window, in front,
        // goes behind it in turn. The floating window is then the only window over its
        // frame, but the two zones' windows in front of it overlap it, so the frontmost of
        // those takes focus, and the floating window stays behind both.
        let stacked = [
            window(2, BOTTOM_MIDDLE),
            window(3, BOTTOM_LEFT),
            window(4, TOP_LEFT),
            window(1, FLOATING),
        ];
        assert_eq!(covered(2, &stacked), [1]);
        assert_eq!(focus(2, &stacked), Some(3));
    }

    #[test]
    fn focus_goes_on_up_past_every_window_that_would_be_raised_over() {
        // 3 overlaps 2, the window over 1's frame; 5 overlaps 3 but not 2.
        let stacked = [
            window(5, Rect::new(1000.0, 0.0, 300.0, 300.0)),
            window(3, Rect::new(500.0, 0.0, 600.0, 400.0)),
            window(1, ZONE),
            window(2, ZONE),
        ];
        assert_eq!(focus(1, &stacked), Some(5));
    }

    #[test]
    fn a_window_in_front_of_them_all_goes_back_in_front_and_one_behind_another_goes_back_under_it() {
        let stacked = three_stacked();
        let front = SendToBack::plan(WindowId(1), &stacked).unwrap();
        assert_eq!(front.sent().under, None);
        let under_one = SendToBack::plan(WindowId(2), &stacked).unwrap();
        assert_eq!(
            under_one.sent().under,
            Some(WindowId(4)),
            "1, in front, overlaps it"
        );
    }

    #[test]
    fn a_window_behind_only_windows_that_dont_overlap_it_goes_back_in_front() {
        // 4, in the zone beside, is in front of 1, and 2 behind it takes focus, which
        // raises 2 over 4: under 4, 1 would end behind 2.
        let stacked = [window(4, NEIGHBOR), window(1, ZONE), window(2, ZONE)];
        let plan = SendToBack::plan(WindowId(1), &stacked).unwrap();
        assert_eq!(plan.sent().under, None);
    }

    #[test]
    fn once_put_back_the_next_press_before_focus_follows_sends_the_window_put_back() {
        let sent = SendToBack::plan(WindowId(1), &three_stacked()).unwrap().sent();
        let put_back = sent.put_back();
        assert_eq!(put_back.under, None, "it's never put back again");
        assert_eq!(put_back.next_target(WindowId(2), &three_stacked()), WindowId(1));
    }

    /// The stack once window 1 went behind 2 and 3.
    fn after_sending_the_first() -> Vec<StackedWindow> {
        vec![
            window(2, ZONE),
            window(4, NEIGHBOR),
            window(3, Rect::new(100.0, 100.0, 500.0, 400.0)),
            window(1, ZONE),
        ]
    }

    const FIRST_SENT: Sent = Sent {
        window: WindowId(1),
        focus: WindowId(2),
        under: None,
    };

    #[test]
    fn before_focus_follows_a_press_the_next_sends_the_window_it_focused() {
        let stacked = after_sending_the_first();
        assert_eq!(FIRST_SENT.next_target(WindowId(1), &stacked), WindowId(2));
    }

    #[test]
    fn a_window_focused_that_doesnt_overlap_the_one_sent_back_is_sent_next_all_the_same() {
        let mut stacked = after_sending_the_first();
        stacked[3].frame = NEIGHBOR;
        assert_eq!(FIRST_SENT.next_target(WindowId(1), &stacked), WindowId(2));
    }

    #[test]
    fn once_focus_follows_a_press_the_next_sends_the_focused_window() {
        let stacked = after_sending_the_first();
        assert_eq!(FIRST_SENT.next_target(WindowId(2), &stacked), WindowId(2));
        assert_eq!(FIRST_SENT.next_target(WindowId(4), &stacked), WindowId(4));
    }

    #[test]
    fn a_window_sent_back_and_brought_to_the_front_again_is_sent_back_again() {
        let mut stacked = after_sending_the_first();
        let clicked = stacked.remove(3);
        stacked.insert(0, clicked);
        assert_eq!(FIRST_SENT.next_target(WindowId(1), &stacked), WindowId(1));
    }

    #[test]
    fn a_window_focused_and_closed_since_leaves_the_press_on_the_focused_window() {
        let stacked: Vec<StackedWindow> = after_sending_the_first().into_iter().skip(1).collect();
        assert_eq!(FIRST_SENT.next_target(WindowId(1), &stacked), WindowId(1));
    }
}
