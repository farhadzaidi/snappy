//! Facts about the process behind a window: its launch, executable, and integrity level.

use std::sync::OnceLock;

use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
    TokenIntegrityLevel,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, GetProcessTimes, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::PWSTR;

use crate::identity::Owner;
use crate::wide::from_wide;

/// A kernel handle closed when dropped.
pub struct OwnedHandle(pub HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle is owned and closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Opening with limited query rights works for every process, elevated ones included.
fn open(pid: u32) -> Option<OwnedHandle> {
    // SAFETY: a plain open; failure returns an error.
    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .ok()
        .map(OwnedHandle)
}

pub fn current_pid() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetCurrentProcessId() }
}

/// The process ID plus its creation time, which together identify one launch.
pub fn owner(pid: u32) -> Option<Owner> {
    let process = open(pid)?;
    let mut created = FILETIME::default();
    let (mut exited, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: valid handle and out pointers.
    unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) }.ok()?;
    Some(Owner {
        pid,
        created: (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
    })
}

/// Seconds between 1601, where `FILETIME` counts from, and 1970.
const FILETIME_UNIX_OFFSET: f64 = 11_644_473_600.0;

/// A launch time in Unix seconds, for recovery keys.
pub fn launch_seconds(owner: Owner) -> f64 {
    owner.created as f64 / 10_000_000.0 - FILETIME_UNIX_OFFSET
}

/// The executable's file name, such as `notepad.exe`.
pub fn executable(pid: u32) -> Option<String> {
    let process = open(pid)?;
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;
    // SAFETY: the buffer and its length match.
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    }
    .ok()?;
    let path = from_wide(&buffer[..length as usize]);
    path.rsplit('\\').next().map(str::to_owned)
}

/// The mandatory integrity level's relative identifier, such as 0x2000 for medium.
fn integrity_level(process: HANDLE) -> Option<u32> {
    let mut token = HANDLE::default();
    // SAFETY: valid process handle and out pointer.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok()?;
    let token = OwnedHandle(token);
    let mut needed = 0u32;
    // SAFETY: a size query with no buffer.
    let _ = unsafe { GetTokenInformation(token.0, TokenIntegrityLevel, None, 0, &mut needed) };
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: the buffer holds `needed` bytes, aligned for the label structure.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenIntegrityLevel,
            Some(buffer.as_mut_ptr().cast()),
            needed,
            &mut needed,
        )
    }
    .ok()?;
    // SAFETY: the call filled the buffer with a TOKEN_MANDATORY_LABEL whose SID points
    // inside it; the SID has at least one sub-authority.
    unsafe {
        let label = &*buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let sid = label.Label.Sid;
        let count = *GetSidSubAuthorityCount(sid);
        Some(*GetSidSubAuthority(sid, u32::from(count.checked_sub(1)?)))
    }
}

fn own_integrity_level() -> Option<u32> {
    static LEVEL: OnceLock<Option<u32>> = OnceLock::new();
    *LEVEL.get_or_init(|| open(current_pid()).and_then(|process| integrity_level(process.0)))
}

/// Whether `pid` runs above Snappy's integrity level. A process whose level can't be
/// read counts as above, since Snappy couldn't move its windows either.
pub fn is_elevated_above_self(pid: u32) -> bool {
    let theirs = open(pid).and_then(|process| integrity_level(process.0));
    match (theirs, own_integrity_level()) {
        (Some(theirs), Some(ours)) => theirs > ours,
        (None, _) => true,
        (Some(_), None) => false,
    }
}
