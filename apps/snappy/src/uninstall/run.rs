//! The steps of `snappy --uninstall`, in order, each said as it happens.

use std::process::ExitCode;

use snappy_engine::Paths;
use snappy_platform::uninstall::{QuitRequest, Removal};

use super::messages::{
    DONE, NO_HOME, NOT_DONE, NOT_RUNNING, NOT_SETTLED, QUIT, SETTLING, STILL_HIDDEN, STILL_RUNNING, deleted,
    kept_settings, not_deleted,
};
use super::{folders, system};
use crate::paths;
use crate::runtime::Mode;

/// Removes everything Snappy added and its files, keeping the settings file when
/// `keep_settings` says so. Fails while Snappy still runs, and when anything is left.
pub fn run(keep_settings: bool) -> ExitCode {
    let Some(paths) = paths::snappy_paths() else {
        println!("{NO_HOME}");
        return ExitCode::FAILURE;
    };
    match system::request_quit(&paths) {
        QuitRequest::Quit => println!("{QUIT}"),
        QuitRequest::NotRunning => {
            println!("{NOT_RUNNING}");
            if left_behind(&paths) {
                settle(&paths);
            }
        }
        QuitRequest::StillRunning => {
            println!("{STILL_RUNNING}");
            return ExitCode::FAILURE;
        }
    }
    if paths.journal().exists() {
        println!("{STILL_HIDDEN}");
    }

    let mut complete = true;
    for removal in system::remove_additions() {
        match removal {
            Removal::Removed(message) => println!("{message}"),
            Removal::Nothing => {}
            Removal::Failed(message) => {
                println!("{message}");
                complete = false;
            }
        }
    }
    let keep = keep_settings.then_some(paths.settings.as_path());
    for target in paths::uninstall_targets(&paths) {
        match folders::remove(&target, keep) {
            Ok(true) => println!("{}", deleted(&target)),
            Ok(false) => {}
            Err(error) => {
                println!("{}", not_deleted(&target, &error));
                complete = false;
            }
        }
    }
    if keep_settings && paths.settings.exists() {
        println!("{}", kept_settings(&paths.settings));
    }

    if complete {
        println!("{DONE}");
        ExitCode::SUCCESS
    } else {
        println!("{NOT_DONE}");
        ExitCode::FAILURE
    }
}

/// Whether a run that ended without quitting left windows hidden or Caps Lock changed.
fn left_behind(paths: &Paths) -> bool {
    paths.journal().exists() || paths.caps_lock().exists()
}

/// Starts Snappy once to pick up what an earlier run left, and quits.
fn settle(paths: &Paths) {
    println!("{SETTLING}");
    if let Err(error) = crate::platform::run(paths.clone(), Mode::Settle) {
        eprintln!("snappy: {error}");
        println!("{NOT_SETTLED}");
    }
}
