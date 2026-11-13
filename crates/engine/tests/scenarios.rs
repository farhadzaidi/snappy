//! End-to-end behavior on the fake platform.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;

use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{KeyChord, Modifier, Mods};
use snappy_core::settings::{DragModifier, FocusShortcut, HistoryStep, LayoutOptions, Settings};
use snappy_core::swap::WhenTaken;
use snappy_core::workspace::ParkingPolicy;
use snappy_engine::{Engine, Input, Paths};
use snappy_fake::{FakePlatform, FakeWindow, OverlayCall};
use snappy_ipc::{AppState, Command, ToSettings};
use snappy_platform::{
    Caption, LiveControl, MarkStyle, OverlayScene, PlatformEvent, Remedy, WindowId, WindowSystem,
};

const LEFT_ZONE: Rect = Rect::new(12.0, 37.0, 702.0, 851.0);
const RIGHT_ZONE: Rect = Rect::new(726.0, 37.0, 702.0, 851.0);

/// Fresh paths for a test: nothing on disk yet.
fn paths(name: &str) -> Paths {
    let kept = paths_kept(name);
    if let Some(root) = kept.settings.parent() {
        let _ = std::fs::remove_dir_all(root);
    }
    kept
}

/// The same paths, keeping whatever an earlier engine in the test wrote.
fn paths_kept(name: &str) -> Paths {
    let root = std::env::temp_dir().join(format!("snappy-engine-{name}-{}", std::process::id()));
    Paths {
        settings: root.join("settings.json"),
        state_dir: root.join("state"),
        log_dir: root.join("logs"),
    }
}

/// Run a test on a single-threaded runtime with paused time, so settle waits are instant.
fn scenario<F: Future<Output = ()>>(test: impl FnOnce() -> F) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, test());
}

/// A live engine on the halves layout, where every layout's zones have the Snappy key with
/// their number as shortcuts.
fn live_engine(name: &str, fake: FakePlatform) -> Engine<FakePlatform> {
    let paths = paths(name);
    let mut settings = Settings::defaults(fake.platform);
    for layout in snappy_core::layout::presets() {
        let mut options = LayoutOptions::empty();
        for (index, zone) in layout.zones.iter().enumerate().take(9) {
            let chord = snappy(&format!("Digit{}", index + 1));
            options.zone_shortcuts.insert(zone.id.clone(), chord);
        }
        settings.layouts.options.insert(layout.id.clone(), options);
    }
    if let Some(folder) = paths.settings.parent() {
        std::fs::create_dir_all(folder).unwrap();
    }
    std::fs::write(&paths.settings, settings.to_json().unwrap()).unwrap();
    let engine = Engine::new(fake, paths);
    engine.start();
    command(
        &engine,
        Command::ApplyLayout {
            display: "main".into(),
            layout: "halves".into(),
        },
    );
    command(&engine, Command::SetSnapping { enabled: true });
    engine
}

fn command(engine: &Engine<FakePlatform>, command: Command) {
    engine.handle(Input::Command { id: 1, command });
}

fn event(engine: &Engine<FakePlatform>, event: PlatformEvent) {
    engine.handle(Input::Platform(event));
}

fn shift() -> Mods {
    Mods::from_iter([Modifier::Shift])
}

fn with_window(frame: Rect) -> FakePlatform {
    let mut fake = FakePlatform::default();
    fake.open(1, FakeWindow::new("com.example.editor", 100, frame));
    fake
}

/// Press a window's title bar, let the app follow the pointer, release, and settle.
async fn drag(engine: &Engine<FakePlatform>, window: WindowId, to: Point, mods: Mods) {
    release(engine, window, to, mods);
    settle(engine).await;
}

/// Drag like `drag`, but return as soon as the button is up.
fn release(engine: &Engine<FakePlatform>, window: WindowId, to: Point, mods: Mods) {
    let start = engine.with_platform(|p| p.frame_of(window));
    let grip = Point::new(start.x + 200.0, start.y + 15.0);
    event(
        engine,
        PlatformEvent::PointerDown {
            at: grip,
            mods,
            window: None,
        },
    );
    let moved = start.with_origin(Point::new(to.x - 200.0, to.y - 15.0));
    engine.with_platform(|p| p.windows.get_mut(&window).unwrap().frame = moved);
    event(engine, PlatformEvent::PointerDragged { at: to, mods });
    event(engine, PlatformEvent::PointerUp { at: to, mods });
}

async fn settle(engine: &Engine<FakePlatform>) {
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        engine.idle().await;
    }
}

fn hotkey_id(engine: &Engine<FakePlatform>, chord: &KeyChord) -> u32 {
    engine.with_platform(|p| {
        p.registered
            .iter()
            .find(|(_, registered)| *registered == chord.combo())
            .map(|(id, _)| *id)
            .unwrap()
    })
}

fn snappy(code: &str) -> KeyChord {
    KeyChord::snappy(code, [])
}

fn started(name: &str, fake: FakePlatform) -> Engine<FakePlatform> {
    let engine = Engine::new(fake, paths(name));
    engine.start();
    engine
}

fn without_permission() -> FakePlatform {
    FakePlatform {
        granted: false,
        ..FakePlatform::default()
    }
}

#[test]
fn a_new_install_with_permission_snaps_at_once() {
    scenario(|| async {
        let engine = started("install-granted", FakePlatform::default());
        assert!(engine.is_live());
        assert_eq!(
            engine.with_platform(|p| (p.access_requests, p.settings_page_opens)),
            (0, 0)
        );
        assert!(engine.with_platform(|p| p.press_monitoring));
    });
}

#[test]
fn a_new_install_asks_once_and_starts_on_the_grant() {
    scenario(|| async {
        let engine = started("install-asks", without_permission());
        assert!(!engine.is_live());
        assert_eq!(engine.with_platform(|p| p.access_requests), 1);
        assert!(engine.with_platform(|p| p.waiting_for_grant));
        assert!(
            engine.with_platform(|p| p.registered.is_empty()),
            "no shortcuts without permission"
        );

        engine.with_platform(|p| p.granted = true);
        event(&engine, PlatformEvent::PermissionChanged);
        assert!(engine.is_live(), "the grant starts snapping with no extra click");
        assert_eq!(
            engine.with_platform(|p| p.access_requests),
            1,
            "the grant needs no second request"
        );
    });
}

#[test]
fn later_launches_without_permission_wait_quietly() {
    scenario(|| async {
        let name = "later-launch";
        drop(started(name, without_permission()));
        let engine = Engine::new(without_permission(), paths_kept(name));
        engine.start();
        assert!(!engine.is_live());
        assert_eq!(
            engine.with_platform(|p| (p.access_requests, p.settings_page_opens)),
            (0, 0)
        );
        assert!(engine.with_platform(|p| p.waiting_for_grant));
    });
}

#[test]
fn turning_snapping_on_without_permission_asks() {
    scenario(|| async {
        let engine = started("snapping-asks", FakePlatform::default());
        command(&engine, Command::SetSnapping { enabled: false });
        engine.with_platform(|p| p.granted = false);
        event(&engine, PlatformEvent::PermissionChanged);
        command(&engine, Command::SetSnapping { enabled: true });
        assert!(!engine.is_live());
        assert_eq!(engine.with_platform(|p| p.access_requests), 1);
        assert_eq!(
            status_text(&engine),
            "Allow Snappy in Accessibility. Snapping will start automatically."
        );
    });
}

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
        event(
            &engine,
            PlatformEvent::PointerDown {
                at: grip,
                mods: shift(),
                window: None,
            },
        );
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
        let undo = snappy("KeyZ").combo();
        assert!(engine.with_platform(|p| p.registered.iter().all(|(_, c)| *c != undo)));
    });
}

#[test]
fn zone_shortcuts_move_the_focused_window() {
    scenario(|| async {
        let engine = live_engine(
            "zone-shortcut",
            with_window(Rect::new(300.0, 200.0, 600.0, 400.0)),
        );
        let id = hotkey_id(&engine, &snappy("Digit2"));
        event(&engine, PlatformEvent::HotKey(id));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
    });
}

#[test]
fn arrow_shortcuts_move_between_zones() {
    scenario(|| async {
        let engine = live_engine("arrow-shortcut", with_window(LEFT_ZONE));
        let id = hotkey_id(&engine, &snappy("ArrowRight"));
        event(&engine, PlatformEvent::HotKey(id));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
    });
}

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

#[test]
fn recording_unregisters_every_shortcut() {
    scenario(|| async {
        let engine = live_engine("recording", FakePlatform::default());
        assert!(!engine.with_platform(|p| p.registered.is_empty()));
        command(&engine, Command::BeginRecording);
        assert!(engine.with_platform(|p| p.registered.is_empty()));
        command(&engine, Command::EndRecording);
        assert!(!engine.with_platform(|p| p.registered.is_empty()));
    });
}

/// The settings window, open on its own and enrolled in workspace 1.
fn with_settings_window() -> (FakePlatform, Rect) {
    let settings_frame = Rect::new(200.0, 100.0, 1080.0, 760.0);
    let mut fake = FakePlatform::default();
    fake.open(9, FakeWindow::new("local.snappy.settings", 4242, settings_frame));
    (fake, settings_frame)
}

#[test]
fn the_settings_window_parks_like_any_window() {
    scenario(|| async {
        let (fake, settings_frame) = with_settings_window();
        let engine = live_engine("settings-parks", fake);
        engine.handle(Input::SettingsAppStarted { pid: 4242 });
        enable_workspaces(&engine);

        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        assert!(
            engine.with_platform(|p| p.frame_of(WindowId(9)).x) >= 1439.0,
            "parked"
        );

        let switch_one = hotkey_id(&engine, &KeyChord::new("Digit1", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_one));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(9))), settings_frame);
    });
}

#[test]
fn opening_settings_switches_to_its_workspace_then_focuses_it() {
    scenario(|| async {
        let (fake, settings_frame) = with_settings_window();
        let engine = live_engine("settings-reveal", fake);
        engine.handle(Input::SettingsAppStarted { pid: 4242 });
        enable_workspaces(&engine);
        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        engine.take_outbox();

        event(&engine, PlatformEvent::OpenSettingsRequested);
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-1");
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(9))), settings_frame);
        assert!(engine.take_outbox().contains(&snappy_ipc::ToSettings::Focus));
    });
}

#[test]
fn layouts_applied_while_workspaces_run_belong_to_the_active_workspace() {
    scenario(|| async {
        let engine = live_engine("workspace-layouts", FakePlatform::default());
        enable_workspaces(&engine);
        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "rows".into(),
            },
        );
        let settings = engine.settings();
        assert_eq!(settings.workspaces.items[0].displays["main"], "rows");
        assert_eq!(settings.workspaces.items[1].displays["main"], "halves");
        assert_eq!(settings.layouts.displays["main"], "halves");
    });
}

#[test]
fn a_switch_asked_for_while_displays_change_runs_once_they_settle() {
    scenario(|| async {
        let start = Rect::new(300.0, 200.0, 600.0, 400.0);
        let engine = live_engine("switch-during-display-change", with_window(start));
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::DisplaysChanged);
        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        assert_eq!(
            engine.settings().workspaces.active_id,
            "workspace-1",
            "held while settling"
        );
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-2");
        assert!(
            engine.with_platform(|p| p.frame_of(WindowId(1)).x) >= 1439.0,
            "parked"
        );
    });
}

#[test]
fn a_parked_window_nobody_remembers_comes_back_on_screen() {
    scenario(|| async {
        let stranded = Rect::new(1439.0, 500.0, 600.0, 400.0);
        let engine = live_engine("stranded-window", with_window(stranded));
        enable_workspaces(&engine);
        let frame = engine.with_platform(|p| p.frame_of(WindowId(1)));
        assert_eq!(
            frame,
            Rect::new(840.0, 500.0, 600.0, 400.0),
            "just inside the right edge"
        );

        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        assert!(
            engine.with_platform(|p| p.frame_of(WindowId(1)).x) >= 1439.0,
            "it joined the workspace and parks with it"
        );
    });
}

#[test]
fn a_visible_window_joins_the_active_workspace_whatever_was_remembered() {
    scenario(|| async {
        let name = "visible-joins-active";
        let visible = Rect::new(300.0, 200.0, 600.0, 400.0);
        let paths = paths(name);
        std::fs::create_dir_all(&paths.state_dir).unwrap();
        let saved = serde_json::json!([{
            "key": { "pid": 100, "launchTime": 1.0, "window": "1" },
            "workspaceId": "workspace-2",
            "frame": visible,
            "parkedOrigin": null,
        }]);
        std::fs::write(
            paths.state_dir.join("workspace-membership.json"),
            saved.to_string(),
        )
        .unwrap();

        let engine = Engine::new(with_window(visible), paths);
        engine.start();
        command(&engine, Command::SetSnapping { enabled: true });
        enable_workspaces(&engine);
        settle(&engine).await;
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(1))),
            visible,
            "never hidden at start"
        );

        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        assert!(
            engine.with_platform(|p| p.frame_of(WindowId(1)).x) >= 1439.0,
            "it belongs to the workspace that was active"
        );
    });
}

#[test]
fn a_window_parked_for_another_workspace_stays_parked_with_its_home() {
    scenario(|| async {
        let name = "parked-keeps-home";
        let home = Rect::new(300.0, 200.0, 600.0, 400.0);
        let parked = Rect::new(1439.0, 500.0, 600.0, 400.0);
        let paths = paths(name);
        std::fs::create_dir_all(&paths.state_dir).unwrap();
        let saved = serde_json::json!([{
            "key": { "pid": 100, "launchTime": 1.0, "window": "1" },
            "workspaceId": "workspace-2",
            "frame": home,
            "parkedOrigin": parked.origin(),
        }]);
        std::fs::write(
            paths.state_dir.join("workspace-membership.json"),
            saved.to_string(),
        )
        .unwrap();

        let engine = Engine::new(with_window(parked), paths);
        engine.start();
        command(&engine, Command::SetSnapping { enabled: true });
        enable_workspaces(&engine);
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(1))),
            parked,
            "still parked"
        );

        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(1))),
            home,
            "back home"
        );
    });
}

#[test]
fn switching_hides_and_restores_windows() {
    scenario(|| async {
        let start = Rect::new(300.0, 200.0, 600.0, 400.0);
        let engine = live_engine("switch", with_window(start));
        enable_workspaces(&engine);
        assert!(engine.settings().workspaces.enabled);

        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        assert!(
            engine.with_platform(|p| p.frame_of(WindowId(1)).x) >= 1439.0,
            "parked at the corner"
        );
        assert_eq!(engine.settings().workspaces.active_id, "workspace-2");

        let switch_one = hotkey_id(&engine, &KeyChord::new("Digit1", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_one));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), start);
    });
}

#[test]
fn quitting_returns_parked_windows() {
    scenario(|| async {
        let start = Rect::new(300.0, 200.0, 600.0, 400.0);
        let engine = live_engine("quit-returns", with_window(start));
        enable_workspaces(&engine);
        let send_two = hotkey_id(
            &engine,
            &KeyChord::new("Digit2", [Modifier::Ctrl, Modifier::Shift]),
        );
        event(&engine, PlatformEvent::HotKey(send_two));
        settle(&engine).await;
        assert!(engine.with_platform(|p| p.frame_of(WindowId(1)).x) >= 1439.0);
        engine.shutdown();
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), start);
    });
}

#[test]
fn an_unreadable_settings_file_is_never_replaced() {
    scenario(|| async {
        let paths = paths("unreadable-settings");
        std::fs::create_dir_all(paths.settings.parent().unwrap()).unwrap();
        std::fs::write(&paths.settings, b"{ broken").unwrap();
        let engine = Engine::new(FakePlatform::default(), paths.clone());
        engine.start();
        command(&engine, Command::SetSnapping { enabled: false });
        assert_eq!(std::fs::read(&paths.settings).unwrap(), b"{ broken");
        assert!(engine.settings().snapping.enabled, "the change is refused");
    });
}

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

fn status_text(engine: &Engine<FakePlatform>) -> String {
    engine.status().map(|status| status.message).unwrap_or_default()
}

#[test]
fn turning_off_while_waiting_cancels_the_pending_start() {
    scenario(|| async {
        let fake = FakePlatform {
            granted: false,
            ..FakePlatform::default()
        };
        let engine = live_engine("off-while-waiting", fake);
        command(&engine, Command::SetSnapping { enabled: false });
        engine.with_platform(|p| p.granted = true);
        event(&engine, PlatformEvent::PermissionChanged);
        assert!(!engine.is_live());
    });
}

#[test]
fn revoking_permission_pauses_and_regranting_resumes() {
    scenario(|| async {
        let engine = live_engine("revoke", FakePlatform::default());
        engine.with_platform(|p| p.granted = false);
        event(&engine, PlatformEvent::PermissionChanged);
        assert!(!engine.is_live());
        assert_eq!(
            status_text(&engine),
            "Accessibility access is off. Snapping is paused."
        );
        engine.with_platform(|p| p.granted = true);
        event(&engine, PlatformEvent::PermissionChanged);
        assert!(engine.is_live());
        assert_eq!(status_text(&engine), "Snapping is on.");
    });
}

#[test]
fn unavailable_live_control_shows_the_platforms_reason_and_registers_nothing() {
    scenario(|| async {
        let fake = FakePlatform {
            live_control: LiveControl::unavailable("Log out and back in to finish setting up."),
            ..FakePlatform::default()
        };
        let engine = live_engine("live-control-unavailable", fake);
        assert!(!engine.is_live());
        assert!(engine.with_platform(|p| p.registered.is_empty()));
        assert!(!engine.with_platform(|p| p.press_monitoring));
        assert_eq!(
            app_state(&engine).snapping.unavailable_reason.as_deref(),
            Some("Log out and back in to finish setting up.")
        );
    });
}

#[test]
fn live_control_arriving_starts_snapping_and_leaving_pauses_it_with_the_reason() {
    scenario(|| async {
        let fake = FakePlatform {
            live_control: LiveControl::unavailable("The extension is off."),
            ..FakePlatform::default()
        };
        let engine = live_engine("live-control-changes", fake);
        engine.with_platform(|p| p.live_control = LiveControl::Available);
        event(&engine, PlatformEvent::LiveControlChanged);
        assert!(engine.is_live());
        assert!(!engine.with_platform(|p| p.registered.is_empty()));
        assert_eq!(status_text(&engine), "Snapping is on.");
        assert_eq!(app_state(&engine).snapping.unavailable_reason, None);

        engine.with_platform(|p| p.live_control = LiveControl::unavailable("The extension is off."));
        event(&engine, PlatformEvent::LiveControlChanged);
        assert!(!engine.is_live());
        assert!(engine.with_platform(|p| p.registered.is_empty()));
        assert_eq!(status_text(&engine), "The extension is off.");
    });
}

#[test]
fn live_control_arriving_reads_the_displays_again() {
    scenario(|| async {
        let fake = FakePlatform {
            live_control: LiveControl::unavailable("The extension is off."),
            ..FakePlatform::default()
        };
        let engine = started("live-control-displays", fake);
        let moved_work_area = Rect::new(0.0, 60.0, 1440.0, 840.0);
        engine.with_platform(|p| {
            p.live_control = LiveControl::Available;
            p.displays[0].work_area = moved_work_area;
        });
        event(&engine, PlatformEvent::LiveControlChanged);
        assert_eq!(app_state(&engine).displays[0].work_area, moved_work_area);
    });
}

#[test]
fn the_platform_knows_the_drag_key_and_hears_when_it_changes() {
    scenario(|| async {
        let engine = live_engine("drag-key-reaches-platform", FakePlatform::default());
        assert_eq!(engine.with_platform(|p| p.drag_modifier), Some(Modifier::Shift));
        command(
            &engine,
            Command::SetDragModifier {
                modifier: DragModifier::Ctrl,
            },
        );
        assert_eq!(engine.with_platform(|p| p.drag_modifier), Some(Modifier::Ctrl));
    });
}

#[test]
fn a_remedy_the_platform_offers_is_published_and_applied_on_request() {
    scenario(|| async {
        let fake = FakePlatform {
            live_control: LiveControl::Unavailable {
                reason: "Log out and sign back in.".into(),
                remedy: Some(Remedy::LogOut),
            },
            ..FakePlatform::default()
        };
        let engine = live_engine("remedy-offered", fake);
        assert_eq!(
            app_state(&engine).snapping.unavailable_remedy,
            Some(Remedy::LogOut)
        );
        command(
            &engine,
            Command::ApplyRemedy {
                remedy: Remedy::LogOut,
            },
        );
        assert_eq!(engine.with_platform(|p| p.remedies.clone()), vec![Remedy::LogOut]);
    });
}

#[test]
fn platform_notes_are_published_and_read_again_when_they_change() {
    scenario(|| async {
        let fake = FakePlatform {
            notes: vec!["Another tool also snaps windows.".into()],
            ..FakePlatform::default()
        };
        let engine = started("platform-notes", fake);
        assert_eq!(
            app_state(&engine).platform_notes,
            vec!["Another tool also snaps windows.".to_owned()]
        );
        engine.with_platform(|p| p.notes.clear());
        event(&engine, PlatformEvent::NotesChanged);
        assert!(app_state(&engine).platform_notes.is_empty());
    });
}

#[test]
fn turning_snapping_on_and_off_explains_itself() {
    scenario(|| async {
        let engine = live_engine("snapping-messages", FakePlatform::default());
        assert_eq!(
            status_text(&engine),
            "Snapping is on. Hold Shift ⇧ while dragging a window."
        );
        command(&engine, Command::SetSnapping { enabled: false });
        assert_eq!(status_text(&engine), "Snapping is paused.");
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
        event(
            &engine,
            PlatformEvent::PointerDown {
                at: Point::new(500.0, 215.0),
                mods: Mods::NONE,
                window: None,
            },
        );
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
        event(
            &engine,
            PlatformEvent::PointerDown {
                at: Point::new(500.0, 215.0),
                mods: shift(),
                window: None,
            },
        );
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

/// Window 1 focused in the left zone, window 2 in the right zone.
fn two_zoned_windows() -> FakePlatform {
    let mut fake = FakePlatform::default();
    fake.open(2, FakeWindow::new("com.example.browser", 200, RIGHT_ZONE));
    fake.open(1, FakeWindow::new("com.example.editor", 100, LEFT_ZONE));
    fake
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

/// Window 1 focused and floating over the left zone, its center inside it, and window 2
/// filling `occupied`.
fn floating_over_the_left_zone(occupied: Rect) -> FakePlatform {
    let mut fake = FakePlatform::default();
    fake.open(2, FakeWindow::new("com.example.browser", 200, occupied));
    fake.open(1, FakeWindow::new("com.example.editor", 100, FLOATING));
    fake
}

/// The source kinds of the swaps in the debug log, oldest first.
fn swap_sources(name: &str) -> Vec<String> {
    std::fs::read_to_string(paths_kept(name).log_dir.join("debug.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|record| record["event"] == "swap")
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

#[test]
fn disabled_layouts_register_no_zone_shortcuts() {
    scenario(|| async {
        let engine = live_engine("disabled-layout-shortcuts", FakePlatform::default());
        command(
            &engine,
            Command::SetLayoutEnabled {
                layout: "halves".into(),
                enabled: false,
            },
        );
        let digit_one = snappy("Digit1").combo();
        let focus_zones =
            engine.with_platform(|p| p.registered.iter().filter(|(_, c)| *c == digit_one).count());
        assert_eq!(
            focus_zones, 1,
            "the display falls back to another enabled layout's zones"
        );
        assert!(
            engine.settings().layouts.options["halves"]
                .zone_shortcuts
                .contains_key("halves-0"),
            "bindings are kept"
        );
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
fn workspaces_start_off_with_three() {
    scenario(|| async {
        let engine = live_engine("workspace-defaults", FakePlatform::default());
        let workspaces = engine.settings().workspaces;
        assert!(!workspaces.enabled);
        assert_eq!(workspaces.items.len(), 3);
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

/// A second display to the right of the main one.
fn side_display() -> snappy_platform::Display {
    snappy_platform::Display {
        id: "side".into(),
        name: "Side".into(),
        frame: Rect::new(1440.0, 0.0, 1920.0, 1080.0),
        work_area: Rect::new(1440.0, 25.0, 1920.0, 1055.0),
        scale: 1.0,
    }
}

/// A zone of the thirds layout on the side display.
fn side_third(index: usize) -> Rect {
    let thirds = snappy_core::layout::preset("thirds").unwrap();
    snappy_core::layout::zone_frame(
        thirds.zones[index].rect,
        side_display().work_area,
        snappy_core::layout::Spacing::STANDARD,
    )
}

/// Thirds on the side display, halves on the main one and as the default.
fn thirds_on_the_side(engine: &Engine<FakePlatform>) {
    for (display, layout) in [("side", "thirds"), ("main", "halves")] {
        command(
            engine,
            Command::ApplyLayout {
                display: display.into(),
                layout: layout.into(),
            },
        );
    }
}

async fn displays_changed(
    engine: &Engine<FakePlatform>,
    change: impl FnOnce(&mut Vec<snappy_platform::Display>),
) {
    engine.with_platform(|p| change(&mut p.displays));
    event(engine, PlatformEvent::DisplaysChanged);
    tokio::time::sleep(Duration::from_millis(400)).await;
}

/// Shift with the swap key, which does the opposite of "When the zone is taken".
fn inverted() -> Mods {
    Mods::from_iter([Modifier::Shift, Modifier::Meta])
}

fn frozen(engine: &Engine<FakePlatform>, window: u64, frozen: bool) {
    engine.with_platform(|p| p.windows.get_mut(&WindowId(window)).unwrap().frozen = frozen);
}

/// Size and position writes to a window so far.
fn writes_to(engine: &Engine<FakePlatform>, window: u64) -> usize {
    engine.with_platform(|p| {
        p.writes
            .iter()
            .filter(|(id, kind)| *id == WindowId(window) && matches!(*kind, "size" | "position"))
            .count()
    })
}

/// Give each workspace Control+n to switch, in place of its default Snappy key with n,
/// which `live_engine`'s zones use, Control+Shift+n to send, and Control+Option+n to move
/// here and switch, then turn workspaces on.
fn enable_workspaces(engine: &Engine<FakePlatform>) {
    for (index, mut workspace) in engine.settings().workspaces.items.into_iter().enumerate() {
        let digit = format!("Digit{}", index + 1);
        let chord = |mods: &[Modifier]| Some(KeyChord::new(&digit, mods.iter().copied()));
        workspace.switch_shortcut = chord(&[Modifier::Ctrl]);
        workspace.send_shortcut = chord(&[Modifier::Ctrl, Modifier::Shift]);
        workspace.send_and_switch_shortcut = chord(&[Modifier::Ctrl, Modifier::Alt]);
        command(engine, Command::UpdateWorkspace { workspace });
    }
    command(engine, Command::SetWorkspacesEnabled { enabled: true });
}

/// Undo or redo through its shortcut, assigning it first since neither has a default.
fn press_history(engine: &Engine<FakePlatform>, step: HistoryStep) {
    let chord = match step {
        HistoryStep::Undo => snappy("KeyU"),
        HistoryStep::Redo => snappy("KeyY"),
    };
    if engine.settings().history.get(step) != Some(&chord) {
        command(
            engine,
            Command::SetHistoryShortcut {
                step,
                chord: Some(chord.clone()),
            },
        );
    }
    event(engine, PlatformEvent::HotKey(hotkey_id(engine, &chord)));
}

fn undo(engine: &Engine<FakePlatform>) {
    press_history(engine, HistoryStep::Undo);
}

fn redo(engine: &Engine<FakePlatform>) {
    press_history(engine, HistoryStep::Redo);
}

fn switch_to(engine: &Engine<FakePlatform>, number: u8) -> u32 {
    hotkey_id(
        engine,
        &KeyChord::new(&format!("Digit{number}"), [Modifier::Ctrl]),
    )
}

fn send_to(engine: &Engine<FakePlatform>, number: u8) -> u32 {
    hotkey_id(
        engine,
        &KeyChord::new(&format!("Digit{number}"), [Modifier::Ctrl, Modifier::Shift]),
    )
}

fn is_parked(engine: &Engine<FakePlatform>, window: u64) -> bool {
    engine.with_platform(|p| p.frame_of(WindowId(window)).x) >= 1439.0
}

/// Workspaces on, window 2 sent to workspace 2 and then shown in the right zone again by
/// its app, and window 1 focused in the left zone.
async fn inactive_window_in_the_right_zone(name: &str) -> Engine<FakePlatform> {
    let engine = live_engine(name, two_zoned_windows());
    enable_workspaces(&engine);
    engine.with_platform(|p| p.focused = Some(WindowId(2)));
    event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
    settle(&engine).await;
    assert!(is_parked(&engine, 2));

    engine.with_platform(|p| {
        p.windows.get_mut(&WindowId(2)).unwrap().frame = RIGHT_ZONE;
        p.focused = Some(WindowId(1));
    });
    engine
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
fn arrow_shortcuts_ignore_inactive_workspace_windows() {
    scenario(|| async {
        let engine = inactive_window_in_the_right_zone("arrow-inactive").await;
        engine.with_platform(|p| p.focused = Some(WindowId(2)));
        let before = writes_to(&engine, 2);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("ArrowLeft"))),
        );
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), RIGHT_ZONE);
        assert_eq!(writes_to(&engine, 2), before);
    });
}

#[test]
fn shortcuts_do_nothing_on_a_display_whose_layout_is_disabled() {
    scenario(|| async {
        let engine = live_engine("arrow-disabled-layout", with_window(LEFT_ZONE));
        for layout in snappy_core::layout::presets() {
            command(
                &engine,
                Command::SetLayoutEnabled {
                    layout: layout.id.clone(),
                    enabled: false,
                },
            );
        }
        let status = engine.status();
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("ArrowRight"))),
        );
        settle(&engine).await;
        assert_eq!(writes_to(&engine, 1), 0);
        assert_eq!(engine.status(), status, "no message either");
    });
}

#[test]
fn only_valid_changes_are_saved() {
    scenario(|| async {
        let engine = live_engine("valid-saves", FakePlatform::default());
        let file = paths_kept("valid-saves").settings;
        std::fs::remove_file(&file).unwrap();

        command(&engine, Command::SetSnapping { enabled: true });
        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "halves".into(),
            },
        );
        assert!(!file.exists(), "nothing changed, so nothing is written");

        let before = engine.settings();
        command(
            &engine,
            Command::SetNavigationShortcut {
                direction: snappy_core::navigate::Direction::Left,
                chord: Some(snappy("Digit1")),
            },
        );
        assert!(engine.status().unwrap().is_error);
        assert_eq!(engine.settings(), before);
        assert!(!file.exists(), "a conflicting change is never written");

        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "thirds".into(),
            },
        );
        let saved = std::fs::read(&file).unwrap();
        let platform = snappy_core::keys::Platform::MacOs;
        assert_eq!(
            snappy_core::settings::Settings::from_json(&saved, platform).unwrap(),
            engine.settings()
        );
    });
}

#[test]
fn the_overlay_names_only_the_swap_keys_reversal_and_outlines_the_source_only_for_a_swap() {
    scenario(|| async {
        let engine = live_engine("swap-overlay", two_zoned_windows());
        engine.force_debug_log();
        stack_on_top(&engine);
        event(
            &engine,
            PlatformEvent::PointerDown {
                at: Point::new(212.0, 52.0),
                mods: shift(),
                window: None,
            },
        );
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
        event(
            &engine,
            PlatformEvent::PointerDown {
                at: Point::new(1000.0, 52.0),
                mods: shift(),
                window: Some(WindowId(2)),
            },
        );
        engine.with_platform(|p| p.windows.get_mut(&WindowId(2)).unwrap().frame = dragged);
        event(&engine, PlatformEvent::PointerDragged { at, mods: shift() });
        assert!(unmarked(&scene()), "swapping is the setting's own action");
        event(&engine, PlatformEvent::ModifiersChanged { at, mods: inverted() });
        let stacking = scene();
        assert_eq!(stacking.zones[1].caption, Some(Caption::Stack));
        assert!(stacking.zones.iter().all(|zone| !zone.swap_source));

        let log = paths_kept("swap-overlay").log_dir.join("debug.jsonl");
        let captions: Vec<serde_json::Value> = std::fs::read_to_string(log)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|record| record["event"] == "overlayCaption")
            .map(|record| record["caption"].clone())
            .collect();
        assert_eq!(captions, [serde_json::json!("Swap"), serde_json::json!("Stack")]);
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

        let drops = std::fs::read(paths_kept("swap-click-away").log_dir.join("drops.json")).unwrap();
        let drops: Vec<serde_json::Value> = serde_json::from_slice(&drops).unwrap();
        assert_eq!(drops.last().unwrap()["outcome"], "swapInterrupted");
    });
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
fn a_switch_refreshes_shortcuts_only_when_they_change_and_always_overlays() {
    scenario(|| async {
        let engine = live_engine("switch-refresh", FakePlatform::default());
        enable_workspaces(&engine);
        let registrations = || engine.with_platform(|p| p.registrations);
        let prepares = || {
            engine.with_platform(|p| {
                p.overlay
                    .iter()
                    .filter(|call| **call == OverlayCall::Prepare("main".into()))
                    .count()
            })
        };

        let (registered, prepared) = (registrations(), prepares());
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-2");
        assert_eq!(registrations(), registered, "same layouts, same shortcuts");
        assert!(prepares() > prepared);

        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "thirds".into(),
            },
        );
        let (registered, prepared) = (registrations(), prepares());
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-1");
        assert_eq!(
            registrations(),
            registered + 1,
            "halves needs other zone shortcuts"
        );
        assert!(prepares() > prepared);
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
fn a_relaunched_apps_windows_join_the_active_workspace() {
    scenario(|| async {
        let start = Rect::new(300.0, 200.0, 600.0, 400.0);
        let engine = live_engine("relaunched-app", with_window(start));
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        assert!(is_parked(&engine, 1));

        engine.with_platform(|p| {
            p.windows.remove(&WindowId(1));
            p.stacking.retain(|id| *id != WindowId(1));
        });
        event(&engine, PlatformEvent::AppTerminated { pid: 100 });
        engine.with_platform(|p| p.open(5, FakeWindow::new("com.example.editor", 101, start)));
        event(&engine, PlatformEvent::AppLaunched { pid: 101 });
        event(&engine, PlatformEvent::WindowCreated { pid: 101 });

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(
            is_parked(&engine, 5),
            "it belongs to workspace 1, not to the old window's workspace"
        );
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(5))), start);
    });
}

#[test]
fn zone_shortcuts_move_the_focused_window_not_the_frontmost() {
    scenario(|| async {
        let mut fake = with_window(Rect::new(300.0, 200.0, 600.0, 400.0));
        let other = Rect::new(400.0, 300.0, 600.0, 400.0);
        fake.open(2, FakeWindow::new("com.example.browser", 200, other));
        fake.focused = Some(WindowId(1));
        let engine = live_engine("focused-not-frontmost", fake);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("Digit2"))),
        );
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), other);
    });
}

#[test]
fn a_zone_shortcut_without_a_focused_window_says_so() {
    scenario(|| async {
        let mut fake = with_window(Rect::new(300.0, 200.0, 600.0, 400.0));
        fake.focused = None;
        let engine = live_engine("no-focused-window", fake);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("Digit2"))),
        );
        settle(&engine).await;
        let status = engine.status().unwrap();
        assert!(status.is_error);
        assert_eq!(status.message, "No movable window is focused.");
        assert_eq!(writes_to(&engine, 1), 0);
    });
}

/// What the settings app would see now.
fn app_state(engine: &Engine<FakePlatform>) -> AppState {
    engine.handle(Input::SettingsAppStarted { pid: 9999 });
    engine
        .take_outbox()
        .into_iter()
        .rev()
        .find_map(|message| match message {
            ToSettings::State(state) => Some(*state),
            _ => None,
        })
        .unwrap()
}

fn window_counts(engine: &Engine<FakePlatform>) -> BTreeMap<String, usize> {
    app_state(engine).workspaces.window_counts
}

fn frame_of(engine: &Engine<FakePlatform>, window: u64) -> Rect {
    engine.with_platform(|p| p.frame_of(WindowId(window)))
}

fn place(engine: &Engine<FakePlatform>, window: u64, frame: Rect) {
    engine.with_platform(|p| p.windows.get_mut(&WindowId(window)).unwrap().frame = frame);
}

/// The outcomes recorded in the drop diagnostics, oldest first.
fn drop_outcomes(name: &str) -> Vec<String> {
    let Ok(bytes) = std::fs::read(paths_kept(name).log_dir.join("drops.json")) else {
        return Vec::new();
    };
    let records: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    records
        .iter()
        .map(|record| record["outcome"].as_str().unwrap().to_owned())
        .collect()
}

fn press(engine: &Engine<FakePlatform>, at: Point, mods: Mods) {
    event(
        engine,
        PlatformEvent::PointerDown {
            at,
            mods,
            window: None,
        },
    );
}

/// A click above the zones, where no window is, which cancels the operation in flight
/// without claiming any window.
fn click_away(engine: &Engine<FakePlatform>) {
    let at = Point::new(720.0, 10.0);
    press(engine, at, Mods::NONE);
    event(engine, PlatformEvent::PointerUp { at, mods: Mods::NONE });
}

const FLOATING: Rect = Rect::new(300.0, 200.0, 600.0, 400.0);
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
fn workspaces_skip_full_screen_and_sheet_windows_and_never_move_minimized_ones() {
    scenario(|| async {
        let mut fake = FakePlatform::default();
        fake.open(1, FakeWindow::new("com.example.editor", 100, FLOATING));
        let mut player = FakeWindow::new("com.example.player", 200, FakePlatform::main_display().frame);
        player.state.full_screen = true;
        fake.open(2, player);
        let mut writer = FakeWindow::new("com.example.writer", 300, Rect::new(200.0, 150.0, 500.0, 400.0));
        writer.state.has_sheet = true;
        fake.open(3, writer);
        fake.open(
            4,
            FakeWindow::new("com.example.mail", 400, Rect::new(250.0, 250.0, 500.0, 400.0)),
        );
        let engine = live_engine("workspace-eligibility", fake);
        enable_workspaces(&engine);
        assert_eq!(
            window_counts(&engine)["workspace-1"],
            2,
            "the editor and the mail window"
        );

        engine.with_platform(|p| p.windows.get_mut(&WindowId(4)).unwrap().state.minimized = true);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(is_parked(&engine, 1));
        for window in [2, 3, 4] {
            assert_eq!(writes_to(&engine, window), 0, "window {window} is never moved");
        }
        assert_eq!(
            window_counts(&engine)["workspace-1"],
            2,
            "the minimized window stays"
        );

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), FLOATING);
        assert_eq!(writes_to(&engine, 4), 0);
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
            ("No zone right of this window.", false, false)
        );

        place(&engine, 1, LEFT_ZONE);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("ArrowLeft"))),
        );
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE, "never wraps around");
        assert_eq!(status_text(&engine), "No zone left of this window.");
    });
}

#[test]
fn live_control_needs_both_the_saved_choice_and_permission() {
    let name = "live-control";
    scenario(|| async {
        let engine = started(name, FakePlatform::default());
        assert!(engine.settings().snapping.enabled, "the choice starts on");
        assert!(engine.is_live());
        command(&engine, Command::SetSnapping { enabled: false });
        assert!(!engine.is_live());
    });

    scenario(|| async {
        let engine = Engine::new(FakePlatform::default(), paths_kept(name));
        engine.start();
        assert!(!engine.settings().snapping.enabled, "the choice is saved");
        assert!(!engine.is_live(), "permission alone doesn't start live control");

        command(&engine, Command::SetSnapping { enabled: true });
        assert!(engine.is_live());
        engine.with_platform(|p| p.granted = false);
        event(&engine, PlatformEvent::PermissionChanged);
        assert!(!engine.is_live());
        assert!(
            engine.settings().snapping.enabled,
            "losing permission keeps the choice"
        );
    });
}

#[test]
fn background_permission_checks_never_open_system_settings() {
    let name = "background-checks";
    scenario(|| async {
        drop(started(name, FakePlatform::default()));
        let engine = Engine::new(without_permission(), paths_kept(name));
        engine.start();
        for granted in [false, false, true, false, false] {
            engine.with_platform(|p| p.granted = granted);
            event(&engine, PlatformEvent::PermissionChanged);
        }
        assert_eq!(
            engine.with_platform(|p| (p.access_requests, p.settings_page_opens)),
            (0, 0)
        );

        command(&engine, Command::OpenPermissionSettings);
        assert_eq!(
            engine.with_platform(|p| p.settings_page_opens),
            1,
            "only when asked"
        );
    });
}

#[test]
fn without_permission_no_global_shortcut_is_registered() {
    let name = "no-permission-shortcuts";
    let switch_one = KeyChord::new("Digit1", [Modifier::Ctrl]).combo();
    scenario(|| async {
        let engine = live_engine(name, FakePlatform::default());
        enable_workspaces(&engine);
        command(&engine, Command::SetSnapping { enabled: false });
        assert!(
            engine.with_platform(|p| p.registered.iter().any(|(_, c)| *c == switch_one)),
            "workspace shortcuts outlive snapping"
        );

        engine.with_platform(|p| p.granted = false);
        event(&engine, PlatformEvent::PermissionChanged);
        assert!(engine.with_platform(|p| p.registered.is_empty()));
    });

    scenario(|| async {
        let engine = Engine::new(without_permission(), paths_kept(name));
        engine.start();
        assert!(engine.settings().workspaces.enabled);
        assert!(engine.with_platform(|p| p.registered.is_empty()));
    });
}

#[test]
fn turning_workspaces_on_without_permission_asks_for_accessibility() {
    scenario(|| async {
        let engine = started("workspaces-asks", without_permission());
        let asked = engine.with_platform(|p| p.access_requests);
        enable_workspaces(&engine);
        assert!(engine.settings().workspaces.enabled, "the choice is saved");
        assert_eq!(engine.with_platform(|p| p.access_requests), asked + 1);
        assert_eq!(engine.with_platform(|p| p.settings_page_opens), 0);
        assert!(!app_state(&engine).workspaces.running);
        assert_eq!(
            status_text(&engine),
            "Allow Snappy in Accessibility. Workspaces will start automatically."
        );

        engine.with_platform(|p| p.granted = true);
        event(&engine, PlatformEvent::PermissionChanged);
        assert!(app_state(&engine).workspaces.running);
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

#[test]
fn switch_timings_are_written_at_quit_and_only_when_there_are_some() {
    scenario(|| async {
        let engine = live_engine("timings-none", with_window(FLOATING));
        enable_workspaces(&engine);
        engine.shutdown();
        let unused = paths_kept("timings-none").log("workspace-performance.json");
        assert!(!unused.exists());

        let engine = live_engine("timings", with_window(FLOATING));
        enable_workspaces(&engine);
        for number in [2, 3, 1] {
            event(&engine, PlatformEvent::HotKey(switch_to(&engine, number)));
            settle(&engine).await;
        }
        let file = paths_kept("timings").log("workspace-performance.json");
        assert!(!file.exists(), "timings stay in memory until quit");
        assert_eq!(engine.switch_timings().len(), 3);

        engine.shutdown();
        let timings: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(timings.len(), 3);
        let mut fields: Vec<&str> = timings[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "discoveryMs",
                "membershipMs",
                "queueMs",
                "refreshMs",
                "settingsMs",
                "succeeded",
                "totalMs",
                "windowMs",
                "windows",
            ]
        );
        assert!(
            timings
                .iter()
                .all(|timing| timing["succeeded"] == true && timing["windows"] == 1)
        );
    });
}

#[test]
fn a_login_item_error_keeps_the_switch_off_and_shows_the_message() {
    scenario(|| async {
        let engine = live_engine("login-item-error", FakePlatform::default());
        let refusal = "The login item could not be registered.";
        engine.with_platform(|p| p.login_item_error = Some(refusal.into()));
        engine.take_outbox();

        command(&engine, Command::SetLoginItem { enabled: true });
        assert!(engine.take_outbox().contains(&ToSettings::Reply {
            id: 1,
            error: Some(refusal.into()),
        }));
        let status = engine.status().unwrap();
        assert_eq!((status.message.as_str(), status.is_error), (refusal, true));
        assert!(!app_state(&engine).login_item_enabled, "the switch reverts");
    });
}

#[test]
fn import_returns_parked_windows_turns_workspaces_off_and_keeps_snapping() {
    scenario(|| async {
        let name = "import";
        let engine = live_engine(name, with_window(FLOATING));
        let folder = paths_kept(name).settings.parent().unwrap().to_owned();
        let exported = folder.join("exported.json").display().to_string();
        let broken = folder.join("broken.json");
        std::fs::write(&broken, b"{ broken").unwrap();

        enable_workspaces(&engine);
        command(
            &engine,
            Command::ExportSettings {
                path: exported.clone(),
            },
        );
        command(&engine, Command::SetSnapping { enabled: false });
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        assert!(is_parked(&engine, 1));

        let before = engine.settings();
        command(
            &engine,
            Command::ImportSettings {
                path: broken.display().to_string(),
            },
        );
        let status = engine.status().unwrap();
        assert!(status.is_error && status.message.starts_with("Import failed: "));
        assert_eq!(engine.settings(), before, "a failed import changes nothing");
        assert!(is_parked(&engine, 1));

        frozen(&engine, 1, true);
        command(
            &engine,
            Command::ImportSettings {
                path: exported.clone(),
            },
        );
        assert_eq!(
            engine.settings(),
            before,
            "windows that can't return stop the import"
        );
        assert!(is_parked(&engine, 1));

        frozen(&engine, 1, false);
        command(&engine, Command::ImportSettings { path: exported });
        assert_eq!(frame_of(&engine, 1), FLOATING, "parked windows return first");
        let settings = engine.settings();
        assert!(!settings.workspaces.enabled);
        assert!(!settings.snapping.enabled, "importing never turns snapping on");
        assert_eq!(status_text(&engine), "Layouts and preferences imported.");
    });
}

#[test]
fn every_save_path_refuses_a_conflict_with_its_message() {
    scenario(|| async {
        let name = "conflicts";
        let engine = live_engine(name, FakePlatform::default());
        enable_workspaces(&engine);
        let refused = |command_to_send: Command, conflict: &str| {
            let before = engine.settings();
            command(&engine, command_to_send);
            let status = engine.status().unwrap();
            assert!(status.is_error, "{}", status.message);
            assert!(
                status.message.ends_with(conflict),
                "{} should end with {conflict}",
                status.message
            );
            assert_eq!(engine.settings(), before);
        };

        refused(
            Command::SetNavigationShortcut {
                direction: snappy_core::navigate::Direction::Left,
                chord: Some(KeyChord::new("Digit1", [Modifier::Ctrl])),
            },
            "Control + 1 is already assigned to Move window left.",
        );
        refused(
            Command::SetWorkspaceStepShortcut {
                step: snappy_core::settings::WorkspaceStep::Previous,
                chord: Some(snappy("ArrowRight")),
            },
            "Snappy + → is already assigned to Move window right.",
        );
        refused(
            Command::SetWorkspaceStepShortcut {
                step: snappy_core::settings::WorkspaceStep::SendNext,
                chord: Some(snappy("Semicolon")),
            },
            "Snappy + ; is already assigned to Previous workspace.",
        );
        refused(
            Command::SetFocusShortcut {
                shortcut: FocusShortcut::SelectWindow,
                chord: Some(snappy("Space")),
            },
            "Snappy + Space is already assigned to Move to the nearest empty zone.",
        );
        refused(
            Command::SetHistoryShortcut {
                step: HistoryStep::Undo,
                chord: Some(snappy("KeyK")),
            },
            "Snappy + K is already assigned to Focus down.",
        );
        refused(
            Command::SetHistoryShortcut {
                step: HistoryStep::Redo,
                chord: Some(snappy("ArrowDown")),
            },
            "Snappy + ↓ is already assigned to Move window down.",
        );
        let mut first = engine.settings().workspaces.items[0].clone();
        first.switch_shortcut = Some(snappy("ArrowUp"));
        refused(
            Command::UpdateWorkspace { workspace: first },
            "Snappy + ↑ is already assigned to Move window up.",
        );
        let halves = snappy_core::layout::preset("halves").unwrap().clone();
        let mut options = engine.settings().layouts.options["halves"].clone();
        options.select_shortcut = Some(snappy("ArrowDown"));
        refused(
            Command::SaveLayout {
                layout: halves,
                options,
                display: "main".into(),
            },
            "Snappy + ↓ is already assigned to Move window down.",
        );

        let file = paths_kept(name).settings.with_file_name("conflicting.json");
        let mut conflicting = serde_json::to_value(engine.settings()).unwrap();
        conflicting["navigation"]["left"] =
            serde_json::to_value(KeyChord::new("Digit1", [Modifier::Ctrl])).unwrap();
        std::fs::write(&file, serde_json::to_vec(&conflicting).unwrap()).unwrap();
        refused(
            Command::ImportSettings {
                path: file.display().to_string(),
            },
            "Control + 1 is already assigned to Move window left.",
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
                "An app could not restore its window. Undo is available to retry.",
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

#[test]
fn the_first_start_copies_each_displays_layout_into_every_workspace() {
    scenario(|| async {
        let mut fake = FakePlatform::default();
        fake.displays.push(side_display());
        let engine = live_engine("workspace-layout-seed", fake);
        for (display, layout) in [("main", "rows"), ("projector", "thirds")] {
            command(
                &engine,
                Command::ApplyLayout {
                    display: display.into(),
                    layout: layout.into(),
                },
            );
        }
        assert_eq!(engine.settings().layouts.default_layout_id, "thirds");

        enable_workspaces(&engine);
        let seeded: BTreeMap<String, String> =
            [("main", "rows"), ("projector", "thirds"), ("side", "thirds")]
                .into_iter()
                .map(|(display, layout)| (display.to_owned(), layout.to_owned()))
                .collect();
        for item in &engine.settings().workspaces.items {
            assert_eq!(item.displays, seeded, "{}", item.id);
        }

        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "six".into(),
            },
        );
        command(&engine, Command::SetWorkspacesEnabled { enabled: false });
        enable_workspaces(&engine);
        let items = engine.settings().workspaces.items;
        assert_eq!(
            items[0].displays["main"], "six",
            "later starts keep each workspace's layouts"
        );
        assert_eq!(items[1].displays["main"], "rows");
    });
}

#[test]
fn turning_workspaces_on_enrolls_visible_windows_and_remembered_minimized_ones() {
    scenario(|| async {
        let engine = live_engine("enroll", two_zoned_windows());
        enable_workspaces(&engine);
        assert_eq!(window_counts(&engine)["workspace-1"], 2);
        engine.with_platform(|p| p.focused = Some(WindowId(2)));
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        command(&engine, Command::SetWorkspacesEnabled { enabled: false });
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);

        engine.with_platform(|p| {
            p.windows.get_mut(&WindowId(2)).unwrap().state.minimized = true;
            let mut forgotten = FakeWindow::new("com.example.notes", 300, FLOATING);
            forgotten.state.minimized = true;
            p.open(3, forgotten);
            p.focused = Some(WindowId(1));
        });
        let written = writes_to(&engine, 2);
        enable_workspaces(&engine);
        let counts = window_counts(&engine);
        assert_eq!(
            (
                counts["workspace-1"],
                counts["workspace-2"],
                counts["workspace-3"]
            ),
            (1, 1, 0),
            "the forgotten minimized window stays out"
        );
        assert_eq!(writes_to(&engine, 2), written, "minimized windows never move");
    });
}

#[test]
fn windows_join_the_active_workspace_from_events_never_by_polling() {
    scenario(|| async {
        let engine = live_engine("join-by-events", with_window(FLOATING));
        enable_workspaces(&engine);
        engine.with_platform(|p| {
            p.open(
                5,
                FakeWindow::new("com.example.notes", 500, Rect::new(100.0, 100.0, 400.0, 300.0)),
            )
        });
        tokio::time::sleep(Duration::from_secs(30)).await;
        assert_eq!(window_counts(&engine)["workspace-1"], 1, "nothing polls");

        event(&engine, PlatformEvent::WindowCreated { pid: 500 });
        assert_eq!(window_counts(&engine)["workspace-1"], 2);

        engine.with_platform(|p| {
            p.open(
                6,
                FakeWindow::new("com.example.music", 600, Rect::new(150.0, 150.0, 400.0, 300.0)),
            )
        });
        event(
            &engine,
            PlatformEvent::FocusChanged {
                pid: 600,
                by_activation: true,
            },
        );
        assert_eq!(window_counts(&engine)["workspace-1"], 3);

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!([1, 5, 6].iter().all(|window| is_parked(&engine, *window)));
    });
}

#[test]
fn a_switch_saves_only_the_active_workspace_and_reverts_when_that_fails() {
    scenario(|| async {
        let name = "switch-save";
        let engine = live_engine(name, two_zoned_windows());
        enable_workspaces(&engine);
        engine.with_platform(|p| p.focused = Some(WindowId(2)));
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;

        let file = paths_kept(name).settings;
        let saved = || serde_json::from_slice::<serde_json::Value>(&std::fs::read(&file).unwrap()).unwrap();
        let mut expected = saved();
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        expected["workspaces"]["activeId"] = "workspace-2".into();
        assert_eq!(saved(), expected);
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);

        // A directory where the save's temporary file goes makes the save fail.
        let blocker = file.with_file_name(".settings.json.tmp");
        std::fs::create_dir(&blocker).unwrap();
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 3)));
        settle(&engine).await;
        std::fs::remove_dir(&blocker).unwrap();
        assert_eq!(engine.settings().workspaces.active_id, "workspace-2");
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE, "switched back");
        assert!(is_parked(&engine, 1));
        let status = engine.status().unwrap();
        assert!(
            status.is_error
                && status
                    .message
                    .starts_with("Could not save the workspace selection")
        );
    });
}

#[test]
fn while_a_switch_is_pending_next_counts_from_the_requested_workspace() {
    scenario(|| async {
        let engine = live_engine("pending-switch", with_window(FLOATING));
        enable_workspaces(&engine);
        let next = hotkey_id(&engine, &snappy("Quote"));
        event(&engine, PlatformEvent::HotKey(next));
        event(&engine, PlatformEvent::HotKey(next));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-3");
        assert!(is_parked(&engine, 1));
    });
}

#[test]
fn switches_to_the_active_workspace_and_while_paused_do_nothing() {
    scenario(|| async {
        let engine = live_engine("switch-noop", with_window(FLOATING));
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(writes_to(&engine, 1), 0);
        assert!(engine.switch_timings().is_empty());

        engine.with_platform(|p| p.on_managed_desktop = false);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-1");
        assert_eq!(writes_to(&engine, 1), 0);
        assert_eq!(window_counts(&engine)["workspace-1"], 1);
    });
}

#[test]
fn move_here_and_switch_sends_the_window_then_follows_it() {
    scenario(|| async {
        let engine = live_engine("send-and-switch", two_zoned_windows());
        enable_workspaces(&engine);
        let chord = KeyChord::new("Digit2", [Modifier::Ctrl, Modifier::Alt]);
        event(&engine, PlatformEvent::HotKey(hotkey_id(&engine, &chord)));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-2");
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE, "the sent window shows");
        assert!(is_parked(&engine, 2));
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
    });
}

/// A live engine on the halves layout with workspaces on and every shortcut at its default.
fn default_workspaces(name: &str, fake: FakePlatform) -> Engine<FakePlatform> {
    let engine = started(name, fake);
    command(
        &engine,
        Command::ApplyLayout {
            display: "main".into(),
            layout: "halves".into(),
        },
    );
    command(&engine, Command::SetSnapping { enabled: true });
    command(&engine, Command::SetWorkspacesEnabled { enabled: true });
    assert!(app_state(&engine).workspaces.running);
    engine
}

async fn press_default(engine: &Engine<FakePlatform>, chord: &str) {
    event(engine, PlatformEvent::HotKey(hotkey_id(engine, &snappy(chord))));
    settle(engine).await;
}

/// Focus a window the way the platform reports it.
fn focus(engine: &Engine<FakePlatform>, window: u64) {
    let pid = engine.with_platform(|p| {
        p.focused = Some(WindowId(window));
        p.windows[&WindowId(window)].pid
    });
    event(
        engine,
        PlatformEvent::FocusChanged {
            pid,
            by_activation: false,
        },
    );
}

fn focused(engine: &Engine<FakePlatform>) -> Option<WindowId> {
    engine.with_platform(|p| p.focused)
}

/// The frames the main display outlines, in either style.
fn marks(engine: &Engine<FakePlatform>) -> Vec<Rect> {
    marked(engine).into_iter().map(|(frame, _)| frame).collect()
}

fn marked(engine: &Engine<FakePlatform>) -> Vec<(Rect, MarkStyle)> {
    engine.with_platform(|p| {
        p.marks
            .get("main")
            .map(|windows| windows.iter().map(|mark| (mark.frame, mark.style)).collect())
            .unwrap_or_default()
    })
}

fn active(engine: &Engine<FakePlatform>) -> String {
    engine.settings().workspaces.active_id
}

/// Four windows of one app, side by side.
fn four_windows() -> FakePlatform {
    let mut fake = FakePlatform::default();
    for id in 1..=4 {
        let frame = Rect::new(40.0 + 300.0 * (id as f64 - 1.0), 100.0, 280.0, 400.0);
        fake.open(id, FakeWindow::new("com.example.editor", 100, frame));
    }
    fake
}

#[test]
fn the_snappy_key_and_a_digit_switch_to_that_workspace() {
    scenario(|| async {
        let engine = default_workspaces("switch-by-number", two_zoned_windows());
        press_default(&engine, "Digit3").await;
        assert_eq!(active(&engine), "workspace-3");
        assert!(is_parked(&engine, 1) && is_parked(&engine, 2));

        press_default(&engine, "Digit1").await;
        assert_eq!(active(&engine), "workspace-1");
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (LEFT_ZONE, RIGHT_ZONE)
        );
    });
}

#[test]
fn sending_to_the_next_workspace_three_times_sends_three_windows_and_stays() {
    scenario(|| async {
        let engine = default_workspaces("send-and-stay", four_windows());
        for window in 1..=4 {
            focus(&engine, window);
        }

        for expected_focus in [3, 2, 1] {
            press_default(&engine, "Slash").await;
            assert_eq!(active(&engine), "workspace-1", "the send stays");
            assert_eq!(focused(&engine), Some(WindowId(expected_focus)));
        }
        assert!([2, 3, 4].into_iter().all(|window| is_parked(&engine, window)));
        assert!(!is_parked(&engine, 1));
        assert_eq!(window_counts(&engine)["workspace-2"], 3);
        assert_eq!(window_counts(&engine)["workspace-1"], 1);

        press_default(&engine, "Digit2").await;
        assert!([2, 3, 4].into_iter().all(|window| !is_parked(&engine, window)));
    });
}

#[test]
fn sending_to_the_previous_workspace_wraps_to_the_last_and_stays() {
    scenario(|| async {
        let engine = default_workspaces("send-previous", two_zoned_windows());
        press_default(&engine, "Period").await;
        assert_eq!(active(&engine), "workspace-1");
        assert!(is_parked(&engine, 1));
        assert_eq!(window_counts(&engine)["workspace-3"], 1);
        assert_eq!(focused(&engine), Some(WindowId(2)));
    });
}

#[test]
fn sending_the_last_window_away_leaves_focus_alone() {
    scenario(|| async {
        let engine = default_workspaces("send-last", with_window(Rect::new(300.0, 200.0, 600.0, 400.0)));
        press_default(&engine, "Slash").await;
        assert!(is_parked(&engine, 1));
        assert_eq!(active(&engine), "workspace-1");
        assert_eq!(focused(&engine), Some(WindowId(1)), "nothing else was focused");
    });
}

#[test]
fn a_selected_window_goes_along_to_the_workspace_switched_to_by_number() {
    scenario(|| async {
        let engine = default_workspaces("take-by-number", two_zoned_windows());
        press_default(&engine, "Enter").await;
        assert_eq!(
            marked(&engine),
            [(LEFT_ZONE, MarkStyle::Selected)],
            "the selected window is outlined"
        );

        press_default(&engine, "Digit3").await;
        assert_eq!(active(&engine), "workspace-3");
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE, "it keeps its frame");
        assert!(is_parked(&engine, 2));
        assert_eq!(focused(&engine), Some(WindowId(1)));
        assert_eq!(window_counts(&engine)["workspace-3"], 1);
        assert!(marks(&engine).is_empty(), "arrived windows lose their outline");

        press_default(&engine, "Digit1").await;
        assert!(is_parked(&engine, 1), "the switch emptied the selection");
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
    });
}

#[test]
fn two_selected_windows_go_along_to_the_next_workspace_and_the_last_selected_gets_focus() {
    scenario(|| async {
        let engine = default_workspaces("take-two-by-step", four_windows());
        let frames: Vec<Rect> = (1..=4).map(|window| frame_of(&engine, window)).collect();
        focus(&engine, 3);
        press_default(&engine, "Enter").await;
        focus(&engine, 1);
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [frames[2], frames[0]]);

        press_default(&engine, "Quote").await;
        assert_eq!(active(&engine), "workspace-2");
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 3)),
            (frames[0], frames[2])
        );
        assert!(is_parked(&engine, 2) && is_parked(&engine, 4));
        assert_eq!(focused(&engine), Some(WindowId(1)));
        assert_eq!(window_counts(&engine)["workspace-2"], 2);
        assert!(marks(&engine).is_empty());
    });
}

#[test]
fn selecting_a_selected_window_again_unselects_it() {
    scenario(|| async {
        let engine = default_workspaces("put-down", two_zoned_windows());
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE]);
        press_default(&engine, "Enter").await;
        assert!(marks(&engine).is_empty());

        press_default(&engine, "Digit2").await;
        assert!(is_parked(&engine, 1) && is_parked(&engine, 2));
        assert_eq!(window_counts(&engine)["workspace-1"], 2);
    });
}

#[test]
fn a_selected_window_that_closes_leaves_the_selection() {
    scenario(|| async {
        let engine = default_workspaces("carried-closed", two_zoned_windows());
        press_default(&engine, "Enter").await;
        focus(&engine, 2);
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE, RIGHT_ZONE]);

        engine.with_platform(|p| {
            p.windows.remove(&WindowId(1));
            p.focused = Some(WindowId(2));
        });
        event(&engine, PlatformEvent::WindowDestroyed(WindowId(1)));
        assert_eq!(marks(&engine), [RIGHT_ZONE]);

        press_default(&engine, "Digit2").await;
        assert_eq!(active(&engine), "workspace-2");
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
    });
}

#[test]
fn a_selected_windows_outline_follows_it_and_survives_a_switch_to_the_same_workspace() {
    scenario(|| async {
        let engine = default_workspaces("carried-moves", two_zoned_windows());
        press_default(&engine, "Enter").await;
        let moved = Rect::new(100.0, 120.0, 500.0, 400.0);
        place(&engine, 1, moved);
        event(&engine, PlatformEvent::WindowMoved(WindowId(1)));
        assert_eq!(marks(&engine), [moved]);

        press_default(&engine, "Digit1").await;
        assert_eq!(marks(&engine), [moved], "already there: still selected");

        event(
            &engine,
            PlatformEvent::Tray(snappy_platform::TrayCommand::SwitchWorkspace(
                "workspace-2".into(),
            )),
        );
        settle(&engine).await;
        assert_eq!(active(&engine), "workspace-2");
        assert_eq!(
            frame_of(&engine, 1),
            moved,
            "the menu's switch takes it along too"
        );
        assert!(marks(&engine).is_empty());
    });
}

#[test]
fn quitting_forgets_the_selection_and_moves_none_of_it() {
    scenario(|| async {
        let engine = default_workspaces("carried-quit", two_zoned_windows());
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE]);
        engine.shutdown();
        assert!(marks(&engine).is_empty());
        assert_eq!(writes_to(&engine, 1), 0);
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
    });
}

#[test]
fn selecting_works_with_workspaces_off_and_backslash_clears_the_selection() {
    scenario(|| async {
        let engine = live_engine("select-without-workspaces", two_zoned_windows());
        assert!(!engine.settings().workspaces.enabled);
        press_default(&engine, "Enter").await;
        focus(&engine, 2);
        press_default(&engine, "Enter").await;
        assert_eq!(
            marked(&engine),
            [
                (LEFT_ZONE, MarkStyle::Selected),
                (RIGHT_ZONE, MarkStyle::Selected)
            ]
        );

        press_default(&engine, "Backslash").await;
        assert!(marks(&engine).is_empty());
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (LEFT_ZONE, RIGHT_ZONE),
            "clearing moves nothing"
        );
    });
}

#[test]
fn a_macos_file_clearing_with_delete_loads_clearing_with_backslash_and_delete_is_refused() {
    scenario(|| async {
        let name = "clear-with-delete";
        let paths = paths(name);
        let mut written =
            serde_json::to_value(Settings::defaults(snappy_core::keys::Platform::MacOs)).unwrap();
        written["navigation"]["clearSelection"] = serde_json::to_value(snappy("Backspace")).unwrap();
        std::fs::create_dir_all(paths.settings.parent().unwrap()).unwrap();
        std::fs::write(&paths.settings, serde_json::to_vec(&written).unwrap()).unwrap();

        let engine = Engine::new(two_zoned_windows(), paths);
        engine.start();
        command(&engine, Command::SetSnapping { enabled: true });
        assert!(engine.status().is_none_or(|status| !status.is_error));
        assert_eq!(
            engine
                .settings()
                .navigation
                .clear_selection
                .map(|chord| chord.combo()),
            Some(snappy("Backslash").combo())
        );
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE]);
        press_default(&engine, "Backslash").await;
        assert!(marks(&engine).is_empty());

        command(
            &engine,
            Command::SetFocusShortcut {
                shortcut: FocusShortcut::ClearSelection,
                chord: Some(snappy("Backspace")),
            },
        );
        let status = engine.status().unwrap();
        assert!(status.is_error);
        assert!(
            status
                .message
                .ends_with("macOS reserves Control + Option + Command + Delete; it never reaches apps."),
            "{}",
            status.message
        );
        assert_eq!(
            engine
                .settings()
                .navigation
                .clear_selection
                .map(|chord| chord.combo()),
            Some(snappy("Backslash").combo())
        );
    });
}

#[test]
fn losing_live_control_clears_the_selection_and_its_shortcuts() {
    scenario(|| async {
        let engine = live_engine("select-live-off", two_zoned_windows());
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE]);
        command(&engine, Command::SetSnapping { enabled: false });
        assert!(marks(&engine).is_empty());
        assert!(
            !engine.with_platform(|p| p
                .registered
                .iter()
                .any(|(_, combo)| *combo == snappy("Enter").combo())),
            "with live control off, Snappy + Return stays the app's"
        );
    });
}

#[test]
fn changing_desktops_clears_the_selection() {
    scenario(|| async {
        let engine = default_workspaces("carried-paused", two_zoned_windows());
        press_default(&engine, "Enter").await;
        engine.with_platform(|p| p.on_managed_desktop = false);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        assert!(marks(&engine).is_empty());
        engine.with_platform(|p| p.on_managed_desktop = true);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        press_default(&engine, "Digit2").await;
        assert!(is_parked(&engine, 1));
    });
}

#[test]
fn sending_one_selected_window_away_empties_the_selection() {
    scenario(|| async {
        let engine = default_workspaces("carried-sent", two_zoned_windows());
        press_default(&engine, "Enter").await;
        press_default(&engine, "Slash").await;
        assert!(is_parked(&engine, 1));
        assert!(marks(&engine).is_empty());
        assert_eq!(focused(&engine), Some(WindowId(2)));
        press_default(&engine, "Digit3").await;
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
        assert_eq!(window_counts(&engine)["workspace-3"], 0);
    });
}

#[test]
fn send_and_stay_sends_the_whole_selection_and_focuses_what_is_left() {
    scenario(|| async {
        let engine = default_workspaces("send-selection", four_windows());
        for window in [2, 4] {
            focus(&engine, window);
            press_default(&engine, "Enter").await;
        }
        focus(&engine, 3);
        focus(&engine, 1);

        press_default(&engine, "Slash").await;
        assert_eq!(active(&engine), "workspace-1", "the send stays");
        assert!(is_parked(&engine, 2) && is_parked(&engine, 4));
        assert!(!is_parked(&engine, 1) && !is_parked(&engine, 3));
        assert_eq!(window_counts(&engine)["workspace-2"], 2);
        assert!(marks(&engine).is_empty(), "sending empties the selection");
        assert_eq!(focused(&engine), Some(WindowId(1)));

        press_default(&engine, "Slash").await;
        assert!(
            is_parked(&engine, 1),
            "without a selection the focused window goes"
        );
        assert!(!is_parked(&engine, 3));
    });
}

#[test]
fn the_debug_log_records_the_selection_and_sending() {
    scenario(|| async {
        let name = "selection-log";
        let engine = default_workspaces(name, two_zoned_windows());
        engine.force_debug_log();
        press_default(&engine, "Enter").await;
        press_default(&engine, "Enter").await;
        press_default(&engine, "Enter").await;
        press_default(&engine, "Digit2").await;
        press_default(&engine, "Period").await;
        press_default(&engine, "Enter").await;
        press_default(&engine, "Backslash").await;
        press_default(&engine, "KeyL").await;
        let log = std::fs::read_to_string(paths_kept(name).log_dir.join("debug.jsonl")).unwrap();
        for event in [
            "selectionAdded",
            "selectionRemoved",
            "selectionCarried",
            "selectionCleared",
            "workspaceSend",
            "focusMove",
        ] {
            assert!(
                log.contains(&format!("\"event\":\"{event}\"")),
                "{event} in {log}"
            );
        }
    });
}

#[test]
fn send_moves_the_focused_window_only_from_the_active_workspace() {
    scenario(|| async {
        let (mut fake, settings_frame) = with_settings_window();
        fake.open(1, FakeWindow::new("com.example.editor", 100, FLOATING));
        fake.focused = Some(WindowId(9));
        let engine = live_engine("send-focused", fake);
        engine.handle(Input::SettingsAppStarted { pid: 4242 });
        enable_workspaces(&engine);

        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        assert!(
            is_parked(&engine, 9),
            "the settings window is sent like any other"
        );
        assert!(!is_parked(&engine, 1));
        assert_eq!(
            engine.with_platform(|p| p.focused),
            Some(WindowId(1)),
            "focus stays on this workspace"
        );

        place(&engine, 9, settings_frame);
        engine.with_platform(|p| p.focused = Some(WindowId(9)));
        let written = writes_to(&engine, 9);
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 3)));
        settle(&engine).await;
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error),
            ("Focus a window on the current workspace first.", true)
        );
        assert_eq!(writes_to(&engine, 9), written);
    });
}

#[test]
fn another_desktop_pauses_workspaces_without_changing_them() {
    scenario(|| async {
        let engine = live_engine("other-desktop", two_zoned_windows());
        enable_workspaces(&engine);
        engine.with_platform(|p| p.focused = Some(WindowId(2)));
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        let written = writes_to(&engine, 2);

        engine.with_platform(|p| p.on_managed_desktop = false);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        let state = app_state(&engine);
        assert!(!state.workspaces.running);
        assert_eq!(
            state.workspaces.pause_message.as_deref(),
            Some("Paused on another macOS desktop. Return to the original desktop to resume.")
        );
        assert!(state.settings.workspaces.enabled);
        assert_eq!(state.workspaces.window_counts["workspace-2"], 1);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-1");
        assert_eq!(writes_to(&engine, 2), written);
        assert!(is_parked(&engine, 2));

        engine.with_platform(|p| p.on_managed_desktop = true);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        let state = app_state(&engine);
        assert!(state.workspaces.running);
        assert_eq!(state.workspaces.pause_message, None);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
    });
}

#[test]
fn membership_is_keyed_by_process_and_window_and_dropped_when_the_app_quits() {
    scenario(|| async {
        let name = "membership";
        let engine = live_engine(name, two_zoned_windows());
        enable_workspaces(&engine);
        engine.with_platform(|p| p.focused = Some(WindowId(2)));
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;

        let file = paths_kept(name).membership();
        let entries =
            || serde_json::from_slice::<Vec<serde_json::Value>>(&std::fs::read(&file).unwrap()).unwrap();
        let pids = || -> Vec<u64> {
            entries()
                .iter()
                .map(|entry| entry["key"]["pid"].as_u64().unwrap())
                .collect()
        };
        let keys = |value: &serde_json::Value| -> Vec<String> {
            let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
            keys.sort();
            keys
        };
        assert_eq!(entries().len(), 2);
        for entry in entries() {
            assert_eq!(keys(&entry), ["frame", "key", "parkedOrigin", "workspaceId"]);
            assert_eq!(keys(&entry["key"]), ["launchTime", "pid", "window"]);
        }

        engine.with_platform(|p| p.windows.get_mut(&WindowId(1)).unwrap().state.minimized = true);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(pids().contains(&100), "a minimized window keeps its entry");

        engine.with_platform(|p| {
            p.windows.remove(&WindowId(2));
            p.stacking.retain(|id| *id != WindowId(2));
        });
        event(&engine, PlatformEvent::AppTerminated { pid: 200 });
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(pids(), [100]);
    });
}

/// Window 1 sent to workspace 2, then the daemon stops without quitting. Returns the
/// platform as the next launch finds it.
async fn crashed_with_window_one_parked(name: &str) -> FakePlatform {
    let engine = live_engine(name, two_zoned_windows());
    enable_workspaces(&engine);
    event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
    settle(&engine).await;
    assert!(is_parked(&engine, 1));
    let mut platform = engine.with_platform(|p| p.clone());
    platform.writes.clear();
    platform
}

fn relaunched(name: &str, platform: FakePlatform) -> Engine<FakePlatform> {
    let engine = Engine::new(platform, paths_kept(name));
    engine.start();
    engine
}

/// Like `relaunched`, with the debug log on from the start.
fn relaunched_logging(name: &str, platform: FakePlatform) -> Engine<FakePlatform> {
    let engine = Engine::new(platform, paths_kept(name));
    engine.force_debug_log();
    engine.start();
    engine
}

/// The debug log's records of `event` so far.
fn logged(name: &str, event: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(paths_kept(name).log_dir.join("debug.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|record| record["event"] == event)
        .collect()
}

/// Every write and bounds-change bracket to any window so far.
fn all_writes(engine: &Engine<FakePlatform>) -> Vec<(WindowId, &'static str)> {
    engine.with_platform(|p| p.writes.clone())
}

/// Stop the way installing a new build does. Returns the platform as the next launch finds
/// it, with its writes cleared.
fn stopped_for_restart(engine: &Engine<FakePlatform>) -> FakePlatform {
    engine.with_platform(|p| p.writes.clear());
    engine.handle(Input::Platform(PlatformEvent::RestartRequested));
    assert_eq!(all_writes(engine), [], "stopping moves nothing");
    let mut platform = engine.with_platform(|p| p.clone());
    platform.writes.clear();
    platform
}

#[test]
fn a_restart_keeps_every_window_in_its_workspace_and_moves_nothing() {
    scenario(|| async {
        let name = "restart-keeps";
        let engine = live_engine(name, two_zoned_windows());
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 3)));
        settle(&engine).await;
        assert!(is_parked(&engine, 1) && is_parked(&engine, 2));
        let parked = (frame_of(&engine, 1), frame_of(&engine, 2));
        let platform = stopped_for_restart(&engine);
        assert!(paths_kept(name).journal().exists(), "the journal stays");

        let engine = relaunched_logging(name, platform);
        assert!(app_state(&engine).workspaces.running);
        assert_eq!(all_writes(&engine), [], "the launch moves nothing");
        assert_eq!((frame_of(&engine, 1), frame_of(&engine, 2)), parked);
        assert_eq!(active(&engine), "workspace-3");
        let counts = window_counts(&engine);
        assert_eq!(
            (
                counts["workspace-1"],
                counts["workspace-2"],
                counts["workspace-3"]
            ),
            (1, 1, 0)
        );
        let started = logged(name, "workspacesStarted");
        let started = started.last().unwrap();
        assert_eq!(
            started["rejoined"],
            serde_json::json!({ "workspace-1": 1, "workspace-2": 1 })
        );
        assert_eq!(
            (&started["staleJournal"], &started["returned"]),
            (&serde_json::json!(0), &serde_json::json!(0))
        );

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert!(is_parked(&engine, 1));
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert!(is_parked(&engine, 2));
    });
}

#[test]
fn quitting_after_a_restart_still_returns_parked_windows() {
    scenario(|| async {
        let name = "restart-then-quit";
        let engine = live_engine(name, two_zoned_windows());
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        let platform = stopped_for_restart(&engine);

        let engine = relaunched(name, platform);
        assert!(is_parked(&engine, 1));
        event(&engine, PlatformEvent::QuitRequested);
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert!(!paths_kept(name).journal().exists());
    });
}

#[test]
fn a_crash_leaves_parked_windows_parked_for_the_next_launch_and_user_moved_ones_alone() {
    scenario(|| async {
        let name = "recovery-identity";
        let mut platform = crashed_with_window_one_parked(name).await;
        std::fs::remove_file(paths_kept(name).membership()).unwrap();
        let parked = platform.windows[&WindowId(1)].frame;
        let moved = RIGHT_ZONE.with_origin(Point::new(500.0, 300.0));
        platform.windows.get_mut(&WindowId(2)).unwrap().frame = moved;

        let engine = relaunched(name, platform);
        assert!(app_state(&engine).workspaces.running);
        assert_eq!(frame_of(&engine, 1), parked, "rejoined from the journal alone");
        assert_eq!(frame_of(&engine, 2), moved);
        assert_eq!(all_writes(&engine), []);
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
        assert!(paths_kept(name).journal().exists(), "window 1 is still parked");

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE, "back at its saved frame");
    });
}

#[test]
fn a_restart_finds_a_parked_window_whose_token_changed_by_its_parked_position() {
    scenario(|| async {
        let name = "recovery-position";
        let mut platform = crashed_with_window_one_parked(name).await;
        let parked = platform.windows.remove(&WindowId(1)).unwrap();
        let parked_frame = parked.frame;
        platform.stacking.retain(|id| *id != WindowId(1));
        platform.open(7, parked);
        platform.open(8, FakeWindow::new("com.example.editor", 100, FLOATING));

        let engine = relaunched(name, platform);
        assert_eq!(frame_of(&engine, 7), parked_frame);
        assert_eq!(frame_of(&engine, 8), FLOATING);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 7), LEFT_ZONE);
        assert!(
            is_parked(&engine, 8),
            "a window it didn't know joined workspace 1"
        );
    });
}

#[test]
fn a_window_parked_for_the_workspace_showing_at_launch_comes_back() {
    scenario(|| async {
        let name = "recovery-active";
        let platform = crashed_with_window_one_parked(name).await;
        let file = paths_kept(name).settings;
        let mut settings: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        settings["workspaces"]["activeId"] = "workspace-2".into();
        std::fs::write(&file, settings.to_string()).unwrap();

        let engine = relaunched(name, platform);
        assert_eq!(active(&engine), "workspace-2");
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(
            frame_of(&engine, 2),
            RIGHT_ZONE,
            "left alone, in the workspace showing"
        );
        assert_eq!(window_counts(&engine)["workspace-2"], 2);
        assert!(!paths_kept(name).journal().exists());
    });
}

#[test]
fn a_process_id_reused_by_another_launch_never_takes_over_a_parked_window() {
    scenario(|| async {
        let name = "recovery-reused-pid";
        let mut platform = crashed_with_window_one_parked(name).await;
        // The editor quit, and a new launch got the same process ID and a window at the
        // spot where the old one was parked.
        let window = platform.windows.get_mut(&WindowId(1)).unwrap();
        window.launch_time = 2.0;
        let stranded = window.frame;

        let engine = relaunched_logging(name, platform);
        assert!(app_state(&engine).workspaces.running);
        assert!(
            !is_parked(&engine, 1),
            "brought on screen like any stranded window"
        );
        assert_eq!(frame_of(&engine, 1).size(), stranded.size());
        assert_eq!(window_counts(&engine)["workspace-2"], 0);
        assert!(!paths_kept(name).journal().exists());
        let started = logged(name, "workspacesStarted");
        let started = started.last().unwrap();
        assert_eq!(started["staleJournal"], 1);
        assert_eq!(started["staleMembership"], 1);
        assert_eq!(started["rejoined"], serde_json::json!({}));
    });
}

#[test]
fn a_restart_keeps_a_parked_tab_group_together() {
    scenario(|| async {
        let name = "restart-tabs";
        let engine = visited_tabs(name, TABBED);
        press_default(&engine, "Slash").await;
        assert!(is_parked(&engine, 3));
        let parked = frame_of(&engine, 3);
        let platform = stopped_for_restart(&engine);

        let engine = relaunched(name, platform);
        assert_eq!(all_writes(&engine), []);
        assert_eq!(frame_of(&engine, 3), parked);
        assert_eq!(window_counts(&engine)["workspace-2"], 1, "the group counts once");
        assert_eq!(window_counts(&engine)["workspace-1"], 1);

        press_default(&engine, "Digit2").await;
        assert_eq!(frame_of(&engine, 3), TABBED);
        pick_tab(&engine, 1);
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
        press_default(&engine, "Digit1").await;
        assert!(is_parked(&engine, 1), "the tab showing now parks with its group");
        assert_eq!(remembered_workspaces(name)[&1], "workspace-2");
    });
}

#[test]
fn the_selection_is_not_kept_across_a_restart() {
    scenario(|| async {
        let name = "restart-selection";
        let engine = default_workspaces(name, two_zoned_windows());
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE]);
        let platform = stopped_for_restart(&engine);

        let engine = relaunched(name, platform);
        assert_eq!(marks(&engine), []);
        press_default(&engine, "Quote").await;
        assert!(
            is_parked(&engine, 1) && is_parked(&engine, 2),
            "nothing selected goes along"
        );
    });
}

#[test]
fn unrecovered_windows_keep_workspaces_off_and_their_entries() {
    scenario(|| async {
        let name = "recovery-pending";
        let mut platform = crashed_with_window_one_parked(name).await;
        platform.windows.remove(&WindowId(1));
        platform.stacking.retain(|id| *id != WindowId(1));
        platform.open(9, FakeWindow::new("com.example.editor", 100, FLOATING));

        let engine = relaunched(name, platform);
        assert!(!app_state(&engine).workspaces.running);
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error),
            (
                "Some hidden windows could not be restored. Check Accessibility, then quit and reopen Snappy to retry.",
                true
            )
        );
        assert!(paths_kept(name).journal().exists(), "kept for the next attempt");
        assert_eq!(writes_to(&engine, 9), 0);
    });
}

#[test]
fn the_menu_bar_tooltip_names_the_active_workspace_while_workspaces_run() {
    scenario(|| async {
        let engine = live_engine("tooltip", with_window(FLOATING));
        let tooltip = || engine.with_platform(|p| p.tray.tooltip.clone());
        assert_eq!(tooltip(), "Snappy — window zones");

        enable_workspaces(&engine);
        assert_eq!(tooltip(), "Snappy · Workspace 1");
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(tooltip(), "Snappy · Workspace 2");

        engine.with_platform(|p| p.on_managed_desktop = false);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        assert_eq!(tooltip(), "Snappy — window zones", "paused");
        engine.with_platform(|p| p.on_managed_desktop = true);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        assert_eq!(tooltip(), "Snappy · Workspace 2");

        command(&engine, Command::SetWorkspacesEnabled { enabled: false });
        assert_eq!(tooltip(), "Snappy — window zones");
    });
}

#[test]
fn zone_shortcuts_follow_each_displays_layout_and_can_share_chords() {
    scenario(|| async {
        let mut fake = with_window(FLOATING);
        fake.displays.push(side_display());
        let engine = live_engine("per-display-shortcuts", fake);
        thirds_on_the_side(&engine);
        let registered = |code: &str| {
            let combo = snappy(code).combo();
            engine.with_platform(|p| p.registered.iter().filter(|(_, c)| *c == combo).count())
        };
        assert_eq!((registered("Digit1"), registered("Digit3")), (1, 1));

        let third = hotkey_id(&engine, &snappy("Digit3"));
        let status = engine.status();
        event(&engine, PlatformEvent::HotKey(third));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), FLOATING, "halves has no third zone");
        assert_eq!(writes_to(&engine, 1), 0);
        assert_eq!(engine.status(), status);

        place(&engine, 1, Rect::new(1600.0, 200.0, 600.0, 400.0));
        event(&engine, PlatformEvent::HotKey(third));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), side_third(2));
    });
}

#[test]
fn shortcuts_are_registered_again_only_when_the_set_changes_or_failed() {
    scenario(|| async {
        let engine = live_engine("reregister", FakePlatform::default());
        let registrations = || engine.with_platform(|p| p.registrations);
        let start = registrations();
        command(
            &engine,
            Command::SetDragModifier {
                modifier: DragModifier::Alt,
            },
        );
        assert_eq!(registrations(), start, "the same set stays registered");

        engine.with_platform(|p| p.taken_elsewhere.insert(snappy("Digit3").combo()));
        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "thirds".into(),
            },
        );
        assert_eq!(registrations(), start + 1);
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error),
            ("Another app is using Snappy + 3.", true)
        );

        command(
            &engine,
            Command::SetDragModifier {
                modifier: DragModifier::Ctrl,
            },
        );
        assert_eq!(
            registrations(),
            start + 2,
            "a failure is retried on the next change"
        );
        engine.with_platform(|p| p.taken_elsewhere.clear());
        command(
            &engine,
            Command::SetDragModifier {
                modifier: DragModifier::Shift,
            },
        );
        assert_eq!(registrations(), start + 3);
        command(
            &engine,
            Command::SetDragModifier {
                modifier: DragModifier::Alt,
            },
        );
        assert_eq!(registrations(), start + 3);
    });
}

#[test]
fn reopening_snappy_brings_the_settings_window_forward() {
    scenario(|| async {
        let (fake, _) = with_settings_window();
        let engine = live_engine("reopen", fake);
        engine.handle(Input::SettingsAppStarted { pid: 4242 });
        engine.take_outbox();
        event(&engine, PlatformEvent::OpenSettingsRequested);
        assert!(engine.take_outbox().contains(&ToSettings::Focus));
    });
}

#[test]
fn closing_settings_keeps_snappy_running_and_quit_returns_parked_windows() {
    scenario(|| async {
        let name = "close-settings";
        let engine = live_engine(name, with_window(FLOATING));
        engine.handle(Input::SettingsAppStarted { pid: 4242 });
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;

        engine.handle(Input::SettingsAppExited);
        assert!(engine.is_live());
        assert!(engine.with_platform(|p| p.press_monitoring && !p.registered.is_empty()));
        assert!(is_parked(&engine, 1), "workspaces keep running");

        event(&engine, PlatformEvent::QuitRequested);
        assert_eq!(frame_of(&engine, 1), FLOATING);
        assert!(!paths_kept(name).journal().exists());
        assert!(engine.with_platform(|p| !p.press_monitoring && p.registered.is_empty()));
        assert_eq!(
            engine.with_platform(|p| p.last_overlay().cloned()),
            Some(OverlayCall::Hide)
        );
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
        let drops = std::fs::read(paths_kept(name).log("drops.json")).unwrap();
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

fn parking_on_a_desktop(mut fake: FakePlatform) -> FakePlatform {
    fake.parking = ParkingPolicy::Desktop;
    fake
}

fn on_parking_desktop(engine: &Engine<FakePlatform>, window: u64) -> bool {
    engine.with_platform(|p| p.windows[&WindowId(window)].state.on_parking_desktop)
}

/// Workspaces on with window 1 sent to workspace 2.
async fn window_one_on_the_parking_desktop(name: &str) -> Engine<FakePlatform> {
    let engine = live_engine(name, parking_on_a_desktop(two_zoned_windows()));
    enable_workspaces(&engine);
    event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
    settle(&engine).await;
    assert!(on_parking_desktop(&engine, 1));
    engine
}

#[test]
fn desktop_parking_switches_workspaces_without_moving_windows() {
    scenario(|| async {
        let engine = window_one_on_the_parking_desktop("desktop-switch").await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(writes_to(&engine, 1), 0);

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(!on_parking_desktop(&engine, 1));
        assert!(on_parking_desktop(&engine, 2));
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert_eq!(engine.with_platform(|p| p.focused), Some(WindowId(1)));
        assert_eq!(
            engine.with_platform(|p| p.visible_windows(None).len()),
            1,
            "hidden windows leave the visible list"
        );
    });
}

#[test]
fn desktop_parking_hides_again_a_window_its_app_brings_back() {
    scenario(|| async {
        let engine = window_one_on_the_parking_desktop("desktop-contain").await;
        engine.with_platform(|p| {
            p.windows.get_mut(&WindowId(1)).unwrap().state.on_parking_desktop = false;
        });
        event(&engine, PlatformEvent::WindowMoved(WindowId(1)));
        settle(&engine).await;
        assert!(on_parking_desktop(&engine, 1));
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
    });
}

#[test]
fn desktop_parking_quitting_brings_every_window_back() {
    scenario(|| async {
        let engine = window_one_on_the_parking_desktop("desktop-quit").await;
        engine.shutdown();
        assert!(!on_parking_desktop(&engine, 1));
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
    });
}

#[test]
fn desktop_parking_a_restart_leaves_windows_on_the_parking_desktop() {
    scenario(|| async {
        let name = "desktop-restart";
        let engine = window_one_on_the_parking_desktop(name).await;
        let platform = stopped_for_restart(&engine);

        let engine = relaunched(name, platform);
        assert!(app_state(&engine).workspaces.running);
        assert!(on_parking_desktop(&engine, 1));
        assert_eq!(all_writes(&engine), []);
        assert_eq!(window_counts(&engine)["workspace-2"], 1);

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(!on_parking_desktop(&engine, 1));
        assert!(on_parking_desktop(&engine, 2));
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
    });
}

#[test]
fn desktop_parking_quitting_brings_windows_back_from_the_parking_desktop_at_their_saved_frame() {
    scenario(|| async {
        let name = "desktop-recovery";
        let engine = window_one_on_the_parking_desktop(name).await;
        let mut platform = engine.with_platform(|p| p.clone());
        std::fs::remove_file(paths_kept(name).membership()).unwrap();
        let moved = LEFT_ZONE.with_origin(Point::new(500.0, 300.0));
        platform.windows.get_mut(&WindowId(1)).unwrap().frame = moved;
        platform.writes.clear();

        let engine = relaunched(name, platform);
        assert!(on_parking_desktop(&engine, 1), "a crash leaves it parked");
        engine.shutdown();
        assert!(!on_parking_desktop(&engine, 1));
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE, "at its saved frame");
        assert!(!paths_kept(name).journal().exists());
    });
}

#[test]
fn desktop_parking_recovery_leaves_windows_the_user_brought_back() {
    scenario(|| async {
        let name = "desktop-recovery-user";
        let engine = window_one_on_the_parking_desktop(name).await;
        let mut platform = engine.with_platform(|p| p.clone());
        std::fs::remove_file(paths_kept(name).membership()).unwrap();
        let window = platform.windows.get_mut(&WindowId(1)).unwrap();
        window.state.on_parking_desktop = false;
        platform.writes.clear();

        let engine = relaunched(name, platform);
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(writes_to(&engine, 1), 0, "its frame alone doesn't make it parked");
        assert!(!paths_kept(name).journal().exists());
    });
}

#[test]
fn desktop_parking_startup_keeps_a_window_parked_for_its_own_workspace() {
    scenario(|| async {
        let name = "desktop-startup-owner";
        let paths = paths(name);
        std::fs::create_dir_all(&paths.state_dir).unwrap();
        let saved = serde_json::json!([{
            "key": { "pid": 100, "launchTime": 1.0, "window": "1" },
            "workspaceId": "workspace-2",
            "frame": FLOATING,
            "parkedOrigin": FLOATING.origin(),
        }]);
        std::fs::write(
            paths.state_dir.join("workspace-membership.json"),
            saved.to_string(),
        )
        .unwrap();
        let mut fake = parking_on_a_desktop(with_window(FLOATING));
        fake.windows
            .get_mut(&WindowId(1))
            .unwrap()
            .state
            .on_parking_desktop = true;

        let engine = Engine::new(fake, paths);
        engine.start();
        command(&engine, Command::SetSnapping { enabled: true });
        enable_workspaces(&engine);
        settle(&engine).await;
        assert!(on_parking_desktop(&engine, 1), "still parked");
        assert_eq!(window_counts(&engine).get("workspace-2"), Some(&1));

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(!on_parking_desktop(&engine, 1));
        assert_eq!(frame_of(&engine, 1), FLOATING);
    });
}

#[test]
fn desktop_parking_startup_brings_back_a_window_nobody_remembers() {
    scenario(|| async {
        let mut fake = parking_on_a_desktop(with_window(FLOATING));
        fake.windows
            .get_mut(&WindowId(1))
            .unwrap()
            .state
            .on_parking_desktop = true;
        let engine = live_engine("desktop-startup-stranded", fake);
        enable_workspaces(&engine);
        settle(&engine).await;
        assert!(!on_parking_desktop(&engine, 1));
        assert_eq!(frame_of(&engine, 1), FLOATING);
        assert_eq!(window_counts(&engine).get("workspace-1"), Some(&1));
    });
}

// MARK: Move to the nearest empty zone

fn press_nearest_empty(engine: &Engine<FakePlatform>) {
    event(engine, PlatformEvent::HotKey(hotkey_id(engine, &snappy("Space"))));
}

/// Snap records in a test's debug log, oldest first.
fn logged_snaps(name: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(paths_kept(name).log_dir.join("debug.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|record| record["event"] == "snap")
        .collect()
}

#[test]
fn snappy_space_snaps_a_floating_window_into_the_nearest_empty_zone_and_undoes() {
    scenario(|| async {
        let name = "nearest-empty-floating";
        let engine = live_engine(name, floating_over_the_left_zone(LEFT_ZONE));
        engine.force_debug_log();
        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(
            frame_of(&engine, 1),
            RIGHT_ZONE,
            "the left zone is nearer but taken"
        );
        assert_eq!(frame_of(&engine, 2), LEFT_ZONE, "the occupant stays");
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error, status.notify),
            ("Snapped.", false, false)
        );
        let snaps = logged_snaps(name);
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0]["outcome"], "snapped");
        assert_eq!(snaps[0]["window"], 1);

        undo(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), FLOATING);
    });
}

#[test]
fn snappy_space_moves_a_window_already_in_a_zone_to_another_empty_zone() {
    scenario(|| async {
        let engine = live_engine("nearest-empty-zoned", with_window(LEFT_ZONE));
        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), RIGHT_ZONE);
    });
}

#[test]
fn snappy_space_with_every_zone_taken_moves_nothing_and_says_so_quietly() {
    scenario(|| async {
        let engine = live_engine("nearest-empty-full", two_zoned_windows());
        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert_eq!(writes_to(&engine, 1), 0);
        assert_eq!(writes_to(&engine, 2), 0);
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error, status.notify),
            ("No empty zone on this display.", false, false)
        );
    });
}

#[test]
fn snappy_space_on_a_full_display_goes_to_the_nearest_empty_zone_on_another() {
    scenario(|| async {
        let mut fake = two_zoned_windows();
        fake.displays.push(side_display());
        fake.open(3, FakeWindow::new("com.example.notes", 300, side_third(0)));
        fake.focused = Some(WindowId(1));
        let engine = live_engine("nearest-empty-other-display", fake);
        thirds_on_the_side(&engine);
        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(
            frame_of(&engine, 1),
            side_third(1),
            "the side display's first third is taken, so the next one is nearest"
        );
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert_eq!(frame_of(&engine, 3), side_third(0));
    });
}

#[test]
fn snappy_space_ignores_windows_of_inactive_workspaces() {
    scenario(|| async {
        let engine = inactive_window_in_the_right_zone("nearest-empty-inactive").await;
        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), RIGHT_ZONE);
    });
}

#[test]
fn the_nearest_empty_zone_shortcut_can_be_changed_or_cleared() {
    scenario(|| async {
        let engine = live_engine("nearest-empty-shortcut", with_window(LEFT_ZONE));
        let registered = |engine: &Engine<FakePlatform>, chord: &KeyChord| {
            engine.with_platform(|p| p.registered.iter().any(|(_, combo)| *combo == chord.combo()))
        };
        assert!(registered(&engine, &snappy("Space")));

        command(
            &engine,
            Command::SetNearestEmptyShortcut {
                chord: Some(snappy("KeyE")),
            },
        );
        assert_eq!(engine.settings().navigation.nearest_empty, Some(snappy("KeyE")));
        assert!(!registered(&engine, &snappy("Space")));
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("KeyE"))),
        );
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), RIGHT_ZONE);

        command(&engine, Command::SetNearestEmptyShortcut { chord: None });
        assert_eq!(engine.settings().navigation.nearest_empty, None);
        assert!(!registered(&engine, &snappy("KeyE")));
    });
}

/// The platform loses control, and while it is gone shows window 1 again, as GNOME's
/// extension does when it is turned off.
async fn control_lost_with_window_one_shown(name: &str) -> Engine<FakePlatform> {
    let engine = window_one_on_the_parking_desktop(name).await;
    engine.with_platform(|p| p.live_control = LiveControl::unavailable("The extension is off."));
    event(&engine, PlatformEvent::LiveControlChanged);
    settle(&engine).await;
    engine.with_platform(|p| {
        p.windows.get_mut(&WindowId(1)).unwrap().state.on_parking_desktop = false;
    });
    engine
}

#[test]
fn losing_live_control_keeps_workspaces_as_they_were_and_moves_nothing() {
    scenario(|| async {
        let engine = window_one_on_the_parking_desktop("control-lost-keeps").await;
        let writes = engine.with_platform(|p| p.writes.len());
        engine.with_platform(|p| p.live_control = LiveControl::unavailable("The extension is off."));
        event(&engine, PlatformEvent::LiveControlChanged);
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.writes.len()), writes, "nothing moved");
        assert_eq!(status_text(&engine), "The extension is off.");
        assert!(!app_state(&engine).workspaces.running);
        assert_eq!(window_counts(&engine)["workspace-2"], 1, "membership is kept");
    });
}

#[test]
fn regaining_live_control_hides_again_what_the_platform_showed_meanwhile() {
    scenario(|| async {
        let engine = control_lost_with_window_one_shown("control-regained").await;
        engine.with_platform(|p| p.live_control = LiveControl::Available);
        event(&engine, PlatformEvent::LiveControlChanged);
        settle(&engine).await;
        assert!(on_parking_desktop(&engine, 1), "window 1 belongs to workspace 2");
        assert!(!on_parking_desktop(&engine, 2));
        assert!(app_state(&engine).workspaces.running);
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
    });
}

#[test]
fn the_platform_starting_over_hides_shown_windows_again() {
    scenario(|| async {
        let engine = window_one_on_the_parking_desktop("platform-restarted").await;
        engine.with_platform(|p| {
            p.windows.get_mut(&WindowId(1)).unwrap().state.on_parking_desktop = false;
        });
        event(&engine, PlatformEvent::LiveControlChanged);
        settle(&engine).await;
        assert!(on_parking_desktop(&engine, 1));
        assert!(app_state(&engine).workspaces.running);
    });
}

#[test]
fn a_remedy_the_platform_no_longer_offers_is_not_applied() {
    scenario(|| async {
        let engine = live_engine("remedy-stale", FakePlatform::default());
        command(
            &engine,
            Command::ApplyRemedy {
                remedy: Remedy::LogOut,
            },
        );
        assert!(engine.with_platform(|p| p.remedies.is_empty()));
    });
}

/// Window 1 in the middle of the main display, focused, with a window on each side of it:
/// 2 to the left, 3 to the right, 4 above, and 5 below.
fn around_a_center() -> FakePlatform {
    let mut fake = FakePlatform::default();
    let frames = [
        (2, Rect::new(60.0, 375.0, 240.0, 150.0)),
        (3, Rect::new(1140.0, 375.0, 240.0, 150.0)),
        (4, Rect::new(600.0, 60.0, 240.0, 150.0)),
        (5, Rect::new(600.0, 700.0, 240.0, 150.0)),
        (1, Rect::new(600.0, 375.0, 240.0, 150.0)),
    ];
    for (id, frame) in frames {
        fake.open(id, FakeWindow::new("com.example.editor", 100, frame));
    }
    fake
}

fn frontmost(engine: &Engine<FakePlatform>) -> WindowId {
    engine.with_platform(|p| p.stacking[0])
}

#[test]
fn focus_shortcuts_focus_and_raise_the_nearest_window_in_each_direction() {
    scenario(|| async {
        let engine = live_engine("focus-directions", around_a_center());
        for (key, expected) in [("KeyL", 3), ("KeyJ", 2), ("KeyI", 4), ("KeyK", 5)] {
            focus(&engine, 1);
            press_default(&engine, key).await;
            assert_eq!(focused(&engine), Some(WindowId(expected)), "{key}");
            assert_eq!(frontmost(&engine), WindowId(expected), "{key} raises it");
        }
        assert_eq!(
            frame_of(&engine, 1),
            Rect::new(600.0, 375.0, 240.0, 150.0),
            "nothing moves"
        );
    });
}

#[test]
fn a_focus_shortcut_with_nothing_that_way_leaves_focus_and_says_so_quietly() {
    scenario(|| async {
        let engine = live_engine("focus-nothing-there", around_a_center());
        focus(&engine, 3);
        press_default(&engine, "KeyL").await;
        assert_eq!(focused(&engine), Some(WindowId(3)), "focus never wraps around");
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error, status.notify),
            ("No window to the right.", false, false)
        );
        assert!(marks(&engine).is_empty());
    });
}

#[test]
fn with_nothing_focused_focus_moves_from_the_middle_of_the_display_under_the_pointer() {
    scenario(|| async {
        let engine = live_engine("focus-from-pointer", around_a_center());
        engine.with_platform(|p| p.focused = None);
        press_default(&engine, "KeyL").await;
        assert_eq!(
            focused(&engine),
            Some(WindowId(3)),
            "window 1 sits on the middle, not past it"
        );
    });
}

#[test]
fn equally_near_windows_go_to_the_one_focused_most_recently() {
    scenario(|| async {
        let mut fake = FakePlatform::default();
        let window =
            |x: f64, y: f64| FakeWindow::new("com.example.editor", 100, Rect::new(x, y, 240.0, 150.0));
        fake.open(2, window(700.0, 225.0));
        fake.open(3, window(700.0, 525.0));
        fake.open(1, window(100.0, 375.0));
        let engine = default_workspaces("focus-ties", fake);
        for (order, expected) in [([2, 3], 3), ([3, 2], 2)] {
            for window in order {
                focus(&engine, window);
            }
            focus(&engine, 1);
            press_default(&engine, "KeyL").await;
            assert_eq!(focused(&engine), Some(WindowId(expected)));
        }
    });
}

/// The focus moves the debug log recorded.
fn logged_focus_moves(name: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(paths_kept(name).log_dir.join("debug.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|record| record["event"] == "focusMove")
        .collect()
}

/// Window 1 focused in the left zone, window 2 in the right zone, and window 3 floating
/// between them, a little lower, where the nearest window by center would be it.
fn zoned_with_a_floating_window_between() -> FakePlatform {
    let mut fake = two_zoned_windows();
    let between = Rect::new(570.0, 400.0, 300.0, 200.0);
    fake.open(3, FakeWindow::new("com.example.notes", 300, between));
    fake.focused = Some(WindowId(1));
    fake
}

#[test]
fn with_windows_in_zones_focus_moves_between_zones_past_a_floating_window() {
    scenario(|| async {
        let name = "focus-zones";
        let engine = live_engine(name, zoned_with_a_floating_window_between());
        engine.force_debug_log();
        press_default(&engine, "KeyL").await;
        assert_eq!(focused(&engine), Some(WindowId(2)));
        press_default(&engine, "KeyJ").await;
        assert_eq!(focused(&engine), Some(WindowId(1)));
        press_default(&engine, "KeyK").await;
        assert_eq!(
            focused(&engine),
            Some(WindowId(1)),
            "the floating window below is skipped"
        );
        assert_eq!(status_text(&engine), "No window below.");

        let moves = logged_focus_moves(name);
        assert_eq!(moves.len(), 3);
        assert!(moves.iter().all(|record| record["mode"] == "zones"), "{moves:?}");
        assert_eq!(
            moves[0]["zone"],
            serde_json::json!({ "display": "main", "zone": "halves-1" })
        );
        assert_eq!(moves[2]["zone"], serde_json::Value::Null);
    });
}

#[test]
fn from_a_floating_focused_window_focus_goes_to_the_zone_ahead() {
    scenario(|| async {
        let engine = live_engine("focus-from-floating", zoned_with_a_floating_window_between());
        for (key, expected) in [("KeyL", 2), ("KeyJ", 1)] {
            focus(&engine, 3);
            press_default(&engine, key).await;
            assert_eq!(focused(&engine), Some(WindowId(expected)), "{key}");
        }
    });
}

#[test]
fn of_windows_stacked_in_a_zone_focus_goes_to_the_most_recently_focused() {
    scenario(|| async {
        let mut fake = two_zoned_windows();
        fake.open(4, FakeWindow::new("com.example.notes", 300, RIGHT_ZONE));
        let engine = default_workspaces("focus-stacked", fake);
        for (order, expected) in [([2, 4], 4), ([4, 2], 2)] {
            for window in order {
                focus(&engine, window);
            }
            focus(&engine, 1);
            press_default(&engine, "KeyL").await;
            assert_eq!(focused(&engine), Some(WindowId(expected)));
        }
    });
}

#[test]
fn focus_moves_between_zones_across_displays_skipping_empty_ones() {
    scenario(|| async {
        let mut fake = FakePlatform::default();
        fake.displays.push(side_display());
        fake.open(2, FakeWindow::new("com.example.browser", 200, side_third(1)));
        fake.open(1, FakeWindow::new("com.example.editor", 100, LEFT_ZONE));
        let engine = live_engine("focus-across-displays", fake);
        thirds_on_the_side(&engine);
        press_default(&engine, "KeyL").await;
        assert_eq!(focused(&engine), Some(WindowId(2)));
        press_default(&engine, "KeyJ").await;
        assert_eq!(focused(&engine), Some(WindowId(1)));
    });
}

#[test]
fn with_no_window_in_a_zone_focus_moves_by_window_centers() {
    scenario(|| async {
        let name = "focus-windows-mode";
        let engine = live_engine(name, around_a_center());
        engine.force_debug_log();
        focus(&engine, 1);
        press_default(&engine, "KeyL").await;
        assert_eq!(focused(&engine), Some(WindowId(3)));
        let moves = logged_focus_moves(name);
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0]["mode"], "windows");
        assert_eq!(moves[0]["zone"], serde_json::Value::Null);
    });
}

#[test]
fn the_newly_focused_window_flashes_an_outline_of_its_own_that_then_goes() {
    scenario(|| async {
        let engine = live_engine("focus-flash", around_a_center());
        let right = frame_of(&engine, 3);
        let middle = frame_of(&engine, 1);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("KeyL"))),
        );
        assert_eq!(marked(&engine), [(right, MarkStyle::Focused)]);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(marked(&engine), [(right, MarkStyle::Focused)], "still showing");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(marks(&engine).is_empty(), "gone after about 400 ms");

        press_default(&engine, "Enter").await;
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("KeyJ"))),
        );
        assert_eq!(
            marked(&engine),
            [(right, MarkStyle::Selected), (middle, MarkStyle::Focused)]
        );
        settle(&engine).await;
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("KeyL"))),
        );
        assert_eq!(
            marked(&engine),
            [(right, MarkStyle::Focused)],
            "the flash shows over the selection"
        );
        settle(&engine).await;
        assert_eq!(marked(&engine), [(right, MarkStyle::Selected)]);
    });
}

/// Two floating windows over the halves layout: 1 nearer the right zone, 2 nearer the left.
fn two_floating_windows() -> FakePlatform {
    let mut fake = FakePlatform::default();
    let window = |app: &str, pid: u32, x: f64| FakeWindow::new(app, pid, Rect::new(x, 300.0, 300.0, 200.0));
    fake.open(2, window("com.example.browser", 200, 100.0));
    fake.open(1, window("com.example.editor", 100, 800.0));
    fake
}

#[test]
fn snappy_space_places_the_selection_into_empty_zones_as_one_undo_step() {
    scenario(|| async {
        let name = "place-selection";
        let engine = live_engine(name, two_floating_windows());
        engine.force_debug_log();
        let before = (frame_of(&engine, 1), frame_of(&engine, 2));
        press_default(&engine, "Enter").await;
        focus(&engine, 2);
        press_default(&engine, "Enter").await;

        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (RIGHT_ZONE, LEFT_ZONE)
        );
        assert!(marks(&engine).is_empty(), "placed windows leave the selection");
        assert_eq!(status_text(&engine), "Snapped 2 windows.");
        let snaps = logged_snaps(name);
        assert_eq!(snaps.len(), 2, "each move is logged as a snap");
        assert!(snaps.iter().all(|snap| snap["outcome"] == "snapped"));

        undo(&engine);
        settle(&engine).await;
        assert_eq!((frame_of(&engine, 1), frame_of(&engine, 2)), before);

        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(
            frame_of(&engine, 2),
            LEFT_ZONE,
            "without a selection, Space moves only the focused window"
        );
        assert_eq!(frame_of(&engine, 1), before.0);
    });
}

#[test]
fn selected_windows_that_dont_fit_stay_selected_and_the_status_says_how_many() {
    scenario(|| async {
        let mut fake = two_floating_windows();
        let middle = Rect::new(620.0, 350.0, 200.0, 200.0);
        fake.open(3, FakeWindow::new("com.example.notes", 300, middle));
        let engine = live_engine("place-overflow", fake);
        for window in [1, 2, 3] {
            focus(&engine, window);
            press_default(&engine, "Enter").await;
        }

        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2), frame_of(&engine, 3)),
            (RIGHT_ZONE, LEFT_ZONE, middle)
        );
        assert_eq!(marked(&engine), [(middle, MarkStyle::Selected)]);
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error, status.notify),
            ("1 window didn't fit.", false, false)
        );
    });
}

const TABBED: Rect = Rect::new(200.0, 150.0, 640.0, 420.0);
const TERMINAL: u32 = 300;

/// An editor window, 4, and a Terminal window with three native tabs, 1 to 3, the last
/// opened showing. Each tab is a window of its own, as macOS makes them.
fn tabbed_terminal(frame: Rect) -> FakePlatform {
    let mut fake = FakePlatform::default();
    let editor = Rect::new(900.0, 300.0, 400.0, 300.0);
    fake.open(4, FakeWindow::new("com.example.editor", 100, editor));
    fake.open(1, FakeWindow::new("com.apple.Terminal", TERMINAL, frame));
    fake.add_tab(2, WindowId(1));
    fake.add_tab(3, WindowId(2));
    fake
}

/// Click a tab, which shows and focuses it.
fn pick_tab(engine: &Engine<FakePlatform>, tab: u64) {
    engine.with_platform(|p| p.select_tab(WindowId(tab)));
    terminal_focused(engine);
}

fn terminal_focused(engine: &Engine<FakePlatform>) {
    event(
        engine,
        PlatformEvent::FocusChanged {
            pid: TERMINAL,
            by_activation: false,
        },
    );
}

/// Workspaces on with every tab of the Terminal window visited once, so each is a member,
/// and tab 3 showing again.
fn visited_tabs(name: &str, frame: Rect) -> Engine<FakePlatform> {
    let engine = default_workspaces(name, tabbed_terminal(frame));
    for tab in [1, 2, 3] {
        pick_tab(&engine, tab);
    }
    engine
}

/// The workspace the membership file gives each window.
fn remembered_workspaces(name: &str) -> BTreeMap<u64, String> {
    let file = std::fs::read(paths_kept(name).membership()).unwrap();
    serde_json::from_slice::<Vec<serde_json::Value>>(&file)
        .unwrap()
        .iter()
        .map(|entry| {
            let window = entry["key"]["window"].as_str().unwrap().parse().unwrap();
            (window, entry["workspaceId"].as_str().unwrap().to_owned())
        })
        .collect()
}

#[test]
fn a_selected_tabbed_window_takes_every_tab_along_and_torn_out_tabs_stay_there() {
    scenario(|| async {
        let name = "tabs-carried";
        let engine = visited_tabs(name, TABBED);
        engine.force_debug_log();
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [TABBED], "the shown tab is outlined");

        press_default(&engine, "Quote").await;
        assert_eq!(active(&engine), "workspace-2");
        assert_eq!(frame_of(&engine, 3), TABBED, "the group keeps its frame");
        assert!(is_parked(&engine, 4));
        assert_eq!(
            window_counts(&engine)["workspace-2"],
            1,
            "a tab group counts once"
        );
        let remembered = remembered_workspaces(name);
        for tab in [1, 2, 3] {
            assert_eq!(remembered[&tab], "workspace-2", "tab {tab}");
        }
        assert_eq!(
            writes_to(&engine, 1) + writes_to(&engine, 2),
            0,
            "hidden tabs never move"
        );

        // Drag a hidden tab out, then the shown one, which shows tab 2 in its place.
        let torn = Rect::new(80.0, 120.0, 500.0, 360.0);
        engine.with_platform(|p| p.tear_out(WindowId(1), torn));
        terminal_focused(&engine);
        let torn_too = Rect::new(700.0, 400.0, 500.0, 360.0);
        engine.with_platform(|p| p.tear_out(WindowId(3), torn_too));
        terminal_focused(&engine);
        assert_eq!(frame_of(&engine, 2), TABBED);

        press_default(&engine, "Digit1").await;
        assert!(
            [1, 2, 3].into_iter().all(|tab| is_parked(&engine, tab)),
            "every torn-out tab stays with workspace 2"
        );
        assert_eq!(frame_of(&engine, 4), Rect::new(900.0, 300.0, 400.0, 300.0));

        press_default(&engine, "Digit2").await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2), frame_of(&engine, 3)),
            (torn, TABBED, torn_too)
        );
        assert_eq!(window_counts(&engine)["workspace-2"], 3);

        let log = std::fs::read_to_string(paths_kept(name).log_dir.join("debug.jsonl")).unwrap();
        let carried = log
            .lines()
            .find(|line| line.contains("\"event\":\"selectionCarried\""))
            .unwrap();
        assert!(carried.contains("\"tabs\":{\"3\":[1,2]}"), "{carried}");
    });
}

#[test]
fn sending_a_tabbed_window_sends_every_tab_and_hidden_tabs_never_move() {
    scenario(|| async {
        let engine = visited_tabs("tabs-sent", TABBED);
        press_default(&engine, "Slash").await;
        assert_eq!(active(&engine), "workspace-1");
        assert!(is_parked(&engine, 3));
        assert_eq!(
            focused(&engine),
            Some(WindowId(4)),
            "focus stays on this workspace"
        );
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
        assert_eq!(writes_to(&engine, 1) + writes_to(&engine, 2), 0);

        press_default(&engine, "Digit2").await;
        assert_eq!(frame_of(&engine, 3), TABBED);
        pick_tab(&engine, 1);
        assert_eq!(frame_of(&engine, 1), TABBED);
        press_default(&engine, "Digit1").await;
        assert!(is_parked(&engine, 1), "the tab showing now parks the group");
        assert_eq!(
            writes_to(&engine, 3),
            2,
            "tab 3, hidden now, moved only while it showed"
        );
    });
}

#[test]
fn a_new_tab_joins_its_groups_workspace_even_while_the_group_is_hidden() {
    scenario(|| async {
        let engine = visited_tabs("tabs-new", TABBED);
        press_default(&engine, "Slash").await;
        assert!(is_parked(&engine, 3));
        let parked = frame_of(&engine, 3);

        // The app opens a tab in the hidden window: it shows where the group is parked.
        engine.with_platform(|p| {
            p.add_tab(5, WindowId(3));
        });
        event(&engine, PlatformEvent::WindowCreated { pid: TERMINAL });
        assert_eq!(frame_of(&engine, 5), parked, "Snappy leaves it parked");
        assert_eq!(window_counts(&engine)["workspace-1"], 1);
        assert_eq!(window_counts(&engine)["workspace-2"], 1);

        press_default(&engine, "Digit2").await;
        assert_eq!(
            frame_of(&engine, 5),
            TABBED,
            "it comes back as the group's shown tab"
        );
    });
}

#[test]
fn a_restart_puts_tabs_remembered_in_other_workspaces_with_their_shown_tab() {
    scenario(|| async {
        let name = "tabs-restart";
        let mut fake = FakePlatform::default();
        fake.open(1, FakeWindow::new("com.apple.Terminal", TERMINAL, TABBED));
        fake.add_tab(2, WindowId(1));
        for window in fake.windows.values_mut() {
            window.state.minimized = true;
        }
        let engine = started(name, fake);
        engine.force_debug_log();
        let saved = |window: u64, workspace: &str| {
            serde_json::json!({
                "key": { "pid": TERMINAL, "launchTime": 1.0, "window": window.to_string() },
                "workspaceId": workspace,
                "frame": TABBED,
                "parkedOrigin": null,
            })
        };
        let entries = serde_json::json!([saved(2, "workspace-2"), saved(1, "workspace-1")]);
        let membership = paths_kept(name).membership();
        std::fs::create_dir_all(membership.parent().unwrap()).unwrap();
        std::fs::write(&membership, entries.to_string()).unwrap();

        command(&engine, Command::SetWorkspacesEnabled { enabled: true });
        assert!(app_state(&engine).workspaces.running);
        let remembered = remembered_workspaces(name);
        assert_eq!(
            (remembered[&1].as_str(), remembered[&2].as_str()),
            ("workspace-2", "workspace-2")
        );
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
        assert_eq!(window_counts(&engine)["workspace-1"], 0);
        let log = std::fs::read_to_string(paths_kept(name).log_dir.join("debug.jsonl")).unwrap();
        assert!(log.contains("\"event\":\"tabsUnified\""), "{log}");
    });
}

#[test]
fn a_tab_group_is_selected_as_one_and_only_its_shown_tab_counts_as_a_zone_occupant() {
    scenario(|| async {
        let engine = visited_tabs("tabs-selected", LEFT_ZONE);
        press_default(&engine, "Enter").await;
        pick_tab(&engine, 1);
        assert_eq!(marks(&engine), [LEFT_ZONE], "one outline, on the tab showing");
        press_default(&engine, "Enter").await;
        assert!(marks(&engine).is_empty(), "the group's other tab unselects it");

        place(&engine, 4, RIGHT_ZONE);
        focus(&engine, 4);
        press_default(&engine, "ArrowLeft").await;
        assert_eq!(
            frame_of(&engine, 4),
            LEFT_ZONE,
            "the hidden tabs don't crowd the zone"
        );
        assert_eq!(frame_of(&engine, 1), RIGHT_ZONE, "the shown tab swaps");
    });
}
