//! What the scenarios share: the context they run in, waiting for frames and the debug log, and
//! where windows count as parked or shown.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{KeyChord, Mods};
use snappy_core::layout::{Spacing, preset, zone_frame};
use snappy_core::settings::{NavigationShortcut, Settings, Workspace, WorkspaceStep};
use snappy_platform::Display;

use super::Outcome;
use crate::fixture::Fixture;
use crate::input::{Input, pause};
use crate::screen::{FLOATING_LAYER, NORMAL_LAYER, on_screen_windows};
use crate::session::Daemon;

/// Window frames this close count as equal. Fixture frames and Snappy's frames come from
/// the same window server, so only rounding separates them.
pub(super) const FRAME_TOLERANCE: f64 = 2.0;

/// How long a move may take before a scenario fails.
pub(super) const SETTLE_TIMEOUT: Duration = Duration::from_secs(3);

/// Where a title bar can be grabbed, below the window's top edge.
pub const TITLE_BAR_GRIP: f64 = 14.0;

/// A snap that needs no settle waits finishes well within this, inside Snappy.
pub(super) const UNWAITED_SNAP_MS: f64 = 50.0;

/// The layout the run's settings make the default.
pub(super) const LAYOUT: &str = "halves";

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

    pub(super) fn zone(&self, index: usize) -> Result<Rect, String> {
        self.zones()
            .get(index)
            .copied()
            .ok_or_else(|| format!("no zone {index}"))
    }

    pub(super) fn open(&mut self, id: &str, content: Rect) -> Result<Rect, String> {
        self.open_with(id, content, None, None)
    }

    pub(super) fn open_with(
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

    /// Open a native tab `id` in `window`'s group; it shows and takes focus.
    pub(super) fn open_tab(&mut self, id: &str, window: &str) -> Result<(), String> {
        self.fixture.tab(id, window)?;
        self.opened.push(id.to_owned());
        Ok(())
    }

    /// Close every window this scenario opened.
    pub fn close_all(&mut self) {
        for id in std::mem::take(&mut self.opened) {
            let _ = self.fixture.close(&id);
        }
        pause(300);
    }

    pub(super) fn focus(&mut self, id: &str) -> Result<(), String> {
        self.fixture.focus(id)?;
        pause(250);
        Ok(())
    }

    /// The active workspace, as the daemon saved it.
    pub(super) fn active_workspace(&self) -> Result<String, String> {
        let path = self.daemon.home().join("settings.json");
        let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let settings: serde_json::Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        settings["workspaces"]["activeId"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "the settings have no active workspace".to_owned())
    }

    /// Wait until the daemon has saved `expected` as the active workspace, which it does
    /// once a switch has finished. A scenario waits for its last switch before it ends, so
    /// closing its windows can't land in the middle of that switch.
    pub(super) fn wait_active(&self, expected: &str) -> Result<(), String> {
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

    pub(super) fn is_parked(&self, frame: Rect) -> bool {
        parked(self.display.frame, frame) || !shown(self.fixture.pid(), frame)
    }

    pub(super) fn wait_parked(&mut self, id: &str) -> Result<f64, String> {
        let (display, pid) = (self.display.frame, self.fixture.pid());
        self.wait_for(id, "a parked position", |frame| {
            parked(display, frame) || !shown(pid, frame)
        })
        .map(|(_, elapsed)| elapsed)
    }

    /// Wait until `id` is back on screen at `target`.
    pub(super) fn wait_shown(&mut self, id: &str, target: Rect) -> Result<f64, String> {
        let pid = self.fixture.pid();
        let what = format!("{} on screen", describe(target));
        self.wait_for(id, &what, |frame| near(frame, target) && shown(pid, frame))
            .map(|(_, elapsed)| elapsed)
    }

    /// Fail unless `id` is on screen at `before`, as a selected window taken along stays.
    pub(super) fn stayed(&mut self, id: &str, before: Rect) -> Result<(), String> {
        let after = self.fixture.frame(id)?;
        if self.is_parked(after) || !near(after, before) {
            return Err(format!("{id} moved to {}", describe(after)));
        }
        Ok(())
    }

    /// Wait until the daemon shows, or no longer shows, the surface over the work area
    /// that outlines the selected windows.
    pub(super) fn wait_outlines(&self, shown: bool) -> Result<(), String> {
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
    pub(super) fn logged(&self, event: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.debug_log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|record| record["event"] == event)
            .collect()
    }

    /// Snap records in the daemon's debug log so far.
    pub(super) fn snaps(&self) -> Vec<serde_json::Value> {
        self.logged("snap")
    }

    /// The hovered zone's captions as the debug log recorded them, one per change.
    pub(super) fn captions(&self) -> Vec<Option<String>> {
        self.logged("overlayCaption")
            .iter()
            .map(|record| record["caption"].as_str().map(str::to_owned))
            .collect()
    }

    /// Wait for the debug log to say the hovered zone shows `expected`, after the first
    /// `seen` caption changes.
    pub(super) fn wait_caption(&self, seen: usize, expected: &str) -> Result<(), String> {
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
    pub(super) fn finished_snap(&self, seen: usize) -> Result<(String, f64), String> {
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
    pub(super) fn begin(&mut self) {
        self.began = Instant::now();
    }

    /// Milliseconds since the action being timed began.
    pub(super) fn elapsed_ms(&self) -> f64 {
        self.began.elapsed().as_secs_f64() * 1000.0
    }

    /// Wait until `id`'s frame satisfies `done`. Returns the frame and the time since the
    /// action began.
    pub(super) fn wait_for(
        &mut self,
        id: &str,
        what: &str,
        done: impl Fn(Rect) -> bool,
    ) -> Result<(Rect, f64), String> {
        self.wait_for_within(id, what, SETTLE_TIMEOUT, done)
    }

    /// Like `wait_for`, failing after `timeout` instead.
    pub(super) fn wait_for_within(
        &mut self,
        id: &str,
        what: &str,
        timeout: Duration,
        done: impl Fn(Rect) -> bool,
    ) -> Result<(Rect, f64), String> {
        let started = Instant::now();
        loop {
            let frame = self.fixture.frame(id)?;
            if done(frame) {
                return Ok((frame, self.elapsed_ms()));
            }
            if started.elapsed() > timeout {
                return Err(format!("{id} never reached {what}; it is at {}", describe(frame)));
            }
            pause(5);
        }
    }

    pub(super) fn wait_in(&mut self, id: &str, target: Rect) -> Result<f64, String> {
        self.wait_for(id, &describe(target), |frame| near(frame, target))
            .map(|(_, elapsed)| elapsed)
    }

    /// The point in `frame`'s title bar a user would grab.
    pub(super) fn grip(frame: Rect) -> Point {
        Point::new(frame.mid_x(), frame.y + TITLE_BAR_GRIP)
    }

    /// Drag `id` by its title bar so that the pointer ends at `to`. Timing starts at the
    /// release, when Snappy takes over.
    pub(super) fn drag_window(&mut self, id: &str, to: Point, mods: Mods) -> Result<(), String> {
        let frame = self.fixture.frame(id)?;
        self.input.drag(Self::grip(frame), to, mods, 12)?;
        self.begin();
        Ok(())
    }

    /// Press a chord. Timing starts here.
    pub(super) fn press(&mut self, chord: &KeyChord) -> Result<(), String> {
        self.begin();
        self.input.chord(&chord.combo(self.settings.snappy_key))
    }

    /// The shortcut for zone `index` of the halves layout.
    pub(super) fn zone_key(&self, index: usize) -> Result<KeyChord, String> {
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

    /// The shortcut for moving between zones, into the nearest empty zone, focus, or the
    /// selection.
    pub(super) fn navigation_key(&self, shortcut: impl Into<NavigationShortcut>) -> Result<KeyChord, String> {
        let shortcut = shortcut.into();
        self.settings
            .navigation
            .get(shortcut)
            .cloned()
            .ok_or_else(|| format!("{} has no shortcut", shortcut.title()))
    }

    /// The modifiers held to snap while dragging.
    pub(super) fn drag_mods(&self) -> Mods {
        Mods::from_iter([self.settings.snapping.drag_modifier.modifier()])
    }

    /// The modifiers held to drop with the opposite of "When the zone is taken": the drag
    /// modifier plus the swap key.
    pub(super) fn inverting_mods(&self) -> Result<Mods, String> {
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
    pub(super) fn workspace_key(
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

    pub(super) fn switch_key(&self, number: usize) -> Result<KeyChord, String> {
        self.workspace_key(number, |workspace| &workspace.switch_shortcut)
    }

    pub(super) fn send_key(&self, number: usize) -> Result<KeyChord, String> {
        self.workspace_key(number, |workspace| &workspace.send_shortcut)
    }

    pub(super) fn send_and_switch_key(&self, number: usize) -> Result<KeyChord, String> {
        self.workspace_key(number, |workspace| &workspace.send_and_switch_shortcut)
    }

    pub(super) fn step_key(&self, step: WorkspaceStep) -> Result<KeyChord, String> {
        self.settings
            .workspaces
            .step(step)
            .cloned()
            .ok_or_else(|| format!("{} has no shortcut", step.title()))
    }

    /// Select the windows `ids`, focusing each in turn, and wait for their outlines.
    pub(super) fn select(&mut self, ids: &[&str]) -> Result<(), String> {
        let select = self.navigation_key(NavigationShortcut::SelectWindow)?;
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
pub(super) fn parked(display: Rect, frame: Rect) -> bool {
    frame.x >= display.max_x() - 2.0 || frame.max_x() <= display.min_x()
}

/// True when the window server shows a window of process `pid` covering most of `frame`.
/// Parking on a native desktop leaves the frame alone and only takes the window off
/// screen. When the window list can't be read, only positions count.
pub(super) fn shown(pid: u32, frame: Rect) -> bool {
    let Ok(windows) = on_screen_windows() else {
        return true;
    };
    windows.iter().any(|window| {
        i64::from(window.pid) == i64::from(pid)
            && window.layer == NORMAL_LAYER
            && window.frame.intersection_area(frame) > frame.area() / 2.0
    })
}

pub(super) fn near(a: Rect, b: Rect) -> bool {
    [(a.x, b.x), (a.y, b.y), (a.width, b.width), (a.height, b.height)]
        .iter()
        .all(|(x, y)| (x - y).abs() <= FRAME_TOLERANCE)
}

pub(super) fn describe(frame: Rect) -> String {
    format!(
        "({:.0}, {:.0}, {:.0} × {:.0})",
        frame.x, frame.y, frame.width, frame.height
    )
}

pub(super) fn timed(elapsed: f64) -> Outcome {
    Outcome {
        elapsed_ms: Some(elapsed),
        ..Outcome::default()
    }
}

/// A window somewhere in the middle of the display, away from any zone's exact frame.
pub(super) fn floating(display: &Display, offset: f64) -> Rect {
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
pub(super) fn side_by_side(display: &Display, index: usize) -> Rect {
    let area = display.work_area;
    let width = area.width * 0.28;
    Rect::new(
        area.x + 40.0 + (width + 40.0) * index as f64,
        area.y + 120.0,
        width,
        area.height * 0.4,
    )
}

/// Window `a` in the left zone and `b` in the right one, with `a` focused.
pub(super) fn two_zoned(context: &mut Context, a: &str, b: &str) -> Result<(Rect, Rect), String> {
    let (left, right) = (context.zone(0)?, context.zone(1)?);
    context.open(b, floating(&context.display, 60.0))?;
    context.press(&context.zone_key(1)?)?;
    context.wait_in(b, right)?;
    context.open(a, floating(&context.display, 80.0))?;
    context.press(&context.zone_key(0)?)?;
    context.wait_in(a, left)?;
    Ok((left, right))
}

/// A window half the size of `zone`, centered on it, so it floats over the zone without
/// filling it.
pub(super) fn floating_over(zone: Rect) -> Rect {
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
pub(super) fn occupied_right_and_floating(
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

/// The daemon's on-screen windows at or above a stacking layer.
pub(super) fn daemon_windows_above(context: &Context, layer_at_least: i64) -> Result<Vec<Rect>, String> {
    let pid = context.daemon.pid().ok_or("the daemon isn't running")?;
    Ok(on_screen_windows()?
        .into_iter()
        .filter(|window| window.pid == pid && window.layer >= layer_at_least)
        .map(|window| window.frame)
        .collect())
}
