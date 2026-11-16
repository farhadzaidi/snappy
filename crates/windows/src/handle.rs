//! Kernel handles closed when dropped.

use windows::Win32::Foundation::{CloseHandle, HANDLE};

/// A kernel handle closed when dropped.
pub struct OwnedHandle(pub HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle is owned and closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}
