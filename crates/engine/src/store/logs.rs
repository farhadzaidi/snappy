//! Diagnostic logs: bounded JSON lists Snappy appends to and people read.

use std::fs;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::atomic_write::write_json;

/// Diagnostic logs are read back only below this size.
const LOG_READ_LIMIT: u64 = 64 * 1024;

/// Write a diagnostic log readable only by the user. Logs are read back only by Snappy's
/// own appends and by people, so they carry no version.
pub fn write_log<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    write_json(path, value)
}

/// Append `entry` to a bounded JSON log, keeping the newest `keep` entries. Failures are
/// ignored: diagnostics never affect window control.
pub fn append_log<T: Serialize + DeserializeOwned>(path: &Path, entry: T, keep: usize) {
    let small_enough = fs::metadata(path)
        .map(|m| m.len() <= LOG_READ_LIMIT)
        .unwrap_or(true);
    let mut entries: Vec<T> = if small_enough {
        fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    entries.push(entry);
    let excess = entries.len().saturating_sub(keep);
    entries.drain(..excess);
    let _ = write_log(path, &entries);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_dirs::temp_dir;

    #[test]
    fn logs_keep_only_the_newest_entries() {
        let dir = temp_dir("log");
        let path = dir.join("drops.json");
        for value in 0..25 {
            append_log(&path, value, 20);
        }
        let kept: Vec<i32> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(kept.first(), Some(&5));
        assert_eq!(kept.len(), 20);
    }
}
