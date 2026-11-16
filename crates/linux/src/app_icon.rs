//! Snappy's app icon in the user's hicolor icon theme, so the desktop entries Snappy writes
//! can name it with `Icon=` the way installed apps do. One scalable SVG covers every size.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The icon's name in the theme, after Snappy's application ID.
pub const NAME: &str = "io.github.farhadzaidi.Snappy";

const SVG: &[u8] = include_bytes!("../../../assets/icon/snappy.svg");

/// `$XDG_DATA_HOME/icons/hicolor/scalable/apps/<name>.svg`, defaulting to `~/.local/share`.
fn path() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|data| path_in(&data))
}

fn path_in(data: &Path) -> PathBuf {
    data.join("icons/hicolor/scalable/apps")
        .join(format!("{NAME}.svg"))
}

/// Puts the icon in place before an entry names it. A missing icon only costs the entry its
/// picture, so a failure goes to the daemon's output and nothing else.
pub fn install() {
    let result = match path() {
        Some(path) => install_at(&path),
        None => Err(io::Error::new(io::ErrorKind::NotFound, "no data folder")),
    };
    if let Err(error) = result {
        eprintln!("snappy: can't install the app icon: {error}");
    }
}

/// Writes the icon unless the same one is already there.
fn install_at(path: &Path) -> io::Result<()> {
    if fs::read(path).is_ok_and(|existing| existing == SVG) {
        return Ok(());
    }
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    fs::write(path, SVG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_written_once_and_replaced_when_it_differs() {
        let folder = std::env::temp_dir().join(format!("snappy-icon-{}", std::process::id()));
        let path = folder.join("hicolor/scalable/apps/snappy.svg");

        install_at(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), SVG);

        fs::write(&path, "<svg/>").unwrap();
        install_at(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), SVG);
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_icon_goes_in_the_hicolor_theme_under_its_name() {
        assert_eq!(
            path_in(Path::new("/home/me/.local/share")),
            Path::new("/home/me/.local/share/icons/hicolor/scalable/apps/io.github.farhadzaidi.Snappy.svg")
        );
        assert!(SVG.starts_with(b"<svg "));
    }
}
