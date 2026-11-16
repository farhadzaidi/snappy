//! Versioned state files: reading them, moving aside ones that can't be read, and writing them.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::atomic_write::write_json;

/// State files are never read past this size.
const STATE_READ_LIMIT: u64 = 1_000_000;

/// The shape of a state file. Each file says which version of its shape it holds, so a later
/// Snappy can tell an older file from a damaged one; a change to the shape bumps `VERSION`.
pub trait StateFile: Serialize + DeserializeOwned {
    const VERSION: u32;

    /// Read a file from before state files had a version.
    fn unversioned(value: serde_json::Value) -> Option<Self>;
}

/// What reading a state file found.
#[derive(Debug, PartialEq)]
pub enum ReadState<T> {
    Missing,
    Read(T),
    /// The file couldn't be read, or holds a version Snappy doesn't know. It was moved
    /// aside, to this path when that worked, so it neither blocks Snappy nor is lost.
    MovedAside(Option<PathBuf>),
}

impl<T> ReadState<T> {
    pub fn read(self) -> Option<T> {
        match self {
            ReadState::Read(value) => Some(value),
            ReadState::Missing | ReadState::MovedAside(_) => None,
        }
    }
}

/// Read a state file, moving it aside when it can't be read.
pub fn read_state<T: StateFile>(path: &Path) -> ReadState<T> {
    let bytes = match fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return ReadState::Missing,
        Ok(metadata) if metadata.len() < STATE_READ_LIMIT => fs::read(path).ok(),
        _ => None,
    };
    match bytes.and_then(|bytes| parse_state(&bytes)) {
        Some(value) => ReadState::Read(value),
        None => ReadState::MovedAside(move_aside(path)),
    }
}

fn parse_state<T: StateFile>(bytes: &[u8]) -> Option<T> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    match value.get("version").map(serde_json::Value::as_u64) {
        None => T::unversioned(value),
        Some(version) if version == Some(u64::from(T::VERSION)) => serde_json::from_value(value).ok(),
        Some(_) => None,
    }
}

/// `name.json` becomes `name.unreadable.json`, replacing an earlier one.
fn move_aside(path: &Path) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    let aside = path.with_file_name(format!("{stem}.unreadable.json"));
    fs::rename(path, &aside).ok().map(|()| aside)
}

/// Write a state file readable only by the user, with its version.
pub fn write_state<T: StateFile>(path: &Path, value: &T) -> std::io::Result<()> {
    let mut json = serde_json::to_value(value).map_err(std::io::Error::other)?;
    let Some(fields) = json.as_object_mut() else {
        return Err(std::io::Error::other("a state file holds an object"));
    };
    fields.insert("version".to_owned(), T::VERSION.into());
    write_json(path, &json)
}

pub fn remove_state(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_dirs::temp_dir;

    #[derive(Debug, PartialEq, Serialize, serde::Deserialize)]
    struct Numbers {
        numbers: Vec<i32>,
    }

    impl StateFile for Numbers {
        const VERSION: u32 = 2;

        fn unversioned(value: serde_json::Value) -> Option<Self> {
            serde_json::from_value(value).ok().map(|numbers| Self { numbers })
        }
    }

    fn numbers() -> Numbers {
        Numbers {
            numbers: vec![1, 2, 3],
        }
    }

    #[cfg(unix)]
    #[test]
    fn state_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("private");
        let path = dir.join("state.json");
        write_state(&path, &numbers()).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn state_files_say_their_version_and_read_back() {
        let dir = temp_dir("versioned");
        let path = dir.join("state.json");
        write_state(&path, &numbers()).unwrap();
        let written: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["version"], 2);
        assert_eq!(read_state(&path), ReadState::Read(numbers()));
        assert_eq!(read_state::<Numbers>(&dir.join("none.json")), ReadState::Missing);
    }

    #[test]
    fn a_state_file_from_before_versions_still_reads() {
        let dir = temp_dir("unversioned");
        let path = dir.join("state.json");
        fs::write(&path, b"[1, 2, 3]").unwrap();
        assert_eq!(read_state(&path), ReadState::Read(numbers()));
    }

    #[test]
    fn an_unreadable_state_file_or_one_of_another_version_is_moved_aside() {
        let dir = temp_dir("aside");
        let path = dir.join("state.json");
        let aside = dir.join("state.unreadable.json");
        for contents in [&b"{ broken"[..], br#"{ "version": 3, "numbers": [1] }"#] {
            fs::write(&path, contents).unwrap();
            assert_eq!(
                read_state::<Numbers>(&path),
                ReadState::MovedAside(Some(aside.clone()))
            );
            assert!(!path.exists());
            assert_eq!(fs::read(&aside).unwrap(), contents, "kept, not lost");
        }
    }
}
