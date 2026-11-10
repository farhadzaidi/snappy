use thiserror::Error;

/// Why the adapter couldn't start.
#[derive(Debug, Error)]
pub enum GnomeError {
    #[error("Snappy could not connect to the session bus: {0}")]
    Bus(#[from] zbus::Error),
    #[error("Snappy could not start a thread: {0}")]
    Thread(#[from] std::io::Error),
    #[error("The connection to the session bus closed.")]
    BusLost,
}
