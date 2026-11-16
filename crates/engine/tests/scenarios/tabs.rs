//! Native tab groups in workspaces and the selection.

use snappy_core::geometry::Rect;
use snappy_core::keys::{KeyChord, Modifier};
use snappy_engine::LogEvent;
use snappy_fake::{FakePlatform, FakeWindow};
use snappy_ipc::Command;
use snappy_platform::{PlatformEvent, WindowId};

use crate::support::{
    LEFT_ZONE, RIGHT_ZONE, TABBED, TERMINAL, active, app_state, command, event, focus, focused, frame_of,
    hotkey_id, is_parked, logged, marks, paths_kept, pick_tab, place, press_default, remembered_workspaces,
    scenario, settle, started, terminal_focused, visited_tabs, window_counts, writes_to,
};

#[test]
fn a_selected_tabbed_window_takes_every_tab_along_and_torn_out_tabs_stay_there() {
    scenario(|| async {
        let name = "tabs-taken-along";
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

        let taken_along = logged(name, LogEvent::SelectionTakenAlong);
        assert_eq!(taken_along[0]["tabs"], serde_json::json!({ "3": [1, 2] }));
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
fn sending_a_tab_and_switching_takes_every_selected_tab_of_its_group_out_of_the_selection() {
    scenario(|| async {
        let name = "tabs-send-and-switch";
        let engine = visited_tabs(name, TABBED);
        engine.force_debug_log();
        let mut second = engine.settings().workspaces.items[1].clone();
        let chord = KeyChord::new("Digit2", [Modifier::Ctrl, Modifier::Alt]);
        second.send_and_switch_shortcut = Some(chord.clone());
        command(&engine, Command::UpdateWorkspace { workspace: second });
        press_default(&engine, "Enter").await;
        pick_tab(&engine, 2);
        assert_eq!(marks(&engine), [TABBED], "tab 3, selected, is behind tab 2 now");

        event(&engine, PlatformEvent::HotKey(hotkey_id(&engine, &chord)));
        settle(&engine).await;
        assert_eq!(active(&engine), "workspace-2");
        assert_eq!(frame_of(&engine, 2), TABBED, "the group went, showing tab 2");
        let removed = logged(name, LogEvent::SelectionRemoved);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0]["window"], 3);
        assert_eq!(removed[0]["reason"], "sent to another workspace");
        assert!(
            logged(name, LogEvent::SelectionTakenAlong).is_empty(),
            "nothing of the group is left to take along"
        );
        assert!(marks(&engine).is_empty());
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
        assert!(!logged(name, LogEvent::TabsUnified).is_empty());
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
