//! How a platform changes another app's window's place in the stack, which each adapter
//! says in its capabilities, and which decides how a window goes to the back and comes back.

/// How a platform changes another app's window's place in the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restacking {
    /// It can raise another app's window but never lower one, so a window goes to the back
    /// by the windows over it being raised (`Raises`).
    RaiseOnly,
    /// It lowers the window behind every other, in place. With `stack_below`, it can also
    /// put a window directly under another, so a window put back goes back under the window
    /// that was just in front of it.
    Lower { stack_below: bool },
}
