use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::error::WindowError;
use crate::geometry::{Point, Rect};
use crate::window::WindowId;

const DISPLAY: Rect = Rect::new(0.0, 0.0, 1440.0, 900.0);
const AREA: Rect = Rect::new(0.0, 25.0, 1440.0, 875.0);

/// A scripted window system that records what happened.
#[derive(Default)]
struct FakeIo {
    frames: BTreeMap<WindowId, Rect>,
    refusing: BTreeSet<WindowId>,
    busy: BTreeSet<WindowId>,
    minimized: BTreeSet<WindowId>,
    /// Windows on the parking desktop.
    on_parking_desktop: BTreeSet<WindowId>,
    journal_fails: bool,
    journals: Vec<Vec<JournalEntry>>,
    moves: Vec<WindowId>,
    focused: Option<WindowId>,
}

impl FakeIo {
    fn with(windows: &[(u64, Rect)]) -> Self {
        Self {
            frames: windows
                .iter()
                .map(|(id, frame)| (WindowId(*id), *frame))
                .collect(),
            ..Self::default()
        }
    }

    fn origin(&self, id: u64) -> Point {
        self.frames[&WindowId(id)].origin()
    }
}

impl WorkspaceIo for FakeIo {
    fn frame(&mut self, window: WindowId) -> Result<Rect, WindowError> {
        self.frames
            .get(&window)
            .copied()
            .ok_or_else(|| WindowError::unavailable("gone"))
    }

    fn is_movable(&mut self, window: WindowId) -> bool {
        self.frames.contains_key(&window) && !self.busy.contains(&window)
    }

    fn is_minimized(&mut self, window: WindowId) -> bool {
        self.minimized.contains(&window)
    }

    fn is_on_parking_desktop(&mut self, window: WindowId) -> bool {
        self.on_parking_desktop.contains(&window)
    }

    fn place(&mut self, window: WindowId, origin: Point, hidden: bool) -> Result<(), WindowError> {
        self.moves.push(window);
        if !self.refusing.contains(&window)
            && let Some(frame) = self.frames.get_mut(&window)
        {
            *frame = frame.with_origin(origin);
            if hidden {
                self.on_parking_desktop.insert(window);
            } else {
                self.on_parking_desktop.remove(&window);
            }
        }
        Ok(())
    }

    fn focus(&mut self, window: WindowId) {
        self.focused = Some(window);
    }

    fn write_journal(&mut self, entries: &[JournalEntry]) -> Result<(), WindowError> {
        if self.journal_fails {
            return Err(WindowError::unavailable("disk full"));
        }
        self.journals.push(entries.to_vec());
        Ok(())
    }
}

fn session_with(io: &mut FakeIo, members: &[(u64, &str)]) -> Session {
    session_parking(ParkingPolicy::CornerSliver, io, members)
}

fn session_parking(policy: ParkingPolicy, io: &mut FakeIo, members: &[(u64, &str)]) -> Session {
    let mut session = Session::new("one", policy);
    for (id, workspace) in members {
        let window = WindowId(*id);
        let frame = io.frames[&window];
        session.enroll(
            window,
            Member {
                workspace: (*workspace).into(),
                frame,
                parked: None,
            },
        );
    }
    session
}

fn two_windows() -> FakeIo {
    FakeIo::with(&[
        (1, Rect::new(100.0, 100.0, 600.0, 400.0)),
        (2, Rect::new(200.0, 150.0, 500.0, 300.0)),
    ])
}

#[test]
fn switching_parks_and_restores_exact_frames() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    assert!(io.origin(2).x > 1400.0, "sent window is parked");

    session.switch_to("two", &[DISPLAY], true, &mut io).unwrap();
    assert_eq!(io.origin(2), Point::new(200.0, 150.0));
    assert!(io.origin(1).x > 1400.0);
    assert_eq!(io.focused, Some(WindowId(2)));

    session.switch_to("one", &[DISPLAY], true, &mut io).unwrap();
    assert_eq!(io.origin(1), Point::new(100.0, 100.0));
}

#[test]
fn incoming_windows_move_before_outgoing() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    io.moves.clear();
    session.switch_to("two", &[DISPLAY], false, &mut io).unwrap();
    assert_eq!(io.moves, [WindowId(2), WindowId(1)]);
}

#[test]
fn a_refused_move_reverses_the_switch() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    let parked = io.origin(2);
    io.refusing.insert(WindowId(1));

    let error = session.switch_to("two", &[DISPLAY], true, &mut io).unwrap_err();
    assert_eq!(
        error.to_string(),
        "An app refused the workspace move. Your previous workspace has been restored."
    );
    assert_eq!(session.active(), "one");
    assert_eq!(io.origin(2), parked);
}

#[test]
fn busy_windows_block_a_switch() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    io.busy.insert(WindowId(1));
    assert!(matches!(
        session.switch_to("two", &[DISPLAY], true, &mut io),
        Err(WindowError::Unavailable(_))
    ));
}

#[test]
fn no_journal_means_no_moves() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    io.journal_fails = true;
    assert!(session.switch_to("two", &[DISPLAY], true, &mut io).is_err());
    assert!(io.moves.is_empty());
}

#[test]
fn the_journal_keeps_restoring_windows_until_they_land() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    io.journals.clear();
    session.switch_to("two", &[DISPLAY], true, &mut io).unwrap();

    let first = &io.journals[0];
    assert!(
        first.iter().any(|entry| entry.window == WindowId(2)),
        "window 2 stays journaled while returning"
    );
    let last = io.journals.last().unwrap();
    assert!(last.iter().all(|entry| entry.window == WindowId(1)));
}

#[test]
fn an_empty_workspace_reports_nothing_focused() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    assert_eq!(session.switch_to("two", &[DISPLAY], true, &mut io), Ok(false));
}

#[test]
fn containment_reparks_revealed_windows() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    let parked = io.origin(2);
    io.frames
        .insert(WindowId(2), Rect::new(300.0, 300.0, 500.0, 300.0));
    assert_eq!(session.contain(WindowId(2), &[DISPLAY], &mut io), Ok(true));
    assert_eq!(io.origin(2), parked);
    assert_eq!(
        session.member(WindowId(2)).unwrap().frame.origin(),
        Point::new(200.0, 150.0)
    );
    assert_eq!(session.contain(WindowId(2), &[DISPLAY], &mut io), Ok(false));
}

#[test]
fn display_updates_reach_visible_windows_without_revealing_parked_ones() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    let small = Rect::new(0.0, 0.0, 1000.0, 700.0);
    let small_area = Rect::new(0.0, 25.0, 1000.0, 675.0);
    io.frames
        .insert(WindowId(1), Rect::new(900.0, 500.0, 600.0, 400.0));

    session.update_displays(&[small], &[small_area], &mut io).unwrap();
    let visible = io.frames[&WindowId(1)];
    assert!(small_area.contains_rect(visible));
    assert!(io.origin(2).x >= 999.0, "inactive window parks at the new corner");
}

#[test]
fn restore_all_returns_every_window() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    assert!(session.restore_all(&[AREA], &mut io));
    assert_eq!(io.origin(2), Point::new(200.0, 150.0));
    assert!(session.journal().is_empty());
}

#[test]
fn repeated_switches_do_not_drift() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    for _ in 0..1000 {
        session.switch_to("two", &[DISPLAY], false, &mut io).unwrap();
        session.switch_to("one", &[DISPLAY], false, &mut io).unwrap();
    }
    assert_eq!(io.frames[&WindowId(1)], Rect::new(100.0, 100.0, 600.0, 400.0));
    assert_eq!(session.members().len(), 2);
}

#[test]
fn minimized_windows_never_move() {
    let mut io = two_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one")]);
    io.minimized.insert(WindowId(2));
    session.switch_to("two", &[DISPLAY], false, &mut io).unwrap();
    assert!(!io.moves.contains(&WindowId(2)));
}

#[test]
fn sending_parks_without_switching_and_sending_back_unparks() {
    let mut io = FakeIo::with(&[
        (1, Rect::new(100.0, 100.0, 600.0, 400.0)),
        (2, Rect::new(200.0, 150.0, 500.0, 300.0)),
        (3, Rect::new(300.0, 200.0, 500.0, 300.0)),
    ]);
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one"), (3, "one")]);
    session.note_focus(WindowId(3));

    session.send(WindowId(3), "two", &[DISPLAY], &mut io).unwrap();
    assert_eq!(session.active(), "one");
    assert!(io.origin(3).x > 1400.0, "parked");
    assert_eq!(session.member(WindowId(3)).unwrap().workspace, "two");

    session.send(WindowId(3), "one", &[DISPLAY], &mut io).unwrap();
    assert_eq!(
        io.origin(3),
        Point::new(300.0, 200.0),
        "sending to the active workspace un-parks"
    );

    session.switch_to("two", &[DISPLAY], true, &mut io).unwrap();
    session.switch_to("one", &[DISPLAY], true, &mut io).unwrap();
    assert_eq!(
        io.focused,
        Some(WindowId(1)),
        "the sent window is no longer the workspace's last focused one"
    );
}

fn desktop_session(io: &mut FakeIo) -> Session {
    session_parking(ParkingPolicy::Desktop, io, &[(1, "one"), (2, "one")])
}

#[test]
fn desktop_parking_hides_windows_without_moving_them() {
    let mut io = two_windows();
    let mut session = desktop_session(&mut io);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    assert_eq!(io.origin(2), Point::new(200.0, 150.0));
    assert!(io.on_parking_desktop.contains(&WindowId(2)));
    assert_eq!(session.journal()[0].parked, Point::new(200.0, 150.0));

    session.switch_to("two", &[DISPLAY], true, &mut io).unwrap();
    assert_eq!(io.on_parking_desktop, BTreeSet::from([WindowId(1)]));
    assert_eq!(io.origin(1), Point::new(100.0, 100.0));
    assert_eq!(io.focused, Some(WindowId(2)));

    session.switch_to("one", &[DISPLAY], true, &mut io).unwrap();
    assert_eq!(io.on_parking_desktop, BTreeSet::from([WindowId(2)]));
}

#[test]
fn desktop_parking_shows_windows_at_their_saved_frames() {
    let mut io = two_windows();
    let mut session = desktop_session(&mut io);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    io.frames.insert(WindowId(2), Rect::new(10.0, 40.0, 500.0, 300.0));

    session.switch_to("two", &[DISPLAY], true, &mut io).unwrap();
    assert_eq!(io.origin(2), Point::new(200.0, 150.0));
    assert!(!io.on_parking_desktop.contains(&WindowId(2)));
}

#[test]
fn desktop_parking_refused_by_the_window_manager_reverses_the_switch() {
    let mut io = two_windows();
    let mut session = desktop_session(&mut io);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    io.refusing.insert(WindowId(1));

    let error = session.switch_to("two", &[DISPLAY], true, &mut io).unwrap_err();
    assert_eq!(
        error.to_string(),
        "An app refused the workspace move. Your previous workspace has been restored."
    );
    assert_eq!(session.active(), "one");
    assert_eq!(io.on_parking_desktop, BTreeSet::from([WindowId(2)]));
}

#[test]
fn desktop_parking_hides_again_a_window_brought_to_the_current_desktop() {
    let mut io = two_windows();
    let mut session = desktop_session(&mut io);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    io.frames
        .insert(WindowId(2), Rect::new(300.0, 300.0, 500.0, 300.0));
    assert_eq!(
        session.contain(WindowId(2), &[DISPLAY], &mut io),
        Ok(false),
        "moving on the parking desktop reveals nothing"
    );

    io.on_parking_desktop.remove(&WindowId(2));
    assert_eq!(session.contain(WindowId(2), &[DISPLAY], &mut io), Ok(true));
    assert!(io.on_parking_desktop.contains(&WindowId(2)));
    assert_eq!(
        session.member(WindowId(2)).unwrap().frame.origin(),
        Point::new(200.0, 150.0)
    );
}

#[test]
fn desktop_parking_leaves_parked_windows_alone_on_a_display_change() {
    let mut io = two_windows();
    let mut session = desktop_session(&mut io);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    io.moves.clear();
    let wide = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    let wide_area = Rect::new(0.0, 25.0, 1920.0, 1055.0);

    session.update_displays(&[wide], &[wide_area], &mut io).unwrap();
    assert!(io.moves.is_empty());
    assert!(io.on_parking_desktop.contains(&WindowId(2)));
}

#[test]
fn desktop_parking_restore_all_shows_every_hidden_window() {
    let mut io = two_windows();
    let mut session = desktop_session(&mut io);
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    assert!(session.restore_all(&[AREA], &mut io));
    assert!(io.on_parking_desktop.is_empty());
    assert_eq!(io.origin(2), Point::new(200.0, 150.0));
    assert!(session.journal().is_empty());
}

fn three_windows() -> FakeIo {
    FakeIo::with(&[
        (1, Rect::new(100.0, 100.0, 600.0, 400.0)),
        (2, Rect::new(200.0, 150.0, 500.0, 300.0)),
        (3, Rect::new(300.0, 200.0, 400.0, 300.0)),
    ])
}

#[test]
fn carried_windows_join_the_target_where_they_are_and_the_last_one_gets_focus() {
    let mut io = three_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one"), (3, "one")]);
    session.note_focus(WindowId(1));
    io.moves.clear();

    let carried = [WindowId(3), WindowId(2)];
    let focused = session
        .switch_carrying("two", &carried, &[DISPLAY], true, &mut io)
        .unwrap();
    assert!(focused);
    assert_eq!(session.active(), "two");
    assert_eq!(io.moves, [WindowId(1)], "only the window left behind moves");
    assert!(io.origin(1).x > 1400.0);
    assert_eq!(io.origin(2), Point::new(200.0, 150.0));
    assert_eq!(io.origin(3), Point::new(300.0, 200.0));
    assert_eq!(session.member(WindowId(3)).unwrap().workspace, "two");
    assert_eq!(io.focused, Some(WindowId(2)), "the last one picked up");
    assert!(session.journal().iter().all(|entry| entry.window == WindowId(1)));

    session.switch_to("one", &[DISPLAY], true, &mut io).unwrap();
    assert!(io.origin(2).x > 1400.0 && io.origin(3).x > 1400.0);
    assert_eq!(io.origin(1), Point::new(100.0, 100.0));
}

#[test]
fn carrying_skips_windows_of_other_workspaces() {
    let mut io = three_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one"), (3, "one")]);
    session.send(WindowId(3), "three", &[DISPLAY], &mut io).unwrap();

    session
        .switch_carrying("two", &[WindowId(3)], &[DISPLAY], true, &mut io)
        .unwrap();
    assert_eq!(session.member(WindowId(3)).unwrap().workspace, "three");
    assert!(io.origin(3).x > 1400.0);
    assert_eq!(io.focused, None, "workspace two has no windows of its own");
}

#[test]
fn focus_after_a_send_goes_to_the_most_recently_focused_window_left() {
    let mut io = three_windows();
    let mut session = session_with(&mut io, &[(1, "one"), (2, "one"), (3, "one")]);
    for id in [2, 1, 3] {
        session.note_focus(WindowId(id));
    }

    session.send(WindowId(3), "two", &[DISPLAY], &mut io).unwrap();
    assert_eq!(session.focus_recent(&mut io), Some(WindowId(1)));
    session.send(WindowId(1), "two", &[DISPLAY], &mut io).unwrap();
    assert_eq!(session.focus_recent(&mut io), Some(WindowId(2)));
    session.send(WindowId(2), "two", &[DISPLAY], &mut io).unwrap();
    io.focused = None;
    assert_eq!(session.focus_recent(&mut io), None);
    assert_eq!(io.focused, None);
}
