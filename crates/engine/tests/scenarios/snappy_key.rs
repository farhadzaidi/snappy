//! Choosing the Snappy key.

use snappy_core::bindings::Refusal;
use snappy_core::keys::{KeyChord, Modifier, Mods, SnappyKey};
use snappy_core::settings::HistoryStep;
use snappy_engine::Engine;
use snappy_fake::FakePlatform;
use snappy_ipc::Command;
use snappy_platform::PlatformEvent;

use crate::support::{
    FLOATING, RIGHT_ZONE, command, control_option, event, frame_of, hotkey_id, live_engine, logged,
    paths_kept, relaunched, reply_to, scenario, settle, snappy, with_window,
};

fn saved_snappy_key(name: &str) -> serde_json::Value {
    let saved = std::fs::read_to_string(paths_kept(name).settings).unwrap();
    serde_json::from_str::<serde_json::Value>(&saved).unwrap()["snappyKey"].clone()
}

#[test]
fn changing_the_snappy_key_registers_every_snappy_shortcut_with_the_new_modifiers_at_once() {
    scenario(|| async {
        let name = "snappy-key-changed";
        let engine = live_engine(name, with_window(FLOATING));
        engine.force_debug_log();
        let before = engine.with_platform(|p| p.registered.len());
        let default_key = SnappyKey::DEFAULT.modifiers();
        assert!(engine.with_platform(|p| p.registered.iter().all(|(_, combo)| combo.mods == default_key)));

        let error = reply_to(
            &engine,
            Command::SetSnappyKey {
                modifiers: control_option(),
            },
        );
        assert_eq!(error, None);
        assert_eq!(engine.settings().snappy_key.modifiers(), control_option());
        let registered = engine.with_platform(|p| p.registered.clone());
        assert_eq!(registered.len(), before, "every shortcut, once each");
        assert!(
            registered.iter().all(|(_, combo)| combo.mods == control_option()),
            "{registered:?}"
        );
        assert_eq!(
            engine.settings().navigation.left,
            Some(snappy("ArrowLeft").with_label("←")),
            "stored as the Snappy key still"
        );
        assert_eq!(
            engine.status().unwrap().message,
            "The Snappy key is now Control ⌃ + Option ⌥."
        );
        assert_eq!(saved_snappy_key(name), serde_json::json!(["ctrl", "alt"]));
        let changed = logged(name, "snappyKeyChanged");
        assert_eq!(changed.len(), 1);
        assert_eq!(
            changed[0]["from"],
            serde_json::json!(["Control", "Option", "Command"])
        );
        assert_eq!(changed[0]["to"], serde_json::json!(["Control", "Option"]));

        let right_zone = hotkey_id(&engine, &snappy("Digit2"));
        event(&engine, PlatformEvent::HotKey(right_zone));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), RIGHT_ZONE);
    });
}

#[test]
fn a_snappy_key_change_that_clashes_with_another_shortcut_is_refused_and_changes_nothing() {
    scenario(|| async {
        let name = "snappy-key-clash";
        let engine = live_engine(name, FakePlatform::default());
        engine.force_debug_log();
        command(
            &engine,
            Command::SetHistoryShortcut {
                step: HistoryStep::Undo,
                chord: Some(KeyChord::new("KeyJ", [Modifier::Ctrl, Modifier::Alt])),
            },
        );
        let settings = engine.settings();
        let registered = engine.with_platform(|p| p.registered.clone());
        let registrations = engine.with_platform(|p| p.registrations);

        let error = reply_to(
            &engine,
            Command::SetSnappyKey {
                modifiers: control_option(),
            },
        );
        let refusal = "Snappy + J (Focus left) would clash with Undo window move.";
        assert_eq!(error, Some(Refusal::from(refusal.to_owned())));
        assert_eq!(engine.settings(), settings);
        assert_eq!(engine.with_platform(|p| p.registered.clone()), registered);
        assert_eq!(engine.with_platform(|p| p.registrations), registrations);
        assert_eq!(saved_snappy_key(name), serde_json::json!(["ctrl", "alt", "meta"]));
        let refused = logged(name, "snappyKeyRefused");
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0]["reason"], refusal);

        let one_modifier = reply_to(
            &engine,
            Command::SetSnappyKey {
                modifiers: Mods::from_iter([Modifier::Meta]),
            },
        );
        assert_eq!(
            one_modifier.map(|refusal| refusal.message).as_deref(),
            Some("The Snappy key needs at least two modifiers.")
        );
        assert_eq!(engine.settings(), settings);
    });
}

#[test]
fn a_shortcut_recorded_without_the_snappy_key_keeps_its_keys_through_key_changes_and_restarts() {
    scenario(|| async {
        let name = "snappy-key-by-hand";
        let engine = live_engine(name, FakePlatform::default());
        let by_hand = KeyChord::new("KeyU", [Modifier::Ctrl, Modifier::Alt, Modifier::Shift]);
        let keys = by_hand.combo(SnappyKey::DEFAULT);
        command(
            &engine,
            Command::SetHistoryShortcut {
                step: HistoryStep::Undo,
                chord: Some(by_hand.clone()),
            },
        );
        let control_option_shift = control_option().with(Modifier::Shift);
        for modifiers in [control_option(), control_option_shift] {
            assert_eq!(reply_to(&engine, Command::SetSnappyKey { modifiers }), None);
            assert_eq!(engine.settings().history.undo, Some(by_hand.clone()));
            assert!(
                engine.with_platform(|p| p.registered.iter().any(|(_, combo)| *combo == keys)),
                "registered with the keys recorded"
            );
        }
        assert_eq!(
            engine.settings().snappy_key.modifiers(),
            control_option_shift,
            "the Snappy key is now exactly the modifiers the shortcut holds"
        );
        drop(engine);

        let restarted = relaunched(name, FakePlatform::default());
        assert_eq!(restarted.settings().history.undo, Some(by_hand));
        let saved = std::fs::read_to_string(paths_kept(name).settings).unwrap();
        let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            saved["history"]["undo"]["mods"],
            serde_json::json!(["shift", "ctrl", "alt"])
        );
    });
}

#[test]
fn a_snappy_key_change_onto_a_chord_the_os_holds_is_refused_naming_the_chord() {
    scenario(|| async {
        let name = "snappy-key-unavailable";
        let engine = live_engine(name, FakePlatform::default());
        engine.force_debug_log();
        let taken = snappy("Space").combo(SnappyKey::new(control_option()).unwrap());
        engine.with_platform(|p| p.taken_elsewhere.insert(taken));
        let settings = engine.settings();
        let registered = engine.with_platform(|p| p.registered.clone());

        let error = reply_to(
            &engine,
            Command::SetSnappyKey {
                modifiers: control_option(),
            },
        );
        let refusal =
            "Snappy + Space for Move to the nearest empty zone is already used by macOS or another app.";
        assert_eq!(
            error,
            Some(Refusal {
                message: refusal.to_owned(),
                setting: Some("Move to the nearest empty zone".to_owned()),
            }),
            "the reply names the shortcut, for the app to point to"
        );
        assert_eq!(engine.settings(), settings);
        assert_eq!(
            engine.with_platform(|p| p.registered.clone()),
            registered,
            "the shortcuts unregistered for probing are back"
        );
        assert_eq!(logged(name, "snappyKeyRefused")[0]["reason"], refusal);

        let option_command = Mods::from_iter([Modifier::Alt, Modifier::Meta]);
        let other = reply_to(
            &engine,
            Command::SetSnappyKey {
                modifiers: option_command,
            },
        );
        assert_eq!(other, None, "a key whose chords are free still works");
        let space = snappy("Space").combo(SnappyKey::new(option_command).unwrap());
        assert!(engine.with_platform(|p| p.registered.iter().any(|(_, combo)| *combo == space)));
    });
}

#[test]
fn a_snappy_key_change_onto_a_chord_snappy_itself_holds_is_not_refused() {
    scenario(|| async {
        let engine = live_engine("snappy-key-own-chord", FakePlatform::default());
        let halves = snappy_core::layout::preset("halves").unwrap().clone();
        let mut options = engine.settings().options(&halves);
        let shifted = KeyChord::snappy("Digit1", [Modifier::Shift]);
        options.zone_shortcuts.insert(halves.zones[0].id.clone(), shifted);
        command(
            &engine,
            Command::SaveLayout {
                layout: halves,
                options,
                display: "main".into(),
            },
        );
        let all_four = Mods::from_iter([Modifier::Ctrl, Modifier::Alt, Modifier::Shift, Modifier::Meta]);
        let held = snappy_core::keys::Combo {
            code: snappy_core::keys::KeyCode::new("Digit1"),
            mods: all_four,
        };
        let holds = |engine: &Engine<FakePlatform>| {
            engine.with_platform(|p| p.registered.iter().filter(|(_, combo)| *combo == held).count())
        };
        assert_eq!(holds(&engine), 1, "Snappy + Shift + 1 of halves");

        let error = reply_to(&engine, Command::SetSnappyKey { modifiers: all_four });
        assert_eq!(
            error, None,
            "the other layouts' Snappy + 1 becomes the chord halves holds now, which is no clash"
        );
        assert_eq!(holds(&engine), 1, "halves' zone 1, now Snappy + 1");
    });
}
