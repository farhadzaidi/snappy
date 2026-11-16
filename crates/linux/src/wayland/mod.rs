//! Just enough of the Wayland wire protocol to bind a compositor's globals and trade
//! messages, passing file descriptors with requests and events that carry them: Snappy reads
//! KWin's modifier state, shows its overlay, and reads the session's keymap this way, and the
//! live tests inject input. A full client library would be far more than any of them needs.

mod connection;
mod keymap;
mod message;
mod rights;

pub use connection::{Connection, Global};
pub use keymap::{Keymaps, keymap};
pub use message::{Arg, ArgReader, Message};
