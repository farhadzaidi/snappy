//! The `snappy-core` rules the settings app needs, exported to WebAssembly.
//!
//! Every function takes one JSON request and returns JSON, so the TypeScript types mirror
//! the Rust serde shapes. Host builds expose the same functions for tests.

mod api;
mod gestures;
mod wire;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Parse a request, run its handler, and serialize the answer. Errors are messages for
/// developers; a well-typed caller never sees one.
fn call<Request, Response>(request: &str, handler: impl Fn(Request) -> Response) -> Result<String, String>
where
    Request: DeserializeOwned,
    Response: Serialize,
{
    let request = serde_json::from_str(request).map_err(|error| error.to_string())?;
    serde_json::to_string(&handler(request)).map_err(|error| error.to_string())
}

macro_rules! exports {
    ($($js_name:ident => $handler:ident),* $(,)?) => {
        $(
            #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = $js_name))]
            pub fn $handler(request: &str) -> Result<String, String> {
                call(request, api::$handler)
            }
        )*
    };
}

exports! {
    zoneFrames => zone_frames,
    dividers => dividers,
    isSeparated => is_separated,
    alignEdges => align_edges,
    duplicateRect => duplicate_rect,
    handleAt => handle_at,
    handlePositions => handle_positions,
    dragMove => drag_move,
    dragResize => drag_resize,
    dragDivider => drag_divider,
    drawZone => draw_zone,
    cutLine => cut_line,
    cutZone => cut_zone,
    setField => set_field,
    chordLabels => chord_labels,
    isValidChord => is_valid_chord,
    recordChord => record_chord,
    expandChord => expand_chord,
    snappyKeyLabel => snappy_key_label,
    changeSnappyKey => change_snappy_key,
    snappyKeyProbes => snappy_key_probes,
    modifierNames => modifier_names,
    shortcutTitles => shortcut_titles,
    workspaceShortcutTitles => workspace_shortcut_titles,
    layoutShortcutTitles => layout_shortcut_titles,
    defaultShortcuts => default_shortcuts,
    presets => presets,
    isValidLayout => is_valid_layout,
    defaultSettings => default_settings,
    validateSettings => validate_settings,
    settingsConflict => settings_conflict,
    layoutConflict => layout_conflict,
    layoutForDisplay => layout_for_display,
    layoutOptions => layout_options,
    newWorkspace => new_workspace,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use snappy_core::editor;
    use snappy_core::geometry::Rect;
    use snappy_core::keys::{KeyChord, Modifier, Platform};
    use snappy_core::layout::{Spacing, Zone, preset, zone_frame};
    use snappy_core::settings::Settings;

    use super::*;

    fn answer(result: Result<String, String>) -> Value {
        serde_json::from_str(&result.unwrap()).unwrap()
    }

    fn to_value<T: Serialize>(value: &T) -> Value {
        serde_json::to_value(value).unwrap()
    }

    fn halves() -> Vec<Zone> {
        preset("halves").unwrap().zones.clone()
    }

    #[test]
    fn preview_frames_match_core() {
        let work_area = Rect::new(0.0, 0.0, 400.0, 250.0);
        let request = json!({ "zones": halves(), "workArea": work_area, "spacingFactor": 0.4 });
        let frames = answer(zone_frames(&request.to_string()));
        let expected: Vec<Rect> = halves()
            .iter()
            .map(|zone| zone_frame(zone.rect, work_area, Spacing::STANDARD.scaled(0.4)))
            .collect();
        assert_eq!(frames, to_value(&expected));
    }

    #[test]
    fn dividers_match_core() {
        let found = answer(dividers(&json!({ "zones": halves() }).to_string()));
        let expected = &editor::dividers(&halves())[0];
        assert_eq!(found[0]["zoneId"], expected.zone_id);
        assert_eq!(found[0]["axis"], "x");
        assert_eq!(found[0]["line"], expected.line);
        assert_eq!(found[0]["id"], expected.id());
    }

    #[test]
    fn handles_use_wire_names() {
        let request = json!({ "point": { "x": 3.0, "y": 3.0 }, "size": { "width": 300.0, "height": 200.0 } });
        assert_eq!(answer(handle_at(&request.to_string())), "northWest");
        let positions = answer(handle_positions(
            &json!({ "size": { "width": 100.0, "height": 40.0 } }).to_string(),
        ));
        assert_eq!(positions.as_array().unwrap().len(), 8);
        assert_eq!(positions[0]["center"], json!({ "x": 7.0, "y": 7.0 }));
    }

    #[test]
    fn recorded_chords_holding_the_snappy_keys_modifiers_come_back_as_the_snappy_key() {
        let recorded = json!({
            "chord": { "code": "Digit1", "mods": ["shift", "ctrl", "alt", "meta"], "label": "1" },
            "snappyKey": ["ctrl", "alt", "meta"],
        });
        let stored = answer(record_chord(&recorded.to_string()));
        assert_eq!(
            stored,
            json!({ "code": "Digit1", "mods": ["snappy", "shift"], "label": "1" })
        );
        let partial = json!({
            "chord": { "code": "KeyK", "mods": ["ctrl", "alt"] },
            "snappyKey": ["ctrl", "alt", "meta"],
        });
        assert_eq!(
            answer(record_chord(&partial.to_string())),
            json!({ "code": "KeyK", "mods": ["ctrl", "alt"] })
        );
        assert_eq!(
            answer(snappy_key_label(
                &json!({ "snappyKey": ["ctrl", "alt", "meta"], "platform": "windows" }).to_string()
            )),
            "Ctrl + Alt + Win"
        );
        let labels = answer(chord_labels(
            &json!({ "chord": { "code": "Digit1", "mods": ["snappy"] }, "platform": "macos" }).to_string(),
        ));
        assert_eq!(labels, json!({ "compact": "Snappy + 1", "long": "Snappy + 1" }));
    }

    #[test]
    fn a_chosen_snappy_key_records_expands_and_labels_with_its_own_modifiers() {
        let control_shift = json!(["ctrl", "shift"]);
        let recorded = json!({
            "chord": { "code": "KeyJ", "mods": ["shift", "ctrl"] },
            "snappyKey": control_shift,
        });
        assert_eq!(
            answer(record_chord(&recorded.to_string())),
            json!({ "code": "KeyJ", "mods": ["snappy"] })
        );
        let symbolic =
            json!({ "chord": { "code": "KeyJ", "mods": ["snappy", "alt"] }, "snappyKey": control_shift });
        assert_eq!(
            answer(expand_chord(&symbolic.to_string())),
            json!({ "code": "KeyJ", "mods": ["shift", "ctrl", "alt"] })
        );
        assert_eq!(
            answer(snappy_key_label(
                &json!({ "snappyKey": control_shift, "platform": "macos" }).to_string()
            )),
            "Control + Shift"
        );
    }

    #[test]
    fn every_shortcut_title_comes_from_core_by_its_settings_field() {
        let titles = answer(shortcut_titles("{}"));
        assert_eq!(titles["navigation"]["left"], "Move window left");
        assert_eq!(
            titles["navigation"]["nearestEmpty"],
            "Move to the nearest empty zone"
        );
        assert_eq!(titles["navigation"]["clearSelection"], "Clear selection");
        assert_eq!(titles["navigation"].as_object().unwrap().len(), 11);
        assert_eq!(
            titles["workspaceSteps"]["sendNext"],
            "Send window to next workspace"
        );
        assert_eq!(titles["history"]["undo"], "Undo window move");

        let own = answer(workspace_shortcut_titles(
            &json!({ "workspace": "Work" }).to_string(),
        ));
        assert_eq!(own["switchShortcut"], "Switch to Work");
        assert_eq!(own["sendShortcut"], "Send window to Work");
        assert_eq!(own["sendAndSwitchShortcut"], "Send window to Work and switch");
        assert_eq!(own.as_object().unwrap().len(), 3);

        let layout = answer(layout_shortcut_titles(
            &json!({ "layout": "Halves", "zones": 2 }).to_string(),
        ));
        assert_eq!(
            layout,
            json!({ "select": "Select Halves", "zones": ["Zone 1 of Halves", "Zone 2 of Halves"] })
        );

        let help = answer(default_shortcuts(&json!({ "platform": "windows" }).to_string()));
        assert_eq!(help[0]["title"], "Move to the neighboring zone");
        assert_eq!(
            help[0]["chord"],
            json!({ "code": "ArrowLeft", "mods": ["snappy"], "label": "← → ↑ ↓" })
        );
    }

    #[test]
    fn changing_the_snappy_key_matches_core() {
        let settings = Settings::defaults(Platform::MacOs);
        let changed = answer(change_snappy_key(
            &json!({ "settings": settings, "modifiers": ["ctrl", "alt"], "platform": "macos" }).to_string(),
        ));
        let expected = settings
            .with_snappy_key(
                [Modifier::Ctrl, Modifier::Alt].into_iter().collect(),
                Platform::MacOs,
            )
            .unwrap();
        assert_eq!(changed["valid"], to_value(&expected));
        let refused = answer(change_snappy_key(
            &json!({ "settings": settings, "modifiers": ["ctrl"], "platform": "macos" }).to_string(),
        ));
        assert_eq!(refused["invalid"], "The Snappy key needs at least two modifiers.");

        let probes = answer(snappy_key_probes(
            &json!({ "settings": expected, "platform": "macos" }).to_string(),
        ));
        let space = probes
            .as_array()
            .unwrap()
            .iter()
            .find(|probe| probe["chord"]["code"] == "Space")
            .unwrap();
        assert_eq!(space["chord"]["mods"], json!(["ctrl", "alt"]));
        assert_eq!(
            space["refusal"],
            "Snappy + Space for Move to the nearest empty zone would be Control + Option + Space, which macOS or another app already uses."
        );
    }

    #[test]
    fn labels_match_core_per_platform() {
        let chord = KeyChord::new("Digit1", [Modifier::Ctrl, Modifier::Alt, Modifier::Shift]);
        for (name, platform) in [
            ("macos", Platform::MacOs),
            ("windows", Platform::Windows),
            ("linux", Platform::Linux),
        ] {
            let labels = answer(chord_labels(
                &json!({ "chord": chord, "platform": name }).to_string(),
            ));
            assert_eq!(labels["compact"], chord.compact_label(platform));
            assert_eq!(labels["long"], chord.long_label(platform));
        }
        let names = answer(modifier_names(
            &json!({ "modifier": "alt", "platform": "macos" }).to_string(),
        ));
        assert_eq!(
            names,
            json!({ "name": "Option", "short": "Opt", "label": "Option ⌥" })
        );
    }

    #[test]
    fn conflicts_match_core() {
        let mut settings = Settings::defaults(Platform::MacOs);
        settings.workspaces.previous = Some(KeyChord::new("KeyQ", [Modifier::Meta]));
        let conflict = answer(settings_conflict(
            &json!({ "settings": settings, "platform": "macos" }).to_string(),
        ));
        assert_eq!(
            conflict,
            "Command + Q is already assigned to a standard app command."
        );
        let validation = answer(validate_settings(
            &json!({ "settings": settings, "platform": "macos" }).to_string(),
        ));
        assert_eq!(validation["invalid"], conflict);
    }

    #[test]
    fn validation_returns_normalized_settings() {
        let settings = Settings::defaults(Platform::MacOs);
        let validation = answer(validate_settings(
            &json!({ "settings": settings, "platform": "macos" }).to_string(),
        ));
        let expected = settings.validate(Platform::MacOs).unwrap();
        assert_eq!(validation["valid"], to_value(&expected));
    }

    #[test]
    fn malformed_requests_are_errors() {
        assert!(drag_move("{}").is_err());
        assert!(presets("{}").is_ok());
    }
}
