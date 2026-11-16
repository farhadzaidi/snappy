//! The settings file the user chose to import or export. The app opens it, not the daemon:
//! a file picker grants the chosen file to the process that showed it, and in the Mac App
//! Store's sandbox only that process may open it. The daemon checks what's inside.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use serde::{Serialize, Serializer};
use snappy_core::settings::MAX_FILE_BYTES;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SettingsFileError {
    #[error("Couldn't read that file.")]
    Unreadable,
    #[error("That file isn't Snappy settings.")]
    NotSettings,
    #[error("Couldn't save to that file.")]
    Unwritable,
}

/// The UI shows the message as it is.
impl Serialize for SettingsFileError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Read a file's text for import, refusing what can't be a settings file before it's sent.
pub fn read(path: &Path) -> Result<String, SettingsFileError> {
    let file = File::open(path).map_err(|_| SettingsFileError::Unreadable)?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SettingsFileError::Unreadable)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(SettingsFileError::NotSettings);
    }
    let text = String::from_utf8(bytes).map_err(|_| SettingsFileError::NotSettings)?;
    // JSON never holds these raw. Each would grow sixfold as an escape in the import command,
    // so refusing them keeps the command well inside a message.
    if text.chars().any(|c| c < ' ' && !matches!(c, '\t' | '\n' | '\r')) {
        return Err(SettingsFileError::NotSettings);
    }
    Ok(text)
}

/// Write exported settings. Straight into the file, not through a temporary one beside it:
/// the sandbox grants the chosen file, not its folder.
pub fn write(path: &Path, contents: &str) -> Result<(), SettingsFileError> {
    fs::write(path, contents).map_err(|_| SettingsFileError::Unwritable)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use snappy_ipc::{Command, MAX_MESSAGE_BYTES, ToDaemon};

    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("snappy-app-{}-{name}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn written_settings_read_back_as_they_were() {
        let path = folder("round-trip").join("Snappy-layouts.json");
        write(&path, "{\n  \"version\": 4\n}").unwrap();
        assert_eq!(read(&path).unwrap(), "{\n  \"version\": 4\n}");
    }

    #[test]
    fn a_missing_file_is_unreadable() {
        let path = folder("missing").join("absent.json");
        assert_eq!(read(&path), Err(SettingsFileError::Unreadable));
    }

    #[test]
    fn files_too_large_binary_or_holding_control_characters_arent_settings() {
        let folder = folder("not-settings");
        let large = folder.join("large.json");
        fs::write(&large, vec![b' '; MAX_FILE_BYTES + 1]).unwrap();
        assert_eq!(read(&large), Err(SettingsFileError::NotSettings));
        let binary = folder.join("binary.json");
        fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(read(&binary), Err(SettingsFileError::NotSettings));
        let zeros = folder.join("zeros.json");
        fs::write(&zeros, [0_u8; 16]).unwrap();
        assert_eq!(read(&zeros), Err(SettingsFileError::NotSettings));
    }

    /// The largest file read sends a command that still fits in a message, even when every
    /// character needs escaping.
    #[test]
    fn the_largest_readable_file_fits_in_one_message() {
        let path = folder("largest").join("quotes.json");
        fs::write(&path, vec![b'"'; MAX_FILE_BYTES]).unwrap();
        let contents = read(&path).unwrap();
        let command = ToDaemon::Command {
            id: u64::MAX,
            command: Box::new(Command::ImportSettings { contents }),
        };
        let mut frame = Vec::new();
        snappy_ipc::write_message(&mut frame, &command).unwrap();
        assert!(frame.len() <= MAX_MESSAGE_BYTES as usize + 4);
    }

    #[test]
    fn errors_reach_the_ui_as_their_message() {
        assert_eq!(
            serde_json::to_string(&SettingsFileError::NotSettings).unwrap(),
            "\"That file isn't Snappy settings.\""
        );
    }
}
