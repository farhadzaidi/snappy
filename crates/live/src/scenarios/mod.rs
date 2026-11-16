//! Live scenarios. Each opens its own fixture windows, acts through real input, and checks
//! where the windows really ended up.

mod caps_lock;
mod focus;
mod moves;
mod selection;
mod shell;
mod snappy_key;
mod support;
mod swaps;
mod tabs;
mod workspaces;

use snappy_core::keys::Platform;

use caps_lock::caps_lock_as_snappy_key;
use focus::{focus_directions, focus_zones};
use moves::{
    arrow_shortcut, drag_snaps, full_screen_refused, grid_snap, minimum_size, nearest_empty_zone,
    zone_shortcut,
};
use selection::{
    place_selection, select_along_by_number, select_two_along_by_step, select_without_workspaces, unselect,
};
use shell::{menu_bar_item, menu_bar_menu_opens, overlay_floats};
use snappy_key::chosen_snappy_key;
pub use snappy_key::{free_snappy_key, with_free_snappy_key};
pub use support::{Context, TITLE_BAR_GRIP};
use swaps::{
    floating_keyboard_swap, floating_swap, floating_swap_fallback, inverted_drag, keyboard_swap, swap_drag,
};
use tabs::tabs_along;
use workspaces::{
    display_change_keeps_windows_parked, quit_returns_windows, restart_keeps_workspaces, send_and_stay_batch,
    send_and_switch, send_to_workspace, step_workspaces, workspace_switch,
};

/// The platform the scenarios run on, for names and the chords it keeps.
pub const PLATFORM: Platform = if cfg!(target_os = "macos") {
    Platform::MacOs
} else if cfg!(windows) {
    Platform::Windows
} else {
    Platform::Linux
};

/// What a passing scenario measured.
#[derive(Debug, Default)]
pub struct Outcome {
    pub elapsed_ms: Option<f64>,
    pub note: Option<String>,
    /// Why the scenario can't apply here, when it can't.
    pub skipped: Option<String>,
}

impl Outcome {
    pub fn skipped(reason: &str) -> Outcome {
        Outcome {
            skipped: Some(reason.to_owned()),
            ..Outcome::default()
        }
    }
}

pub type Scenario = fn(&mut Context) -> Result<Outcome, String>;

pub const ALL: &[(&str, Scenario)] = &[
    ("shift_drag_snaps_to_the_zone", drag_snaps),
    ("zone_shortcut_moves_the_focused_window", zone_shortcut),
    ("arrow_shortcut_moves_to_the_next_zone", arrow_shortcut),
    ("zone_shortcut_into_an_occupied_zone_swaps", keyboard_swap),
    ("dropping_on_an_occupied_zone_swaps", swap_drag),
    ("the_swap_key_stacks_instead_of_swapping", inverted_drag),
    ("grid_sized_windows_snap_without_waiting", grid_snap),
    ("minimum_sizes_are_respected", minimum_size),
    (
        "a_floating_window_sends_the_occupant_to_the_zone_under_it",
        floating_swap,
    ),
    (
        "a_floating_window_moved_by_keyboard_sends_the_occupant_to_the_zone_under_it",
        floating_keyboard_swap,
    ),
    (
        "a_floating_window_over_the_destination_swaps_into_its_frame",
        floating_swap_fallback,
    ),
    (
        "the_nearest_empty_zone_shortcut_skips_the_occupied_zone",
        nearest_empty_zone,
    ),
    (
        "overlays_float_above_windows_without_taking_focus",
        overlay_floats,
    ),
    ("the_menu_bar_item_is_shown", menu_bar_item),
    ("the_menu_bar_item_opens_its_menu_every_time", menu_bar_menu_opens),
    (
        "switching_workspaces_by_number_parks_and_returns_windows",
        workspace_switch,
    ),
    ("previous_and_next_shortcuts_cycle_workspaces", step_workspaces),
    ("sending_parks_the_window_in_its_new_workspace", send_to_workspace),
    ("send_and_switch_follows_the_window", send_and_switch),
    (
        "sending_to_the_next_workspace_three_times_sends_three_windows_and_stays",
        send_and_stay_batch,
    ),
    (
        "a_selected_window_goes_along_to_the_workspace_switched_to_by_number",
        select_along_by_number,
    ),
    (
        "two_selected_windows_go_along_to_the_next_workspace",
        select_two_along_by_step,
    ),
    ("selecting_a_selected_window_again_unselects_it", unselect),
    (
        "a_tabbed_window_taken_along_keeps_every_tab_there_once_they_are_dragged_out",
        tabs_along,
    ),
    ("focus_shortcuts_move_focus_between_windows", focus_directions),
    (
        "focus_shortcuts_move_between_zoned_windows_past_a_floating_one",
        focus_zones,
    ),
    (
        "space_places_two_selected_floating_windows_into_empty_zones",
        place_selection,
    ),
    (
        "selecting_works_with_workspaces_off_and_clearing_removes_the_outlines",
        select_without_workspaces,
    ),
    (
        "a_chosen_snappy_key_moves_windows_with_its_own_modifiers",
        chosen_snappy_key,
    ),
    (
        "caps_lock_held_is_the_snappy_key_and_tapped_alone_toggles_caps_lock",
        caps_lock_as_snappy_key,
    ),
    (
        "restarting_keeps_every_window_in_its_workspace_and_quitting_still_returns_them",
        restart_keeps_workspaces,
    ),
    (
        "a_window_parked_while_the_display_changes_size_stays_parked_without_a_warning",
        display_change_keeps_windows_parked,
    ),
    ("quitting_returns_parked_windows", quit_returns_windows),
    ("full_screen_windows_are_never_moved", full_screen_refused),
];
