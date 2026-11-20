//! Sending the focused window to the back, so the windows it covers show: which windows it
//! goes behind, which takes focus, and which the next press sends (`plan`), how a platform
//! restacks windows (`restacking`), and, where a platform can raise another app's window
//! but never lower one, which windows are raised over it (`raised_over`) and in what steps
//! (`raises`). A press held long enough is a peek, which puts the window back as it's let
//! go (`press`).

mod overlap;
mod plan;
mod press;
mod raised_over;
mod raises;
mod restacking;
#[cfg(test)]
mod test_support;

pub use plan::{SendToBack, Sent, StackedWindow};
pub use press::{HOLD_AFTER, PressKind, ShortcutPress};
pub use raises::{RaiseStep, Raises, Step};
pub use restacking::Restacking;
