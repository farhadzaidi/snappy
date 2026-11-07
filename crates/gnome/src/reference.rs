//! Read by the live-test harness, so its expectations use Snappy's own view of displays and
//! keys.

use snappy_core::keys::Combo;
use snappy_platform::{Display, WindowSystem};

use crate::bus::Bus;
use crate::platform::GnomePlatform;
use crate::shared::Shared;

/// Every display, primary first, as the adapter reports them.
pub fn displays() -> Result<Vec<Display>, String> {
    let bus = Bus::session().map_err(|error| error.to_string())?;
    let mut platform = GnomePlatform::new(std::sync::Arc::new(Shared::new(bus, true)));
    Ok(platform.displays())
}

/// The accelerator a shortcut is grabbed with.
pub fn accelerator(combo: &Combo) -> Option<String> {
    crate::keys::accelerator(combo)
}
