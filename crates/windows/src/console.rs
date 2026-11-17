//! The daemon is a windowed program with no console of its own, so what it prints for a
//! command run from a terminal would go nowhere. Attaching to the terminal that started it
//! makes its standard output and error that terminal's.

use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

/// Writes from here on go to the console of the program that started this one, when it has
/// one. Output that was redirected stays where it was sent.
pub fn attach_to_parent_console() {
    // SAFETY: attaching has no preconditions; without a parent console it fails harmlessly.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}
