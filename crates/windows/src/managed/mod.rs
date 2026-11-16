//! Other apps' windows, read and moved from the engine thread: which ones Snappy manages,
//! how each is identified across handle reuse, their frames, and the processes behind them.

mod eligibility;
pub mod frames;
pub mod identity;
pub mod process;
pub mod record;
mod top_level;

pub use top_level::Windows;
