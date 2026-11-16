use thiserror::Error;

/// Why the adapter couldn't start.
#[derive(Debug, Error)]
pub enum GnomeError {
    #[error("Snappy couldn't connect to the session bus: {0}")]
    Bus(#[from] zbus::Error),
    #[error("Snappy couldn't start a thread: {0}")]
    Thread(#[from] std::io::Error),
    #[error("The connection to the session bus closed.")]
    BusLost,
}
