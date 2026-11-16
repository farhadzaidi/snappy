//! The notification-area icon and its menu, the Windows counterpart of the menu bar item.
//! The menu is built from the latest content each time it opens.

mod icon;
mod menu;
mod popup;

#[cfg(feature = "reference")]
pub use icon::ICON_ID;
pub use icon::Tray;
pub use popup::callback;
