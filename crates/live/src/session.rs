//! The daemon under test: scratch settings, launch, and stop.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use snappy_core::settings::Settings;

use crate::input::pause;

/// How long the daemon gets to register shortcuts and observers after it appears.
const SETTLE_AFTER_LAUNCH: u64 = 1500;
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Daemon {
    app: PathBuf,
    home: PathBuf,
    /// The daemon process when the runner started it directly, so it can be reaped
    /// instead of lingering as a zombie that still looks like it runs.
    child: Mutex<Option<Child>>,
}

impl Daemon {
    /// `app` is the Snappy app bundle on macOS, the daemon binary elsewhere. `home` holds
    /// every file the daemon writes (`SNAPPY_HOME`).
    pub fn new(app: &Path, home: &Path) -> Self {
        Self {
            app: app.to_owned(),
            home: home.to_owned(),
            child: Mutex::new(None),
        }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn write_settings(&self, settings: &Settings) -> Result<(), String> {
        std::fs::create_dir_all(&self.home).map_err(|error| error.to_string())?;
        let bytes = settings.to_json().map_err(|error| error.to_string())?;
        std::fs::write(self.home.join("settings.json"), bytes).map_err(|error| error.to_string())
    }

    pub fn start(&self) -> Result<(), String> {
        self.launch()?;
        let started = Instant::now();
        while !self.is_running() {
            if started.elapsed() > LAUNCH_TIMEOUT {
                return Err("the daemon didn't start".to_owned());
            }
            pause(100);
        }
        pause(SETTLE_AFTER_LAUNCH);
        Ok(())
    }

    /// Launch through Launch Services, so the daemon is responsible for its own permissions
    /// instead of inheriting the runner's.
    #[cfg(target_os = "macos")]
    fn launch(&self) -> Result<(), String> {
        let status = Command::new("open")
            .arg("-n")
            .arg("--env")
            .arg(format!("SNAPPY_HOME={}", self.home.display()))
            .args(["--env", "SNAPPY_DEBUG=1"])
            .arg(&self.app)
            .args(["--args", "--background"])
            .status()
            .map_err(|error| error.to_string())?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| format!("open exited with {status}"))
    }

    /// Start the binary directly, with its output appended to `daemon.log` in its home.
    #[cfg(not(target_os = "macos"))]
    fn launch(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.home).map_err(|error| error.to_string())?;
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.home.join("daemon.log"))
            .map_err(|error| error.to_string())?;
        let errors = log.try_clone().map_err(|error| error.to_string())?;
        let child = Command::new(&self.app)
            .arg("--background")
            .env("SNAPPY_HOME", &self.home)
            .env("SNAPPY_DEBUG", "1")
            .stdout(log)
            .stderr(errors)
            .spawn()
            .map_err(|error| error.to_string())?;
        *self.child.lock().unwrap_or_else(PoisonError::into_inner) = Some(child);
        Ok(())
    }

    /// Ask the daemon to quit, as `killall` or `taskkill` would, and wait for it to finish.
    pub fn quit(&self) -> Result<(), String> {
        processes::request_quit();
        let started = Instant::now();
        while self.is_running() {
            if started.elapsed() > LAUNCH_TIMEOUT {
                return Err("the daemon didn't quit".to_owned());
            }
            pause(100);
        }
        Ok(())
    }

    /// The daemon's process ID while it runs.
    pub fn pid(&self) -> Option<i32> {
        processes::pid()
    }

    /// Stop the daemon abruptly, as a crash would.
    pub fn kill(&self) {
        processes::kill();
        let started = Instant::now();
        while self.is_running() && started.elapsed() < LAUNCH_TIMEOUT {
            pause(100);
        }
    }

    fn is_running(&self) -> bool {
        self.reap();
        processes::pid().is_some()
    }

    /// Collect the exit status of a daemon this runner started, once it has ended.
    fn reap(&self) {
        let mut child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        if child
            .as_mut()
            .is_some_and(|process| !matches!(process.try_wait(), Ok(None)))
        {
            *child = None;
        }
    }
}

/// The daemon's process ID while it runs, for code that has no `Daemon` at hand.
pub fn daemon_pid() -> Option<i32> {
    processes::pid()
}

/// Finding and stopping the daemon by its process name.
#[cfg(unix)]
mod processes {
    use std::process::Command;

    const PROCESS: &str = "snappy";

    /// SIGTERM, which the daemon handles like the menu's Quit.
    pub fn request_quit() {
        let _ = Command::new("pkill").args(["-TERM", "-x", PROCESS]).status();
    }

    pub fn kill() {
        let _ = Command::new("pkill").args(["-KILL", "-x", PROCESS]).status();
    }

    pub fn pid() -> Option<i32> {
        let output = Command::new("pgrep").args(["-x", PROCESS]).output().ok()?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()?
            .trim()
            .parse()
            .ok()
    }
}

#[cfg(windows)]
mod processes {
    use std::process::Command;

    const IMAGE: &str = "snappy.exe";

    /// A close request to the daemon's window, which it handles like the menu's Quit.
    pub fn request_quit() {
        if let Some(pid) = pid().and_then(|pid| u32::try_from(pid).ok()) {
            snappy_windows::reference::request_quit(pid);
        }
    }

    pub fn kill() {
        let _ = Command::new("taskkill").args(["/F", "/IM", IMAGE]).output();
    }

    /// From `tasklist`'s CSV output: `"snappy.exe","1234",...`.
    pub fn pid() -> Option<i32> {
        let filter = format!("IMAGENAME eq {IMAGE}");
        let output = Command::new("tasklist")
            .args(["/FI", &filter, "/FO", "CSV", "/NH"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| line.split(',').nth(1)?.trim_matches('"').parse().ok())
    }
}
