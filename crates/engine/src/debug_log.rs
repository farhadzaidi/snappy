//! The debug log: an opt-in record of what Snappy saw, what it decided, and what
//! actually happened, for finding out why a window, shortcut, or setting didn't behave as
//! expected. One JSON object per line. `warn` marks a result that differs from what was
//! asked for. Window titles are never recorded.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// Past this size the log moves to `debug.1.jsonl` and starts again.
const MAX_BYTES: u64 = 5 * 1024 * 1024;
const FILE_NAME: &str = "debug.jsonl";
const PREVIOUS_FILE_NAME: &str = "debug.1.jsonl";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warn => "warn",
        }
    }
}

pub(crate) struct DebugLog {
    dir: PathBuf,
    file: Option<File>,
    written: u64,
    /// Turned on for this run regardless of the setting, as live tests do.
    forced: bool,
}

impl DebugLog {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            file: None,
            written: 0,
            forced: false,
        }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(FILE_NAME)
    }

    pub fn is_on(&self) -> bool {
        self.file.is_some()
    }

    /// Follow the setting, unless the log was forced on.
    pub fn configure(&mut self, enabled: bool) {
        match (enabled || self.forced, self.file.is_some()) {
            (true, false) => self.open(),
            (false, true) => self.file = None,
            _ => {}
        }
    }

    pub fn force_on(&mut self) {
        self.forced = true;
        self.configure(true);
    }

    /// Append one record. `details`' fields join the line; anything else goes under
    /// `detail`.
    pub fn record(&mut self, at: f64, level: Level, event: &str, details: Value) {
        let Some(file) = &mut self.file else { return };
        let mut line = Map::new();
        line.insert("t".into(), Value::from((at * 1000.0).round() / 1000.0));
        line.insert("level".into(), level.as_str().into());
        line.insert("event".into(), event.into());
        match details {
            Value::Object(fields) => line.extend(fields),
            Value::Null => {}
            other => {
                line.insert("detail".into(), other);
            }
        }
        let Ok(mut text) = serde_json::to_string(&Value::Object(line)) else {
            return;
        };
        text.push('\n');
        if file.write_all(text.as_bytes()).is_err() {
            self.file = None;
            return;
        }
        self.written += text.len() as u64;
        if self.written > MAX_BYTES {
            self.roll();
        }
    }

    fn open(&mut self) {
        let path = self.path();
        self.file = fs::create_dir_all(&self.dir)
            .and_then(|()| open_private(&path))
            .ok();
        self.written = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    }

    fn roll(&mut self) {
        self.file = None;
        let _ = fs::rename(self.path(), self.dir.join(PREVIOUS_FILE_NAME));
        self.open();
    }
}

#[cfg(unix)]
fn open_private(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_private(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use serde_json::json;

    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("snappy-debug-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn lines(log: &DebugLog) -> Vec<Value> {
        fs::read_to_string(log.path())
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn the_debug_log_is_off_until_turned_on() {
        let mut log = DebugLog::new(dir("off"));
        log.record(1.0, Level::Info, "shortcut", json!({"action": "zone"}));
        assert!(!log.path().exists());

        log.configure(true);
        log.record(1.25, Level::Warn, "snap", json!({"outcome": "windowWriteFailed"}));
        log.configure(false);
        log.record(2.0, Level::Info, "ignored", Value::Null);
        assert_eq!(
            lines(&log),
            [json!({"t": 1.25, "level": "warn", "event": "snap", "outcome": "windowWriteFailed"})]
        );
    }

    #[test]
    fn a_forced_log_ignores_the_setting() {
        let mut log = DebugLog::new(dir("forced"));
        log.force_on();
        log.configure(false);
        assert!(log.is_on());
    }

    #[test]
    fn a_full_log_rolls_over() {
        let mut log = DebugLog::new(dir("roll"));
        log.configure(true);
        log.written = MAX_BYTES;
        log.record(1.0, Level::Info, "first", Value::Null);
        log.record(2.0, Level::Info, "second", Value::Null);
        assert_eq!(lines(&log).len(), 1);
        assert!(log.dir.join(PREVIOUS_FILE_NAME).exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_log_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let mut log = DebugLog::new(dir("private"));
        log.configure(true);
        let mode = fs::metadata(log.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
