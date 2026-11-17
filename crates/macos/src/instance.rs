//! One daemon per user: a lock on `daemon.lock` in the state folder, which holds the
//! daemon's process ID. A second launch exits, and opening Snappy.app reaches the running
//! daemon as a reopen event instead. `snappy --uninstall` sends that process `SIGTERM`.
//!
//! Requests from outside. `SIGTERM`, from `kill`, `killall Snappy`, or logging out, quits
//! the way the menu bar's Quit does, so parked windows come back first, and so do `SIGINT`
//! and `SIGHUP`, from Ctrl+C or closing the terminal Snappy was started from. `SIGUSR2` stops
//! for a restart, as when installing a new build: windows stay where they are, and the next
//! launch puts them back in their workspaces.

use std::ffi::c_void;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use dispatch2::{DispatchObject, DispatchQueue, DispatchRetained, DispatchSource};
use snappy_platform::PlatformEvent;
use snappy_platform::uninstall::QuitRequest;

use crate::events::emit;

const LOCK_NAME: &str = "daemon.lock";

pub enum Claim {
    /// This process is the daemon. Keep the lock for its lifetime.
    Primary(InstanceLock),
    /// Another daemon holds the lock.
    Secondary,
}

/// The running daemon's lock, released when dropped.
pub struct InstanceLock {
    _file: File,
}

/// Claims the daemon role with an exclusive lock in `state_dir`, which then holds this
/// process's ID for `request_quit`. A lock the file system can't take at all doesn't stop
/// Snappy from running.
pub fn claim(state_dir: &Path) -> std::io::Result<Claim> {
    std::fs::create_dir_all(state_dir)?;
    // Opened without truncating, so a second launch leaves the running daemon's ID alone.
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(state_dir.join(LOCK_NAME))?;
    match file.try_lock() {
        Ok(()) => {
            // Snappy runs whether or not its ID could be written; only `request_quit` needs it.
            let _ = write_pid(&mut file);
            Ok(Claim::Primary(InstanceLock { _file: file }))
        }
        Err(TryLockError::Error(_)) => Ok(Claim::Primary(InstanceLock { _file: file })),
        Err(TryLockError::WouldBlock) => Ok(Claim::Secondary),
    }
}

fn write_pid(file: &mut File) -> std::io::Result<()> {
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    write!(file, "{}", std::process::id())?;
    file.flush()
}

/// How often `request_quit` looks whether the daemon has let go of its lock.
const QUIT_POLL: Duration = Duration::from_millis(100);

/// Asks the daemon holding the lock in `state_dir` to quit, as `SIGTERM` does, and waits up
/// to `wait` for it to let go of the lock as it exits.
pub fn request_quit(state_dir: &Path, wait: Duration) -> QuitRequest {
    let Ok(mut file) = OpenOptions::new().read(true).open(state_dir.join(LOCK_NAME)) else {
        return QuitRequest::NotRunning;
    };
    match file.try_lock() {
        Ok(()) => return QuitRequest::NotRunning,
        Err(TryLockError::Error(_)) => return QuitRequest::StillRunning,
        Err(TryLockError::WouldBlock) => {}
    }
    let mut contents = String::new();
    let pid = file
        .read_to_string(&mut contents)
        .ok()
        .and_then(|_| contents.trim().parse::<libc::pid_t>().ok())
        .filter(|pid| *pid > 0);
    // SAFETY: `kill` has no memory-safety preconditions.
    let Some(_) = pid.filter(|pid| unsafe { libc::kill(*pid, libc::SIGTERM) } == 0) else {
        return QuitRequest::StillRunning;
    };
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return QuitRequest::Quit,
            Err(TryLockError::WouldBlock) if started.elapsed() < wait => std::thread::sleep(QUIT_POLL),
            Err(_) => return QuitRequest::StillRunning,
        }
    }
}

/// Deliver the signals to the main queue instead of ending the process. Main thread.
pub fn watch_termination() {
    watch(libc::SIGTERM, quit_requested);
    watch(libc::SIGUSR2, restart_requested);
    // A launcher that ignores these, as `nohup` does `SIGHUP`, means Snappy to outlive the
    // terminal, so an ignored one stays ignored.
    for signal in [libc::SIGINT, libc::SIGHUP] {
        if !is_ignored(signal) {
            watch(signal, quit_requested);
        }
    }
}

fn is_ignored(signal: libc::c_int) -> bool {
    // SAFETY: a null new action only reads the current one into `current`.
    unsafe {
        let mut current: libc::sigaction = std::mem::zeroed();
        libc::sigaction(signal, std::ptr::null(), &mut current) == 0 && current.sa_sigaction == libc::SIG_IGN
    }
}

fn watch(signal: libc::c_int, handler: extern "C" fn(*mut c_void)) {
    // SAFETY: ignoring the signal's default action is what lets the dispatch source see it.
    unsafe { libc::signal(signal, libc::SIG_IGN) };
    let kind = (&raw const dispatch2::_dispatch_source_type_signal).cast_mut();
    // SAFETY: a signal source takes the signal number as its handle and no mask.
    let source: DispatchRetained<DispatchSource> =
        unsafe { DispatchSource::new(kind, signal as usize, 0, Some(DispatchQueue::main())) };
    source.set_event_handler_f(handler);
    source.resume();
    // The source lives as long as the process.
    std::mem::forget(source);
}

extern "C" fn quit_requested(_context: *mut c_void) {
    emit(PlatformEvent::QuitRequested);
}

extern "C" fn restart_requested(_context: *mut c_void) {
    emit(PlatformEvent::RestartRequested);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hangup_the_launcher_ignores_reads_as_ignored() {
        // SAFETY: ignoring a signal has no memory-safety preconditions.
        unsafe { libc::signal(libc::SIGHUP, libc::SIG_IGN) };
        assert!(is_ignored(libc::SIGHUP));
    }

    #[test]
    fn a_second_claim_finds_the_lock_taken() {
        let folder = std::env::temp_dir().join(format!("snappy-instance-{}", std::process::id()));
        let first = claim(&folder).unwrap();
        assert!(matches!(first, Claim::Primary(_)));
        assert!(matches!(claim(&folder).unwrap(), Claim::Secondary));
        assert_eq!(
            std::fs::read_to_string(folder.join(LOCK_NAME)).unwrap(),
            std::process::id().to_string(),
            "a second claim leaves the daemon's ID in place"
        );
        drop(first);
        assert_eq!(request_quit(&folder, Duration::ZERO), QuitRequest::NotRunning);
        assert!(matches!(claim(&folder).unwrap(), Claim::Primary(_)));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
