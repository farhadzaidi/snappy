//! Files the engine owns: settings, workspace state, and diagnostics.

mod atomic_write;
pub mod file_names;
mod logs;
mod paths;
mod settings_file;
mod state_files;
#[cfg(test)]
mod test_dirs;

pub use logs::{append_log, write_log};
pub use paths::Paths;
pub use settings_file::SettingsStore;
pub use state_files::{ReadState, StateFile, read_state, remove_state, write_state};
