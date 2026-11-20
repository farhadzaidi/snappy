//! Which windows a platform that can raise another app's window but never lower one raises
//! over the window sent to the back: the covered windows that show where it was once it's
//! gone, and every window in front of one of those that overlaps it, as a raise lands near
//! the very front and would otherwise cover that window. A covered window hidden there
//! behind others stays behind the window sent back, which looks the same, so presses on
//! such a platform go round only the windows that show.

use super::overlap::{MIN_OVERLAP, overlaps};
use super::plan::{SendToBack, StackedWindow};
use crate::geometry::{Point, Rect};

/// The windows raised over `send`'s window, front to back as `stacked`, frontmost first,
/// lists them.
pub(super) fn raised_over(send: &SendToBack, stacked: &[StackedWindow]) -> Vec<StackedWindow> {
    let sent = send.window();
    let others: Vec<StackedWindow> = stacked
        .iter()
        .filter(|listed| listed.id != sent.id)
        .copied()
        .collect();
    let mut raised: Vec<bool> = others
        .iter()
        .enumerate()
        .map(|(index, listed)| send.covered().contains(&listed.id) && shows(&others, index, sent.frame))
        .collect();
    let mut unchecked: Vec<usize> = (0..others.len()).filter(|&index| raised[index]).collect();
    while let Some(below) = unchecked.pop() {
        for above in 0..below {
            if !raised[above] && overlaps(others[above].frame, others[below].frame) {
                raised[above] = true;
                unchecked.push(above);
            }
        }
    }
    others
        .into_iter()
        .zip(raised)
        .filter_map(|(listed, raised)| raised.then_some(listed))
        .collect()
}

/// Whether window `index` of `others`, front to back, shows within `frame` past the windows
/// in front of it.
fn shows(others: &[StackedWindow], index: usize, frame: Rect) -> bool {
    let Some(area) = others[index].frame.intersection(frame) else {
        return false;
    };
    let in_front: Vec<Rect> = others[..index].iter().map(|listed| listed.frame).collect();
    shows_past(area, &in_front)
}

/// Whether some of `area`, at least `MIN_OVERLAP` across and down, lies outside every one
/// of `covers`. The edges of `covers` cut `area` into cells that each lie wholly inside or
/// wholly outside each cover, so a cell's center tells which.
fn shows_past(area: Rect, covers: &[Rect]) -> bool {
    let columns = cuts(
        area.min_x(),
        area.max_x(),
        covers.iter().flat_map(|cover| [cover.min_x(), cover.max_x()]),
    );
    let rows = cuts(
        area.min_y(),
        area.max_y(),
        covers.iter().flat_map(|cover| [cover.min_y(), cover.max_y()]),
    );
    columns.windows(2).any(|column| {
        rows.windows(2).any(|row| {
            let cell = Rect::new(column[0], row[0], column[1] - column[0], row[1] - row[0]);
            let center = Point::new(cell.mid_x(), cell.mid_y());
            cell.width >= MIN_OVERLAP
                && cell.height >= MIN_OVERLAP
                && !covers.iter().any(|cover| cover.contains(center))
        })
    })
}

/// `start`, `end`, and the `edges` between them, in order.
fn cuts(start: f64, end: f64, edges: impl Iterator<Item = f64>) -> Vec<f64> {
    let mut cuts = vec![start, end];
    cuts.extend(edges.filter(|edge| *edge > start && *edge < end));
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    cuts
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{NEIGHBOR, ZONE, ids, window};
    use super::*;
    use crate::window::WindowId;

    /// Over both zones.
    const BOTH: Rect = Rect::new(200.0, 100.0, 1000.0, 600.0);

    /// Window `id` offset `step` down and right of `ZONE`'s origin, a little smaller.
    fn offset(id: u64, step: f64) -> StackedWindow {
        window(id, Rect::new(ZONE.x + step, ZONE.y + step, 500.0, 400.0))
    }

    fn raised(window: u64, stacked: &[StackedWindow]) -> Vec<u64> {
        let send = SendToBack::plan(WindowId(window), stacked).unwrap();
        ids(raised_over(&send, stacked).iter().map(|listed| listed.id))
    }

    #[test]
    fn only_the_front_window_of_a_zone_shows_so_only_it_is_raised() {
        let stacked = [window(1, ZONE), window(2, ZONE), window(3, ZONE)];
        assert_eq!(raised(1, &stacked), [2]);
    }

    #[test]
    fn a_covered_window_peeking_out_from_behind_another_is_raised_too() {
        let stacked = [offset(1, 0.0), offset(2, 40.0), offset(3, 80.0)];
        assert_eq!(raised(1, &stacked), [2], "3 shows only outside 1");
        let stacked = [offset(2, 40.0), offset(1, 0.0), offset(3, 80.0)];
        assert_eq!(raised(2, &stacked), [1, 3], "3 shows past 1 within 2");
    }

    #[test]
    fn a_window_showing_by_less_than_a_sliver_is_left_behind() {
        let taller = |id, by| window(id, Rect::new(ZONE.x, ZONE.y, ZONE.width, ZONE.height + by));
        let stacked = [taller(1, 100.0), window(2, ZONE), taller(3, MIN_OVERLAP - 1.0)];
        assert_eq!(raised(1, &stacked), [2]);
        let stacked = [taller(1, 100.0), window(2, ZONE), taller(3, MIN_OVERLAP)];
        assert_eq!(raised(1, &stacked), [2, 3]);
    }

    /// Window 1 and 2 in the left zone, 3 in the right zone, and 4 over both zones at the
    /// back, behind 3.
    fn one_over_both_zones() -> Vec<StackedWindow> {
        vec![
            window(1, ZONE),
            window(3, NEIGHBOR),
            window(2, ZONE),
            window(4, BOTH),
        ]
    }

    #[test]
    fn a_window_in_front_of_a_raised_one_that_overlaps_it_is_raised_too_so_it_stays_in_front() {
        let mut stacked = one_over_both_zones();
        stacked[2].frame = Rect::new(ZONE.x, ZONE.y, ZONE.width, 300.0);
        assert_eq!(
            raised(1, &stacked),
            [3, 2, 4],
            "4 shows below 2; 3 is in front of 4"
        );
    }

    #[test]
    fn windows_raised_too_bring_the_windows_in_front_of_them_along_in_turn() {
        let mut stacked = one_over_both_zones();
        stacked[2].frame = Rect::new(ZONE.x, ZONE.y, ZONE.width, 300.0);
        stacked.insert(0, window(5, Rect::new(1000.0, 500.0, 300.0, 300.0)));
        stacked.insert(0, window(6, Rect::new(1500.0, 37.0, 100.0, 100.0)));
        assert_eq!(
            raised(1, &stacked),
            [5, 3, 2, 4],
            "5 is in front of 3, which is in front of 4; 6 overlaps none of them"
        );
    }

    #[test]
    fn a_covered_window_hidden_where_the_window_was_brings_nothing_along() {
        assert_eq!(
            raised(1, &one_over_both_zones()),
            [2],
            "4 is behind 2 in the left zone"
        );
    }

    #[test]
    fn a_window_sent_back_from_under_a_floating_one_raises_what_shows_and_whats_in_front_of_that() {
        // The second press of a floating window's: 2, in its zone, goes behind the floating
        // window 1, which 3 and 4, in other zones, stay in front of.
        let stacked = [
            window(2, Rect::new(600.0, 450.0, 600.0, 450.0)),
            window(3, Rect::new(0.0, 450.0, 600.0, 450.0)),
            window(4, Rect::new(0.0, 0.0, 600.0, 450.0)),
            window(1, Rect::new(300.0, 200.0, 600.0, 500.0)),
        ];
        assert_eq!(raised(2, &stacked), [3, 4, 1]);
    }

    #[test]
    fn an_area_shows_past_covers_unless_they_leave_only_slivers() {
        let area = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert!(shows_past(area, &[]));
        assert!(!shows_past(area, &[area]));
        let halves = [
            Rect::new(0.0, 0.0, 50.0, 100.0),
            Rect::new(50.0, 0.0, 50.0, 100.0),
        ];
        assert!(!shows_past(area, &halves));
        let gap = [
            Rect::new(0.0, 0.0, 45.0, 100.0),
            Rect::new(55.0, 0.0, 45.0, 100.0),
        ];
        assert!(shows_past(area, &gap));
        let sliver = [
            Rect::new(0.0, 0.0, 47.0, 100.0),
            Rect::new(53.0, 0.0, 47.0, 100.0),
        ];
        assert!(!shows_past(area, &sliver));
    }
}
