//! Where windows stand in the window server's front-to-back list: the place of each of an
//! app's windows (`stacking_positions`), and whether it lists windows in an order
//! (`listed_in_order`), which raising windows over one sent to the back awaits.

use snappy_core::geometry::Rect;

use super::occupancy::{ServerWindow, same_frame};

/// Where each of process `pid`'s windows with `frames`, listed the app's own front to back,
/// stands in the window server's front-to-back `rows`. The rows can't tell the app's
/// windows with one frame apart, so those take the rows with that frame in turn, and
/// another app's window listed between them stays between them. A window with no row left
/// comes after every row.
pub fn stacking_positions(frames: &[Rect], pid: i32, rows: &[ServerWindow]) -> Vec<usize> {
    frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let twins_before = frames[..index]
                .iter()
                .filter(|other| same_frame(**other, *frame))
                .count();
            rows.iter()
                .enumerate()
                .filter(|(_, row)| row.pid == pid && row.is_app_window() && same_frame(*frame, row.frame))
                .nth(twins_before)
                .map_or(rows.len(), |(position, _)| position)
        })
        .collect()
}

/// A window as the window server's list can tell it: by its number once that's known, else
/// by its process and frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListedWindow {
    pub pid: i32,
    pub frame: Rect,
    pub number: Option<u32>,
}

impl ListedWindow {
    fn is(&self, row: &ServerWindow) -> bool {
        row.is_app_window()
            && match self.number {
                Some(number) => row.number == number,
                None => row.pid == self.pid && same_frame(self.frame, row.frame),
            }
    }
}

/// Whether the window server lists `windows` in this order, front to back. A window whose
/// number isn't known matches by process and frame, so another window of its app with its
/// frame can't be told from it, and any row that matches may be the window's.
pub fn listed_in_order(rows: &[ServerWindow], windows: &[ListedWindow]) -> bool {
    let mut rows = rows.iter();
    windows.iter().all(|window| rows.any(|row| window.is(row)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZONE: Rect = Rect::new(12.0, 37.0, 700.0, 851.0);

    fn row(pid: i32, layer: i64, frame: Rect) -> ServerWindow {
        ServerWindow {
            pid,
            number: 0,
            layer,
            frame,
        }
    }

    #[test]
    fn windows_stand_where_the_window_server_lists_them() {
        let other = Rect::new(800.0, 37.0, 600.0, 851.0);
        let rows = [
            row(11, 0, other),
            row(10, 3, ZONE),
            row(10, 0, ZONE),
            row(11, 0, ZONE),
        ];
        assert_eq!(stacking_positions(&[ZONE, other], 11, &rows), [3, 0]);
        assert_eq!(
            stacking_positions(&[ZONE], 10, &rows),
            [2],
            "not the floating layer's row"
        );
        assert_eq!(
            stacking_positions(&[ZONE], 12, &rows),
            [rows.len()],
            "not on screen"
        );
    }

    #[test]
    fn an_apps_windows_with_one_frame_take_its_rows_with_that_frame_in_turn() {
        let rows = [row(10, 0, ZONE), row(11, 0, ZONE), row(10, 0, ZONE)];
        assert_eq!(
            stacking_positions(&[ZONE, ZONE], 10, &rows),
            [0, 2],
            "the other app's window stays between them"
        );
        assert_eq!(
            stacking_positions(&[ZONE, ZONE, ZONE], 10, &rows),
            [0, 2, rows.len()],
            "one more than the rows show"
        );
    }

    fn listed(pid: i32, frame: Rect) -> ListedWindow {
        ListedWindow {
            pid,
            frame,
            number: None,
        }
    }

    #[test]
    fn windows_are_listed_in_order_when_their_rows_come_in_that_order() {
        let other = Rect::new(800.0, 37.0, 600.0, 851.0);
        let rows = [
            row(11, 0, other),
            row(10, 3, ZONE),
            row(12, 0, ZONE),
            row(10, 0, ZONE),
            row(13, 0, other),
        ];
        assert!(listed_in_order(&rows, &[listed(12, ZONE), listed(10, ZONE)]));
        assert!(!listed_in_order(&rows, &[listed(10, ZONE), listed(12, ZONE)]));
        assert!(
            !listed_in_order(&rows, &[listed(12, ZONE), listed(10, ZONE), listed(14, ZONE)]),
            "one isn't on screen"
        );
    }

    #[test]
    fn windows_of_one_app_with_one_frame_are_listed_in_order_either_way() {
        let rows = [row(10, 0, ZONE), row(11, 0, ZONE), row(10, 0, ZONE)];
        assert!(listed_in_order(
            &rows,
            &[listed(10, ZONE), listed(11, ZONE), listed(10, ZONE)]
        ));
        assert!(!listed_in_order(
            &rows,
            &[listed(11, ZONE), listed(10, ZONE), listed(10, ZONE)]
        ));
    }

    #[test]
    fn another_row_of_a_windows_app_with_its_frame_may_be_it_and_never_hides_the_order() {
        let rows = [row(10, 0, ZONE), row(11, 0, ZONE), row(10, 0, ZONE)];
        assert!(
            listed_in_order(&rows, &[listed(11, ZONE), listed(10, ZONE)]),
            "the row of 10 behind 11 may be the window"
        );
        assert!(
            listed_in_order(&rows, &[listed(10, ZONE), listed(11, ZONE)]),
            "so may the one in front"
        );
        let in_front_of_none = [row(11, 0, ZONE), row(10, 0, ZONE), row(10, 0, ZONE)];
        assert!(
            !listed_in_order(&in_front_of_none, &[listed(10, ZONE), listed(11, ZONE)]),
            "no row of 10 is in front of 11"
        );
    }

    #[test]
    fn a_window_whose_number_is_known_is_told_from_another_of_its_app_with_its_frame() {
        let numbered = |number| ServerWindow {
            number,
            ..row(10, 0, ZONE)
        };
        let known = |number| ListedWindow {
            number: Some(number),
            ..listed(10, ZONE)
        };
        let rows = [numbered(7), row(11, 0, ZONE), numbered(8)];
        assert!(listed_in_order(&rows, &[known(7), listed(11, ZONE), known(8)]));
        assert!(
            !listed_in_order(&rows, &[listed(11, ZONE), known(7)]),
            "its app's other row behind 11 isn't it"
        );
        assert!(!listed_in_order(&rows, &[known(8), known(7)]));
    }
}
