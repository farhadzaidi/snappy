//! The real daemon, end to end, in the KWin session.

use std::process::{Child, Command};
use std::time::{Duration, Instant};

use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{KeyChord, Modifier, Os};
use snappy_core::settings::Settings;
use snappy_platform::launch;

use crate::input::input;
use crate::panel::{Edge, Panel};
use crate::probes::{kwin_frame, kwin_skips_taskbar, overlay_windows, wait_for_overlay};
use crate::support::{Fixture, TITLE_BAR_GRIP, WAIT};

/// Where the end-to-end tests send the daemon's output, in its home folder.
const DAEMON_OUTPUT: &str = "daemon.log";

/// The real daemon, with scratch settings: snapping on and the halves layout everywhere.
struct Daemon {
    child: Child,
}

impl Daemon {
    /// Starts the daemon on settings that are the defaults with snapping on, the halves
    /// layout everywhere, and whatever `change` makes.
    fn start(home: &std::path::Path, change: impl FnOnce(&mut Settings)) -> Option<Daemon> {
        let program = std::env::var("SNAPPY_KWIN_DAEMON").ok()?;
        let mut settings = Settings::defaults(Os::Linux);
        settings.snapping.enabled = true;
        settings.layouts.default_layout_id = "halves".to_owned();
        change(&mut settings);
        let settings = settings.validate(Os::Linux).unwrap();
        std::fs::create_dir_all(home).unwrap();
        std::fs::write(
            snappy_engine::Paths::in_folder(home).settings,
            settings.to_json().unwrap(),
        )
        .unwrap();
        let log = std::fs::File::create(home.join(DAEMON_OUTPUT)).unwrap();
        let child = Command::new(program)
            .arg(launch::BACKGROUND)
            .env(launch::HOME_ENV, home)
            .env(launch::DEBUG_ENV, "1")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        Some(Daemon { child })
    }

    /// Asks the daemon to quit as `killall` would, and waits for it.
    fn quit(&mut self) {
        let pid = self.child.id().to_string();
        Command::new("kill").args(["-TERM", &pid]).status().unwrap();
        let deadline = Instant::now() + WAIT;
        while matches!(self.child.try_wait(), Ok(None)) {
            assert!(Instant::now() < deadline, "the daemon didn't quit");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn wait_for_frame(
    bus: &zbus::blocking::Connection,
    title: &str,
    what: &str,
    done: impl Fn(Rect) -> bool,
) -> Rect {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(frame) = kwin_frame(bus, title).filter(|frame| done(*frame)) {
            return frame;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting until {what}; the window is at {:?}",
            kwin_frame(bus, title)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn the_daemon_snaps_inside_the_panels_with_its_overlay_showing_and_by_shortcut() {
    if std::env::var("SNAPPY_KWIN_TEST").ok().as_deref() != Some("1") {
        println!("skipped: set SNAPPY_KWIN_TEST=1 inside the KWin test session");
        return;
    }
    let Some(home) =
        std::env::var_os("SNAPPY_KWIN_OUT").map(|out| std::path::PathBuf::from(out).join("daemon"))
    else {
        println!("skipped: SNAPPY_KWIN_OUT names where the daemon's files go");
        return;
    };
    drop(input());
    let bus = zbus::blocking::Connection::session().unwrap();
    // Zones have no shortcuts by default, so the halves layout's second zone gets one.
    let Some(mut daemon) = Daemon::start(&home, |settings| {
        let halves = settings.layout("halves").unwrap().clone();
        let mut options = settings.options(&halves);
        options
            .zone_shortcuts
            .insert(halves.zones[1].id.clone(), KeyChord::snappy("Digit2", []));
        settings.layouts.options.insert(halves.id, options);
    }) else {
        println!("skipped: set SNAPPY_KWIN_DAEMON to the daemon binary");
        return;
    };
    let _fixture = Fixture::open("e2e", Size::new(600.0, 400.0));
    let title = "Fixture e2e";
    let frame = wait_for_frame(&bus, title, "the window appears", |frame| frame.width > 1.0);
    // Give the daemon time to load its script and take its shortcuts, then add panels as a
    // user would after Snappy started: 40 points at the top and 51 at the bottom of the
    // first display, which leave 0, 40, 1920 × 989 for windows.
    std::thread::sleep(Duration::from_millis(1500));
    // The daemon prepares its overlay at start, before any drag, at the whole display.
    let prepared = wait_for_overlay(daemon.child.id(), Rect::new(0.0, 0.0, 1920.0, 1080.0));
    let _top = Panel::open("Virtual-0", Edge::Top, 40);
    let _bottom = Panel::open("Virtual-0", Edge::Bottom, 51);
    let work_area = Rect::new(0.0, 40.0, 1920.0, 989.0);
    // The daemon reads displays again once a change has settled.
    std::thread::sleep(Duration::from_millis(600));

    let mut input = input();
    let grip = Point::new(frame.x + frame.width / 2.0, frame.y + TITLE_BAR_GRIP);
    input.move_to(grip);
    input.button(true);
    for step in 1..=4 {
        input.move_to(Point::new(
            grip.x - 25.0 * f64::from(step),
            grip.y + 10.0 * f64::from(step),
        ));
    }
    input.modifier(Modifier::Shift, true);
    let target = Point::new(400.0, 540.0);
    for step in 1..=10 {
        let t = f64::from(step) / 10.0;
        input.move_to(Point::new(
            grip.x + (target.x - grip.x) * t,
            grip.y + (target.y - grip.y) * t,
        ));
        std::thread::sleep(Duration::from_millis(16));
    }
    std::thread::sleep(Duration::from_millis(100));
    // The overlay covers the new work area while the drag key is held: the same window,
    // moved and resized, never closed and opened again for KWin's effects to animate.
    let during = wait_for_overlay(daemon.child.id(), work_area);
    assert_eq!(during.uuid, prepared.uuid);
    input.button(false);
    input.modifier(Modifier::Shift, false);

    let inside =
        |frame: Rect| frame.y >= work_area.y && frame.y + frame.height <= work_area.y + work_area.height;
    let left = wait_for_frame(
        &bus,
        title,
        "the window fills the left half between the panels",
        |frame| {
            frame.x < 40.0
                && frame.width > 900.0
                && frame.width < 961.0
                && frame.height > 900.0
                && inside(frame)
        },
    );

    // The halves layout's second zone, by its shortcut, moves the focused window.
    input.press_chord(&KeyChord::snappy("Digit2", []));
    let right = wait_for_frame(
        &bus,
        title,
        "the window fills the right half between the panels",
        |frame| frame.x >= 960.0 && inside(frame),
    );
    assert_eq!(right.width, left.width);
    drop(input);
    // Each display keeps its own surface; hiding leaves this one where it was.
    assert!(
        overlay_windows(daemon.child.id()).contains(&during),
        "hiding keeps the window"
    );

    daemon.quit();
    let output = std::fs::read_to_string(home.join(DAEMON_OUTPUT)).unwrap();
    assert!(
        !output.contains("zone overlay"),
        "the overlay had trouble during the drag; the daemon said:\n{output}"
    );
    let mut scripting = None;
    for _ in 0..20 {
        let reply = bus.call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "isScriptLoaded",
            &("snappy",),
        );
        scripting = reply
            .ok()
            .and_then(|reply| reply.body().deserialize::<bool>().ok());
        if scripting == Some(false) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(scripting, Some(false), "quitting unloads the script");
}

#[test]
fn the_daemons_workspaces_hide_and_bring_back_windows_with_the_default_shortcuts() {
    if std::env::var("SNAPPY_KWIN_TEST").ok().as_deref() != Some("1") {
        println!("skipped: set SNAPPY_KWIN_TEST=1 inside the KWin test session");
        return;
    }
    let Some(home) =
        std::env::var_os("SNAPPY_KWIN_OUT").map(|out| std::path::PathBuf::from(out).join("workspaces"))
    else {
        println!("skipped: SNAPPY_KWIN_OUT names where the daemon's files go");
        return;
    };
    drop(input());
    let bus = zbus::blocking::Connection::session().unwrap();
    // Only "Enable workspaces": every shortcut is the default.
    let Some(mut daemon) = Daemon::start(&home, |settings| settings.workspaces.enabled = true) else {
        println!("skipped: set SNAPPY_KWIN_DAEMON to the daemon binary");
        return;
    };
    let _fixture = Fixture::open("workspaces", Size::new(500.0, 300.0));
    let title = "Fixture workspaces";
    let frame = wait_for_frame(&bus, title, "the window appears", |frame| frame.width > 1.0);
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(kwin_skips_taskbar(&bus, title), Some(false));

    let mut input = input();
    // Snappy + 2 goes to the second workspace, which has no windows yet.
    input.press_chord(&KeyChord::snappy("Digit2", []));
    let deadline = Instant::now() + WAIT;
    while kwin_skips_taskbar(&bus, title) != Some(true) {
        assert!(
            Instant::now() < deadline,
            "the window wasn't hidden on Snappy's desktop"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // Snappy + ; comes back, with the window where it was.
    input.press_chord(&KeyChord::snappy("Semicolon", []));
    let back = wait_for_frame(&bus, title, "the window comes back", |_| {
        kwin_skips_taskbar(&bus, title) == Some(false)
    });
    assert_eq!(back, frame);
    drop(input);

    daemon.quit();
    let output = std::fs::read_to_string(home.join(DAEMON_OUTPUT)).unwrap();
    assert!(
        !output.contains("snappy:"),
        "the daemon reported trouble:\n{output}"
    );
}
