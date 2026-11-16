use thiserror::Error;

/// Why the adapter couldn't start.
#[derive(Debug, Error)]
pub enum MacError {
    #[error("Snappy has to start on the main thread.")]
    NotMainThread,
}
