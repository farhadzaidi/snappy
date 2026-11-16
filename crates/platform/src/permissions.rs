//! The system permission Snappy needs to control windows, where the platform has one.

pub trait Permissions {
    fn is_granted(&mut self) -> bool;
    /// Ask the system for permission, listing Snappy where the user grants it.
    /// False when the request couldn't be made.
    fn request_access(&mut self) -> bool;
    fn open_settings_page(&mut self) -> bool;
    /// Poll for a grant only while one is awaited.
    fn watch(&mut self, waiting_for_grant: bool);
}
