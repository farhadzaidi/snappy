//! An in-memory operating system for engine tests: displays, windows that behave like
//! real apps (lagging, refusing, constraining), and recorders for everything Snappy shows.

mod caps_lock;
mod desktop_guard;
mod fake_window;
mod hotkeys;
mod input;
mod login_item;
mod overlay;
mod permissions;
mod platform;
mod tray;
mod windows;

pub use fake_window::FakeWindow;
pub use overlay::OverlayCall;
pub use platform::{FakePlatform, StackChange};
