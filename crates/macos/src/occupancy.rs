//! Matching Accessibility windows against the window server's on-screen list: swap
//! discovery and visible-window enumeration.

use std::collections::BTreeMap;

use snappy_core::error::WindowError;
use snappy_core::geometry::Rect;

use crate::messages::{SWAP_NOT_SWAPPABLE, SWAP_UNIDENTIFIED, unavailable};
use crate::thresholds::WINDOW_LIST_MATCH;

/// One row of `CGWindowListCopyWindowInfo`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerWindow {
    pub pid: i32,
    pub layer: i64,
    pub frame: Rect,
}

impl ServerWindow {
    /// Normal app windows live on layer 0.
    pub fn is_app_window(&self) -> bool {
        self.layer == 0
    }
}

/// True when an Accessibility frame and a window server frame describe the same window.
pub fn same_frame(ax: Rect, server: Rect) -> bool {
    [
        (ax.x, server.x),
        (ax.y, server.y),
        (ax.width, server.width),
        (ax.height, server.height),
    ]
    .iter()
    .all(|(a, b)| (a - b).abs() < WINDOW_LIST_MATCH)
}

/// True when a window with `frame` fills `zone`, by the same rule snapping uses.
pub fn fills_zone(frame: Rect, zone: Rect) -> bool {
    snappy_core::swap::fills_zone(frame, zone)
}

/// Layer-0 window server frames that fill any of `zones`, grouped by owning process.
/// Snappy's own process is skipped.
pub fn occupants_by_process(rows: &[ServerWindow], zones: &[Rect], own_pid: i32) -> BTreeMap<i32, Vec<Rect>> {
    let mut found: BTreeMap<i32, Vec<Rect>> = BTreeMap::new();
    let occupying = rows.iter().filter(|row| {
        row.is_app_window() && row.pid != own_pid && zones.iter().any(|zone| fills_zone(row.frame, *zone))
    });
    for row in occupying {
        found.entry(row.pid).or_default().push(row.frame);
    }
    found
}

/// An app's Accessibility window, as far as swapping cares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwapCandidate {
    /// `None` when the frame couldn't be read.
    pub frame: Option<Rect>,
    pub minimized: bool,
    /// Eligible, not full screen, and without a sheet.
    pub movable: bool,
}

/// The indices of `candidates` behind one app's occupying frames. Ambiguous
/// matches are all kept, so the swap planner refuses stacked windows instead of
/// treating their zone as empty.
pub fn match_occupants(
    server_frames: &[Rect],
    candidates: &[SwapCandidate],
) -> Result<Vec<usize>, WindowError> {
    let mut matched = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let Some(frame) = candidate.frame else { continue };
        if !server_frames.iter().any(|server| same_frame(frame, *server)) {
            continue;
        }
        if candidate.minimized || !candidate.movable {
            return Err(unavailable(SWAP_NOT_SWAPPABLE));
        }
        matched.push(index);
    }
    if matched.is_empty() {
        return Err(unavailable(SWAP_UNIDENTIFIED));
    }
    Ok(matched)
}

/// Whether the window with `frame` is on screen for process `pid`.
pub fn is_on_screen(frame: Rect, pid: i32, rows: &[ServerWindow]) -> bool {
    rows.iter()
        .any(|row| row.pid == pid && row.is_app_window() && same_frame(frame, row.frame))
}

/// Which of one app's windows to keep: windows sharing an identical frame are
/// ambiguous and dropped, unless one is the app's focused window.
pub fn unambiguous(frames: &[Rect], focused: Option<usize>) -> Vec<bool> {
    frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let twins = frames.iter().filter(|other| *other == frame).count();
            twins == 1 || focused == Some(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZONE: Rect = Rect::new(12.0, 37.0, 700.0, 851.0);

    fn row(pid: i32, layer: i64, frame: Rect) -> ServerWindow {
        ServerWindow { pid, layer, frame }
    }

    fn candidate(frame: Rect) -> SwapCandidate {
        SwapCandidate {
            frame: Some(frame),
            minimized: false,
            movable: true,
        }
    }

    fn message(result: Result<Vec<usize>, WindowError>) -> String {
        result.err().map(|error| error.to_string()).unwrap_or_default()
    }

    #[test]
    fn frames_match_within_the_window_list_tolerance() {
        let near = Rect::new(14.9, 35.1, 702.9, 848.1);
        assert!(same_frame(ZONE, near));
        assert!(!same_frame(ZONE, Rect::new(15.0, 37.0, 700.0, 851.0)));
    }

    #[test]
    fn only_layer_zero_windows_filling_a_zone_are_occupants() {
        let rows = [
            row(10, 0, ZONE),
            row(11, 0, Rect::new(100.0, 100.0, 300.0, 300.0)),
            row(12, 25, ZONE),
            row(99, 0, ZONE),
            row(10, 0, Rect::new(16.0, 40.0, 694.0, 846.0)),
        ];
        let found = occupants_by_process(&rows, &[ZONE], 99);
        assert_eq!(found.keys().copied().collect::<Vec<_>>(), vec![10]);
        assert_eq!(found[&10].len(), 2);
    }

    #[test]
    fn matched_windows_are_returned() {
        let other = Rect::new(800.0, 37.0, 600.0, 851.0);
        let found = match_occupants(&[ZONE], &[candidate(other), candidate(ZONE)]);
        assert_eq!(found, Ok(vec![1]));
    }

    #[test]
    fn stacked_matches_are_all_kept() {
        let found = match_occupants(&[ZONE], &[candidate(ZONE), candidate(ZONE)]);
        assert_eq!(found, Ok(vec![0, 1]));
    }

    #[test]
    fn minimized_or_unmovable_match_cancels() {
        let minimized = SwapCandidate {
            minimized: true,
            ..candidate(ZONE)
        };
        let fixed = SwapCandidate {
            movable: false,
            ..candidate(ZONE)
        };
        let expected = "A window in that zone does not allow swapping. The drag was cancelled.";
        assert_eq!(message(match_occupants(&[ZONE], &[minimized])), expected);
        assert_eq!(message(match_occupants(&[ZONE], &[fixed])), expected);
    }

    #[test]
    fn no_match_cannot_be_identified() {
        let unreadable = SwapCandidate {
            frame: None,
            ..candidate(ZONE)
        };
        let elsewhere = candidate(Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(
            message(match_occupants(&[ZONE], &[unreadable, elsewhere])),
            "The window in that zone could not be identified safely. Try again."
        );
    }

    #[test]
    fn on_screen_needs_the_same_process_on_layer_zero() {
        let rows = [row(10, 0, ZONE), row(11, 3, ZONE)];
        assert!(is_on_screen(ZONE, 10, &rows));
        assert!(!is_on_screen(ZONE, 11, &rows));
        assert!(!is_on_screen(ZONE, 12, &rows));
    }

    #[test]
    fn identical_frames_are_dropped_unless_focused() {
        let other = Rect::new(0.0, 0.0, 5.0, 5.0);
        assert_eq!(unambiguous(&[ZONE, ZONE, other], None), vec![false, false, true]);
        assert_eq!(
            unambiguous(&[ZONE, ZONE, other], Some(1)),
            vec![false, true, true]
        );
    }
}
