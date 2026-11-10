//! Runs the live scenarios against a real daemon inside a sandbox, then writes
//! `results.json` and a screenshot of each failure to the output folder, next to the
//! screenshots scenarios take of what they show.
//!
//! Usage: `snappy-live --app <Snappy.app or daemon> --fixture <snappy-fixture> --out <folder>
//! [--only <scenario name prefix>]`

use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;
use snappy_core::keys::{KeyChord, Modifier, Platform};
use snappy_core::settings::Settings;
use snappy_live::fixture::Fixture;
use snappy_live::input::{pause, platform_input};
use snappy_live::scenarios::{ALL, Context};
use snappy_live::screen::screenshot;
use snappy_live::session::Daemon;
use snappy_platform::Display;

struct Options {
    app: PathBuf,
    fixture: PathBuf,
    out: PathBuf,
    only: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioResult {
    name: String,
    passed: bool,
    /// Why the scenario couldn't apply here; it counts as passed.
    skipped: Option<String>,
    elapsed_ms: Option<f64>,
    detail: Option<String>,
}

fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("snappy-live: {error}");
            return ExitCode::from(2);
        }
    };
    match run(&options) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("snappy-live: {error}");
            ExitCode::from(2)
        }
    }
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let (mut app, mut fixture, mut out, mut only) = (None, None, None, None);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--app" => app = Some(PathBuf::from(value)),
            "--fixture" => fixture = Some(PathBuf::from(value)),
            "--out" => out = Some(PathBuf::from(value)),
            "--only" => only = Some(value),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(Options {
        app: app.ok_or("--app is required")?,
        fixture: fixture.ok_or("--fixture is required")?,
        out: out.ok_or("--out is required")?,
        only,
    })
}

const PLATFORM: Platform = if cfg!(target_os = "macos") {
    Platform::MacOs
} else if cfg!(windows) {
    Platform::Windows
} else {
    Platform::Linux
};

/// Settings for the run: the defaults with snapping on, the halves layout everywhere, and
/// workspaces on. Every default shortcut is kept, so the run presses the chords users get.
/// What has no default, the halves layout's zones and each workspace's send and move here
/// and switch, gets chords this platform leaves free.
fn test_settings() -> Result<Settings, String> {
    let mut settings = Settings::defaults(PLATFORM);
    settings.snapping.enabled = true;
    settings.layouts.default_layout_id = "halves".to_owned();
    settings.workspaces.enabled = true;
    let chords = match PLATFORM {
        Platform::MacOs => MACOS_CHORDS,
        Platform::Windows => WINDOWS_CHORDS,
        Platform::Linux if is_gnome() => GNOME_CHORDS,
        Platform::Linux => LINUX_CHORDS,
    };
    chords.assign(&mut settings)?;
    settings
        .validate(PLATFORM)
        .map_err(|error| format!("the test settings are invalid: {error}"))
}

/// Chords for the shortcuts without a default: `key(n)` held with `zone` for zone n, with
/// `send` to send to workspace n, and with `send_and_switch` to move here and switch to it.
struct RunChords {
    key: fn(usize) -> (String, String),
    zone: &'static [Modifier],
    send: &'static [Modifier],
    send_and_switch: &'static [Modifier],
}

impl RunChords {
    fn chord(&self, n: usize, mods: &[Modifier]) -> KeyChord {
        let (code, label) = (self.key)(n);
        KeyChord::new(&code, mods.iter().copied()).with_label(label)
    }

    fn assign(&self, settings: &mut Settings) -> Result<(), String> {
        let halves = settings
            .layout("halves")
            .cloned()
            .ok_or("there is no halves layout")?;
        let mut options = settings.options(&halves);
        for (index, zone) in halves.zones.iter().enumerate() {
            options
                .zone_shortcuts
                .insert(zone.id.clone(), self.chord(index + 1, self.zone));
        }
        settings.layouts.options.insert(halves.id.clone(), options);
        for (index, workspace) in settings.workspaces.items.iter_mut().enumerate() {
            workspace.send_shortcut = Some(self.chord(index + 1, self.send));
            workspace.send_and_switch_shortcut = Some(self.chord(index + 1, self.send_and_switch));
        }
        Ok(())
    }
}

fn digit(n: usize) -> (String, String) {
    (format!("Digit{n}"), n.to_string())
}

/// Control with a digit, plus Shift to send or Option to move here and switch.
const MACOS_CHORDS: RunChords = RunChords {
    key: digit,
    zone: &[Modifier::Ctrl],
    send: &[Modifier::Ctrl, Modifier::Shift],
    send_and_switch: &[Modifier::Ctrl, Modifier::Alt],
};

/// The taskbar owns Win with Ctrl, Alt, or Shift plus a digit, so these use F-keys, which
/// nothing in Windows uses with these modifiers.
const WINDOWS_CHORDS: RunChords = RunChords {
    key: |n| (format!("F{n}"), format!("F{n}")),
    zone: &[Modifier::Ctrl, Modifier::Alt, Modifier::Meta],
    send: &[Modifier::Ctrl, Modifier::Alt, Modifier::Shift, Modifier::Meta],
    send_and_switch: &[Modifier::Ctrl, Modifier::Alt, Modifier::Shift],
};

/// Super with Ctrl or Alt and a digit, which the container's window manager leaves free.
const LINUX_CHORDS: RunChords = RunChords {
    key: digit,
    zone: &[Modifier::Ctrl, Modifier::Meta],
    send: &[Modifier::Ctrl, Modifier::Shift, Modifier::Meta],
    send_and_switch: &[Modifier::Alt, Modifier::Meta],
};

/// GNOME binds Super with Ctrl and a digit to opening an app's new window, and Ctrl+Alt
/// with an F-key switches to a text console, so these use Super with Alt, Shift, or both.
const GNOME_CHORDS: RunChords = RunChords {
    key: digit,
    zone: &[Modifier::Alt, Modifier::Meta],
    send: &[Modifier::Ctrl, Modifier::Shift, Modifier::Meta],
    send_and_switch: &[Modifier::Alt, Modifier::Shift, Modifier::Meta],
};

/// A freshly started GNOME Shell ignores title bar drags of a Wayland app until the session
/// has handled some input, so one throwaway window is clicked and dragged first.
#[cfg(target_os = "linux")]
fn warm_up(
    fixture: &mut Fixture,
    input: &mut dyn snappy_live::input::Input,
    display: &Display,
) -> Result<(), String> {
    if !is_gnome() {
        return Ok(());
    }
    let area = display.work_area;
    let frame = fixture.open(
        "warm-up",
        snappy_core::geometry::Rect::new(area.x + 200.0, area.y + 200.0, 500.0, 300.0),
        None,
        None,
    )?;
    fixture.focus("warm-up")?;
    pause(500);
    let grip =
        snappy_core::geometry::Point::new(frame.mid_x(), frame.y + snappy_live::scenarios::TITLE_BAR_GRIP);
    let to = snappy_core::geometry::Point::new(grip.x + 150.0, grip.y + 100.0);
    // The drag that wakes the session may be the one GNOME ignores, so it needn't move.
    if let Err(error) = input.drag(grip, to, snappy_core::keys::Mods::NONE, 6) {
        eprintln!("warm-up: {error}");
    }
    pause(300);
    fixture.close("warm-up")?;
    pause(300);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn warm_up(
    _fixture: &mut Fixture,
    _input: &mut dyn snappy_live::input::Input,
    _display: &Display,
) -> Result<(), String> {
    Ok(())
}

/// On GNOME the test helper's virtual devices keep whatever a failed scenario left held.
#[cfg(target_os = "linux")]
fn release_input() {
    if is_gnome() {
        snappy_live::gnome::release_all();
    }
}

#[cfg(not(target_os = "linux"))]
fn release_input() {}

#[cfg(target_os = "linux")]
fn is_gnome() -> bool {
    snappy_live::gnome::is_session()
}

#[cfg(not(target_os = "linux"))]
fn is_gnome() -> bool {
    false
}

fn run(options: &Options) -> Result<bool, String> {
    std::fs::create_dir_all(&options.out).map_err(|error| error.to_string())?;
    let settings = test_settings()?;
    let daemon = Daemon::new(&options.app, &options.out.join("home"));
    let _ = std::fs::remove_dir_all(daemon.home());
    daemon.write_settings(&settings)?;
    daemon.kill();
    daemon.start()?;

    let display = primary_display()?;
    let mut fixture = Fixture::launch(&options.fixture)?;
    let mut input = platform_input()?;
    warm_up(&mut fixture, input.as_mut(), &display)?;
    let mut results = Vec::new();
    {
        let debug_log = daemon.home().join("logs").join("debug.jsonl");
        let mut context = Context::new(
            &daemon,
            &settings,
            &mut fixture,
            input.as_mut(),
            display,
            debug_log,
            options.out.clone(),
        );
        for (name, scenario) in ALL {
            if options
                .only
                .as_ref()
                .is_some_and(|only| !name.starts_with(only.as_str()))
            {
                continue;
            }
            let outcome = scenario(&mut context);
            if outcome.is_err() {
                screenshot(&options.out.join(format!("{name}.png")));
            }
            release_input();
            context.close_all();
            let result = match outcome {
                Ok(outcome) => ScenarioResult {
                    name: (*name).to_owned(),
                    passed: true,
                    skipped: outcome.skipped,
                    elapsed_ms: outcome.elapsed_ms,
                    detail: outcome.note,
                },
                Err(error) => ScenarioResult {
                    name: (*name).to_owned(),
                    passed: false,
                    skipped: None,
                    elapsed_ms: None,
                    detail: Some(error),
                },
            };
            report(&result);
            results.push(result);
        }
    }
    drop(fixture);
    pause(300);
    // Quit the way a user would, so nothing stays parked for the next run.
    if daemon.quit().is_err() {
        daemon.kill();
    }

    let json = serde_json::to_vec_pretty(&results).map_err(|error| error.to_string())?;
    std::fs::write(options.out.join("results.json"), json).map_err(|error| error.to_string())?;
    let passed = results.iter().filter(|result| result.passed).count();
    let skipped = results.iter().filter(|result| result.skipped.is_some()).count();
    println!(
        "{passed} of {} scenarios passed, {skipped} of them skipped as not applicable",
        results.len()
    );
    Ok(passed == results.len())
}

fn report(result: &ScenarioResult) {
    let mark = match (result.passed, &result.skipped) {
        (true, Some(_)) => "skip",
        (true, None) => "pass",
        (false, _) => "FAIL",
    };
    let timing = result
        .elapsed_ms
        .map(|ms| format!(" ({ms:.0} ms)"))
        .unwrap_or_default();
    let detail = result
        .detail
        .as_deref()
        .or(result.skipped.as_deref())
        .map(|d| format!(": {d}"))
        .unwrap_or_default();
    println!("{mark} {}{timing}{detail}", result.name);
}

#[cfg(target_os = "macos")]
fn primary_display() -> Result<Display, String> {
    let main_thread = objc2::MainThreadMarker::new().ok_or("not on the main thread")?;
    snappy_macos::reference::displays(main_thread)
        .into_iter()
        .next()
        .ok_or_else(|| "no display".to_owned())
}

#[cfg(target_os = "linux")]
fn primary_display() -> Result<Display, String> {
    if snappy_live::gnome::is_session() {
        return snappy_gnome::reference::displays()?
            .into_iter()
            .next()
            .ok_or_else(|| "no display".to_owned());
    }
    let (conn, screen) = x11rb::connect(None).map_err(|error| error.to_string())?;
    snappy_x11::reference::displays(&conn, screen)
        .map_err(|error| error.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| "no display".to_owned())
}

#[cfg(windows)]
fn primary_display() -> Result<Display, String> {
    snappy_windows::reference::set_dpi_aware();
    snappy_windows::reference::displays()
        .into_iter()
        .next()
        .ok_or_else(|| "no display".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_settings_keep_every_default_and_give_the_rest_a_chord() {
        let settings = test_settings().unwrap();
        let defaults = Settings::defaults(PLATFORM);
        let workspaces = &settings.workspaces;
        assert!(workspaces.enabled);
        assert_eq!(workspaces.steps().count(), 5);
        assert!(
            workspaces
                .steps()
                .all(|(step, chord)| defaults.workspaces.step(step) == Some(chord))
        );
        for (workspace, default) in workspaces.items.iter().zip(&defaults.workspaces.items) {
            assert_eq!(workspace.switch_shortcut, default.switch_shortcut);
            assert_eq!(workspace.chords().count(), 3, "{}", workspace.id);
        }
        assert_eq!(settings.navigation, defaults.navigation);
        let halves = settings.layout("halves").unwrap();
        assert_eq!(settings.options(halves).zone_shortcuts.len(), 2);
    }
}
