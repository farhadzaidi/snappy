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
use snappy_live::input::{PlatformInput, pause};
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
/// workspaces on with shortcuts for each. The previous, next, and move-window steps keep
/// their defaults, which are the same on every platform.
fn test_settings() -> Result<Settings, String> {
    let mut settings = Settings::defaults(PLATFORM);
    settings.snapping.enabled = true;
    settings.layouts.default_layout_id = "halves".to_owned();
    settings.workspaces.enabled = true;
    match PLATFORM {
        Platform::MacOs => assign_macos_workspace_shortcuts(&mut settings),
        Platform::Windows => assign_windows_workspace_shortcuts(&mut settings),
        Platform::Linux => assign_linux_workspace_shortcuts(&mut settings),
    }
    settings
        .validate(PLATFORM)
        .map_err(|error| format!("the test settings are invalid: {error}"))
}

/// Give workspace n the chords `keys(n)` returns, to switch, send, and move here and switch.
fn assign_workspace_shortcuts(
    settings: &mut Settings,
    keys: impl Fn(usize) -> (String, String),
    switch: &[Modifier],
    send: &[Modifier],
    send_and_switch: &[Modifier],
) {
    for (index, workspace) in settings.workspaces.items.iter_mut().enumerate() {
        let (code, label) = keys(index + 1);
        let chord = |mods: &[Modifier]| Some(KeyChord::new(&code, mods.iter().copied()).with_label(&label));
        workspace.switch_shortcut = chord(switch);
        workspace.send_shortcut = chord(send);
        workspace.send_and_switch_shortcut = chord(send_and_switch);
    }
}

fn digit(n: usize) -> (String, String) {
    (format!("Digit{n}"), n.to_string())
}

/// Control with a digit, plus Shift to send or Option to move here and switch.
fn assign_macos_workspace_shortcuts(settings: &mut Settings) {
    assign_workspace_shortcuts(
        settings,
        digit,
        &[Modifier::Ctrl],
        &[Modifier::Ctrl, Modifier::Shift],
        &[Modifier::Ctrl, Modifier::Alt],
    );
}

/// The taskbar owns Win with Ctrl, Alt, or Shift plus a digit, so these use F-keys with
/// the Snappy key, which nothing in Windows uses.
fn assign_windows_workspace_shortcuts(settings: &mut Settings) {
    assign_workspace_shortcuts(
        settings,
        |n| (format!("F{n}"), format!("F{n}")),
        &[Modifier::Ctrl, Modifier::Alt, Modifier::Meta],
        &[Modifier::Ctrl, Modifier::Alt, Modifier::Shift, Modifier::Meta],
        &[Modifier::Ctrl, Modifier::Alt, Modifier::Shift],
    );
}

/// Super with Ctrl or Alt and a digit, which the zone shortcuts and the container's
/// window manager leave free.
fn assign_linux_workspace_shortcuts(settings: &mut Settings) {
    assign_workspace_shortcuts(
        settings,
        digit,
        &[Modifier::Ctrl, Modifier::Meta],
        &[Modifier::Ctrl, Modifier::Shift, Modifier::Meta],
        &[Modifier::Alt, Modifier::Meta],
    );
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
    let mut input = PlatformInput::new()?;
    let mut results = Vec::new();
    {
        let debug_log = daemon.home().join("logs").join("debug.jsonl");
        let mut context = Context::new(
            &daemon,
            &settings,
            &mut fixture,
            &mut input,
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
            context.close_all();
            let result = match outcome {
                Ok(outcome) => ScenarioResult {
                    name: (*name).to_owned(),
                    passed: true,
                    elapsed_ms: outcome.elapsed_ms,
                    detail: outcome.note,
                },
                Err(error) => ScenarioResult {
                    name: (*name).to_owned(),
                    passed: false,
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
    println!("{passed} of {} scenarios passed", results.len());
    Ok(passed == results.len())
}

fn report(result: &ScenarioResult) {
    let mark = if result.passed { "pass" } else { "FAIL" };
    let timing = result
        .elapsed_ms
        .map(|ms| format!(" ({ms:.0} ms)"))
        .unwrap_or_default();
    let detail = result
        .detail
        .as_deref()
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
    fn the_test_settings_are_valid_and_cover_every_workspace_shortcut() {
        let settings = test_settings().unwrap();
        let workspaces = &settings.workspaces;
        assert!(workspaces.enabled);
        assert_eq!(workspaces, &{
            let mut expected = Settings::defaults(PLATFORM).workspaces;
            expected.enabled = true;
            expected.items.clone_from(&workspaces.items);
            expected
        });
        assert_eq!(workspaces.steps().count(), 4);
        for workspace in &workspaces.items {
            assert_eq!(workspace.chords().count(), 3, "{}", workspace.id);
        }
    }
}
