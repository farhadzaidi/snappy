//! Connected displays from RandR 1.5 monitors, with work areas derived from panel struts
//! because `_NET_WORKAREA` is one rectangle for all monitors.

mod edid;
mod monitors;
mod resources;
mod work_areas;

pub use monitors::read;
