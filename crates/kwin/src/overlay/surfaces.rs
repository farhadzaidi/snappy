//! Each display's layer surfaces: made over the work area, moved and resized in place when
//! it changes, and destroyed only when the display or its output goes.

use std::io;
use std::time::Duration;

use snappy_core::geometry::{Point, Rect};
use snappy_linux::wayland::{Arg, Message};

use super::protocol::{
    ALPHA_DESTROY, ALPHA_GET_SURFACE, ANCHOR_TOP_LEFT, CREATE_REGION, CREATE_SURFACE, GET_LAYER_SURFACE,
    IGNORE_EXCLUSIVE_ZONES, LAYER_CONFIGURE, LAYER_DESTROY, LAYER_SET_ANCHOR, LAYER_SET_EXCLUSIVE_ZONE,
    LAYER_SET_KEYBOARD_INTERACTIVITY, LAYER_SET_MARGIN, LAYER_SET_SIZE, NAMESPACE, NO_KEYBOARD,
    OVERLAY_LAYER, REGION_DESTROY, SURFACE_COMMIT, SURFACE_DESTROY, SURFACE_SET_BUFFER_SCALE,
    SURFACE_SET_INPUT_REGION, int,
};
use super::session::{Key, Layer, Session};
use super::thread::Target;

/// How long KWin may take to configure a surface; it does so once its current event is
/// handled, after a roundtrip returns.
const CONFIGURE_TIMEOUT: Duration = Duration::from_secs(1);

/// A display's layer surface, which stays while the display does.
pub struct Surface {
    output: String,
    pub surface: u32,
    pub layer: u32,
    /// The surface's alpha control; without one, hiding destroys the surface.
    pub alpha: Option<u32>,
    placement: (Point, Rect, u32),
}

/// The protocol objects of a display's surface, copied out to use while the session changes.
#[derive(Clone, Copy)]
pub struct SurfaceIds {
    pub surface: u32,
    pub layer: u32,
    pub alpha: Option<u32>,
}

impl SurfaceIds {
    fn of(surface: &Surface) -> SurfaceIds {
        SurfaceIds {
            surface: surface.surface,
            layer: surface.layer,
            alpha: surface.alpha,
        }
    }
}

impl Session {
    /// The surface where it belongs: made when there is none, moved and resized in place
    /// when the work area or scale changed, and made again only for a new output.
    pub fn place(&mut self, key: &Key, target: &Target, area: Rect, factor: u32) -> io::Result<SurfaceIds> {
        let placement = (target.origin, area, factor);
        match self.surfaces.get(key) {
            Some(surface) if surface.output == target.output && surface.placement == placement => {}
            Some(surface) if surface.output == target.output => {
                let ids = SurfaceIds::of(surface);
                self.configure(ids, target, area, factor)?;
                if let Some(surface) = self.surfaces.get_mut(key) {
                    surface.placement = placement;
                }
            }
            _ => {
                self.destroy_surface(key)?;
                self.create(key, target, area, factor)?;
            }
        }
        self.surfaces
            .get(key)
            .map(SurfaceIds::of)
            .ok_or_else(|| io::Error::other("the overlay surface is gone"))
    }

    /// Makes a layer surface over the display and waits for KWin to configure it.
    fn create(&mut self, key: &Key, target: &Target, area: Rect, factor: u32) -> io::Result<()> {
        let output = self.output(&target.output)?;
        let surface = self.connection.new_id();
        self.send(self.compositor, CREATE_SURFACE, &[Arg::Uint(surface)])?;
        // An empty input region lets every click through.
        let region = self.connection.new_id();
        self.send(self.compositor, CREATE_REGION, &[Arg::Uint(region)])?;
        self.send(surface, SURFACE_SET_INPUT_REGION, &[Arg::Uint(region)])?;
        self.send(region, REGION_DESTROY, &[])?;
        let alpha = match self.alpha_modifier {
            Some(modifier) => {
                let control = self.connection.new_id();
                self.send(
                    modifier,
                    ALPHA_GET_SURFACE,
                    &[Arg::Uint(control), Arg::Uint(surface)],
                )?;
                Some(control)
            }
            None => None,
        };
        let layer = self.connection.new_id();
        self.send(
            self.layer_shell,
            GET_LAYER_SURFACE,
            &[
                Arg::Uint(layer),
                Arg::Uint(surface),
                Arg::Uint(output),
                Arg::Uint(OVERLAY_LAYER),
                Arg::Str(NAMESPACE),
            ],
        )?;
        self.send(layer, LAYER_SET_ANCHOR, &[Arg::Uint(ANCHOR_TOP_LEFT)])?;
        self.send(
            layer,
            LAYER_SET_EXCLUSIVE_ZONE,
            &[Arg::Int(IGNORE_EXCLUSIVE_ZONES)],
        )?;
        self.send(layer, LAYER_SET_KEYBOARD_INTERACTIVITY, &[Arg::Uint(NO_KEYBOARD)])?;
        let created = Surface {
            output: target.output.clone(),
            surface,
            layer,
            alpha,
            placement: (target.origin, area, factor),
        };
        let ids = SurfaceIds::of(&created);
        self.surfaces.insert(key.clone(), created);
        self.configure(ids, target, area, factor).inspect_err(|_| {
            self.surfaces.remove(key);
        })
    }

    /// Places the surface over the work area and waits for KWin's configure, which `event`
    /// acknowledges.
    fn configure(&mut self, ids: SurfaceIds, target: &Target, area: Rect, factor: u32) -> io::Result<()> {
        let top = (area.y - target.origin.y).round() as i32;
        let left = (area.x - target.origin.x).round() as i32;
        self.send(
            ids.layer,
            LAYER_SET_MARGIN,
            &[Arg::Int(top), Arg::Int(0), Arg::Int(0), Arg::Int(left)],
        )?;
        self.send(
            ids.layer,
            LAYER_SET_SIZE,
            &[
                Arg::Uint(area.width.round() as u32),
                Arg::Uint(area.height.round() as u32),
            ],
        )?;
        self.send(ids.surface, SURFACE_SET_BUFFER_SCALE, &[Arg::Int(int(factor)?)])?;
        self.send(ids.surface, SURFACE_COMMIT, &[])?;
        let layer = ids.layer;
        let awaited = |message: &Message| message.object == layer && message.opcode == LAYER_CONFIGURE;
        let events = self
            .connection
            .read_until(CONFIGURE_TIMEOUT, awaited)
            .map_err(|error| {
                io::Error::other(format!("KWin didn't configure the overlay surface: {error}"))
            })?;
        for message in &events {
            self.event(message)?;
        }
        Ok(())
    }

    pub fn destroy_surface(&mut self, key: &Key) -> io::Result<()> {
        let Some(surface) = self.surfaces.remove(key) else {
            return Ok(());
        };
        if key.1 == Layer::Zones && self.visible.as_ref() == Some(&key.0) {
            self.visible = None;
        }
        if let Some(control) = surface.alpha {
            self.send(control, ALPHA_DESTROY, &[])?;
        }
        self.send(surface.layer, LAYER_DESTROY, &[])?;
        self.send(surface.surface, SURFACE_DESTROY, &[])?;
        self.roundtrip()
    }
}
