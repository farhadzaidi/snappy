//! One daemon per session: a lock file holding the daemon's process ID. A
//! second launch sends that process `SIGUSR1`, which opens the settings window.
//! `SIGTERM`, from `kill` or the session ending, quits the way the tray's Quit does, so
//! parked windows come back first.

use std::ffi::{c_int, c_void};
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::IntoRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::Sender;

use snappy_platform::PlatformEvent;

const LOCK_NAME: &str = "snappy.lock";
const SIGUSR1: c_int = 10;
const SIGTERM: c_int = 15;
/// `SIG_ERR` as `signal` returns it.
const SIGNAL_ERROR: usize = usize::MAX;

unsafe extern "C" {
    fn signal(signum: c_int, handler: usize) -> usize;
    fn kill(pid: i32, sig: c_int) -> c_int;
    fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
}

/// The pipe the signal handler writes to; -1 until a listener is installed.
static WAKE_FD: AtomicI32 = AtomicI32::new(-1);

pub enum Claim {
    /// This process is the daemon. Keep the lock for its lifetime.
    Primary(InstanceLock),
    /// Another daemon runs and has been asked to open the settings window.
    Secondary,
}

pub struct InstanceLock {
    file: File,
}

/// `$XDG_RUNTIME_DIR/snappy.lock`, or `fallback_dir` when the runtime folder is unset.
pub fn lock_path(fallback_dir: &Path) -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| fallback_dir.to_path_buf())
        .join(LOCK_NAME)
}

/// Claims the daemon role, or signals the daemon that holds it.
pub fn claim(path: &Path) -> std::io::Result<Claim> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Claim::Primary(InstanceLock { file })),
        Err(TryLockError::WouldBlock) => {
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            if let Ok(pid) = contents.trim().parse::<i32>()
                && pid > 0
            {
                // SAFETY: `kill` has no memory-safety preconditions.
                unsafe { kill(pid, SIGUSR1) };
            }
            Ok(Claim::Secondary)
        }
        Err(TryLockError::Error(error)) => Err(error),
    }
}

impl InstanceLock {
    /// Forwards `SIGUSR1` to `events` as `OpenSettingsRequested`, then records this
    /// process ID so second launches can find it. The handler is installed first, because
    /// the signal's default action ends the process.
    pub fn publish(&mut self, events: Sender<PlatformEvent>) -> std::io::Result<()> {
        listen(events)?;
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        write!(self.file, "{}", std::process::id())?;
        self.file.flush()
    }
}

/// Passes the signal number through the pipe to the listening thread.
extern "C" fn on_signal(signum: c_int) {
    let fd = WAKE_FD.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte = u8::try_from(signum).unwrap_or(0);
        // SAFETY: `write` is async-signal-safe and the buffer outlives the call.
        unsafe { write(fd, (&raw const byte).cast(), 1) };
    }
}

/// The event a received signal stands for.
fn signal_event(signum: c_int) -> Option<PlatformEvent> {
    match signum {
        SIGUSR1 => Some(PlatformEvent::OpenSettingsRequested),
        SIGTERM => Some(PlatformEvent::QuitRequested),
        _ => None,
    }
}

fn listen(events: Sender<PlatformEvent>) -> std::io::Result<()> {
    let (mut reader, writer) = std::io::pipe()?;
    // The write end stays open for the process lifetime; the handler owns it.
    WAKE_FD.store(writer.into_raw_fd(), Ordering::Relaxed);
    std::thread::Builder::new()
        .name("snappy-signals".into())
        .spawn(move || {
            let mut byte = [0u8; 1];
            while reader.read_exact(&mut byte).is_ok() {
                let Some(event) = signal_event(c_int::from(byte[0])) else {
                    continue;
                };
                if events.send(event).is_err() {
                    return;
                }
            }
        })?;
    for signum in [SIGUSR1, SIGTERM] {
        // SAFETY: the handler only performs an atomic load and `write`.
        let previous = unsafe { signal(signum, on_signal as extern "C" fn(c_int) as usize) };
        if previous == SIGNAL_ERROR {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_launch_signals_the_daemon_to_open_settings() {
        let folder = std::env::temp_dir().join(format!("snappy-instance-{}", std::process::id()));
        let path = folder.join(LOCK_NAME);
        let Claim::Primary(mut lock) = claim(&path).unwrap() else {
            panic!("the first claim must win");
        };
        let (events, received) = std::sync::mpsc::channel();
        lock.publish(events).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            std::process::id().to_string()
        );

        assert!(matches!(claim(&path).unwrap(), Claim::Secondary));
        let event = received.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(event, PlatformEvent::OpenSettingsRequested);

        drop(lock);
        assert!(matches!(claim(&path).unwrap(), Claim::Primary(_)));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn sigterm_quits_like_the_tray_quit() {
        assert_eq!(signal_event(SIGTERM), Some(PlatformEvent::QuitRequested));
        assert_eq!(signal_event(SIGUSR1), Some(PlatformEvent::OpenSettingsRequested));
        assert_eq!(signal_event(0), None);
    }

    #[test]
    fn the_lock_lives_in_the_runtime_folder() {
        let path = lock_path(Path::new("/var/lib/snappy"));
        assert!(path.ends_with(LOCK_NAME));
    }
}
