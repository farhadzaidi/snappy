//! Starting Snappy at login.

pub trait LoginItem {
    fn is_enabled(&mut self) -> bool;
    fn set_enabled(&mut self, on: bool) -> Result<(), String>;
}
