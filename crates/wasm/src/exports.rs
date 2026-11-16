//! Every exported function: JSON in, JSON out, through its handler in `api`.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::api;

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
    placeCopies => place_copies,
    mergeZones => merge_zones,
    handleAt => handle_at,
    handlePositions => handle_positions,
    dragMove => drag_move,
    dragResize => drag_resize,
    dragDivider => drag_divider,
    drawZone => draw_zone,
    cutPreview => cut_preview,
    cutZones => cut_zones,
    setField => set_field,
    chordLabels => chord_labels,
    chordProblem => chord_problem,
    recordChord => record_chord,
    expandChord => expand_chord,
    snappyKeyLabel => snappy_key_label,
    changeSnappyKey => change_snappy_key,
    snappyKeyProbes => snappy_key_probes,
    unavailableShortcut => unavailable_shortcut,
    modifierNames => modifier_names,
    shortcutSections => shortcut_sections,
    workspaceShortcutTitles => workspace_shortcut_titles,
    layoutShortcutTitles => layout_shortcut_titles,
    defaultShortcuts => default_shortcuts,
    limits => limits,
    presets => presets,
    isValidLayout => is_valid_layout,
    defaultSettings => default_settings,
    validateSettings => validate_settings,
    recordingConflict => recording_conflict_of,
    layoutConflict => layout_conflict,
    layoutForDisplay => layout_for_display,
    layoutOptions => layout_options,
    applyEdit => apply_edit,
    resetSettings => reset_settings,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use snappy_core::editor;
    use snappy_core::geometry::Rect;
    use snappy_core::keys::{KeyChord, Modifier, Os};
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
        assert_eq!(positions[0]["center"], json!({ "x": 12.0, "y": 10.0 }));
    }

    #[test]
    fn cuts_take_a_zone_or_the_whole_layout_as_their_scope() {
        let preview = answer(cut_preview(
            &json!({ "zones": halves(), "scope": "through", "axis": "y", "at": 0.334, "tolerance": 0.01 })
                .to_string(),
        ));
        assert_eq!(preview["zoneIds"].as_array().unwrap().len(), 2);
        assert_eq!(preview["guides"]["y"].as_array().unwrap().len(), 1);

        let first = halves()[0].id.clone();
        let cut = answer(cut_zones(
            &json!({
                "zones": halves(),
                "scope": { "zone": first },
                "axis": "y",
                "at": 0.5,
                "newIds": ["new"],
            })
            .to_string(),
        ));
        assert_eq!(cut.as_array().unwrap().len(), 3);
        assert_eq!(cut[1]["id"], "new");
        let through = answer(cut_zones(
            &json!({ "zones": halves(), "scope": "through", "axis": "y", "at": 0.5, "newIds": ["one"] })
                .to_string(),
        ));
        assert_eq!(through, Value::Null, "too few new IDs for both halves");
    }

    #[test]
    fn merging_answers_the_merged_layout_or_why_not() {
        let ids: Vec<String> = halves().iter().map(|zone| zone.id.clone()).collect();
        let chord = json!({ "code": "KeyM", "mods": ["snappy"] });
        let merged = answer(merge_zones(
            &json!({ "zones": halves(), "ids": ids, "zoneShortcuts": { ids[1].clone(): chord } }).to_string(),
        ));
        assert_eq!(merged["merged"]["zones"].as_array().unwrap().len(), 1);
        assert_eq!(merged["merged"]["kept"], ids[0]);
        assert_eq!(
            merged["merged"]["zoneShortcuts"],
            json!({ ids[0].clone(): chord })
        );

        let refused = answer(merge_zones(
            &json!({ "zones": halves(), "ids": [ids[0].clone()], "zoneShortcuts": {} }).to_string(),
        ));
        assert_eq!(refused["refused"], editor::MergeRefusal::TooFew.to_string());
    }

    #[test]
    fn copies_answer_their_places_or_why_not() {
        let placed = answer(place_copies(
            &json!({ "zones": [], "copied": [{ "x": 0.1, "y": 0.1, "width": 0.3, "height": 0.3 }] })
                .to_string(),
        ));
        assert_eq!(
            placed,
            json!({ "placed": [{ "x": 0.1, "y": 0.1, "width": 0.3, "height": 0.3 }] })
        );
        let refused = answer(place_copies(
            &json!({ "zones": halves(), "copied": [{ "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }] })
                .to_string(),
        ));
        assert_eq!(
            refused,
            json!({ "refused": editor::PlaceRefusal::NoRoom.to_string() })
        );
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
        assert_eq!(
            labels,
            json!({ "long": "Snappy + 1", "parts": { "snappyKey": true, "keys": "1" } })
        );
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
            "Control ⌃ + Shift ⇧"
        );
    }

    #[test]
    fn every_shortcut_title_comes_from_core() {
        let settings = Settings::defaults(Os::MacOs);
        let sections = answer(shortcut_sections(&json!({ "settings": settings }).to_string()));
        let titles: Vec<&Value> = sections
            .as_array()
            .unwrap()
            .iter()
            .map(|section| &section["title"])
            .collect();
        assert_eq!(
            titles,
            [
                "Move between zones",
                "Focus and selection",
                "Workspaces",
                "Window history"
            ]
        );
        assert_eq!(sections[0]["section"], "moveBetweenZones");
        assert_eq!(
            sections[0]["shortcuts"][0],
            json!({
                "id": { "kind": "navigation", "shortcut": "left" },
                "title": "Move window left",
                "chord": { "code": "ArrowLeft", "mods": ["snappy"], "label": "←" },
            })
        );
        assert_eq!(
            sections[0]["shortcuts"][4]["title"],
            "Move to the nearest empty zone"
        );
        assert_eq!(sections[1]["shortcuts"].as_array().unwrap().len(), 6);
        assert_eq!(
            sections[2]["shortcuts"][3]["title"],
            "Send window to next workspace"
        );
        assert_eq!(sections[3]["shortcuts"][0]["title"], "Undo window move");
        assert_eq!(sections[3]["shortcuts"][0]["chord"], Value::Null);

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
        let settings = Settings::defaults(Os::MacOs);
        let changed = answer(change_snappy_key(
            &json!({ "settings": settings, "modifiers": ["ctrl", "alt"], "platform": "macos" }).to_string(),
        ));
        let expected = settings
            .with_snappy_key([Modifier::Ctrl, Modifier::Alt].into_iter().collect(), Os::MacOs)
            .unwrap();
        assert_eq!(changed["changed"], to_value(&expected));
        let refused = answer(change_snappy_key(
            &json!({ "settings": settings, "modifiers": ["ctrl"], "platform": "macos" }).to_string(),
        ));
        assert_eq!(refused["refused"], "The Snappy key needs at least two modifiers.");
        let unchanged = answer(change_snappy_key(
            &json!({ "settings": settings, "modifiers": ["meta", "alt", "ctrl"], "platform": "macos" })
                .to_string(),
        ));
        assert_eq!(unchanged, "unchanged");

        let focus_up = json!({
            "id": { "kind": "navigation", "shortcut": "focusUp" },
            "title": "Focus up",
        });
        let recorded = json!({
            "chord": { "code": "KeyK", "mods": ["ctrl", "alt"] },
            "shortcut": focus_up,
            "platform": "windows",
        });
        assert_eq!(
            answer(unavailable_shortcut(&recorded.to_string())),
            json!({
                "message": "Ctrl + Alt + K for Focus up is already used by Windows or another app.",
                "shortcut": focus_up,
            })
        );

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
            json!({
                "message": "Snappy + Space for Move to the nearest empty zone is already used by macOS or another app.",
                "shortcut": {
                    "id": { "kind": "navigation", "shortcut": "nearestEmpty" },
                    "title": "Move to the nearest empty zone",
                },
            })
        );
    }

    #[test]
    fn labels_match_core_per_platform() {
        let chords = [
            KeyChord::new("Digit1", [Modifier::Ctrl, Modifier::Alt, Modifier::Shift]),
            KeyChord::snappy("KeyJ", [Modifier::Shift]),
        ];
        for chord in chords {
            for (name, platform) in [
                ("macos", Os::MacOs),
                ("windows", Os::Windows),
                ("linux", Os::Linux),
            ] {
                let labels = answer(chord_labels(
                    &json!({ "chord": chord, "platform": name }).to_string(),
                ));
                assert_eq!(labels["long"], chord.long_label(platform));
                let parts = chord.label_parts(platform);
                assert_eq!(
                    labels["parts"],
                    json!({ "snappyKey": parts.snappy_key, "keys": parts.keys })
                );
            }
        }
        let without = json!({ "chord": { "code": "KeyJ", "mods": ["shift"] }, "platform": "windows" });
        assert_eq!(
            answer(chord_problem(&without.to_string())),
            "Include Ctrl, Alt, or Win."
        );
        let with = json!({ "chord": { "code": "KeyJ", "mods": ["snappy"] }, "platform": "windows" });
        assert_eq!(answer(chord_problem(&with.to_string())), Value::Null);

        let names = answer(modifier_names(
            &json!({ "modifier": "alt", "platform": "macos" }).to_string(),
        ));
        assert_eq!(names, json!({ "name": "Option", "label": "Option ⌥" }));
    }

    #[test]
    fn edits_follow_core_and_say_when_nothing_changes() {
        let request = |edit: Value| {
            json!({ "settings": Settings::defaults(Os::MacOs), "edit": edit, "platform": "macos" })
                .to_string()
        };
        let added = answer(apply_edit(&request(json!({ "kind": "addWorkspace" }))));
        assert_eq!(added["edited"]["addedWorkspace"], "workspace-4");
        assert_eq!(
            added["edited"]["settings"]["workspaces"]["items"][3]["name"],
            "Workspace 4"
        );

        let preset = answer(apply_edit(&request(
            json!({ "kind": "removeLayout", "layout": "halves" }),
        )));
        assert_eq!(preset, "unchanged");

        let moved = answer(apply_edit(&request(
            json!({ "kind": "reorderWorkspaces", "from": [0], "to": 3 }),
        )));
        assert_eq!(
            moved["edited"]["settings"]["workspaces"]["items"][2]["id"],
            "workspace-1"
        );

        let unknown = answer(apply_edit(&request(
            json!({ "kind": "setLayoutEnabled", "layout": "missing", "enabled": true }),
        )));
        assert_eq!(unknown, json!({ "refused": "Unknown layout." }));
    }

    #[test]
    fn resets_match_core_and_say_whether_anything_changes() {
        let request = |settings: &Settings| {
            json!({
                "settings": settings,
                "reset": { "scope": "shortcuts", "section": "windowHistory" },
                "platform": "macos",
            })
            .to_string()
        };
        let defaults = Settings::defaults(Os::MacOs);
        assert_eq!(answer(reset_settings(&request(&defaults)))["changed"], false);

        let mut changed = defaults.clone();
        changed.history.undo = Some(KeyChord::snappy("KeyZ", []));
        let outcome = answer(reset_settings(&request(&changed)));
        assert_eq!(outcome["changed"], true);
        assert_eq!(outcome["settings"]["history"]["undo"], Value::Null);
        assert_eq!(outcome["message"], "Window history shortcuts reset.");
    }

    #[test]
    fn conflicts_match_core() {
        let settings = Settings::defaults(Os::MacOs);
        let request = |chord: KeyChord| {
            json!({
                "settings": settings,
                "shortcut": { "kind": "navigation", "shortcut": "left" },
                "chord": chord,
                "platform": "macos",
            })
            .to_string()
        };
        let quit = KeyChord::new("KeyQ", [Modifier::Meta]);
        let conflict = answer(recording_conflict_of(&request(quit.clone())));
        assert_eq!(
            conflict["message"],
            "Command ⌘ + Q is already assigned to a standard app command."
        );
        assert_eq!(conflict["shortcut"], Value::Null);
        let mut recorded = settings.clone();
        recorded.navigation.left = Some(quit);
        let validation = answer(validate_settings(
            &json!({ "settings": recorded, "platform": "macos" }).to_string(),
        ));
        assert_eq!(validation["invalid"], conflict["message"]);

        let conflict = answer(recording_conflict_of(&request(KeyChord::snappy(
            "ArrowRight",
            [],
        ))));
        assert_eq!(
            conflict["shortcut"],
            json!({
                "id": { "kind": "navigation", "shortcut": "right" },
                "title": "Move window right",
            })
        );
        assert_eq!(
            answer(recording_conflict_of(&request(KeyChord::snappy("KeyM", [])))),
            Value::Null
        );
    }

    #[test]
    fn validation_returns_normalized_settings() {
        let settings = Settings::defaults(Os::MacOs);
        let validation = answer(validate_settings(
            &json!({ "settings": settings, "platform": "macos" }).to_string(),
        ));
        let expected = settings.validate(Os::MacOs).unwrap();
        assert_eq!(validation["valid"], to_value(&expected));
    }

    #[test]
    fn limits_are_the_ones_core_enforces() {
        assert_eq!(
            answer(limits("{}")),
            json!({ "zones": 32, "workspaces": 32, "undoSteps": 50, "nameLength": 80 })
        );
    }

    #[test]
    fn malformed_requests_are_errors() {
        assert!(drag_move("{}").is_err());
        assert!(presets("{}").is_ok());
    }
}
