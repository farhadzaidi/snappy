//! Open at login. Service Management needs no particular thread, so this runs on the
//! engine's; only opening System Settings goes to the main thread.

use objc2::rc::Retained;
#[cfg(not(feature = "mac-app-store"))]
use objc2_foundation::ns_string;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

use crate::main_thread;

/// The launch agent bundled in the app, which starts the daemon with `--background`.
#[cfg(not(feature = "mac-app-store"))]
fn service() -> Retained<SMAppService> {
    // SAFETY: plain class constructor.
    unsafe { SMAppService::agentServiceWithPlistName(ns_string!("io.github.farhadzaidi.Snappy.Agent.plist")) }
}

/// The app itself. The Mac App Store build can't carry a launch agent, so macOS opens the
/// app at login, and `login_launch` tells that from the user opening it.
#[cfg(feature = "mac-app-store")]
fn service() -> Retained<SMAppService> {
    // SAFETY: plain class constructor.
    unsafe { SMAppService::mainAppService() }
}

fn status(service: &SMAppService) -> SMAppServiceStatus {
    // SAFETY: plain property read.
    unsafe { service.status() }
}

/// Registered, including while the system waits for the user's approval.
pub fn is_enabled() -> bool {
    let status = status(&service());
    status == SMAppServiceStatus::Enabled || status == SMAppServiceStatus::RequiresApproval
}

pub fn set_enabled(on: bool) -> Result<(), String> {
    let service = service();
    // SAFETY: plain method calls on a valid service.
    let result = unsafe {
        if on {
            service.registerAndReturnError()
        } else {
            service.unregisterAndReturnError()
        }
    };
    if on && status(&service) == SMAppServiceStatus::RequiresApproval {
        // SAFETY: plain class method.
        main_thread::spawn(|_| unsafe { SMAppService::openSystemSettingsLoginItems() });
        return Ok(());
    }
    result.map_err(|error| error.localizedDescription().to_string())
}
