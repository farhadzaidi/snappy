//! Open at login: the Run key for an unpackaged Snappy, the package's startup task for one
//! installed from an MSIX package.

mod mechanism;
mod run_key;
mod startup_task;

pub use mechanism::{is_enabled, set_enabled};
