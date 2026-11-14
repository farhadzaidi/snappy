//! Turning Caps Lock on or off by pressing the keymap's real Caps Lock through KWin's
//! `org_kde_kwin_fake_input`, which KWin handles like a keyboard, so the lock and every
//! keyboard's light follow. With Snappy's option the Caps Lock key's own first level is
//! the Snappy key, and its second, with Shift, is the real Caps Lock.
//!
//! Plasma 6.8 lets any program outside a sandbox use fake input. Earlier versions let only a
//! program named by an applications entry whose `Exec` is its executable and whose
//! `X-KDE-Wayland-Interfaces` lists the protocol, so Snappy writes one while Caps Lock is the
//! Snappy key.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use snappy_linux::wayland::{Arg, Connection};

const INTERFACE: &str = "org_kde_kwin_fake_input";
/// Version 4 added keyboard keys.
const VERSION: u32 = 4;
const AUTHENTICATE: u16 = 0;
const KEYBOARD_KEY: u16 = 10;
const PRESSED: u32 = 1;
const RELEASED: u32 = 0;
/// Linux input codes, which the protocol takes.
const LEFT_SHIFT: u32 = 42;
const CAPS_LOCK: u32 = 58;

/// A fake keyboard, kept while Caps Lock is the Snappy key.
pub struct FakeKeyboard {
    connection: Connection,
    device: u32,
}

impl FakeKeyboard {
    pub fn open() -> io::Result<FakeKeyboard> {
        let mut connection = Connection::open()?;
        let (device, version) = connection.bind(INTERFACE, VERSION)?;
        if version < VERSION {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "KWin's fake input can't press keys",
            ));
        }
        connection.send(
            device,
            AUTHENTICATE,
            &[
                Arg::Str("Snappy"),
                Arg::Str("Turn Caps Lock on or off when Caps Lock is the Snappy key"),
            ],
        )?;
        connection.roundtrip()?;
        Ok(FakeKeyboard { connection, device })
    }

    /// Shift + Caps Lock, pressed and let go, and waits until KWin has handled it.
    pub fn press_caps_lock(&mut self) -> io::Result<()> {
        for (key, state) in [
            (LEFT_SHIFT, PRESSED),
            (CAPS_LOCK, PRESSED),
            (CAPS_LOCK, RELEASED),
            (LEFT_SHIFT, RELEASED),
        ] {
            self.connection
                .send(self.device, KEYBOARD_KEY, &[Arg::Uint(key), Arg::Uint(state)])?;
        }
        self.connection.roundtrip().map(drop)
    }
}

const ENTRY_NAME: &str = "io.github.farhadzaidi.Snappy.CapsLock.desktop";

/// `$XDG_DATA_HOME/applications/…`, defaulting to `~/.local/share`.
pub fn entry_path() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|data| data.join("applications").join(ENTRY_NAME))
}

/// The entry naming `executable`. KWin takes the first word of `Exec`, as Qt splits a
/// command, and compares its canonical path with the program's, so the path is quoted;
/// KConfig reads a doubled backslash as one.
pub fn entry(executable: &Path) -> String {
    let path = executable.to_string_lossy().replace('\\', "\\\\");
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Snappy\n\
         Comment=Lets Snappy turn Caps Lock on and off while Caps Lock is the Snappy key\n\
         Exec=\"{path}\"\n\
         NoDisplay=true\n\
         X-KDE-Wayland-Interfaces={INTERFACE}\n"
    )
}

/// Writes the entry for this program.
pub fn write_entry() -> io::Result<()> {
    let path = entry_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no data folder"))?;
    let executable = std::env::current_exe()?.canonicalize()?;
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    fs::write(path, entry(&executable))
}

pub fn remove_entry() -> io::Result<()> {
    let Some(path) = entry_path() else {
        return Ok(());
    };
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_names_the_program_and_asks_for_fake_input_without_showing_in_menus() {
        let entry = entry(Path::new("/home/me/.local/bin/snappy"));
        assert!(entry.starts_with("[Desktop Entry]\nType=Application\n"));
        assert!(entry.contains("\nExec=\"/home/me/.local/bin/snappy\"\n"));
        assert!(entry.contains("\nNoDisplay=true\n"));
        assert!(entry.contains("\nX-KDE-Wayland-Interfaces=org_kde_kwin_fake_input\n"));
    }

    #[test]
    fn a_path_with_spaces_stays_one_word() {
        let entry = entry(Path::new("/opt/My Apps/snappy"));
        assert!(entry.contains("\nExec=\"/opt/My Apps/snappy\"\n"));
    }
}
