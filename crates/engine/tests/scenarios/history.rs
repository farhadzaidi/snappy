//! Undo and redo of window moves.

use std::time::Duration;

use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{Mods, SnappyKey};
use snappy_core::settings::HistoryStep;
use snappy_engine::Engine;
use snappy_fake::{FakePlatform, FakeWindow};
use snappy_ipc::Command;
use snappy_platform::{PlatformEvent, WindowId};

use crate::support::{
    FLOATING, LEFT_ZONE, RIGHT_ZONE, command, drag, enable_workspaces, event, frame_of, frozen, is_parked,
    live_engine, press_history, scenario, send_to, settle, shift, snappy, status_text, switch_to,
    two_zoned_windows, undo, with_window,
};

#[test]
fn undo_returns_a_dragged_window() {
    scenario(|| async {
        let start = Rect::new(300.0, 200.0, 600.0, 400.0);
        let engine = live_engine("undo-drag", with_window(start));
        drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
        undo(&engine);
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), start);
    });
}

#[test]
fn there_is_no_undo_shortcut() {
    scenario(|| async {
        let engine = live_engine("undo-keys", FakePlatform::default());
        let undo = snappy("KeyZ").combo(SnappyKey::DEFAULT);
        assert!(engine.with_platform(|p| p.registered.iter().all(|(_, c)| *c != undo)));
    });
}

#[test]
fn undo_with_an_empty_history_says_so() {
    scenario(|| async {
        let engine = live_engine("undo-empty", with_window(LEFT_ZONE));
        undo(&engine);
        settle(&engine).await;
        assert_eq!(status_text(&engine), "There is no window move to undo.");
    });
}

#[test]
fn undo_and_redo_have_no_default_shortcuts() {
    scenario(|| async {
        let engine = live_engine("history-defaults", FakePlatform::default());
        let history = engine.settings().history;
        assert_eq!((history.undo, history.redo), (None, None));
    });
}

#[test]
fn redo_reapplies_an_undone_drag() {
    scenario(|| async {
        let engine = live_engine("redo-drag", with_window(FLOATING));
        drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
        undo(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), FLOATING);

        redo(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(status_text(&engine), "Last live move redone.");

        undo(&engine);
        settle(&engine).await;
        assert_eq!(
            frame_of(&engine, 1),
            FLOATING,
            "a redone move can be undone again"
        );
    });
}

#[test]
fn redo_moves_both_windows_of_an_undone_swap_back() {
    scenario(|| async {
        let engine = live_engine("redo-swap", two_zoned_windows());
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), shift()).await;
        undo(&engine);
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (LEFT_ZONE, RIGHT_ZONE)
        );

        redo(&engine);
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (RIGHT_ZONE, LEFT_ZONE)
        );
    });
}

#[test]
fn redo_with_nothing_undone_says_so() {
    scenario(|| async {
        let engine = live_engine("redo-empty", with_window(FLOATING));
        redo(&engine);
        settle(&engine).await;
        assert_eq!(status_text(&engine), "There is no window move to redo.");
    });
}

#[test]
fn a_new_move_after_undo_leaves_nothing_to_redo() {
    scenario(|| async {
        let engine = live_engine("redo-cleared", with_window(FLOATING));
        drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
        undo(&engine);
        settle(&engine).await;
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), shift()).await;
        redo(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), RIGHT_ZONE);
        assert_eq!(status_text(&engine), "There is no window move to redo.");
    });
}

fn redo(engine: &Engine<FakePlatform>) {
    press_history(engine, HistoryStep::Redo);
}

#[test]
fn a_press_during_a_grouped_undo_protects_the_window_and_keeps_the_entry() {
    scenario(|| async {
        let engine = live_engine("undo-protected", two_zoned_windows());
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), shift()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), LEFT_ZONE);

        frozen(&engine, 2, true);
        undo(&engine);
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(1))),
            LEFT_ZONE,
            "the undo is under way"
        );

        let pressed = Point::new(300.0, 400.0);
        event(
            &engine,
            PlatformEvent::PointerDown {
                at: pressed,
                mods: Mods::NONE,
                window: Some(WindowId(1)),
            },
        );
        event(
            &engine,
            PlatformEvent::PointerUp {
                at: pressed,
                mods: Mods::NONE,
            },
        );
        frozen(&engine, 2, false);
        settle(&engine).await;
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(1))),
            LEFT_ZONE,
            "recovery leaves the pressed window where the user holds it"
        );

        undo(&engine);
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), LEFT_ZONE);
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), RIGHT_ZONE);
        assert_eq!(status_text(&engine), "Last live move undone.");
    });
}

#[test]
fn sending_a_window_removes_its_undo_entries_everywhere() {
    scenario(|| async {
        let start = Rect::new(300.0, 200.0, 600.0, 400.0);
        let engine = live_engine("send-forgets-undo", with_window(start));
        drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
        enable_workspaces(&engine);
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), shift()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);

        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        assert!(is_parked(&engine, 1));
        undo(&engine);
        settle(&engine).await;
        assert_eq!(status_text(&engine), "There is no window move to undo.");
        assert!(is_parked(&engine, 1));

        command(&engine, Command::SetWorkspacesEnabled { enabled: false });
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        undo(&engine);
        settle(&engine).await;
        assert_eq!(status_text(&engine), "There is no window move to undo.");
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
    });
}

#[test]
fn each_workspace_has_its_own_history_and_otherwise_one_covers_every_window() {
    scenario(|| async {
        let other = Rect::new(250.0, 250.0, 500.0, 350.0);
        let mut fake = with_window(FLOATING);
        fake.open(2, FakeWindow::new("com.example.browser", 200, other));
        let engine = live_engine("undo-scopes", fake);
        let undo = || async {
            undo(&engine);
            settle(&engine).await;
        };

        drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
        drag(&engine, WindowId(2), Point::new(1000.0, 400.0), shift()).await;
        undo().await;
        assert_eq!((frame_of(&engine, 1), frame_of(&engine, 2)), (LEFT_ZONE, other));
        undo().await;
        assert_eq!(frame_of(&engine, 1), FLOATING);

        enable_workspaces(&engine);
        drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        undo().await;
        assert_eq!(status_text(&engine), "There is no window move to undo.");

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        undo().await;
        assert_eq!(frame_of(&engine, 1), FLOATING);
    });
}

#[test]
fn a_swap_undo_that_cannot_land_returns_both_windows_and_keeps_the_entry() {
    scenario(|| async {
        let engine = live_engine("undo-swap-fails", two_zoned_windows());
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), shift()).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (RIGHT_ZONE, LEFT_ZONE)
        );

        frozen(&engine, 2, true);
        undo(&engine);
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (RIGHT_ZONE, LEFT_ZONE)
        );
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error),
            (
                "An app couldn't restore its window. Undo is available to retry.",
                true
            )
        );

        frozen(&engine, 2, false);
        undo(&engine);
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (LEFT_ZONE, RIGHT_ZONE)
        );
        assert_eq!(status_text(&engine), "Last live move undone.");
    });
}
