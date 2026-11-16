//! Writing a file whole or not at all, through a temporary file renamed into place.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::Serialize;

/// Write JSON readable only by the user.
pub(super) fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    write_atomic(path, &bytes, true)
}

/// Write via a temporary file in the same folder and rename it into place.
pub(super) fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> std::io::Result<()> {
    let folder = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(folder)?;
    let temporary = folder.join(format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("snappy")
    ));
    {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if private {
        restrict_to_owner(&temporary)?;
    }
    fs::rename(&temporary, path)
}

#[cfg(unix)]
fn restrict_to_owner(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
