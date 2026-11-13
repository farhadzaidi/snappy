//! Live scenarios. Each opens its own fixture windows, acts through real input, and checks
//! where the windows really ended up.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{KeyChord, Mods};
use snappy_core::layout::{Spacing, preset, zone_frame};
use snappy_core::navigate::Direction;
use snappy_core::settings::{FocusShortcut, Settings, Workspace, WorkspaceStep};
use snappy_platform::Display;

use crate::fixture::Fixture;
use crate::input::{Input, pause};
use crate::screen::{FLOATING_LAYER, NORMAL_LAYER, on_screen_windows, screenshot};
use crate::session::Daemon;

/// Window frames this close count as equal. Fixture frames and Snappy's frames come from
/// the same window server, so only rounding separates them.
const FRAME_TOLERANCE: f64 = 2.0;
/// How long a move may take before a scenario fails.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(3);
/// Where a title bar can be grabbed, below the window's top edge.
pub const TITLE_BAR_GRIP: f64 = 14.0;
/// A snap that needs no settle waits finishes well within this, inside Snappy.
const UNWAITED_SNAP_MS: f64 = 50.0;
/// The layout the run's settings make the default.
const LAYOUT: &str = "halves";

pub struct Context<'a> {
    pub daemon: &'a Daemon,
    /// The settings the daemon runs with; every shortcut and modifier comes from them.
    pub settings: &'a Settings,
    pub fixture: &'a mut Fixture,
    pub input: &'a mut dyn Input,
    pub display: Display,
    /// The daemon's debug log, for what Snappy itself measured.
    pub debug_log: PathBuf,
    /// The output folder, where scenarios save screenshots of what they show.
    pub out: PathBuf,
    opened: Vec<String>,
    /// When the action being timed began.
    began: Instant,
}

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
    (
        "switching_workspaces_by_number_parks_and_returns_windows",
        workspace_switch,
    ),
    ("previous_and_next_shortcuts_cycle_workspaces", step_workspaces),
    ("sending_parks_the_window_in_its_new_workspace", send_to_workspace),
    ("move_here_and_switch_follows_the_window", move_here_and_switch),
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
        "restarting_keeps_every_window_in_its_workspace_and_quitting_still_returns_them",
        restart_keeps_workspaces,
    ),
    ("quitting_returns_parked_windows", quit_returns_windows),
    ("full_screen_windows_are_never_moved", full_screen_refused),
];

impl<'a> Context<'a> {
    pub fn new(
        daemon: &'a Daemon,
        settings: &'a Settings,
        fixture: &'a mut Fixture,
        input: &'a mut dyn Input,
        display: Display,
        debug_log: PathBuf,
        out: PathBuf,
    ) -> Self {
        Self {
            daemon,
            settings,
            fixture,
            input,
            display,
            debug_log,
            out,
            opened: Vec::new(),
            began: Instant::now(),
        }
    }

    /// The halves layout's zones on the display, as Snappy computes them.
    pub fn zones(&self) -> Vec<Rect> {
        preset(LAYOUT)
            .map(|layout| {
                layout
                    .zones
                    .iter()
                    .map(|zone| zone_frame(zone.rect, self.display.work_area, Spacing::STANDARD))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn zone(&self, index: usize) -> Result<Rect, String> {
        self.zones()
            .get(index)
            .copied()
            .ok_or_else(|| format!("no zone {index}"))
    }

    fn open(&mut self, id: &str, content: Rect) -> Result<Rect, String> {
        self.open_with(id, content, None, None)
    }

    fn open_with(
        &mut self,
        id: &str,
        content: Rect,
        min: Option<Size>,
        grid: Option<Size>,
    ) -> Result<Rect, String> {
        let frame = self.fixture.open(id, content, min, grid)?;
        self.opened.push(id.to_owned());
        self.fixture.focus(id)?;
        pause(300);
        Ok(frame)
    }

    /// Close every window this scenario opened.
    pub fn close_all(&mut self) {
        for id in std::mem::take(&mut self.opened) {
            let _ = self.fixture.close(&id);
        }
        pause(300);
    }

    fn focus(&mut self, id: &str) -> Result<(), String> {
        self.fixture.focus(id)?;
        pause(250);
        Ok(())
    }

    /// The active workspace, as the daemon saved it.
    fn active_workspace(&self) -> Result<String, String> {
        let path = self.daemon.home().join("settings.json");
        let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let settings: serde_json::Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        settings["workspaces"]["activeId"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "the settings have no active workspace".to_owned())
    }

    /// Wait for the daemon to save `expected` as the active workspace.
    fn wait_active(&self, expected: &str) -> Result<(), String> {
        let started = Instant::now();
        loop {
            let active = self.active_workspace()?;
            if active == expected {
                return Ok(());
            }
            if started.elapsed() > SETTLE_TIMEOUT {
                return Err(format!("the active workspace is {active}, not {expected}"));
            }
            pause(20);
        }
    }

    fn is_parked(&self, frame: Rect) -> bool {
        parked(self.display.frame, frame) || !shown(self.fixture.pid(), frame)
    }

    fn wait_parked(&mut self, id: &str) -> Result<f64, String> {
        let (display, pid) = (self.display.frame, self.fixture.pid());
        self.wait_for(id, "a parked position", |frame| {
            parked(display, frame) || !shown(pid, frame)
        })
        .map(|(_, elapsed)| elapsed)
    }

    /// Wait until `id` is back on screen at `target`.
    fn wait_shown(&mut self, id: &str, target: Rect) -> Result<f64, String> {
        let pid = self.fixture.pid();
        let what = format!("{} on screen", describe(target));
        self.wait_for(id, &what, |frame| near(frame, target) && shown(pid, frame))
            .map(|(_, elapsed)| elapsed)
    }

    /// Fail unless `id` is on screen at `before`, as a selected window taken along stays.
    fn stayed(&mut self, id: &str, before: Rect) -> Result<(), String> {
        let after = self.fixture.frame(id)?;
        if self.is_parked(after) || !near(after, before) {
            return Err(format!("{id} moved to {}", describe(after)));
        }
        Ok(())
    }

    /// Wait until the daemon shows, or no longer shows, the surface over the work area
    /// that outlines the selected windows.
    fn wait_outlines(&self, shown: bool) -> Result<(), String> {
        let started = Instant::now();
        loop {
            let covering = daemon_windows_above(self, FLOATING_LAYER)?
                .iter()
                .any(|frame| frame.contains_rect(self.display.work_area));
            if covering == shown {
                return Ok(());
            }
            if started.elapsed() > SETTLE_TIMEOUT {
                return Err(if shown {
                    "the selected window has no outline".to_owned()
                } else {
                    "the outline stayed on screen".to_owned()
                });
            }
            pause(20);
        }
    }

    /// Records of `event` in the daemon's debug log so far.
    fn logged(&self, event: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.debug_log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|record| record["event"] == event)
            .collect()
    }

    /// Snap records in the daemon's debug log so far.
    fn snaps(&self) -> Vec<serde_json::Value> {
        self.logged("snap")
    }

    /// The hovered zone's captions as the debug log recorded them, one per change.
    fn captions(&self) -> Vec<Option<String>> {
        self.logged("overlayCaption")
            .iter()
            .map(|record| record["caption"].as_str().map(str::to_owned))
            .collect()
    }

    /// Wait for the debug log to say the hovered zone shows `expected`, after the first
    /// `seen` caption changes.
    fn wait_caption(&self, seen: usize, expected: &str) -> Result<(), String> {
        let started = Instant::now();
        loop {
            let captions = self.captions();
            let last = captions.get(seen..).and_then(<[_]>::last).cloned().flatten();
            if last.as_deref() == Some(expected) {
                return Ok(());
            }
            if started.elapsed() > SETTLE_TIMEOUT {
                return Err(format!("the hovered zone's caption is {last:?}, not {expected}"));
            }
            pause(20);
        }
    }

    /// Wait for Snappy to log the snap that follows the first `seen` ones. Returns its
    /// outcome and how long Snappy took to settle it.
    fn finished_snap(&self, seen: usize) -> Result<(String, f64), String> {
        let started = Instant::now();
        loop {
            if let Some(record) = self.snaps().get(seen) {
                let outcome = record["outcome"].as_str().unwrap_or_default().to_owned();
                let settled = record["steps"]
                    .as_array()
                    .and_then(|steps| steps.last()?.get(2)?.as_f64())
                    .unwrap_or(0.0);
                return Ok((outcome, settled));
            }
            if started.elapsed() > SETTLE_TIMEOUT {
                return Err("Snappy never logged the snap".to_owned());
            }
            pause(10);
        }
    }

    /// Start timing an action: elapsed times count from here.
    fn begin(&mut self) {
        self.began = Instant::now();
    }

    /// Wait until `id`'s frame satisfies `done`. Returns the frame and the time since the
    /// action began.
    fn wait_for(&mut self, id: &str, what: &str, done: impl Fn(Rect) -> bool) -> Result<(Rect, f64), String> {
        let started = Instant::now();
        loop {
            let frame = self.fixture.frame(id)?;
            if done(frame) {
                return Ok((frame, self.began.elapsed().as_secs_f64() * 1000.0));
            }
            if started.elapsed() > SETTLE_TIMEOUT {
                return Err(format!("{id} never reached {what}; it is at {}", describe(frame)));
            }
            pause(5);
        }
    }

    fn wait_in(&mut self, id: &str, target: Rect) -> Result<f64, String> {
        self.wait_for(id, &describe(target), |frame| near(frame, target))
            .map(|(_, elapsed)| elapsed)
    }

    /// The point in `frame`'s title bar a user would grab.
    fn grip(frame: Rect) -> Point {
        Point::new(frame.mid_x(), frame.y + TITLE_BAR_GRIP)
    }

    /// Drag `id` by its title bar so that the pointer ends at `to`. Timing starts at the
    /// release, when Snappy takes over.
    fn drag_window(&mut self, id: &str, to: Point, mods: Mods) -> Result<(), String> {
        let frame = self.fixture.frame(id)?;
        self.input.drag(Self::grip(frame), to, mods, 12)?;
        self.begin();
        Ok(())
    }

    /// Press a chord. Timing starts here.
    fn press(&mut self, chord: &KeyChord) -> Result<(), String> {
        self.begin();
        self.input.chord(&chord.combo())
    }

    /// The shortcut for zone `index` of the halves layout.
    fn zone_key(&self, index: usize) -> Result<KeyChord, String> {
        let layout = self
            .settings
            .layout(LAYOUT)
            .ok_or("the settings have no halves layout")?;
        let zone = layout
            .zones
            .get(index)
            .ok_or_else(|| format!("no zone {index}"))?;
        self.settings
            .options(layout)
            .zone_shortcuts
            .get(&zone.id)
            .cloned()
            .ok_or_else(|| format!("zone {} has no shortcut", index + 1))
    }

    fn arrow_key(&self, direction: Direction) -> Result<KeyChord, String> {
        self.settings
            .navigation
            .get(direction)
            .cloned()
            .ok_or_else(|| format!("moving {direction:?} has no shortcut"))
    }

    fn nearest_empty_key(&self) -> Result<KeyChord, String> {
        self.settings
            .navigation
            .nearest_empty
            .clone()
            .ok_or_else(|| "moving to the nearest empty zone has no shortcut".to_owned())
    }

    /// The modifiers held to snap while dragging.
    fn drag_mods(&self) -> Mods {
        Mods::from_iter([self.settings.snapping.drag_modifier.modifier()])
    }

    /// The modifiers held to drop with the opposite of "When the zone is taken": the drag
    /// modifier plus the swap key.
    fn inverting_mods(&self) -> Result<Mods, String> {
        let drag = self.settings.snapping.drag_modifier.modifier();
        let swap = self
            .settings
            .snapping
            .swap_modifier
            .effective(drag)
            .modifier()
            .ok_or("the settings have no swap key")?;
        Ok(Mods::from_iter([drag, swap]))
    }

    /// One of workspace `number`'s shortcuts, picked by `which`.
    fn workspace_key(
        &self,
        number: usize,
        which: fn(&Workspace) -> &Option<KeyChord>,
    ) -> Result<KeyChord, String> {
        self.settings
            .workspaces
            .items
            .get(number - 1)
            .and_then(|workspace| which(workspace).clone())
            .ok_or_else(|| format!("workspace {number} lacks the shortcut"))
    }

    fn switch_key(&self, number: usize) -> Result<KeyChord, String> {
        self.workspace_key(number, |workspace| &workspace.switch_shortcut)
    }

    fn send_key(&self, number: usize) -> Result<KeyChord, String> {
        self.workspace_key(number, |workspace| &workspace.send_shortcut)
    }

    fn send_and_switch_key(&self, number: usize) -> Result<KeyChord, String> {
        self.workspace_key(number, |workspace| &workspace.send_and_switch_shortcut)
    }

    fn step_key(&self, step: WorkspaceStep) -> Result<KeyChord, String> {
        self.settings
            .workspaces
            .step(step)
            .cloned()
            .ok_or_else(|| format!("{} has no shortcut", step.title()))
    }

    fn focus_key(&self, shortcut: FocusShortcut) -> Result<KeyChord, String> {
        self.settings
            .navigation
            .focus_shortcut(shortcut)
            .cloned()
            .ok_or_else(|| format!("{} has no shortcut", shortcut.title()))
    }

    /// Select the windows `ids`, focusing each in turn, and wait for their outlines.
    fn select(&mut self, ids: &[&str]) -> Result<(), String> {
        let select = self.focus_key(FocusShortcut::SelectWindow)?;
        for id in ids {
            self.focus(id)?;
            self.press(&select)?;
            pause(300);
        }
        self.wait_outlines(true)
    }
}

/// True when `frame` sits in a parking position: off the right of the display, where
/// macOS parks with a sliver showing, or entirely off its left, where Windows and X11
/// park.
fn parked(display: Rect, frame: Rect) -> bool {
    frame.x >= display.max_x() - 2.0 || frame.max_x() <= display.min_x()
}

/// True when the window server shows a window of process `pid` covering most of `frame`.
/// Parking on a native desktop leaves the frame alone and only takes the window off
/// screen. When the window list can't be read, only positions count.
fn shown(pid: u32, frame: Rect) -> bool {
    let Ok(windows) = on_screen_windows() else {
        return true;
    };
    windows.iter().any(|window| {
        i64::from(window.pid) == i64::from(pid)
            && window.layer == NORMAL_LAYER
            && window.frame.intersection_area(frame) > frame.area() / 2.0
    })
}

fn near(a: Rect, b: Rect) -> bool {
    [(a.x, b.x), (a.y, b.y), (a.width, b.width), (a.height, b.height)]
        .iter()
        .all(|(x, y)| (x - y).abs() <= FRAME_TOLERANCE)
}

fn describe(frame: Rect) -> String {
    format!(
        "({:.0}, {:.0}, {:.0} × {:.0})",
        frame.x, frame.y, frame.width, frame.height
    )
}

fn timed(elapsed: f64) -> Outcome {
    Outcome {
        elapsed_ms: Some(elapsed),
        ..Outcome::default()
    }
}

/// A window somewhere in the middle of the display, away from any zone's exact frame.
fn floating(display: &Display, offset: f64) -> Rect {
    let area = display.work_area;
    Rect::new(
        area.x + area.width * 0.2 + offset,
        area.y + 80.0 + offset,
        area.width * 0.35,
        area.height * 0.4,
    )
}

/// Window `index` of three side by side, apart from each other: where parking leaves frames
/// alone, a window counts as shown while another of the fixture's covers its frame.
fn side_by_side(display: &Display, index: usize) -> Rect {
    let area = display.work_area;
    let width = area.width * 0.28;
    Rect::new(
        area.x + 40.0 + (width + 40.0) * index as f64,
        area.y + 120.0,
        width,
        area.height * 0.4,
    )
}

fn drag_snaps(context: &mut Context) -> Result<Outcome, String> {
    let left = context.zone(0)?;
    let start = floating(&context.display, 0.0);
    context.open("drag", start)?;
    context.drag_window("drag", left.center(), context.drag_mods())?;
    context.wait_in("drag", left).map(timed)
}

fn zone_shortcut(context: &mut Context) -> Result<Outcome, String> {
    let right = context.zone(1)?;
    context.open("key", floating(&context.display, 20.0))?;
    context.press(&context.zone_key(1)?)?;
    context.wait_in("key", right).map(timed)
}

fn arrow_shortcut(context: &mut Context) -> Result<Outcome, String> {
    let (left, right) = (context.zone(0)?, context.zone(1)?);
    context.open("arrow", floating(&context.display, 40.0))?;
    context.press(&context.zone_key(0)?)?;
    context.wait_in("arrow", left)?;
    context.press(&context.arrow_key(Direction::Right)?)?;
    context.wait_in("arrow", right).map(timed)
}

/// Window `a` in the left zone and `b` in the right one, with `a` focused.
fn two_zoned(context: &mut Context, a: &str, b: &str) -> Result<(Rect, Rect), String> {
    let (left, right) = (context.zone(0)?, context.zone(1)?);
    context.open(b, floating(&context.display, 60.0))?;
    context.press(&context.zone_key(1)?)?;
    context.wait_in(b, right)?;
    context.open(a, floating(&context.display, 80.0))?;
    context.press(&context.zone_key(0)?)?;
    context.wait_in(a, left)?;
    Ok((left, right))
}

fn keyboard_swap(context: &mut Context) -> Result<Outcome, String> {
    let (left, right) = two_zoned(context, "ka", "kb")?;
    context.focus("ka")?;
    context.press(&context.zone_key(1)?)?;
    let elapsed = context.wait_in("ka", right)?;
    context.wait_in("kb", left)?;
    Ok(timed(elapsed))
}

fn swap_drag(context: &mut Context) -> Result<Outcome, String> {
    let (left, right) = two_zoned(context, "da", "db")?;
    context.drag_window("da", right.center(), context.drag_mods())?;
    let elapsed = context.wait_in("da", right)?;
    context.wait_in("db", left)?;
    Ok(timed(elapsed))
}

/// With swapping the default, the swap key makes the drop cover the occupant instead.
fn inverted_drag(context: &mut Context) -> Result<Outcome, String> {
    let (_, right) = two_zoned(context, "sa", "sb")?;
    let inverted = context.inverting_mods()?;
    context.drag_window("sa", right.center(), inverted)?;
    let elapsed = context.wait_in("sa", right)?;
    pause(400);
    let occupant = context.fixture.frame("sb")?;
    if !near(occupant, right) {
        return Err(format!(
            "the occupant moved to {} instead of staying under the dropped window",
            describe(occupant)
        ));
    }
    Ok(timed(elapsed))
}

/// Terminals size in character cells, so they can't land on the exact zone size. The snap
/// must still finish at once instead of waiting out the settle schedule.
fn grid_snap(context: &mut Context) -> Result<Outcome, String> {
    if !context.fixture.supports_size_steps() {
        return Ok(Outcome::skipped(
            "Wayland has no way for an app to ask to size in steps; the Xwayland run covers it",
        ));
    }
    let left = context.zone(0)?;
    let cell = Size::new(7.0, 17.0);
    context.open_with("grid", floating(&context.display, 100.0), None, Some(cell))?;
    let seen = context.snaps().len();
    context.press(&context.zone_key(0)?)?;
    let (frame, elapsed) = context.wait_for("grid", "the left zone's origin", |frame| {
        (frame.x - left.x).abs() <= FRAME_TOLERANCE && (frame.y - left.y).abs() <= FRAME_TOLERANCE
    })?;
    let off_by = Size::new(
        (frame.width - left.width).abs(),
        (frame.height - left.height).abs(),
    );
    if off_by.width > cell.width || off_by.height > cell.height {
        return Err(format!(
            "landed at {}, more than a cell from {}",
            describe(frame),
            describe(left)
        ));
    }
    let (_, settled) = context.finished_snap(seen)?;
    if settled > UNWAITED_SNAP_MS {
        return Err(format!("Snappy took {settled:.0} ms to settle, so it waited"));
    }
    Ok(Outcome {
        elapsed_ms: Some(elapsed),
        note: Some(format!(
            "Snappy settled in {settled:.1} ms at {}",
            describe(frame)
        )),
        ..Outcome::default()
    })
}

fn minimum_size(context: &mut Context) -> Result<Outcome, String> {
    let left = context.zone(0)?;
    let min = Size::new(left.width + 120.0, 200.0);
    let start = Rect::new(
        context.display.work_area.x + 40.0,
        context.display.work_area.y + 60.0,
        min.width,
        300.0,
    );
    context.open_with("min", start, Some(min), None)?;
    let seen = context.snaps().len();
    context.press(&context.zone_key(0)?)?;
    let (outcome, _) = context.finished_snap(seen)?;
    if outcome != "appConstrainedBounds" {
        return Err(format!(
            "Snappy reported {outcome}, not a snap the app constrained"
        ));
    }
    let (frame, elapsed) = context.wait_for("min", "the left zone's top left", |frame| {
        (frame.x - left.x).abs() <= FRAME_TOLERANCE && (frame.y - left.y).abs() <= FRAME_TOLERANCE
    })?;
    if frame.width + FRAME_TOLERANCE < min.width || (frame.height - left.height).abs() > FRAME_TOLERANCE {
        return Err(format!(
            "{} should keep the minimum width {:.0} and fill the zone's height",
            describe(frame),
            min.width
        ));
    }
    Ok(timed(elapsed))
}

fn workspace_switch(context: &mut Context) -> Result<Outcome, String> {
    let start = floating(&context.display, 140.0);
    let before = context.open("ws", start)?;
    let before = context.fixture.frame("ws").unwrap_or(before);
    context.press(&context.switch_key(3)?)?;
    let elapsed = context.wait_parked("ws")?;
    context.wait_active("workspace-3")?;
    context.press(&context.switch_key(1)?)?;
    context.wait_shown("ws", before)?;
    context.wait_active("workspace-1")?;
    Ok(timed(elapsed))
}

/// A window half the size of `zone`, centered on it, so it floats over the zone without
/// filling it.
fn floating_over(zone: Rect) -> Rect {
    let size = Size::new(zone.width * 0.5, zone.height * 0.5);
    Rect::new(
        zone.mid_x() - size.width / 2.0,
        zone.mid_y() - size.height / 2.0,
        size.width,
        size.height,
    )
}

/// `occupant` in the right zone, then `floater` floating over zone `over`, focused.
/// Returns the right zone and the floater's frame.
fn occupied_right_and_floating(
    context: &mut Context,
    occupant: &str,
    floater: &str,
    over: usize,
) -> Result<(Rect, Rect), String> {
    let right = context.zone(1)?;
    context.open(occupant, floating(&context.display, 160.0))?;
    context.press(&context.zone_key(1)?)?;
    context.wait_in(occupant, right)?;
    context.open(floater, floating_over(context.zone(over)?))?;
    Ok((right, context.fixture.frame(floater)?))
}

/// A swap trades zones: the occupant goes to the zone under the floater's center.
fn floating_swap(context: &mut Context) -> Result<Outcome, String> {
    let (right, _) = occupied_right_and_floating(context, "occupant", "floater", 0)?;
    context.drag_window("floater", right.center(), context.drag_mods())?;
    let elapsed = context.wait_in("floater", right)?;
    context.wait_in("occupant", context.zone(0)?)?;
    Ok(timed(elapsed))
}

fn floating_keyboard_swap(context: &mut Context) -> Result<Outcome, String> {
    let (right, _) = occupied_right_and_floating(context, "kocc", "kfloat", 0)?;
    context.press(&context.zone_key(1)?)?;
    let elapsed = context.wait_in("kfloat", right)?;
    context.wait_in("kocc", context.zone(0)?)?;
    Ok(timed(elapsed))
}

/// When the zone under the floater is the destination itself, the occupant takes the
/// floater's exact frame instead.
fn floating_swap_fallback(context: &mut Context) -> Result<Outcome, String> {
    let (right, before) = occupied_right_and_floating(context, "focc", "ffloat", 1)?;
    context.drag_window("ffloat", right.center(), context.drag_mods())?;
    let elapsed = context.wait_in("ffloat", right)?;
    context.wait_in("focc", before)?;
    Ok(timed(elapsed))
}

/// A floater over the occupied right zone goes to the left one, the nearest empty zone,
/// as an ordinary snap.
fn nearest_empty_zone(context: &mut Context) -> Result<Outcome, String> {
    let (right, _) = occupied_right_and_floating(context, "nocc", "nfloat", 1)?;
    let left = context.zone(0)?;
    let seen = context.snaps().len();
    context.press(&context.nearest_empty_key()?)?;
    let elapsed = context.wait_in("nfloat", left)?;
    context.wait_in("nocc", right)?;
    let (outcome, _) = context.finished_snap(seen)?;
    if outcome != "snapped" {
        return Err(format!("Snappy logged the move as {outcome}, not a snap"));
    }
    Ok(timed(elapsed))
}

fn send_to_workspace(context: &mut Context) -> Result<Outcome, String> {
    context.open("sent", floating(&context.display, 200.0))?;
    let before = context.fixture.frame("sent")?;
    context.press(&context.send_key(2)?)?;
    let elapsed = context.wait_parked("sent")?;
    if context.active_workspace()? != "workspace-1" {
        return Err("sending switched workspaces".to_owned());
    }
    context.press(&context.switch_key(2)?)?;
    context.wait_shown("sent", before)?;
    context.press(&context.switch_key(1)?)?;
    context.wait_parked("sent")?;
    Ok(timed(elapsed))
}

fn move_here_and_switch(context: &mut Context) -> Result<Outcome, String> {
    context.open("follow", floating(&context.display, 220.0))?;
    let before = context.fixture.frame("follow")?;
    context.press(&context.send_and_switch_key(2)?)?;
    pause(600);
    let after = context.fixture.frame("follow")?;
    if context.is_parked(after) || !near(after, before) {
        return Err(format!("the window moved to {}", describe(after)));
    }
    context.wait_active("workspace-2")?;
    context.press(&context.switch_key(1)?)?;
    context.wait_parked("follow")?;
    Ok(Outcome::default())
}

/// Stopping the daemon the way installing a new build does, then starting it again, moves
/// nothing: the window sent to workspace 2 stays parked, the other stays on screen, the
/// first workspace stays active, and the new daemon switches both as before. Quitting
/// afterwards still brings the parked window back.
fn restart_keeps_workspaces(context: &mut Context) -> Result<Outcome, String> {
    context.open("kept", side_by_side(&context.display, 0))?;
    let kept = context.fixture.frame("kept")?;
    context.open("away", side_by_side(&context.display, 2))?;
    let away = context.fixture.frame("away")?;
    context.press(&context.send_key(2)?)?;
    context.wait_parked("away")?;
    let parked = context.fixture.frame("away")?;

    let seen = context.logged("workspacesStarted").len();
    context.begin();
    context.daemon.restart()?;
    let elapsed = context.began.elapsed().as_secs_f64() * 1000.0;
    context.stayed("kept", kept)?;
    let still = context.fixture.frame("away")?;
    if !context.is_parked(still) || !near(still, parked) {
        return Err(format!(
            "the parked window moved to {} during the restart",
            describe(still)
        ));
    }
    if context.active_workspace()? != "workspace-1" {
        return Err("the restart changed the active workspace".to_owned());
    }
    let started = context.logged("workspacesStarted");
    let rejoined = started
        .get(seen..)
        .and_then(<[_]>::last)
        .map(|record| record["rejoined"]["workspace-2"].clone())
        .unwrap_or_default();
    if rejoined != 1 {
        return Err(format!(
            "Snappy logged {rejoined} windows rejoining workspace 2, not 1"
        ));
    }

    context.press(&context.switch_key(2)?)?;
    context.wait_shown("away", away)?;
    context.wait_parked("kept")?;
    context.wait_active("workspace-2")?;
    context.press(&context.switch_key(1)?)?;
    context.wait_shown("kept", kept)?;
    context.wait_parked("away")?;
    context.wait_active("workspace-1")?;

    context.daemon.quit()?;
    context.wait_shown("away", away)?;
    context.daemon.start()?;
    Ok(timed(elapsed))
}

fn quit_returns_windows(context: &mut Context) -> Result<Outcome, String> {
    context.open("quit", floating(&context.display, 240.0))?;
    let before = context.fixture.frame("quit")?;
    context.press(&context.send_key(2)?)?;
    context.wait_parked("quit")?;
    context.begin();
    context.daemon.quit()?;
    let elapsed = context.wait_shown("quit", before)?;
    context.daemon.start()?;
    Ok(timed(elapsed))
}

/// The daemon's on-screen windows at or above a stacking layer.
fn daemon_windows_above(context: &Context, layer_at_least: i64) -> Result<Vec<Rect>, String> {
    let pid = context.daemon.pid().ok_or("the daemon isn't running")?;
    Ok(on_screen_windows()?
        .into_iter()
        .filter(|window| window.pid == pid && window.layer >= layer_at_least)
        .map(|window| window.frame)
        .collect())
}

/// The process owning the frontmost normal window.
fn frontmost_normal_window_pid() -> Result<Option<i32>, String> {
    Ok(on_screen_windows()?
        .into_iter()
        .find(|window| window.layer == NORMAL_LAYER)
        .map(|window| window.pid))
}

/// Also checks the hovered zone's caption: none for the setting's own action, and the
/// opposite action named while the swap key is held, saved as `overlay_caption.png`.
fn overlay_floats(context: &mut Context) -> Result<Outcome, String> {
    context.open("overlay", floating(&context.display, 260.0))?;
    let frame = context.fixture.frame("overlay")?;
    let front_before = frontmost_normal_window_pid()?;
    let shift = context.drag_mods();
    let reversing = context.inverting_mods()?;
    let swap_key: Mods = reversing
        .iter()
        .filter(|modifier| !shift.contains(*modifier))
        .collect();
    let reversed = if context.settings.snapping.when_taken.swaps(true) {
        "Swap"
    } else {
        "Stack"
    };
    let hovered = context.zone(0)?.center();
    let seen = context.captions().len();
    let grip = Context::grip(frame);
    let input = &mut *context.input;
    input.modifiers(shift, true)?;
    input.pointer_down(grip, shift)?;
    pause(30);
    for step in 1..=6 {
        input.pointer_drag(
            Point::new(grip.x - 20.0 * f64::from(step), grip.y + 20.0 * f64::from(step)),
            shift,
        )?;
        pause(16);
    }
    input.pointer_drag(hovered, shift)?;
    pause(200);
    let shown = daemon_windows_above(context, FLOATING_LAYER)?;
    let front_during = frontmost_normal_window_pid()?;
    let plain_captions = context.captions().split_off(seen);

    let input = &mut *context.input;
    input.modifiers(swap_key, true)?;
    input.pointer_drag(Point::new(hovered.x + 10.0, hovered.y + 10.0), reversing)?;
    let captioned = context.wait_caption(seen, reversed);
    screenshot(&context.out.join("overlay_caption.png"));

    // Letting go of the modifiers first cancels the snap, so the drop leaves the window be.
    let input = &mut *context.input;
    input.modifiers(reversing, false)?;
    pause(50);
    input.pointer_up(grip, Mods::NONE)?;
    pause(300);
    let after = daemon_windows_above(context, FLOATING_LAYER)?;

    let covers_display = shown
        .iter()
        .any(|overlay| overlay.contains_rect(context.display.work_area));
    if !covers_display {
        return Err(format!(
            "no overlay covered the display during the drag: {shown:?}"
        ));
    }
    if front_during != front_before {
        return Err("the overlay changed which window is in front".to_owned());
    }
    if !plain_captions.is_empty() {
        return Err(format!(
            "the setting's own action showed a caption: {plain_captions:?}"
        ));
    }
    captioned?;
    if after
        .iter()
        .any(|overlay| overlay.contains_rect(context.display.work_area))
    {
        return Err("the overlay stayed on screen after the drop".to_owned());
    }
    Ok(Outcome::default())
}

/// Windows shows the item as a notification-area icon; the shell reports where it is.
#[cfg(windows)]
fn menu_bar_item(context: &mut Context) -> Result<Outcome, String> {
    let pid = context.daemon.pid().ok_or("the daemon isn't running")?;
    let pid = u32::try_from(pid).map_err(|error| error.to_string())?;
    let started = Instant::now();
    while !snappy_windows::reference::has_tray_icon(pid) {
        if started.elapsed() > SETTLE_TIMEOUT {
            return Err(format!("process {pid} has no notification-area icon"));
        }
        pause(100);
    }
    Ok(Outcome::default())
}

#[cfg(target_os = "macos")]
fn menu_bar_item(context: &mut Context) -> Result<Outcome, String> {
    let items = daemon_windows_above(context, crate::screen::STATUS_BAR_LAYER)?;
    let menu_bar_bottom = context.display.work_area.y;
    if items
        .iter()
        .any(|item| item.max_y() <= menu_bar_bottom + 1.0 && item.width > 0.0)
    {
        Ok(Outcome::default())
    } else {
        Err(format!("no status item in the menu bar: {items:?}"))
    }
}

/// X11's status item is a StatusNotifierItem on the session bus, not a window. It is
/// drawn by the desktop's tray host, so this checks that the daemon registered it. On GNOME
/// on Wayland, Snappy's extension puts the item in the top bar.
#[cfg(target_os = "linux")]
fn menu_bar_item(context: &mut Context) -> Result<Outcome, String> {
    if crate::gnome::is_session() {
        let started = Instant::now();
        while !crate::gnome::indicator_shown()? {
            if started.elapsed() > SETTLE_TIMEOUT {
                return Err("Snappy's item isn't in the top bar".to_owned());
            }
            pause(100);
        }
        return Ok(Outcome::default());
    }
    let pid = context.daemon.pid().ok_or("the daemon isn't running")?;
    let started = Instant::now();
    loop {
        let names = crate::screen::session_bus_names()?;
        let prefix = format!("org.kde.StatusNotifierItem-{pid}-");
        if let Some(name) = names.iter().find(|name| name.starts_with(&prefix)) {
            return Ok(Outcome {
                elapsed_ms: None,
                note: Some(format!("registered as {name}; no tray host runs here to draw it")),
                ..Outcome::default()
            });
        }
        if started.elapsed() > SETTLE_TIMEOUT {
            return Err(format!(
                "no StatusNotifierItem of process {pid} on the session bus"
            ));
        }
        pause(100);
    }
}

fn step_workspaces(context: &mut Context) -> Result<Outcome, String> {
    context.open("step", floating(&context.display, 280.0))?;
    let before = context.fixture.frame("step")?;
    context.press(&context.step_key(WorkspaceStep::Next)?)?;
    let elapsed = context.wait_parked("step")?;
    context.wait_active("workspace-2")?;
    context.press(&context.step_key(WorkspaceStep::Previous)?)?;
    context.wait_shown("step", before)?;
    Ok(timed(elapsed))
}

/// Three windows focused in turn, then sent to the next workspace one press at a time:
/// each press sends the focused window and focus moves to the one used before it.
fn send_and_stay_batch(context: &mut Context) -> Result<Outcome, String> {
    let ids = ["batch-a", "batch-b", "batch-c"];
    let mut frames = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        context.open(id, side_by_side(&context.display, index))?;
        frames.push(context.fixture.frame(id)?);
    }
    // A window's first focus can come before Snappy can enroll it, which leaves it out of
    // the focus order, so each is focused again once all are open.
    for id in ids {
        context.focus(id)?;
    }
    let send = context.step_key(WorkspaceStep::SendNext)?;
    let mut slowest: f64 = 0.0;
    for id in ids.iter().rev() {
        context.press(&send)?;
        slowest = slowest.max(context.wait_parked(id)?);
        if context.active_workspace()? != "workspace-1" {
            return Err("sending switched workspaces".to_owned());
        }
        pause(300);
    }
    context.press(&context.switch_key(2)?)?;
    for (id, before) in ids.iter().zip(&frames) {
        context.wait_shown(id, *before)?;
    }
    context.press(&context.switch_key(1)?)?;
    for id in ids {
        context.wait_parked(id)?;
    }
    Ok(timed(slowest))
}

/// A selected window is outlined, then comes along, where it was, when the Snappy key and
/// 3 switch to the third workspace.
fn select_along_by_number(context: &mut Context) -> Result<Outcome, String> {
    context.open("along", floating(&context.display, 120.0))?;
    let before = context.fixture.frame("along")?;
    context.press(&context.focus_key(FocusShortcut::SelectWindow)?)?;
    context.wait_outlines(true)?;
    screenshot(&context.out.join("selection_outline.png"));

    context.press(&context.switch_key(3)?)?;
    context.wait_active("workspace-3")?;
    let elapsed = context.began.elapsed().as_secs_f64() * 1000.0;
    pause(600);
    context.stayed("along", before)?;
    context.wait_outlines(false)?;

    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    context.wait_parked("along")?;
    Ok(timed(elapsed))
}

/// Two of three windows selected go along to the next workspace; the third stays behind.
fn select_two_along_by_step(context: &mut Context) -> Result<Outcome, String> {
    let ids = ["pair-a", "pair-b", "pair-left"];
    let mut frames = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        context.open(id, side_by_side(&context.display, index))?;
        frames.push(context.fixture.frame(id)?);
    }
    context.select(&ids[..2])?;

    context.press(&context.step_key(WorkspaceStep::Next)?)?;
    context.wait_active("workspace-2")?;
    let elapsed = context.wait_parked("pair-left")?;
    pause(300);
    for (id, before) in ids.iter().zip(&frames).take(2) {
        context.stayed(id, *before)?;
    }
    context.wait_outlines(false)?;

    context.press(&context.step_key(WorkspaceStep::Previous)?)?;
    context.wait_active("workspace-1")?;
    context.wait_shown("pair-left", frames[2])?;
    for id in &ids[..2] {
        context.wait_parked(id)?;
    }
    Ok(timed(elapsed))
}

/// A window with three native tabs is selected and taken along to the next workspace, then
/// two tabs are dragged out into windows of their own. All three windows stay with that
/// workspace: they park together on switching back and return together. Only macOS makes
/// each tab a window of its own, and so the bug this guards against is macOS's.
fn tabs_along(context: &mut Context) -> Result<Outcome, String> {
    if !cfg!(target_os = "macos") {
        return Ok(Outcome::skipped(
            "Only macOS makes each native tab a window of its own.",
        ));
    }
    let outcome = tabs_along_and_out(context);
    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    outcome
}

fn tabs_along_and_out(context: &mut Context) -> Result<Outcome, String> {
    let ids = ["tab-a", "tab-b", "tab-c"];
    context.open(ids[0], floating(&context.display, 60.0))?;
    // Each new tab shows and takes focus, so Snappy meets every tab.
    for id in &ids[1..] {
        context.fixture.tab(id, ids[0])?;
        context.opened.push((*id).to_owned());
        pause(500);
    }
    let group = context.fixture.frame("tab-c")?;
    context.press(&context.focus_key(FocusShortcut::SelectWindow)?)?;
    context.wait_outlines(true)?;

    context.press(&context.step_key(WorkspaceStep::Next)?)?;
    context.wait_active("workspace-2")?;
    let elapsed = context.began.elapsed().as_secs_f64() * 1000.0;
    pause(600);
    context.stayed("tab-c", group)?;
    let carried = context.logged("selectionCarried");
    let tabs = carried
        .last()
        .map(|record| record["tabs"].clone())
        .unwrap_or_default();
    let along = tabs
        .as_object()
        .and_then(|groups| groups.values().next())
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    if along != 2 {
        return Err(format!("the hidden tabs didn't go along: {tabs}"));
    }

    let area = context.display.work_area;
    let spots = [
        Point::new(area.x + 40.0, area.max_y() - group.height - 40.0),
        Point::new(
            area.max_x() - group.width - 40.0,
            area.max_y() - group.height - 40.0,
        ),
    ];
    for (id, at) in ids.iter().zip(spots) {
        context.fixture.tear_out(id, at)?;
        pause(600);
    }
    let mut frames = Vec::new();
    for id in ids {
        let frame = context.fixture.frame(id)?;
        if context.is_parked(frame) {
            return Err(format!("{id} isn't on screen after the tabs came apart"));
        }
        frames.push(frame);
    }

    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    for id in ids {
        context.wait_parked(id)?;
    }
    context.press(&context.switch_key(2)?)?;
    context.wait_active("workspace-2")?;
    for (id, frame) in ids.iter().zip(frames) {
        context.wait_shown(id, frame)?;
    }
    Ok(timed(elapsed))
}

/// Pressing the select key again unselects the window: its outline goes, and it stays
/// behind at the next switch.
fn unselect(context: &mut Context) -> Result<Outcome, String> {
    context.open("down", floating(&context.display, 160.0))?;
    let before = context.fixture.frame("down")?;
    let select = context.focus_key(FocusShortcut::SelectWindow)?;
    context.press(&select)?;
    context.wait_outlines(true)?;
    context.press(&select)?;
    context.wait_outlines(false)?;

    context.press(&context.switch_key(2)?)?;
    let elapsed = context.wait_parked("down")?;
    context.press(&context.switch_key(1)?)?;
    context.wait_shown("down", before)?;
    Ok(timed(elapsed))
}

/// Three windows side by side and a fourth below the middle one. Focus goes right, down,
/// up, right, and left, each press starting from the window the one before focused, and
/// ends on the middle window, which the right zone's shortcut then moves. Other apps'
/// windows, such as a test VM's terminal, would be in the way, so this runs on the third
/// workspace, which holds none.
fn focus_directions(context: &mut Context) -> Result<Outcome, String> {
    context.press(&context.switch_key(3)?)?;
    context.wait_active("workspace-3")?;
    let outcome = focus_around(context);
    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    outcome
}

fn focus_around(context: &mut Context) -> Result<Outcome, String> {
    let ids = ["focus-a", "focus-b", "focus-c"];
    let mut frames = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        context.open(id, side_by_side(&context.display, index))?;
        frames.push(context.fixture.frame(id)?);
    }
    let middle = frames[1];
    let area = context.display.work_area;
    let below = Rect::new(middle.x, middle.max_y() + 40.0, middle.width, area.height * 0.25);
    context.open("focus-d", below)?;
    frames.push(context.fixture.frame("focus-d")?);
    context.focus("focus-a")?;

    let seen = context.logged("focusMove").len();
    let presses = [
        FocusShortcut::FocusRight,
        FocusShortcut::FocusDown,
        FocusShortcut::FocusUp,
        FocusShortcut::FocusRight,
        FocusShortcut::FocusLeft,
    ];
    context.begin();
    for shortcut in presses {
        let key = context.focus_key(shortcut)?;
        context.input.chord(&key.combo())?;
        pause(400);
    }
    let elapsed = context.began.elapsed().as_secs_f64() * 1000.0;
    let moves: Vec<(Option<u64>, Option<u64>)> = context
        .logged("focusMove")
        .iter()
        .skip(seen)
        .map(|record| (record["from"].as_u64(), record["to"].as_u64()))
        .collect();
    if moves.len() != presses.len() {
        return Err(format!(
            "Snappy logged {} focus moves, not {}",
            moves.len(),
            presses.len()
        ));
    }
    let to: Vec<Option<u64>> = moves.iter().map(|(_, to)| *to).collect();
    if to.iter().any(Option::is_none) {
        return Err(format!("a focus move found no window: {moves:?}"));
    }
    let followed = moves.windows(2).all(|pair| pair[1].0 == pair[0].1);
    let back_to_middle =
        to[0] == to[2] && to[2] == to[4] && to[1] != to[0] && to[3] != to[0] && to[1] != to[3];
    if !followed || !back_to_middle {
        return Err(format!("focus went {moves:?}"));
    }

    context.press(&context.zone_key(1)?)?;
    context.wait_in("focus-b", context.zone(1)?)?;
    for (id, before) in ["focus-a", "focus-c", "focus-d"]
        .iter()
        .zip([frames[0], frames[2], frames[3]])
    {
        context.stayed(id, before)?;
    }
    Ok(timed(elapsed))
}

/// Windows in both zones and a third floating between them, a little lower, so it is the
/// nearest window by center both right of and below the left one. Focus goes right, left,
/// and down from the left window, then right and left from the floating one, and reaches
/// only the zoned windows; the right zone's shortcut then swaps the left window into it,
/// so focus really ended there. Runs on the third workspace, as `focus_directions` does.
fn focus_zones(context: &mut Context) -> Result<Outcome, String> {
    context.press(&context.switch_key(3)?)?;
    context.wait_active("workspace-3")?;
    let outcome = focus_between_zones(context);
    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    outcome
}

fn focus_between_zones(context: &mut Context) -> Result<Outcome, String> {
    let (left, right) = two_zoned(context, "zone-a", "zone-b")?;
    let area = context.display.work_area;
    let (width, height) = (area.width * 0.3, area.height * 0.3);
    let between = Rect::new(
        area.mid_x() - width / 2.0,
        area.mid_y() + area.height * 0.05 - height / 2.0,
        width,
        height,
    );
    let floating = context.open("zone-f", between)?;
    context.focus("zone-a")?;

    let seen = context.logged("focusMove").len();
    let press = |context: &mut Context, shortcut| -> Result<(), String> {
        let key = context.focus_key(shortcut)?;
        context.input.chord(&key.combo())?;
        pause(400);
        Ok(())
    };
    context.begin();
    for shortcut in [
        FocusShortcut::FocusRight,
        FocusShortcut::FocusLeft,
        FocusShortcut::FocusDown,
    ] {
        press(context, shortcut)?;
    }
    context.focus("zone-f")?;
    press(context, FocusShortcut::FocusRight)?;
    press(context, FocusShortcut::FocusLeft)?;
    let elapsed = context.began.elapsed().as_secs_f64() * 1000.0;

    let moves: Vec<serde_json::Value> = context.logged("focusMove").into_iter().skip(seen).collect();
    let to: Vec<Option<u64>> = moves.iter().map(|record| record["to"].as_u64()).collect();
    let from: Vec<Option<u64>> = moves.iter().map(|record| record["from"].as_u64()).collect();
    let described = || format!("focus went {:?} from {:?}", to, from);
    if moves.len() != 5 {
        return Err(format!(
            "Snappy logged {} focus moves, not 5: {}",
            moves.len(),
            described()
        ));
    }
    if moves.iter().any(|record| record["mode"] != "zones") {
        return Err(format!("not every focus move went by zones: {moves:?}"));
    }
    let (a, b, f) = (from[0], to[0], from[3]);
    let distinct = a.is_some() && b.is_some() && f.is_some() && a != b && f != a && f != b;
    if !distinct || to != [b, a, None, b, a] {
        return Err(described());
    }

    context.press(&context.zone_key(1)?)?;
    context.wait_in("zone-a", right)?;
    context.wait_in("zone-b", left)?;
    context.stayed("zone-f", floating)?;
    Ok(timed(elapsed))
}

/// Two floating windows, selected, go into the two empty zones nearest them with one press
/// of the nearest-empty-zone key, and leave the selection.
fn place_selection(context: &mut Context) -> Result<Outcome, String> {
    let area = context.display.work_area;
    let size = |x: f64, y: f64| {
        Rect::new(
            area.x + area.width * x,
            area.y + y,
            area.width * 0.3,
            area.height * 0.4,
        )
    };
    context.open("place-left", size(0.08, 100.0))?;
    context.open("place-right", size(0.6, 160.0))?;
    context.select(&["place-left", "place-right"])?;

    let seen = context.snaps().len();
    context.press(&context.nearest_empty_key()?)?;
    let left = context.wait_in("place-left", context.zone(0)?)?;
    let right = context.wait_in("place-right", context.zone(1)?)?;
    context.wait_outlines(false)?;
    pause(200);
    let outcomes: Vec<String> = context
        .snaps()
        .iter()
        .skip(seen)
        .map(|record| record["outcome"].as_str().unwrap_or_default().to_owned())
        .collect();
    if outcomes != ["snapped", "snapped"] {
        return Err(format!("Snappy logged the moves as {outcomes:?}"));
    }
    Ok(timed(left.max(right)))
}

/// With workspaces off, the select key still outlines windows, and the clear key takes
/// every outline away without moving anything. Restarts the daemon without workspaces,
/// then with the run's settings again.
fn select_without_workspaces(context: &mut Context) -> Result<Outcome, String> {
    let mut without = context.settings.clone();
    without.workspaces.enabled = false;
    context.daemon.quit()?;
    context.daemon.write_settings(&without)?;
    context.daemon.start()?;
    let outcome = select_and_clear(context);
    context.close_all();
    context.daemon.quit()?;
    context.daemon.write_settings(context.settings)?;
    context.daemon.start()?;
    outcome
}

fn select_and_clear(context: &mut Context) -> Result<Outcome, String> {
    let ids = ["solo-a", "solo-b"];
    let mut frames = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        context.open(id, side_by_side(&context.display, index))?;
        frames.push(context.fixture.frame(id)?);
    }
    context.select(&ids)?;
    context.press(&context.focus_key(FocusShortcut::ClearSelection)?)?;
    context.wait_outlines(false)?;
    let elapsed = context.began.elapsed().as_secs_f64() * 1000.0;
    for (id, before) in ids.iter().zip(frames) {
        context.stayed(id, before)?;
    }
    Ok(timed(elapsed))
}

/// Full-screen windows are refused, not moved. Runs last: it leaves a full-screen Space.
fn full_screen_refused(context: &mut Context) -> Result<Outcome, String> {
    context.open("full", floating(&context.display, 300.0))?;
    context.fixture.full_screen("full", true)?;
    pause(1500);
    let before = context.fixture.frame("full")?;
    let seen = context.snaps().len();
    context.press(&context.zone_key(0)?)?;
    pause(800);
    let after = context.fixture.frame("full")?;
    context.fixture.full_screen("full", false)?;
    pause(1500);
    if !near(after, before) {
        return Err(format!("the full-screen window moved to {}", describe(after)));
    }
    if context.snaps().len() > seen {
        return Err("Snappy tried to snap a full-screen window".to_owned());
    }
    Ok(Outcome::default())
}
