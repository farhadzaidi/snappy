//! Warnings on standard error, marked for the journal. When systemd connects standard error
//! to the journal it says so in `JOURNAL_STREAM`, and a `<4>` prefix files the line at warning
//! priority; elsewhere the line is written as it is.

use std::fmt::Display;
use std::os::unix::fs::MetadataExt;

/// syslog's warning priority, as a journal stream prefix.
const WARNING_PREFIX: &str = "<4>";

/// Whether standard error is the journal stream systemd named: `device:inode`.
fn stderr_is_journal(stream: Option<&str>, stderr: Option<(u64, u64)>) -> bool {
    let (Some(stream), Some((device, inode))) = (stream, stderr) else {
        return false;
    };
    stream == format!("{device}:{inode}")
}

fn stderr_identity() -> Option<(u64, u64)> {
    let metadata = std::fs::metadata("/proc/self/fd/2").ok()?;
    Some((metadata.dev(), metadata.ino()))
}

fn line(message: &dyn Display, journal: bool) -> String {
    let prefix = if journal { WARNING_PREFIX } else { "" };
    format!("{prefix}snappy: {message}")
}

/// Writes `snappy: <message>` to standard error at warning priority.
pub fn warn(message: impl Display) {
    let stream = std::env::var("JOURNAL_STREAM").ok();
    let journal = stderr_is_journal(stream.as_deref(), stderr_identity());
    eprintln!("{}", line(&message, journal));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_named_journal_stream_gets_the_warning_prefix() {
        assert!(stderr_is_journal(Some("8:1234"), Some((8, 1234))));
        assert!(!stderr_is_journal(Some("8:1234"), Some((8, 99))));
        assert!(!stderr_is_journal(None, Some((8, 1234))));
        assert_eq!(line(&"overlay failed", true), "<4>snappy: overlay failed");
        assert_eq!(line(&"overlay failed", false), "snappy: overlay failed");
    }
}
