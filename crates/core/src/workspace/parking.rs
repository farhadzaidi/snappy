use crate::geometry::{Point, Rect, Size, largest_overlap};

/// A parked window may cover any display by at most this much in width or height.
const SLIVER: f64 = 1.1;
/// Off-screen parking keeps windows this far outside every display.
const OFF_SCREEN_MARGIN: f64 = 64.0;
/// A window this close to its parking position counts as parked.
pub const PARKED_TOLERANCE: f64 = 4.0;

/// How a platform hides workspace windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParkingPolicy {
    /// macOS pulls fully off-screen windows back, so park at an exposed lower corner
    /// with a one-point sliver left on screen.
    CornerSliver,
    /// Park entirely outside every display.
    OffScreen,
    /// Move windows to a native desktop of Snappy's own, leaving their frames where they
    /// are. Only the platform can tell whether a window is there.
    Desktop,
}

impl ParkingPolicy {
    /// Whether parking moves windows, so a frame alone shows whether a window is parked.
    pub fn moves_windows(self) -> bool {
        !matches!(self, ParkingPolicy::Desktop)
    }
}

/// Where a window now at `frame` waits while parked, or `None` when no safe position
/// exists. Desktop parking leaves it where it is.
pub fn parking_origin(policy: ParkingPolicy, frame: Rect, displays: &[Rect]) -> Option<Point> {
    let size = frame.size();
    if size.width <= 1.0 || size.height <= 1.0 || displays.is_empty() {
        return None;
    }
    match policy {
        ParkingPolicy::CornerSliver => corner_sliver(size, displays),
        ParkingPolicy::OffScreen => Some(off_screen(size, displays)),
        ParkingPolicy::Desktop => Some(frame.origin()),
    }
}

/// Try the lowest displays first, right corner then left; accept the first position that
/// shows at most a sliver on every display.
fn corner_sliver(size: Size, displays: &[Rect]) -> Option<Point> {
    let mut lowest_first: Vec<Rect> = displays.to_vec();
    lowest_first.sort_by(|a, b| b.max_y().total_cmp(&a.max_y()));

    lowest_first.iter().find_map(|display| {
        let y = display.min_y().max(display.max_y() - size.height);
        [display.max_x() - 1.0, display.min_x() - size.width + 1.0]
            .into_iter()
            .map(|x| Point::new(x, y))
            .find(|&origin| shows_only_a_sliver(Rect::from_origin_size(origin, size), displays))
    })
}

fn shows_only_a_sliver(parked: Rect, displays: &[Rect]) -> bool {
    displays.iter().all(|display| {
        parked
            .intersection(*display)
            .is_none_or(|shown| shown.width <= SLIVER || shown.height <= SLIVER)
    })
}

/// Left of every display, aligned with the top of the highest one.
fn off_screen(size: Size, displays: &[Rect]) -> Point {
    let left = displays
        .iter()
        .map(|display| display.min_x())
        .fold(f64::INFINITY, f64::min);
    let top = displays
        .iter()
        .map(|display| display.min_y())
        .fold(f64::INFINITY, f64::min);
    Point::new(left - size.width - OFF_SCREEN_MARGIN, top)
}

/// True when a window at `frame` is parked. `on_parking_desktop` is what the platform
/// reports, and decides alone under desktop parking.
pub fn is_parked(policy: ParkingPolicy, frame: Rect, displays: &[Rect], on_parking_desktop: bool) -> bool {
    if !policy.moves_windows() {
        return on_parking_desktop;
    }
    parking_origin(policy, frame, displays).is_some_and(|origin| {
        (frame.x - origin.x).abs() < PARKED_TOLERANCE && (frame.y - origin.y).abs() < PARKED_TOLERANCE
    })
}

/// `frame` moved into the work area it overlaps most, so as much of it as possible is
/// reachable. Sizes never change.
pub fn reachable(frame: Rect, areas: &[Rect]) -> Rect {
    let Some(index) = largest_overlap(frame, areas) else {
        return frame;
    };
    let area = areas[index];
    let x = frame
        .x
        .max(area.min_x())
        .min((area.max_x() - frame.width).max(area.min_x()));
    let y = frame
        .y
        .max(area.min_y())
        .min((area.max_y() - frame.height).max(area.min_y()));
    frame.with_origin(Point::new(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Rect = Rect::new(0.0, 0.0, 1440.0, 900.0);
    const WINDOW: Rect = Rect::new(100.0, 80.0, 800.0, 600.0);

    #[test]
    fn corner_parking_leaves_one_sliver() {
        let origin = parking_origin(ParkingPolicy::CornerSliver, WINDOW, &[MAIN]).unwrap();
        assert_eq!(origin, Point::new(1439.0, 300.0));
        assert!(is_parked(
            ParkingPolicy::CornerSliver,
            Rect::new(1440.0, 301.0, 800.0, 600.0),
            &[MAIN],
            false
        ));
    }

    #[test]
    fn corner_parking_avoids_neighboring_displays() {
        let right = Rect::new(1440.0, 0.0, 1920.0, 1080.0);
        let origin = parking_origin(ParkingPolicy::CornerSliver, WINDOW, &[MAIN, right]).unwrap();
        let parked = Rect::from_origin_size(origin, Size::new(800.0, 600.0));
        assert!(shows_only_a_sliver(parked, &[MAIN, right]));
    }

    #[test]
    fn off_screen_parking_touches_no_display() {
        let origin = parking_origin(ParkingPolicy::OffScreen, WINDOW, &[MAIN]).unwrap();
        assert!(
            Rect::from_origin_size(origin, Size::new(800.0, 600.0))
                .intersection(MAIN)
                .is_none()
        );
    }

    #[test]
    fn desktop_parking_leaves_the_frame_where_it_is() {
        assert_eq!(
            parking_origin(ParkingPolicy::Desktop, WINDOW, &[MAIN]),
            Some(WINDOW.origin())
        );
        assert!(!ParkingPolicy::Desktop.moves_windows());
    }

    #[test]
    fn desktop_parking_asks_the_platform_whether_a_window_is_parked() {
        assert!(is_parked(ParkingPolicy::Desktop, WINDOW, &[MAIN], true));
        assert!(!is_parked(ParkingPolicy::Desktop, WINDOW, &[MAIN], false));
        assert!(!is_parked(ParkingPolicy::OffScreen, WINDOW, &[MAIN], true));
    }

    #[test]
    fn reachable_pulls_windows_into_the_best_area() {
        let moved = reachable(Rect::new(1300.0, 850.0, 400.0, 300.0), &[MAIN]);
        assert_eq!(moved, Rect::new(1040.0, 600.0, 400.0, 300.0));
    }
}
