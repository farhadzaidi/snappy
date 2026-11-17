//! What `snappy --uninstall` asks of each adapter: stopping the running daemon the way the
//! menu's Quit does, and removing what Snappy added to the system outside its own folders.

/// What asking the running daemon to quit came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitRequest {
    /// The daemon quit, bringing back every window it had hidden.
    Quit,
    /// No daemon was running.
    NotRunning,
    /// The daemon was asked but still runs after the wait, or couldn't be asked.
    StillRunning,
}

/// What removing one thing Snappy added came to, each with a sentence for the person
/// removing Snappy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removal {
    /// It was there and now isn't.
    Removed(String),
    /// There was nothing to remove.
    Nothing,
    /// It's still there.
    Failed(String),
}
