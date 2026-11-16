//! The zone overlay: a layer-shell surface per display that the daemon draws itself, into
//! shared memory, with the renderer the X11 overlay uses. KWin only composites the finished
//! image: nothing of Snappy's runs or draws inside KWin, so a failure here can't take KWin
//! down, and it is logged rather than lost.
//!
//! The surface sits on the overlay layer, above every window, takes no input or keyboard
//! focus, and covers the display's work area. A thread of its own talks to KWin, so the
//! engine never waits for drawing; it connects on first use and again after a failure.
//!
//! Each display's surface is made once, when the engine prepares its overlay, and stays:
//! showing and hiding only changes its alpha (`wp_alpha_modifier_v1`). KWin's effects
//! animate windows appearing and closing, as `fadingpopups` fades on-screen displays, so a
//! surface made and destroyed on every drag would fade in and out. Hidden at alpha 0, the
//! surface is skipped when KWin plans direct scanout, so full-screen apps lose nothing.
//!
//! Marked windows' outlines go on a second surface per display, made the same way the
//! first time there are any, so they stay while the zones show and hide.

mod buffers;
#[cfg(test)]
mod fake_compositor;
mod protocol;
mod scale;
mod session;
mod surfaces;
mod thread;

pub use thread::{Command, Overlays, Target};
