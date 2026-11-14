//! Layouts on each display: applying them, display changes, and selecting a layout by shortcut.

use std::time::Duration;

use snappy_core::geometry::Rect;
use snappy_core::keys::{KeyChord, Modifier};
use snappy_engine::Engine;
use snappy_fake::{FakePlatform, OverlayCall};
use snappy_ipc::Command;
use snappy_platform::{PlatformEvent, WindowId};

use crate::support::{
    LEFT_ZONE, app_state, command, drag, event, frame_of, hotkey_id, live_engine, paths_kept, place,
    scenario, settle, shift, side_display, side_third, snappy, thirds_on_the_side, with_window,
};

#[test]
fn applying_a_layout_assigns_it_to_the_display() {
    scenario(|| async {
        let engine = live_engine("apply-layout", FakePlatform::default());
        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "thirds".into(),
            },
        );
        let settings = engine.settings();
        assert_eq!(settings.layouts.displays["main"], "thirds");
        assert_eq!(settings.layouts.default_layout_id, "thirds");
    });
}

#[test]
fn selection_shortcuts_apply_to_the_display_under_the_pointer() {
    scenario(|| async {
        let engine = live_engine("select-layout", FakePlatform::default());
        let rows = snappy_core::layout::preset("rows").unwrap().clone();
        let mut options = engine.settings().layouts.options["rows"].clone();
        let chord = KeyChord::new("KeyR", [Modifier::Ctrl, Modifier::Alt]);
        options.select_shortcut = Some(chord.clone());
        command(
            &engine,
            Command::SaveLayout {
                layout: rows,
                options,
                display: "main".into(),
            },
        );
        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "halves".into(),
            },
        );
        event(&engine, PlatformEvent::HotKey(hotkey_id(&engine, &chord)));
        assert_eq!(engine.settings().layouts.displays["main"], "rows");
    });
}

#[test]
fn display_changes_settle_then_refresh() {
    scenario(|| async {
        let engine = live_engine("display-change", FakePlatform::default());
        engine.with_platform(|p| p.displays.push(side_display()));
        event(&engine, PlatformEvent::DisplaysChanged);
        tokio::time::sleep(Duration::from_millis(400)).await;
        let prepared = engine.with_platform(|p| {
            p.overlay
                .iter()
                .any(|call| *call == OverlayCall::Prepare("side".into()))
        });
        assert!(prepared);
    });
}

async fn displays_changed(
    engine: &Engine<FakePlatform>,
    change: impl FnOnce(&mut Vec<snappy_platform::Display>),
) {
    engine.with_platform(|p| change(&mut p.displays));
    event(engine, PlatformEvent::DisplaysChanged);
    tokio::time::sleep(Duration::from_millis(400)).await;
}

#[test]
fn a_display_keeps_its_layout_across_reconnects_and_restarts() {
    let floating = Rect::new(300.0, 200.0, 600.0, 400.0);
    let two_displays = || {
        let mut fake = with_window(floating);
        fake.displays.push(side_display());
        fake
    };

    scenario(|| async {
        let engine = live_engine("display-layout-kept", two_displays());
        thirds_on_the_side(&engine);

        displays_changed(&engine, |displays| displays.retain(|d| d.id != "side")).await;
        assert_eq!(engine.settings().layouts.displays["side"], "thirds");
        displays_changed(&engine, |displays| displays.push(side_display())).await;
        drag(&engine, WindowId(1), side_third(0).center(), shift()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), side_third(0));
    });

    scenario(|| async {
        let engine = Engine::new(two_displays(), paths_kept("display-layout-kept"));
        engine.start();
        assert!(engine.is_live());
        drag(&engine, WindowId(1), side_third(2).center(), shift()).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), side_third(2));
    });
}

#[test]
fn a_window_belongs_to_the_display_it_overlaps_most() {
    scenario(|| async {
        let mut fake = with_window(Rect::new(1300.0, 200.0, 600.0, 400.0));
        fake.displays.push(side_display());
        let engine = live_engine("display-overlap", fake);
        thirds_on_the_side(&engine);
        let first_zone = hotkey_id(&engine, &snappy("Digit1"));

        event(&engine, PlatformEvent::HotKey(first_zone));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), side_third(0));

        place(&engine, 1, Rect::new(1000.0, 200.0, 600.0, 400.0));
        event(&engine, PlatformEvent::HotKey(first_zone));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
    });
}

#[test]
fn displays_keep_the_operating_systems_order() {
    scenario(|| async {
        let mut fake = FakePlatform::default();
        fake.displays.insert(0, side_display());
        let engine = live_engine("display-order", fake);
        let tray: Vec<String> =
            engine.with_platform(|p| p.tray.displays.iter().map(|d| d.id.clone()).collect());
        assert_eq!(tray, ["side", "main"]);
        let listed: Vec<String> = app_state(&engine).displays.into_iter().map(|d| d.id).collect();
        assert_eq!(listed, ["side", "main"]);
    });
}

#[test]
fn preset_shapes_and_names_never_change_but_their_options_do() {
    scenario(|| async {
        let engine = live_engine("preset-options", FakePlatform::default());
        let halves = snappy_core::layout::preset("halves").unwrap().clone();
        let mut reshaped = halves.clone();
        reshaped.name = "My halves".into();
        reshaped.zones.truncate(1);
        reshaped.zones[0].rect = snappy_core::layout::UnitRect::new(0.0, 0.0, 1.0, 1.0);
        let mut options = engine.settings().layouts.options["halves"].clone();
        options.select_shortcut = Some(KeyChord::new("KeyH", [Modifier::Ctrl, Modifier::Alt]));
        options.zone_shortcuts.insert(
            "halves-0".into(),
            KeyChord::new("KeyJ", [Modifier::Ctrl, Modifier::Alt]),
        );

        command(
            &engine,
            Command::SaveLayout {
                layout: reshaped,
                options: options.clone(),
                display: "main".into(),
            },
        );
        let settings = engine.settings();
        assert_eq!(settings.layout("halves"), Some(&halves));
        assert!(settings.layouts.custom.is_empty());
        assert_eq!(settings.layouts.options["halves"], options);

        command(
            &engine,
            Command::SetLayoutEnabled {
                layout: "halves".into(),
                enabled: false,
            },
        );
        let settings = engine.settings();
        assert!(!settings.layouts.options["halves"].enabled);
        assert_eq!(settings.layout("halves"), Some(&halves));
    });
}
