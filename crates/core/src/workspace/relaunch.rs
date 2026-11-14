//! What a new daemon does with a window a previous run left parked: one restarted while apps
//! kept running finds its parked windows still hidden and keeps them that way.

use super::parking::{ParkingPolicy, is_parked, same_size, sits_at};
use crate::geometry::{Point, Rect, Size};

/// A window the journal lists as parked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Journaled<'a> {
    pub workspace: &'a str,
    /// The workspace is still in the settings.
    pub workspace_exists: bool,
    /// The window's size, which parking never changes.
    pub size: Size,
    pub parked: Point,
    /// Where it was parked before, while it was moving to a new parking spot.
    pub prior_parked: Option<Point>,
}

impl Journaled<'_> {
    /// Whether a window at `frame` is at one of the spots this window was parked at.
    pub fn at_parked_spot(&self, frame: Rect) -> bool {
        [Some(self.parked), self.prior_parked]
            .into_iter()
            .flatten()
            .any(|spot| sits_at(frame, spot))
    }

    /// Whether a window of the same process at `frame` can be this one when its token
    /// changed: the same size, at one of its parked spots.
    pub fn found_at(&self, frame: Rect) -> bool {
        same_size(frame.size(), self.size) && self.at_parked_spot(frame)
    }
}

/// The window found for a journal entry, as it is now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Found {
    pub frame: Rect,
    pub minimized: bool,
    pub on_parking_desktop: bool,
    /// Not full screen and without an open sheet.
    pub movable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relaunch {
    /// Still hidden, or minimized, for a workspace that isn't showing: it rejoins that
    /// workspace without moving.
    StaysParked,
    /// Still where it was parked, but nothing keeps it hidden: bring it back.
    ComesBack,
    /// Somewhere else; the user or its app has moved it, so leave it alone.
    LeftAlone,
}

/// Decide for one journaled window. `active` is the workspace showing now, or `None` when
/// every parked window comes back, as when workspaces are turned off.
pub fn relaunch(
    policy: ParkingPolicy,
    displays: &[Rect],
    journaled: &Journaled,
    found: &Found,
    active: Option<&str>,
) -> Relaunch {
    let hidden_now = is_parked(policy, found.frame, displays, found.on_parking_desktop);
    let inactive = active.is_some_and(|active| journaled.workspace_exists && journaled.workspace != active);
    if inactive && found.movable && (hidden_now || found.minimized) {
        return Relaunch::StaysParked;
    }
    let at_parked_spot = if policy.moves_windows() {
        journaled.at_parked_spot(found.frame)
    } else {
        found.on_parking_desktop
    };
    if at_parked_spot {
        Relaunch::ComesBack
    } else {
        Relaunch::LeftAlone
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::parking_origin;

    const MAIN: Rect = Rect::new(0.0, 0.0, 1440.0, 900.0);
    const HOME: Rect = Rect::new(100.0, 80.0, 800.0, 600.0);

    fn parked_spot() -> Point {
        parking_origin(ParkingPolicy::CornerSliver, HOME, &[MAIN]).unwrap()
    }

    fn journaled(workspace: &str) -> Journaled<'_> {
        Journaled {
            workspace,
            workspace_exists: true,
            size: HOME.size(),
            parked: parked_spot(),
            prior_parked: None,
        }
    }

    fn found_at(origin: Point) -> Found {
        Found {
            frame: HOME.with_origin(origin),
            minimized: false,
            on_parking_desktop: false,
            movable: true,
        }
    }

    fn decide(journaled: &Journaled, found: &Found, active: Option<&str>) -> Relaunch {
        relaunch(ParkingPolicy::CornerSliver, &[MAIN], journaled, found, active)
    }

    #[test]
    fn a_window_still_parked_for_another_workspace_stays_parked() {
        let found = found_at(parked_spot());
        assert_eq!(
            decide(&journaled("two"), &found, Some("one")),
            Relaunch::StaysParked
        );
    }

    #[test]
    fn a_window_parked_for_the_workspace_showing_comes_back() {
        let found = found_at(parked_spot());
        assert_eq!(
            decide(&journaled("one"), &found, Some("one")),
            Relaunch::ComesBack
        );
    }

    #[test]
    fn a_window_of_a_deleted_workspace_comes_back() {
        let gone = Journaled {
            workspace_exists: false,
            ..journaled("two")
        };
        assert_eq!(
            decide(&gone, &found_at(parked_spot()), Some("one")),
            Relaunch::ComesBack
        );
    }

    #[test]
    fn every_parked_window_comes_back_when_none_may_stay() {
        assert_eq!(
            decide(&journaled("two"), &found_at(parked_spot()), None),
            Relaunch::ComesBack
        );
    }

    #[test]
    fn a_window_moved_away_from_its_parked_spot_is_left_alone() {
        let moved = found_at(Point::new(300.0, 200.0));
        assert_eq!(
            decide(&journaled("two"), &moved, Some("one")),
            Relaunch::LeftAlone
        );
        assert_eq!(decide(&journaled("two"), &moved, None), Relaunch::LeftAlone);
    }

    #[test]
    fn a_minimized_window_rejoins_its_workspace() {
        let minimized = Found {
            minimized: true,
            ..found_at(HOME.origin())
        };
        assert_eq!(
            decide(&journaled("two"), &minimized, Some("one")),
            Relaunch::StaysParked
        );
    }

    #[test]
    fn a_window_that_cant_be_moved_is_not_kept_hidden() {
        let full_screen = Found {
            movable: false,
            ..found_at(parked_spot())
        };
        assert_eq!(
            decide(&journaled("two"), &full_screen, Some("one")),
            Relaunch::ComesBack
        );
    }

    #[test]
    fn desktop_parking_asks_the_platform_where_the_window_is() {
        let entry = Journaled {
            parked: HOME.origin(),
            ..journaled("two")
        };
        let on_desktop = Found {
            on_parking_desktop: true,
            ..found_at(HOME.origin())
        };
        let decide = |found: &Found, active| relaunch(ParkingPolicy::Desktop, &[MAIN], &entry, found, active);
        assert_eq!(decide(&on_desktop, Some("one")), Relaunch::StaysParked);
        assert_eq!(decide(&on_desktop, None), Relaunch::ComesBack);
        assert_eq!(
            decide(&found_at(HOME.origin()), Some("one")),
            Relaunch::LeftAlone,
            "brought back while Snappy was away"
        );
    }

    #[test]
    fn a_window_whose_token_changed_is_found_only_at_its_size_and_a_parked_spot() {
        let prior = Point::new(10.0, 20.0);
        let entry = Journaled {
            prior_parked: Some(prior),
            ..journaled("two")
        };
        let spot = parked_spot();
        assert!(entry.found_at(HOME.with_origin(Point::new(spot.x + 3.0, spot.y))));
        assert!(
            entry.found_at(HOME.with_origin(prior)),
            "the spot it was moving from"
        );
        assert!(!entry.found_at(HOME.with_origin(Point::new(spot.x + 5.0, spot.y))));
        assert!(!entry.found_at(Rect::from_origin_size(spot, Size::new(500.0, 600.0))));
    }
}
