use thiserror::Error;

/// Why the adapter couldn't start.
#[derive(Debug, Error)]
pub enum X11Error {
    #[error("{}", crate::messages::WAYLAND)]
    WaylandWithoutX,
    #[error("Snappy could not connect to the X server: {0}")]
    Connect(#[from] x11rb::errors::ConnectError),
    #[error("The connection to the X server failed: {0}")]
    Connection(#[from] x11rb::errors::ConnectionError),
    #[error("The X server refused a request: {0}")]
    Reply(#[from] x11rb::errors::ReplyError),
    #[error("The X server refused a request: {0}")]
    ReplyOrId(#[from] x11rb::errors::ReplyOrIdError),
    #[error("The X server has no screen for this display.")]
    NoScreen,
    #[error("Snappy could not start a thread: {0}")]
    Thread(#[from] std::io::Error),
}
