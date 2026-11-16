//! Starting up, Accessibility permission, live control coming and going, and the app's window.

use snappy_core::geometry::Rect;
use snappy_core::keys::{KeyChord, Modifier, SnappyKey};
use snappy_core::settings::DragModifier;
use snappy_engine::{Engine, Input};
use snappy_fake::{FakePlatform, OverlayCall};
use snappy_ipc::{Command, ToApp};
use snappy_platform::{LiveControl, PlatformEvent, Remedy, WindowId};

use crate::support::{
    FLOATING, LEFT_ZONE, app_state, command, enable_workspaces, event, frame_of, hotkey_id, is_parked,
    live_engine, paths_kept, scenario, send_to, settle, snappy, started, status_text, with_app_window,
    with_window,
};

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
    let switch_one = KeyChord::new("Digit1", [Modifier::Ctrl]).combo(SnappyKey::DEFAULT);
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
fn a_login_item_error_keeps_the_switch_off_and_shows_the_message() {
    scenario(|| async {
        let engine = live_engine("login-item-error", FakePlatform::default());
        let refusal = "The login item couldn't be registered.";
        engine.with_platform(|p| p.login_item_error = Some(refusal.into()));
        engine.take_outbox();

        command(&engine, Command::SetLoginItem { enabled: true });
        assert!(engine.take_outbox().contains(&ToApp::Reply {
            id: 1,
            error: Some(refusal.to_owned().into()),
        }));
        let status = engine.status().unwrap();
        assert_eq!((status.message.as_str(), status.is_error), (refusal, true));
        assert!(!app_state(&engine).login_item_enabled, "the switch reverts");
    });
}

#[test]
fn what_finishes_between_inputs_signals_the_app_at_once() {
    scenario(|| async {
        let engine = live_engine("outbox-signal", with_window(LEFT_ZONE));
        engine.handle(Input::AppStarted { pid: 4242 });
        let ready = engine.outbox_ready();
        let signalled = || async {
            tokio::time::timeout(std::time::Duration::ZERO, ready.notified())
                .await
                .is_ok()
        };

        engine.with_platform(|p| p.focused = Some(WindowId(1)));
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("ArrowRight"))),
        );
        engine.take_outbox();
        signalled().await;
        settle(&engine).await;
        assert!(
            signalled().await,
            "the snap settling is told without another input"
        );
        assert!(!engine.take_outbox().is_empty());
    });
}

#[test]
fn reopening_snappy_brings_the_app_window_forward() {
    scenario(|| async {
        let (fake, _) = with_app_window();
        let engine = live_engine("reopen", fake);
        engine.handle(Input::AppStarted { pid: 4242 });
        engine.take_outbox();
        event(&engine, PlatformEvent::OpenAppRequested);
        assert!(engine.take_outbox().contains(&ToApp::Focus));
    });
}

#[test]
fn closing_settings_keeps_snappy_running_and_quit_returns_parked_windows() {
    scenario(|| async {
        let name = "close-settings";
        let engine = live_engine(name, with_window(FLOATING));
        engine.handle(Input::AppStarted { pid: 4242 });
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;

        engine.handle(Input::AppExited);
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
