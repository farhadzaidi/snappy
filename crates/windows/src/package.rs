//! The MSIX package Snappy runs from, when one installed it. A packaged process has the
//! package's identity, which changes how Windows starts it at login and where its writes to
//! `%APPDATA%` and `%LOCALAPPDATA%` land.

use std::path::PathBuf;

use windows::ApplicationModel::Activation::ActivationKind;
use windows::ApplicationModel::AppInstance;
use windows::Storage::ApplicationData;
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

/// Whether this process runs with a package's identity.
pub fn is_packaged() -> bool {
    let mut length = 0u32;
    // SAFETY: with no buffer and a length of zero the call only reports the length it needs.
    let status = unsafe { GetCurrentPackageFullName(&mut length, None) };
    has_identity(status)
}

/// Asked for the name's length, a packaged process hears the buffer is too small; an unpackaged
/// one hears `APPMODEL_ERROR_NO_PACKAGE`.
fn has_identity(status: WIN32_ERROR) -> bool {
    status == ERROR_INSUFFICIENT_BUFFER || status == ERROR_SUCCESS
}

/// The package's `LocalState` folder, or `None` when unpackaged. Windows redirects a package's
/// writes to `%LOCALAPPDATA%` into a hidden folder of its own, so a path there that Snappy
/// shows would lead nowhere in Explorer; this folder is where it says it is.
pub fn package_data_folder() -> Option<PathBuf> {
    if !is_packaged() {
        return None;
    }
    let path = ApplicationData::Current()
        .and_then(|data| data.LocalFolder())
        .and_then(|folder| folder.Path())
        .ok()?;
    Some(PathBuf::from(path.to_os_string()))
}

/// Whether Windows started this process as the package's startup task, at sign-in. A startup
/// task can't pass arguments, so this stands in for `--background`.
pub fn started_at_login() -> bool {
    is_packaged()
        && AppInstance::GetActivatedEventArgs()
            .and_then(|arguments| arguments.Kind())
            .is_ok_and(|kind| kind == ActivationKind::StartupTask)
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_INVALID_PARAMETER};

    use super::*;

    #[test]
    fn a_process_has_a_package_only_when_its_name_needs_room() {
        assert!(has_identity(ERROR_INSUFFICIENT_BUFFER));
        assert!(!has_identity(APPMODEL_ERROR_NO_PACKAGE));
        assert!(
            !has_identity(ERROR_INVALID_PARAMETER),
            "an unexpected failure isn't a package"
        );
    }

    #[test]
    fn a_development_build_runs_unpackaged() {
        assert!(!is_packaged());
        assert_eq!(package_data_folder(), None);
        assert!(!started_at_login());
    }
}
