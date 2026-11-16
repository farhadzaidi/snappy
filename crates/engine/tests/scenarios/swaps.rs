//! Moving a window into a zone another window fills: swapping, stacking on top, and floating windows.

use std::time::Duration;

use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{Modifier, Mods};
use snappy_core::placement::swap::WhenTaken;
use snappy_engine::{Engine, LogEvent, file_names};
use snappy_fake::{FakePlatform, FakeWindow, OverlayCall};
use snappy_ipc::Command;
use snappy_platform::{Caption, OverlayScene, PlatformEvent, WindowId};

use crate::support::{
    FLOATING, LEFT_ZONE, RIGHT_ZONE, click_away, command, drag, drop_outcomes, drop_records, event,
    floating_over_the_left_zone, frame_of, frozen, hotkey_id, inactive_window_in_the_right_zone, live_engine,
    logged, paths_kept, place, press, press_on, release, scenario, settle, shift, side_display, side_third,
    snappy, status_text, thirds_on_the_side, two_zoned_windows, undo, with_window,
};

#[test]
fn dropping_on_an_occupied_zone_exchanges_the_two_windows() {
    scenario(|| async {
        let mut fake = with_window(LEFT_ZONE);
        fake.open(2, FakeWindow::new("com.example.browser", 200, RIGHT_ZONE));
        let engine = live_engine("swap-drag", fake);
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), shift()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), LEFT_ZONE);
    });
}

fn stack_on_top(engine: &Engine<FakePlatform>) {
    command(
        engine,
        Command::SetWhenTaken {
            choice: WhenTaken::Stack,
        },
    );
}

#[test]
fn a_zone_shortcut_into_an_occupied_zone_swaps() {
    scenario(|| async {
        let engine = live_engine("keyboard-swap-zone", two_zoned_windows());
        let id = hotkey_id(&engine, &snappy("Digit2"));
        event(&engine, PlatformEvent::HotKey(id));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), LEFT_ZONE);
    });
}

#[test]
fn an_arrow_shortcut_into_an_occupied_zone_swaps() {
    scenario(|| async {
        let engine = live_engine("keyboard-swap-arrow", two_zoned_windows());
        let id = hotkey_id(&engine, &snappy("ArrowRight"));
        event(&engine, PlatformEvent::HotKey(id));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), LEFT_ZONE);
        assert_eq!(status_text(&engine), "Windows swapped.");
    });
}

#[test]
fn a_keyboard_move_into_an_empty_zone_snaps_as_before() {
    scenario(|| async {
        let engine = live_engine("keyboard-swap-empty", with_window(LEFT_ZONE));
        let id = hotkey_id(&engine, &snappy("Digit2"));
        event(&engine, PlatformEvent::HotKey(id));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(status_text(&engine), "Snapped.");
    });
}

#[test]
fn with_stack_on_top_keyboard_moves_cover_the_occupant() {
    scenario(|| async {
        let engine = live_engine("keyboard-stack", two_zoned_windows());
        stack_on_top(&engine);
        assert_eq!(engine.settings().snapping.when_taken, WhenTaken::Stack);
        for chord in [snappy("Digit2"), snappy("ArrowLeft"), snappy("ArrowRight")] {
            event(&engine, PlatformEvent::HotKey(hotkey_id(&engine, &chord)));
            settle(&engine).await;
        }
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(2))),
            RIGHT_ZONE,
            "the occupant stays where it was"
        );
        assert_eq!(status_text(&engine), "Snapped.");
    });
}

#[test]
fn with_stack_on_top_a_drop_covers_the_occupant() {
    scenario(|| async {
        let engine = live_engine("drag-stack", two_zoned_windows());
        stack_on_top(&engine);
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), shift()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), RIGHT_ZONE);
    });
}

#[test]
fn the_swap_key_makes_a_drop_stack_when_moves_swap() {
    scenario(|| async {
        let engine = live_engine("drag-inverted-stack", two_zoned_windows());
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), inverted()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), RIGHT_ZONE);
        assert_eq!(drop_outcomes("drag-inverted-stack"), ["snapped"]);
    });
}

#[test]
fn the_swap_key_makes_a_drop_swap_when_moves_stack() {
    scenario(|| async {
        let engine = live_engine("drag-inverted-swap", two_zoned_windows());
        stack_on_top(&engine);
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), inverted()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), LEFT_ZONE);
        assert_eq!(drop_outcomes("drag-inverted-swap"), ["swapped"]);
    });
}

/// The source kinds of the swaps in the debug log, oldest first.
fn swap_sources(name: &str) -> Vec<String> {
    logged(name, LogEvent::Swap)
        .iter()
        .map(|record| record["source"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn a_floating_window_moved_by_keyboard_sends_the_occupant_to_the_zone_under_its_center() {
    scenario(|| async {
        let engine = live_engine("floating-keyboard-swap", floating_over_the_left_zone(RIGHT_ZONE));
        engine.force_debug_log();
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("Digit2"))),
        );
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (RIGHT_ZONE, LEFT_ZONE)
        );
        assert_eq!(status_text(&engine), "Windows swapped.");
        assert_eq!(swap_sources("floating-keyboard-swap"), ["nearestZone"]);

        undo(&engine);
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (FLOATING, RIGHT_ZONE),
            "both windows return to their exact frames"
        );
    });
}

#[test]
fn a_floating_window_dropped_on_an_occupied_zone_sends_the_occupant_to_the_zone_under_its_center() {
    scenario(|| async {
        let engine = live_engine("floating-swap", floating_over_the_left_zone(RIGHT_ZONE));
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), shift()).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (RIGHT_ZONE, LEFT_ZONE)
        );
        assert_eq!(drop_outcomes("floating-swap"), ["swapped"]);
    });
}

#[test]
fn the_occupant_takes_the_floating_window_frame_when_the_zone_under_it_cannot_take_it() {
    scenario(|| async {
        // The zone under the floating window's center is the destination itself.
        let engine = live_engine(
            "floating-swap-destination",
            floating_over_the_left_zone(LEFT_ZONE),
        );
        engine.force_debug_log();
        drag(&engine, WindowId(1), Point::new(300.0, 400.0), shift()).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (LEFT_ZONE, FLOATING)
        );
        assert_eq!(swap_sources("floating-swap-destination"), ["floating"]);
    });

    scenario(|| async {
        // A third window already fills the zone under the floating window's center.
        let mut fake = floating_over_the_left_zone(RIGHT_ZONE);
        fake.open(3, FakeWindow::new("com.example.notes", 300, LEFT_ZONE));
        fake.focused = Some(WindowId(1));
        let engine = live_engine("floating-swap-taken", fake);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("Digit2"))),
        );
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (RIGHT_ZONE, FLOATING)
        );
        assert_eq!(frame_of(&engine, 3), LEFT_ZONE, "the third window stays put");
        assert_eq!(status_text(&engine), "Windows swapped.");
    });
}

#[test]
fn successful_swaps_post_a_quiet_status() {
    scenario(|| async {
        let mut fake = with_window(LEFT_ZONE);
        fake.open(2, FakeWindow::new("com.example.browser", 200, RIGHT_ZONE));
        let engine = live_engine("swap-status", fake);
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), shift()).await;
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.notify),
            ("Windows swapped.", false)
        );
    });
}

/// Shift with the swap key, which does the opposite of "When the zone is taken".
fn inverted() -> Mods {
    Mods::from_iter([Modifier::Shift, Modifier::Meta])
}

#[test]
fn the_overlay_names_only_the_swap_keys_reversal_and_outlines_the_source_only_for_a_swap() {
    scenario(|| async {
        let engine = live_engine("swap-overlay", two_zoned_windows());
        engine.force_debug_log();
        stack_on_top(&engine);
        press(&engine, Point::new(212.0, 52.0), shift());
        let dragged = LEFT_ZONE.with_origin(Point::new(800.0, 385.0));
        engine.with_platform(|p| p.windows.get_mut(&WindowId(1)).unwrap().frame = dragged);
        let at = Point::new(1000.0, 400.0);
        let scene = || match engine.with_platform(|p| p.last_overlay().cloned()) {
            Some(OverlayCall::Show(_, scene)) => scene,
            other => panic!("expected the overlay, got {other:?}"),
        };

        let unmarked = |scene: &OverlayScene| {
            scene
                .zones
                .iter()
                .all(|zone| zone.caption.is_none() && !zone.swap_source)
        };

        event(&engine, PlatformEvent::PointerDragged { at, mods: shift() });
        let stacking = scene();
        assert!(stacking.zones[1].highlighted);
        assert!(unmarked(&stacking), "stacking is the setting's own action");

        event(&engine, PlatformEvent::ModifiersChanged { at, mods: inverted() });
        let swapping = scene();
        assert!(swapping.zones[1].highlighted);
        assert_eq!(swapping.zones[1].caption, Some(Caption::Swap));
        assert!(!swapping.zones[1].swap_source);
        assert!(swapping.zones[0].swap_source && swapping.zones[0].caption.is_none());

        command(
            &engine,
            Command::SetWhenTaken {
                choice: WhenTaken::Swap,
            },
        );
        press_on(&engine, Point::new(1000.0, 52.0), shift(), WindowId(2));
        engine.with_platform(|p| p.windows.get_mut(&WindowId(2)).unwrap().frame = dragged);
        event(&engine, PlatformEvent::PointerDragged { at, mods: shift() });
        assert!(unmarked(&scene()), "swapping is the setting's own action");
        event(&engine, PlatformEvent::ModifiersChanged { at, mods: inverted() });
        let stacking = scene();
        assert_eq!(stacking.zones[1].caption, Some(Caption::Stack));
        assert!(stacking.zones.iter().all(|zone| !zone.swap_source));

        let captions: Vec<serde_json::Value> = logged("swap-overlay", LogEvent::OverlayCaption)
            .iter()
            .map(|record| record["caption"].clone())
            .collect();
        assert_eq!(captions, [serde_json::json!("Swap"), serde_json::json!("Stack")]);
    });
}

#[test]
fn a_press_reported_once_the_move_began_swaps_with_the_frame_from_before_it() {
    scenario(|| async {
        // As on KDE: the press, low in the title bar, arrives once the move is under way and
        // names where the pointer and the window were as it began; by the time it is read,
        // the window has moved on with the pointer.
        let engine = live_engine("swap-pressed-frame", floating_over_the_left_zone(LEFT_ZONE));
        let moved = FLOATING.with_origin(Point::new(FLOATING.x - 100.0, FLOATING.y + 40.0));
        engine.with_platform(|p| p.windows.get_mut(&WindowId(1)).unwrap().frame = moved);
        event(
            &engine,
            PlatformEvent::PointerDown {
                at: Point::new(FLOATING.x + 200.0, FLOATING.y + 40.0),
                mods: shift(),
                window: Some(WindowId(1)),
                frame: Some(FLOATING),
            },
        );
        let to = Point::new(300.0, 400.0);
        engine.with_platform(|p| {
            p.windows.get_mut(&WindowId(1)).unwrap().frame = FLOATING.with_origin(Point::new(100.0, 360.0))
        });
        event(
            &engine,
            PlatformEvent::PointerDragged {
                at: to,
                mods: shift(),
            },
        );
        event(
            &engine,
            PlatformEvent::PointerUp {
                at: to,
                mods: shift(),
            },
        );
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (LEFT_ZONE, FLOATING),
            "the occupant takes the frame the window had before the move"
        );
        let drops = drop_records("swap-pressed-frame");
        assert_eq!(
            drops[0]["startedInMoveArea"], true,
            "the press is placed on the window as it was, inside the title bar"
        );
    });
}

#[test]
fn inactive_workspace_windows_are_never_swap_candidates() {
    scenario(|| async {
        let engine = inactive_window_in_the_right_zone("swap-inactive").await;
        drag(&engine, WindowId(1), Point::new(1000.0, 400.0), shift()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(2))),
            RIGHT_ZONE,
            "the inactive window is left alone"
        );
    });
}

#[test]
fn swaps_work_across_displays_with_each_displays_layout() {
    scenario(|| async {
        let mut fake = FakePlatform::default();
        fake.displays.push(side_display());
        fake.open(2, FakeWindow::new("com.example.browser", 200, side_third(0)));
        fake.open(1, FakeWindow::new("com.example.editor", 100, LEFT_ZONE));
        let engine = live_engine("swap-across-displays", fake);
        thirds_on_the_side(&engine);

        drag(&engine, WindowId(1), side_third(0).center(), shift()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), side_third(0));
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), LEFT_ZONE);
        assert_eq!(status_text(&engine), "Windows swapped.");
    });
}

#[test]
fn a_click_elsewhere_during_an_exchange_moves_both_windows_back() {
    scenario(|| async {
        let engine = live_engine("swap-click-away", two_zoned_windows());
        frozen(&engine, 1, true);
        release(&engine, WindowId(1), Point::new(1000.0, 400.0), shift());
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(2))),
            LEFT_ZONE,
            "the exchange is under way"
        );

        click_away(&engine);
        frozen(&engine, 1, false);
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), LEFT_ZONE);
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), RIGHT_ZONE);
        assert!(
            !engine.status().unwrap().is_error,
            "a cancelled swap is not a failure"
        );

        let drops = std::fs::read(paths_kept("swap-click-away").log_dir.join(file_names::DROPS)).unwrap();
        let drops: Vec<serde_json::Value> = serde_json::from_slice(&drops).unwrap();
        assert_eq!(drops.last().unwrap()["outcome"], "swapInterrupted");
    });
}

#[test]
fn keyboard_swaps_ignore_inactive_workspace_windows() {
    scenario(|| async {
        let engine = inactive_window_in_the_right_zone("keyboard-swap-inactive").await;
        let id = hotkey_id(&engine, &snappy("ArrowRight"));
        event(&engine, PlatformEvent::HotKey(id));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(2))),
            RIGHT_ZONE,
            "the inactive window is not the occupant"
        );
    });
}

#[test]
fn occupants_are_looked_up_only_at_the_swap_drop() {
    scenario(|| async {
        let engine = live_engine("swap-lookup", two_zoned_windows());
        press(&engine, Point::new(212.0, 52.0), shift());
        for step in 1..=5 {
            let at = Point::new(300.0 + 140.0 * f64::from(step), 400.0);
            place(
                &engine,
                1,
                LEFT_ZONE.with_origin(Point::new(at.x - 200.0, at.y - 15.0)),
            );
            event(&engine, PlatformEvent::PointerDragged { at, mods: shift() });
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(engine.with_platform(|p| p.occupancy_queries), 0);

        event(
            &engine,
            PlatformEvent::PointerUp {
                at: Point::new(1000.0, 400.0),
                mods: shift(),
            },
        );
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (RIGHT_ZONE, LEFT_ZONE)
        );
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(engine.with_platform(|p| p.occupancy_queries), 1);
    });
}

#[test]
fn swap_drops_record_their_outcome() {
    scenario(|| async {
        let name = "swap-outcomes";
        let mut fake = two_zoned_windows();
        fake.open(3, FakeWindow::new("com.example.notes", 300, RIGHT_ZONE));
        fake.focused = Some(WindowId(1));
        let engine = live_engine(name, fake);
        let (left, right) = (Point::new(300.0, 400.0), Point::new(1000.0, 400.0));

        drag(&engine, WindowId(1), right, shift()).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE, "two occupants block the swap");

        place(&engine, 3, Rect::new(400.0, 500.0, 200.0, 200.0));
        drag(&engine, WindowId(1), right, shift()).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (RIGHT_ZONE, LEFT_ZONE)
        );

        frozen(&engine, 2, true);
        drag(&engine, WindowId(1), left, shift()).await;
        frozen(&engine, 2, false);
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (RIGHT_ZONE, LEFT_ZONE)
        );

        place(&engine, 2, Rect::new(500.0, 150.0, 300.0, 200.0));
        drag(&engine, WindowId(1), left, shift()).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);

        place(&engine, 2, RIGHT_ZONE);
        frozen(&engine, 1, true);
        release(&engine, WindowId(1), right, shift());
        tokio::time::sleep(Duration::from_millis(1)).await;
        click_away(&engine);
        frozen(&engine, 1, false);
        settle(&engine).await;

        assert_eq!(
            drop_outcomes(name),
            [
                "swapBlocked",
                "swapped",
                "swapFailed",
                "snapped",
                "swapInterrupted",
            ]
        );
    });
}
