//! Dragging a window onto a zone: the overlay, presses, drops, and how a snap settles.

use std::time::Duration;

use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{Modifier, Mods};
use snappy_core::settings::DragModifier;
use snappy_engine::{Engine, file_names};
use snappy_fake::{FakePlatform, FakeWindow, OverlayCall};
use snappy_ipc::Command;
use snappy_platform::{PlatformEvent, WindowId};

use crate::support::{
    FLOATING, LEFT_ZONE, RIGHT_ZONE, click_away, command, drag, drop_outcomes, event, frame_of, frozen,
    hotkey_id, live_engine, paths_kept, place, press, release, scenario, settle, shift, side_display,
    side_third, snappy, status_text, thirds_on_the_side, undo, with_window, writes_to,
};

#[test]
fn dropping_on_a_zone_snaps_the_window() {
    scenario(|| async {
        let engine = live_engine("drop-snaps", with_window(Rect::new(300.0, 200.0, 600.0, 400.0)));
        drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), LEFT_ZONE);
    });
}

#[test]
fn zones_show_only_with_the_modifier_after_moving() {
    scenario(|| async {
        let engine = live_engine(
            "zones-after-moving",
            with_window(Rect::new(300.0, 200.0, 600.0, 400.0)),
        );
        let grip = Point::new(500.0, 215.0);
        press(&engine, grip, shift());
        event(
            &engine,
            PlatformEvent::PointerDragged {
                at: grip,
                mods: shift(),
            },
        );
        assert!(!matches!(
            engine.with_platform(|p| p.last_overlay().cloned()),
            Some(OverlayCall::Show(..))
        ));

        engine.with_platform(|p| p.windows.get_mut(&WindowId(1)).unwrap().frame.x += 100.0);
        event(
            &engine,
            PlatformEvent::PointerDragged {
                at: Point::new(600.0, 215.0),
                mods: shift(),
            },
        );
        let shown = engine.with_platform(|p| p.last_overlay().cloned());
        let Some(OverlayCall::Show(display, scene)) = shown else {
            panic!("expected the overlay")
        };
        assert_eq!(display, "main");
        assert!(scene.zones[0].highlighted);
    });
}

#[test]
fn releasing_without_the_modifier_does_nothing() {
    scenario(|| async {
        let start = Rect::new(300.0, 200.0, 600.0, 400.0);
        let engine = live_engine("drop-without-modifier", with_window(start));
        drag(&engine, WindowId(1), Point::new(300.0, 400.0), Mods::NONE).await;
        assert_ne!(engine.with_platform(|p| p.frame_of(WindowId(1))), LEFT_ZONE);
    });
}

#[test]
fn overlays_are_prepared_before_the_first_drag() {
    scenario(|| async {
        let engine = live_engine("overlays-prepared", FakePlatform::default());
        let prepared = engine.with_platform(|p| {
            p.overlay
                .iter()
                .any(|call| *call == OverlayCall::Prepare("main".into()))
        });
        assert!(prepared);
    });
}

#[test]
fn a_press_hides_the_preview() {
    scenario(|| async {
        let engine = live_engine(
            "press-hides-preview",
            with_window(Rect::new(300.0, 200.0, 600.0, 400.0)),
        );
        command(
            &engine,
            Command::ShowPreview {
                display: "main".into(),
                layout: "halves".into(),
            },
        );
        assert!(matches!(
            engine.with_platform(|p| p.last_overlay().cloned()),
            Some(OverlayCall::Show(..))
        ));
        press(&engine, Point::new(500.0, 215.0), Mods::NONE);
        assert_eq!(
            engine.with_platform(|p| p.last_overlay().cloned()),
            Some(OverlayCall::Hide)
        );
    });
}

#[test]
fn sleep_cancels_a_drag() {
    scenario(|| async {
        let engine = live_engine("sleep", with_window(Rect::new(300.0, 200.0, 600.0, 400.0)));
        press(&engine, Point::new(500.0, 215.0), shift());
        event(&engine, PlatformEvent::WillSleep);
        engine.with_platform(|p| p.windows.get_mut(&WindowId(1)).unwrap().frame.x = 0.0);
        event(
            &engine,
            PlatformEvent::PointerUp {
                at: Point::new(300.0, 400.0),
                mods: shift(),
            },
        );
        settle(&engine).await;
        assert_ne!(engine.with_platform(|p| p.frame_of(WindowId(1))), LEFT_ZONE);
    });
}

#[test]
fn a_snap_posts_a_quiet_status() {
    scenario(|| async {
        let engine = live_engine("snap-status", with_window(Rect::new(300.0, 200.0, 600.0, 400.0)));
        drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error, status.notify),
            ("Snapped.", false, false)
        );
    });
}

#[test]
fn a_snap_the_app_constrained_is_reported_quietly() {
    scenario(|| async {
        let mut fake = with_window(Rect::new(300.0, 200.0, 900.0, 400.0));
        fake.windows.get_mut(&WindowId(1)).unwrap().min_size = snappy_core::geometry::Size::new(900.0, 1.0);
        let engine = live_engine("snap-constrained", fake);
        drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1)).width), 900.0);
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error, status.notify),
            ("Snapped, limited by the app's window size.", false, false)
        );
    });
}

const FLOATING_GRIP: Point = Point::new(500.0, 215.0);

#[test]
fn the_drag_modifier_is_shift_by_default_and_can_be_option_or_control() {
    scenario(|| async {
        let engine = live_engine("drag-modifier", with_window(FLOATING));
        assert_eq!(engine.settings().snapping.drag_modifier, DragModifier::Shift);
        drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);

        for (choice, key) in [
            (DragModifier::Alt, Modifier::Alt),
            (DragModifier::Ctrl, Modifier::Ctrl),
        ] {
            command(&engine, Command::SetDragModifier { modifier: choice });
            place(&engine, 1, FLOATING);
            drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
            assert_ne!(frame_of(&engine, 1), LEFT_ZONE, "Shift no longer snaps");

            place(&engine, 1, FLOATING);
            drag(
                &engine,
                WindowId(1),
                Point::new(300.0, 400.0),
                Mods::from_iter([key]),
            )
            .await;
            assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        }
    });
}

#[test]
fn a_press_takes_the_frontmost_window_under_the_pointer_and_undo_returns_it() {
    scenario(|| async {
        let behind = Rect::new(200.0, 150.0, 600.0, 400.0);
        let mut fake = with_window(behind);
        fake.open(2, FakeWindow::new("com.example.browser", 200, FLOATING));
        let engine = live_engine("press-frontmost", fake);
        assert!(engine.with_platform(|p| p.press_monitoring));

        drag(&engine, WindowId(2), Point::new(300.0, 400.0), shift()).await;
        assert_eq!(frame_of(&engine, 2), LEFT_ZONE);
        assert_eq!(frame_of(&engine, 1), behind);
        undo(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 2), FLOATING, "back to the frame at the press");

        command(&engine, Command::SetSnapping { enabled: false });
        assert!(!engine.with_platform(|p| p.press_monitoring));
        drag(&engine, WindowId(2), Point::new(300.0, 400.0), shift()).await;
        assert_ne!(frame_of(&engine, 2), LEFT_ZONE, "no live control, no tracking");
    });
}

#[test]
fn a_press_cancels_the_operation_in_flight_and_the_drag_in_progress() {
    scenario(|| async {
        let engine = live_engine("press-cancels", with_window(FLOATING));
        frozen(&engine, 1, true);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("Digit2"))),
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
        let written = writes_to(&engine, 1);
        assert!(written > 0, "the snap is under way");

        press(&engine, FLOATING_GRIP, Mods::NONE);
        frozen(&engine, 1, false);
        event(
            &engine,
            PlatformEvent::PointerUp {
                at: FLOATING_GRIP,
                mods: Mods::NONE,
            },
        );
        settle(&engine).await;
        assert_eq!(writes_to(&engine, 1), written, "nothing more is written");
        assert_eq!(frame_of(&engine, 1), FLOATING);

        press(&engine, FLOATING_GRIP, shift());
        let moved = FLOATING.with_origin(Point::new(100.0, 385.0));
        place(&engine, 1, moved);
        event(
            &engine,
            PlatformEvent::PointerDragged {
                at: Point::new(300.0, 400.0),
                mods: shift(),
            },
        );
        press(&engine, Point::new(1300.0, 800.0), shift());
        assert_eq!(
            engine.with_platform(|p| p.last_overlay().cloned()),
            Some(OverlayCall::Hide)
        );
        event(
            &engine,
            PlatformEvent::PointerUp {
                at: Point::new(300.0, 400.0),
                mods: shift(),
            },
        );
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), moved, "the earlier drag never drops");
    });
}

#[test]
fn the_overlay_follows_the_display_and_zone_under_the_pointer() {
    scenario(|| async {
        let mut fake = with_window(FLOATING);
        fake.displays.push(side_display());
        let engine = live_engine("overlay-follows", fake);
        thirds_on_the_side(&engine);
        press(&engine, FLOATING_GRIP, shift());
        place(&engine, 1, FLOATING.with_origin(Point::new(400.0, 200.0)));

        let shown = |at: Point| {
            event(&engine, PlatformEvent::PointerDragged { at, mods: shift() });
            match engine.with_platform(|p| p.last_overlay().cloned()) {
                Some(OverlayCall::Show(display, scene)) => (
                    display,
                    scene.zones.len(),
                    scene.zones.iter().position(|zone| zone.highlighted),
                ),
                other => panic!("expected the overlay, got {other:?}"),
            }
        };
        assert_eq!(shown(side_third(1).center()), ("side".into(), 3, Some(1)));
        assert_eq!(shown(Point::new(300.0, 400.0)), ("main".into(), 2, Some(0)));
        assert_eq!(shown(Point::new(720.0, 400.0)), ("main".into(), 2, None), "gap");
        assert_eq!(shown(Point::new(5.0, 400.0)), ("main".into(), 2, None), "padding");
        assert_eq!(
            shown(Point::new(300.0, 10.0)),
            ("main".into(), 2, None),
            "menu bar"
        );
    });
}

#[test]
fn every_modifier_drag_records_one_drop_outcome() {
    scenario(|| async {
        let name = "drop-outcomes";
        let engine = live_engine(name, with_window(FLOATING));
        let left = Point::new(300.0, 400.0);
        let attempt = |to: Point, mods: Mods| {
            let engine = &engine;
            async move {
                place(engine, 1, FLOATING);
                drag(engine, WindowId(1), to, mods).await;
            }
        };

        attempt(left, shift()).await;
        attempt(Point::new(720.0, 400.0), shift()).await;
        attempt(Point::new(-100.0, 400.0), shift()).await;
        attempt(left, Mods::NONE).await;

        place(&engine, 1, FLOATING);
        press(&engine, FLOATING_GRIP, shift());
        event(
            &engine,
            PlatformEvent::PointerUp {
                at: FLOATING_GRIP,
                mods: shift(),
            },
        );

        press(&engine, FLOATING_GRIP, shift());
        place(&engine, 1, FLOATING.with_origin(Point::new(100.0, 385.0)));
        event(
            &engine,
            PlatformEvent::PointerDragged {
                at: left,
                mods: shift(),
            },
        );
        event(
            &engine,
            PlatformEvent::PointerUp {
                at: left,
                mods: Mods::NONE,
            },
        );

        press(&engine, Point::new(1300.0, 800.0), shift());
        event(
            &engine,
            PlatformEvent::PointerUp {
                at: Point::new(1300.0, 800.0),
                mods: shift(),
            },
        );

        for layout in snappy_core::layout::presets() {
            command(
                &engine,
                Command::SetLayoutEnabled {
                    layout: layout.id.clone(),
                    enabled: false,
                },
            );
        }
        attempt(left, shift()).await;

        assert_eq!(
            drop_outcomes(name),
            [
                "snapped",
                "outsideZone",
                "outsideDisplay",
                "noWindowMovement",
                "modifierReleased",
                "targetLookupFailed",
                "layoutDisabled",
            ]
        );
    });
}

#[test]
fn without_a_candidate_the_window_stays_and_snappy_says_so_quietly() {
    scenario(|| async {
        let mut fake = with_window(RIGHT_ZONE);
        fake.displays.push(side_display());
        let engine = live_engine("arrow-no-candidate", fake);

        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("ArrowRight"))),
        );
        settle(&engine).await;
        assert_eq!(
            frame_of(&engine, 1),
            RIGHT_ZONE,
            "never crosses to the side display"
        );
        assert_eq!(writes_to(&engine, 1), 0);
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error, status.notify),
            ("No zone to the right of this window.", false, false)
        );

        place(&engine, 1, LEFT_ZONE);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("ArrowLeft"))),
        );
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE, "never wraps around");
        assert_eq!(status_text(&engine), "No zone to the left of this window.");
    });
}

fn bounds_calls(engine: &Engine<FakePlatform>, window: u64) -> Vec<&'static str> {
    engine.with_platform(|p| {
        p.writes
            .iter()
            .filter(|(id, _)| *id == WindowId(window))
            .map(|(_, kind)| *kind)
            .collect()
    })
}

#[test]
fn bounds_changes_are_bracketed_and_reread_after_the_restore_delay() {
    scenario(|| async {
        let name = "bounds-bracket";
        let engine = live_engine(name, with_window(FLOATING));
        engine.with_platform(|p| p.restore_delay = Duration::from_millis(300));
        release(&engine, WindowId(1), Point::new(300.0, 400.0), shift());
        tokio::time::sleep(Duration::from_millis(1)).await;
        let calls = bounds_calls(&engine, 1);
        assert_eq!(calls.first(), Some(&"beginBoundsChange"));
        assert_eq!(calls.last(), Some(&"endBoundsChange"));
        assert!(calls.len() > 2);
        assert_ne!(status_text(&engine), "Snapped.", "still waiting for the app");

        let restored = LEFT_ZONE.with_origin(Point::new(40.0, 37.0));
        place(&engine, 1, restored);
        settle(&engine).await;
        assert_eq!(status_text(&engine), "Snapped, limited by the app's window size.");
        let drops = std::fs::read(paths_kept(name).log(file_names::DROPS)).unwrap();
        let drops: Vec<serde_json::Value> = serde_json::from_slice(&drops).unwrap();
        assert_eq!(
            drops.last().unwrap()["actual"],
            serde_json::to_value(restored).unwrap()
        );

        engine.with_platform(|p| {
            p.restore_delay = Duration::ZERO;
            p.writes.clear();
        });
        place(&engine, 1, FLOATING);
        frozen(&engine, 1, true);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("Digit2"))),
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
        click_away(&engine);
        frozen(&engine, 1, false);
        settle(&engine).await;
        let calls = bounds_calls(&engine, 1);
        assert_eq!(calls.first(), Some(&"beginBoundsChange"));
        assert_eq!(
            calls.last(),
            Some(&"endBoundsChange"),
            "a cancelled change still ends"
        );
    });
}
