//! Saving, refusing, and importing settings.

use snappy_core::keys::{KeyChord, Modifier};
use snappy_core::navigate::Direction;
use snappy_core::settings::{FocusShortcut, HistoryStep, Settings};
use snappy_engine::Engine;
use snappy_fake::FakePlatform;
use snappy_ipc::Command;
use snappy_platform::PlatformEvent;

use crate::support::{
    FLOATING, command, enable_workspaces, event, frame_of, frozen, is_parked, live_engine, paths, paths_kept,
    scenario, send_to, settle, snappy, status_text, with_window,
};

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
                shortcut: Direction::Left.into(),
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
        assert_eq!(Settings::from_json(&saved, platform).unwrap(), engine.settings());
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
                shortcut: Direction::Left.into(),
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
            Command::SetNavigationShortcut {
                shortcut: FocusShortcut::SelectWindow.into(),
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
