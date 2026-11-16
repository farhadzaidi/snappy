use thiserror::Error;

/// Why the adapter couldn't start.
#[derive(Debug, Error)]
pub enum KWinError {
    #[error("Snappy couldn't connect to the session bus: {0}")]
    Bus(#[from] zbus::Error),
    #[error("Snappy couldn't start a thread: {0}")]
    Thread(#[from] std::io::Error),
    #[error("Another copy of Snappy holds its name on the session bus.")]
    NameTaken,
    #[error("The connection to the session bus closed.")]
    BusLost,
}
