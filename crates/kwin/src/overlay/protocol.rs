//! The Wayland interfaces the overlay speaks, with the versions, request and event numbers,
//! and values it uses, as their protocol files define them.

use std::io;

pub const COMPOSITOR: &str = "wl_compositor";
pub const COMPOSITOR_VERSION: u32 = 4;
pub const SHM: &str = "wl_shm";
pub const LAYER_SHELL: &str = "zwlr_layer_shell_v1";
pub const LAYER_SHELL_VERSION: u32 = 3;
pub const ALPHA_MODIFIER: &str = "wp_alpha_modifier_v1";
pub const OUTPUT: &str = "wl_output";
/// Version 4 tells each output's connector name.
pub const OUTPUT_VERSION: u32 = 4;

pub const CREATE_SURFACE: u16 = 0;
pub const CREATE_REGION: u16 = 1;
pub const SURFACE_DESTROY: u16 = 0;
pub const SURFACE_ATTACH: u16 = 1;
pub const SURFACE_SET_INPUT_REGION: u16 = 5;
pub const SURFACE_COMMIT: u16 = 6;
pub const SURFACE_SET_BUFFER_SCALE: u16 = 8;
pub const SURFACE_DAMAGE_BUFFER: u16 = 9;
pub const REGION_DESTROY: u16 = 0;
pub const SHM_CREATE_POOL: u16 = 0;
pub const POOL_CREATE_BUFFER: u16 = 0;
pub const POOL_DESTROY: u16 = 1;
pub const BUFFER_DESTROY: u16 = 0;
pub const BUFFER_RELEASE: u16 = 0;
pub const OUTPUT_NAME: u16 = 4;
pub const GET_LAYER_SURFACE: u16 = 0;
pub const LAYER_SET_SIZE: u16 = 0;
pub const LAYER_SET_ANCHOR: u16 = 1;
pub const LAYER_SET_EXCLUSIVE_ZONE: u16 = 2;
pub const LAYER_SET_MARGIN: u16 = 3;
pub const LAYER_SET_KEYBOARD_INTERACTIVITY: u16 = 4;
pub const LAYER_ACK_CONFIGURE: u16 = 6;
pub const LAYER_DESTROY: u16 = 7;
pub const LAYER_CONFIGURE: u16 = 0;
pub const LAYER_CLOSED: u16 = 1;
pub const ALPHA_GET_SURFACE: u16 = 1;
pub const ALPHA_DESTROY: u16 = 0;
pub const ALPHA_SET_MULTIPLIER: u16 = 1;

/// Above every window, full-screen ones included.
pub const OVERLAY_LAYER: u32 = 3;
pub const ANCHOR_TOP_LEFT: u32 = 1 | 4;
/// Placed from the display's edge, not after panels' reserved space.
pub const IGNORE_EXCLUSIVE_ZONES: i32 = -1;
pub const NO_KEYBOARD: u32 = 0;
/// KWin gives layer surfaces with this namespace the on-screen display window type, which
/// Snappy's script leaves out of the windows it manages.
pub const NAMESPACE: &str = "on-screen-display";
/// Premultiplied ARGB, blue first in memory.
pub const FORMAT_ARGB8888: u32 = 0;
/// The alpha modifier's multipliers: invisible, and as drawn.
pub const HIDDEN: u32 = 0;
pub const OPAQUE: u32 = u32::MAX;

/// A size or count as the protocol's signed `int`.
pub fn int(value: u32) -> io::Result<i32> {
    i32::try_from(value).map_err(io::Error::other)
}
