//! Open at login: the value `Snappy` under the current user's Run key, holding the
//! daemon's quoted path and `--background`.

use std::path::Path;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

use crate::wide::{from_wide, wide};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE: PCWSTR = w!("Snappy");

/// What the Run value holds for `executable`.
pub fn command_line(executable: &Path) -> String {
    format!("\"{}\" --background", executable.display())
}

/// Whether a stored command starts this executable. Paths compare without case, as
/// Windows paths do.
pub fn starts(command: &str, executable: &Path) -> bool {
    let quoted = format!("\"{}\"", executable.display()).to_lowercase();
    command.trim().to_lowercase().starts_with(&quoted)
}

fn stored() -> Option<String> {
    let mut buffer = [0u16; 2048];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: the buffer and its byte size match.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    (read == ERROR_SUCCESS).then(|| from_wide(&buffer))
}

pub fn is_enabled(executable: &Path) -> bool {
    stored().is_some_and(|command| starts(&command, executable))
}

pub fn set_enabled(executable: &Path, on: bool) -> Result<(), String> {
    let result = if on {
        let command = wide(&command_line(executable));
        // SAFETY: the data is a null-terminated UTF-16 string of the given byte size.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE,
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                (command.len() * 2) as u32,
            )
        }
    } else {
        if stored().is_none() {
            return Ok(());
        }
        // SAFETY: plain registry call with constant names.
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE) }
    };
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!(
            "Snappy could not change its startup setting (error {}).",
            result.0
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn the_run_value_quotes_the_path_and_starts_in_the_background() {
        let exe = PathBuf::from(r"C:\Program Files\Snappy\snappy.exe");
        assert_eq!(
            command_line(&exe),
            r#""C:\Program Files\Snappy\snappy.exe" --background"#
        );
        assert!(starts(
            r#""c:\program files\snappy\SNAPPY.exe" --background"#,
            &exe
        ));
        assert!(!starts(r#""C:\Other\snappy.exe" --background"#, &exe));
    }
}
