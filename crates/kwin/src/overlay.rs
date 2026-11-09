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

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use snappy_core::geometry::{Point, Rect};
use snappy_linux::journal;
use snappy_linux::overlay_image::{pixel_size, render_rows, to_argb};
use snappy_linux::wayland::{Arg, Connection, Message};
use snappy_platform::{OverlayScene, OverlayZone};

const COMPOSITOR: &str = "wl_compositor";
const COMPOSITOR_VERSION: u32 = 4;
const SHM: &str = "wl_shm";
const LAYER_SHELL: &str = "zwlr_layer_shell_v1";
const LAYER_SHELL_VERSION: u32 = 3;
const ALPHA_MODIFIER: &str = "wp_alpha_modifier_v1";
const OUTPUT: &str = "wl_output";
/// Version 4 tells each output's connector name.
const OUTPUT_VERSION: u32 = 4;

const CREATE_SURFACE: u16 = 0;
const CREATE_REGION: u16 = 1;
const SURFACE_DESTROY: u16 = 0;
const SURFACE_ATTACH: u16 = 1;
const SURFACE_SET_INPUT_REGION: u16 = 5;
const SURFACE_COMMIT: u16 = 6;
const SURFACE_SET_BUFFER_SCALE: u16 = 8;
const SURFACE_DAMAGE_BUFFER: u16 = 9;
const REGION_DESTROY: u16 = 0;
const SHM_CREATE_POOL: u16 = 0;
const POOL_CREATE_BUFFER: u16 = 0;
const POOL_DESTROY: u16 = 1;
const BUFFER_DESTROY: u16 = 0;
const BUFFER_RELEASE: u16 = 0;
const OUTPUT_NAME: u16 = 4;
const GET_LAYER_SURFACE: u16 = 0;
const LAYER_SET_SIZE: u16 = 0;
const LAYER_SET_ANCHOR: u16 = 1;
const LAYER_SET_EXCLUSIVE_ZONE: u16 = 2;
const LAYER_SET_MARGIN: u16 = 3;
const LAYER_SET_KEYBOARD_INTERACTIVITY: u16 = 4;
const LAYER_ACK_CONFIGURE: u16 = 6;
const LAYER_DESTROY: u16 = 7;
const LAYER_CONFIGURE: u16 = 0;
const LAYER_CLOSED: u16 = 1;
const ALPHA_GET_SURFACE: u16 = 1;
const ALPHA_DESTROY: u16 = 0;
const ALPHA_SET_MULTIPLIER: u16 = 1;

/// Above every window, full-screen ones included.
const OVERLAY_LAYER: u32 = 3;
const ANCHOR_TOP_LEFT: u32 = 1 | 4;
/// Placed from the display's edge, not after panels' reserved space.
const IGNORE_EXCLUSIVE_ZONES: i32 = -1;
const NO_KEYBOARD: u32 = 0;
/// KWin gives layer surfaces with this namespace the on-screen display window type, which
/// Snappy's script leaves out of the windows it manages.
const NAMESPACE: &str = "on-screen-display";
/// Premultiplied ARGB, blue first in memory.
const FORMAT_ARGB8888: u32 = 0;
const PIXEL_BYTES: u32 = 4;
/// Rows drawn and written at a time, so no whole image is held in memory.
const BAND_ROWS: u32 = 128;
/// How long KWin may take to configure a surface; it does so once its current event is
/// handled, after a roundtrip returns.
const CONFIGURE_TIMEOUT: Duration = Duration::from_secs(1);
/// Two buffers per display: one KWin may still be showing, one to draw into.
const SLOTS: usize = 2;
/// The alpha modifier's multipliers: invisible, and as drawn.
const HIDDEN: u32 = 0;
const OPAQUE: u32 = u32::MAX;

/// Where a display's overlay goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// The output's connector name, such as `HDMI-A-1`.
    pub output: String,
    /// The output's top-left corner in KWin's coordinates.
    pub origin: Point,
    /// The output's scale; buffers are drawn at the next whole multiple.
    pub scale: f64,
}

pub enum Command {
    /// Makes the display's surface, invisibly, with the scene drawn, ahead of a drag.
    Prepare {
        display: String,
        target: Target,
        scene: OverlayScene,
    },
    Show {
        display: String,
        target: Target,
        scene: OverlayScene,
    },
    Hide,
    /// Drops what is kept for displays that are gone.
    Retain(Vec<String>),
    Stop,
}

/// The overlay thread's handle.
pub struct Overlays {
    commands: Sender<Command>,
}

impl Overlays {
    pub fn start() -> io::Result<Overlays> {
        let (commands, received) = mpsc::channel();
        std::thread::Builder::new()
            .name("snappy-kwin-overlay".into())
            .spawn(move || run(&received))?;
        Ok(Overlays { commands })
    }

    pub fn send(&self, command: Command) {
        if self.commands.send(command).is_err() {
            journal::warn("the zone overlay's thread has stopped");
        }
    }
}

fn run(commands: &Receiver<Command>) {
    let mut session: Option<Session> = None;
    // The last failure, so one that repeats on every pointer move is logged once.
    let mut last_failure: Option<String> = None;
    for command in commands {
        let result = match command {
            Command::Stop => return,
            // A connection that failed, as when KWin restarted, gets one fresh try.
            Command::Prepare {
                display,
                target,
                scene,
            } => retried(&mut session, |open| open.prepare(&display, &target, &scene)),
            Command::Show {
                display,
                target,
                scene,
            } => retried(&mut session, |open| open.show(&display, &target, &scene)),
            Command::Hide => session.as_mut().map_or(Ok(()), Session::hide),
            Command::Retain(displays) => session.as_mut().map_or(Ok(()), |open| open.retain(&displays)),
        };
        match result {
            Ok(()) => last_failure = None,
            Err(error) => {
                let failure = error.to_string();
                if last_failure.as_ref() != Some(&failure) {
                    journal::warn(format_args!("the zone overlay can't be shown: {failure}"));
                    last_failure = Some(failure);
                }
                // Start over with a fresh connection next time; KWin may have restarted.
                session = None;
            }
        }
    }
}

/// Runs `action` on the session, connecting first if needed, and once more on a fresh
/// connection if it fails.
fn retried(session: &mut Option<Session>, action: impl Fn(&mut Session) -> io::Result<()>) -> io::Result<()> {
    connected(session).and_then(&action).or_else(|_| {
        *session = None;
        connected(session).and_then(&action)
    })
}

fn connected(session: &mut Option<Session>) -> io::Result<&mut Session> {
    if session.is_none() {
        *session = Some(Session::open()?);
    }
    session
        .as_mut()
        .ok_or_else(|| io::Error::other("no connection to KWin"))
}

struct Output {
    id: u32,
    name: Option<String>,
}

/// A display's layer surface, which stays while the display does.
struct Surface {
    output: String,
    surface: u32,
    layer: u32,
    /// The surface's alpha control; without one, hiding destroys the surface.
    alpha: Option<u32>,
    placement: (Point, Rect, u32),
}

struct Slot {
    buffer: u32,
    busy: bool,
}

/// A display's shared-memory pool and its buffers, for one size and scale.
struct Buffers {
    file: File,
    pool: u32,
    size: (u32, u32),
    slots: Vec<Slot>,
}

struct Session {
    connection: Connection,
    compositor: u32,
    shm: u32,
    layer_shell: u32,
    alpha_modifier: Option<u32>,
    outputs: Vec<Output>,
    surfaces: HashMap<String, Surface>,
    /// The display whose overlay is showing.
    visible: Option<String>,
    buffers: HashMap<String, Buffers>,
}

impl Session {
    fn open() -> io::Result<Session> {
        Session::on(Connection::open()?)
    }

    fn on(mut connection: Connection) -> io::Result<Session> {
        let (compositor, _) = connection.bind(COMPOSITOR, COMPOSITOR_VERSION)?;
        let (shm, _) = connection.bind(SHM, 1)?;
        let (layer_shell, _) = connection.bind(LAYER_SHELL, LAYER_SHELL_VERSION)?;
        let alpha_modifier = match connection.bind(ALPHA_MODIFIER, 1) {
            Ok((id, _)) => Some(id),
            Err(_) => {
                journal::warn("KWin can't change a surface's alpha; the zone overlay will fade in and out");
                None
            }
        };
        let output_globals: Vec<_> = connection
            .globals()
            .iter()
            .filter(|global| global.interface == OUTPUT)
            .cloned()
            .collect();
        let mut outputs = Vec::new();
        for global in &output_globals {
            let (id, _) = connection.bind_global(global, OUTPUT_VERSION)?;
            outputs.push(Output { id, name: None });
        }
        let mut session = Session {
            connection,
            compositor,
            shm,
            layer_shell,
            alpha_modifier,
            outputs,
            surfaces: HashMap::new(),
            visible: None,
            buffers: HashMap::new(),
        };
        session.roundtrip()?;
        Ok(session)
    }

    /// Waits for KWin to handle every request so far and takes in what it said meanwhile.
    fn roundtrip(&mut self) -> io::Result<()> {
        for message in self.connection.roundtrip()? {
            self.event(&message)?;
        }
        Ok(())
    }

    /// Output names, buffers KWin is done with, and configures, which are acknowledged at
    /// once, so the next commit answers the latest.
    fn event(&mut self, message: &Message) -> io::Result<()> {
        let mut reader = message.reader();
        if let Some(output) = self.outputs.iter_mut().find(|output| output.id == message.object) {
            if message.opcode == OUTPUT_NAME {
                output.name = reader.string();
            }
            return Ok(());
        }
        for buffers in self.buffers.values_mut() {
            if let Some(slot) = buffers
                .slots
                .iter_mut()
                .find(|slot| slot.buffer == message.object)
            {
                if message.opcode == BUFFER_RELEASE {
                    slot.busy = false;
                }
                return Ok(());
            }
        }
        let Some(display) = self
            .surfaces
            .iter()
            .find(|(_, surface)| surface.layer == message.object)
            .map(|(display, _)| display.clone())
        else {
            return Ok(());
        };
        match message.opcode {
            LAYER_CONFIGURE => {
                if let Some(serial) = reader.uint() {
                    self.send(message.object, LAYER_ACK_CONFIGURE, &[Arg::Uint(serial)])?;
                }
            }
            // KWin took the surface away, as when its output went; it is made again next time.
            LAYER_CLOSED => self.destroy_surface(&display)?,
            _ => {}
        }
        Ok(())
    }

    fn output(&self, name: &str) -> io::Result<u32> {
        self.outputs
            .iter()
            .find(|output| output.name.as_deref() == Some(name))
            .map(|output| output.id)
            .ok_or_else(|| io::Error::other(format!("KWin has no output named {name}")))
    }

    /// Draws the scene on the display's surface, invisibly, unless it is showing.
    fn prepare(&mut self, display: &str, target: &Target, scene: &OverlayScene) -> io::Result<()> {
        if self.visible.as_deref() == Some(display) {
            return Ok(());
        }
        self.draw_on(display, target, scene, HIDDEN)
    }

    fn show(&mut self, display: &str, target: &Target, scene: &OverlayScene) -> io::Result<()> {
        if self.visible.as_deref().is_some_and(|visible| visible != display) {
            self.hide()?;
        }
        self.draw_on(display, target, scene, OPAQUE)?;
        self.visible = Some(display.to_owned());
        Ok(())
    }

    /// Draws the scene on the display's surface at `alpha`, making or moving the surface
    /// first when needed. Without an alpha control a hidden scene isn't drawn ahead.
    fn draw_on(
        &mut self,
        display: &str,
        target: &Target,
        scene: &OverlayScene,
        alpha: u32,
    ) -> io::Result<()> {
        if alpha == HIDDEN && self.alpha_modifier.is_none() {
            return Ok(());
        }
        self.roundtrip()?;
        let factor = buffer_scale(target.scale);
        let surface = self.place(display, target, scene.work_area, factor)?;
        let scaled = scaled(scene, f64::from(factor));
        let (width, height) =
            pixel_size(scaled.work_area).ok_or_else(|| io::Error::other("the work area has no pixels"))?;
        let buffer = self.draw(display, &scaled, f64::from(factor), (width, height))?;
        self.send(
            surface.surface,
            SURFACE_ATTACH,
            &[Arg::Uint(buffer), Arg::Int(0), Arg::Int(0)],
        )?;
        self.send(
            surface.surface,
            SURFACE_DAMAGE_BUFFER,
            &[
                Arg::Int(0),
                Arg::Int(0),
                Arg::Int(int(width)?),
                Arg::Int(int(height)?),
            ],
        )?;
        if let Some(control) = surface.alpha {
            self.send(control, ALPHA_SET_MULTIPLIER, &[Arg::Uint(alpha)])?;
        }
        self.send(surface.surface, SURFACE_COMMIT, &[])?;
        self.roundtrip()
    }

    /// Hides what shows by making it invisible, so KWin sees no window close.
    fn hide(&mut self) -> io::Result<()> {
        let Some(display) = self.visible.take() else {
            return Ok(());
        };
        let Some(surface) = self.surfaces.get(&display) else {
            return Ok(());
        };
        match surface.alpha {
            Some(control) => {
                let target = surface.surface;
                self.send(control, ALPHA_SET_MULTIPLIER, &[Arg::Uint(HIDDEN)])?;
                self.send(target, SURFACE_COMMIT, &[])?;
                self.roundtrip()
            }
            None => self.destroy_surface(&display),
        }
    }

    /// The display's surface where it belongs: made when there is none, moved and resized
    /// in place when the work area or scale changed, and made again only for a new output.
    fn place(&mut self, display: &str, target: &Target, area: Rect, factor: u32) -> io::Result<SurfaceIds> {
        let placement = (target.origin, area, factor);
        match self.surfaces.get(display) {
            Some(surface) if surface.output == target.output && surface.placement == placement => {}
            Some(surface) if surface.output == target.output => {
                let ids = SurfaceIds::of(surface);
                self.configure(ids, target, area, factor)?;
                if let Some(surface) = self.surfaces.get_mut(display) {
                    surface.placement = placement;
                }
            }
            _ => {
                self.destroy_surface(display)?;
                self.create(display, target, area, factor)?;
            }
        }
        self.surfaces
            .get(display)
            .map(SurfaceIds::of)
            .ok_or_else(|| io::Error::other("the overlay surface is gone"))
    }

    /// Makes the display's layer surface and waits for KWin to configure it.
    fn create(&mut self, display: &str, target: &Target, area: Rect, factor: u32) -> io::Result<()> {
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
        self.surfaces.insert(display.to_owned(), created);
        self.configure(ids, target, area, factor).inspect_err(|_| {
            self.surfaces.remove(display);
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

    fn destroy_surface(&mut self, display: &str) -> io::Result<()> {
        let Some(surface) = self.surfaces.remove(display) else {
            return Ok(());
        };
        if self.visible.as_deref() == Some(display) {
            self.visible = None;
        }
        if let Some(control) = surface.alpha {
            self.send(control, ALPHA_DESTROY, &[])?;
        }
        self.send(surface.layer, LAYER_DESTROY, &[])?;
        self.send(surface.surface, SURFACE_DESTROY, &[])?;
        self.roundtrip()
    }

    /// Draws the scene into a buffer KWin isn't using and returns it.
    fn draw(&mut self, display: &str, scene: &OverlayScene, scale: f64, size: (u32, u32)) -> io::Result<u32> {
        if self
            .buffers
            .get(display)
            .is_none_or(|buffers| buffers.size != size)
        {
            self.drop_buffers(display)?;
            let buffers = self.allocate(size)?;
            self.buffers.insert(display.to_owned(), buffers);
        }
        let Some(buffers) = self.buffers.get_mut(display) else {
            return Err(io::Error::other("the overlay's buffers are gone"));
        };
        let Some(index) = buffers.slots.iter().position(|slot| !slot.busy) else {
            return Err(io::Error::other("KWin still holds both overlay buffers"));
        };
        let (width, height) = size;
        let stride = u64::from(width * PIXEL_BYTES);
        let start = index as u64 * stride * u64::from(height);
        let mut top = 0;
        while top < height {
            let rows = BAND_ROWS.min(height - top);
            let mut band = render_rows(scene, scale, top, rows)
                .ok_or_else(|| io::Error::other("the overlay couldn't be drawn"))?;
            to_argb(band.data_mut(), false);
            buffers
                .file
                .write_all_at(band.data(), start + u64::from(top) * stride)?;
            top += rows;
        }
        let slot = &mut buffers.slots[index];
        slot.busy = true;
        Ok(slot.buffer)
    }

    /// A shared-memory pool for two images of `size`, in a file nothing else can open.
    fn allocate(&mut self, size: (u32, u32)) -> io::Result<Buffers> {
        let (width, height) = size;
        let stride = width * PIXEL_BYTES;
        let frame = u64::from(stride) * u64::from(height);
        let file = anonymous_file()?;
        file.set_len(frame * SLOTS as u64)?;
        let pool = self.connection.new_id();
        let pool_size = i32::try_from(frame * SLOTS as u64).map_err(io::Error::other)?;
        self.connection.send_with_fd(
            self.shm,
            SHM_CREATE_POOL,
            &[Arg::Uint(pool), Arg::Int(pool_size)],
            file.as_fd(),
        )?;
        let mut slots = Vec::with_capacity(SLOTS);
        for index in 0..SLOTS {
            let buffer = self.connection.new_id();
            let offset = i32::try_from(frame * index as u64).map_err(io::Error::other)?;
            self.send(
                pool,
                POOL_CREATE_BUFFER,
                &[
                    Arg::Uint(buffer),
                    Arg::Int(offset),
                    Arg::Int(int(width)?),
                    Arg::Int(int(height)?),
                    Arg::Int(int(stride)?),
                    Arg::Uint(FORMAT_ARGB8888),
                ],
            )?;
            slots.push(Slot { buffer, busy: false });
        }
        Ok(Buffers {
            file,
            pool,
            size,
            slots,
        })
    }

    fn drop_buffers(&mut self, display: &str) -> io::Result<()> {
        let Some(buffers) = self.buffers.remove(display) else {
            return Ok(());
        };
        for slot in &buffers.slots {
            self.send(slot.buffer, BUFFER_DESTROY, &[])?;
        }
        self.send(buffers.pool, POOL_DESTROY, &[])
    }

    fn retain(&mut self, displays: &[String]) -> io::Result<()> {
        let gone: Vec<String> = self
            .surfaces
            .keys()
            .chain(self.buffers.keys())
            .filter(|display| !displays.contains(display))
            .cloned()
            .collect();
        for display in gone {
            self.destroy_surface(&display)?;
            self.drop_buffers(&display)?;
        }
        Ok(())
    }

    fn send(&mut self, object: u32, opcode: u16, args: &[Arg]) -> io::Result<()> {
        self.connection.send(object, opcode, args)
    }
}

/// The protocol objects of a display's surface, copied out to use while the session changes.
#[derive(Clone, Copy)]
struct SurfaceIds {
    surface: u32,
    layer: u32,
    alpha: Option<u32>,
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

fn int(value: u32) -> io::Result<i32> {
    i32::try_from(value).map_err(io::Error::other)
}

/// Whole buffer scales only: a fractional scale draws at the next whole one, and KWin
/// shrinks the image to fit.
fn buffer_scale(scale: f64) -> u32 {
    if scale.is_finite() && scale > 1.0 {
        scale.ceil() as u32
    } else {
        1
    }
}

/// The scene in buffer pixels: every length times `factor`.
fn scaled(scene: &OverlayScene, factor: f64) -> OverlayScene {
    let scale = |rect: Rect| {
        Rect::new(
            rect.x * factor,
            rect.y * factor,
            rect.width * factor,
            rect.height * factor,
        )
    };
    OverlayScene {
        work_area: scale(scene.work_area),
        zones: scene
            .zones
            .iter()
            .map(|zone| OverlayZone {
                frame: scale(zone.frame),
                ..zone.clone()
            })
            .collect(),
    }
}

/// A file in the session's runtime folder (else the temporary one), removed from it at once:
/// KWin gets it by descriptor, and nothing is left behind if Snappy stops.
fn anonymous_file() -> io::Result<File> {
    let folder =
        std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let path = folder.join(format!("snappy-overlay-{}-{}", std::process::id(), unique()));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    std::fs::remove_file(&path)?;
    Ok(file)
}

fn unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    use snappy_platform::Caption;

    use super::*;

    /// Just enough of a compositor to watch what the overlay asks of it: it offers the
    /// globals Snappy binds, names one output, configures layer surfaces when they commit
    /// a new size, releases a buffer when the next is attached, and records every request
    /// by interface and opcode.
    struct FakeCompositor {
        stream: UnixStream,
        interfaces: HashMap<u32, &'static str>,
        /// Layer surface to its wl_surface, and the layer surfaces with a size to configure.
        layers: HashMap<u32, u32>,
        unconfigured: HashSet<u32>,
        attached: HashMap<u32, u32>,
        requests: Vec<(&'static str, u16)>,
        /// Every alpha multiplier set, in order.
        alphas: Vec<u32>,
        serial: u32,
    }

    /// Every request by interface and opcode, and every alpha multiplier set, in order.
    type Recorded = (Vec<(&'static str, u16)>, Vec<u32>);

    const GLOBALS: [(u32, &str); 5] = [
        (1, COMPOSITOR),
        (2, SHM),
        (3, LAYER_SHELL),
        (4, ALPHA_MODIFIER),
        (5, OUTPUT),
    ];

    fn event(object: u32, opcode: u16, words: &[u32], text: Option<&str>) -> Vec<u8> {
        let mut body: Vec<u8> = words.iter().flat_map(|word| word.to_ne_bytes()).collect();
        if let Some(text) = text {
            let length = text.len() + 1;
            body.extend_from_slice(&(length as u32).to_ne_bytes());
            body.extend_from_slice(text.as_bytes());
            body.resize(body.len() + length.div_ceil(4) * 4 - text.len(), 0);
        }
        let mut message = object.to_ne_bytes().to_vec();
        message.extend_from_slice(&((((8 + body.len()) as u32) << 16) | u32::from(opcode)).to_ne_bytes());
        message.extend_from_slice(&body);
        message
    }

    impl FakeCompositor {
        fn serve(stream: UnixStream) -> std::thread::JoinHandle<Recorded> {
            std::thread::spawn(move || {
                let mut fake = FakeCompositor {
                    stream,
                    interfaces: HashMap::from([(1, "wl_display")]),
                    layers: HashMap::new(),
                    unconfigured: HashSet::new(),
                    attached: HashMap::new(),
                    requests: Vec::new(),
                    alphas: Vec::new(),
                    serial: 0,
                };
                while fake.next() {}
                (fake.requests, fake.alphas)
            })
        }

        fn send(&mut self, bytes: &[u8]) {
            self.stream.write_all(bytes).unwrap();
        }

        /// Handles one request; false once the client hangs up.
        fn next(&mut self) -> bool {
            let mut header = [0; 8];
            if self.stream.read_exact(&mut header).is_err() {
                return false;
            }
            let object = u32::from_ne_bytes(header[0..4].try_into().unwrap());
            let word = u32::from_ne_bytes(header[4..8].try_into().unwrap());
            let opcode = (word & 0xffff) as u16;
            let mut body = vec![0; (word >> 16) as usize - 8];
            self.stream.read_exact(&mut body).unwrap();
            let words: Vec<u32> = body
                .as_chunks::<4>()
                .0
                .iter()
                .map(|chunk| u32::from_ne_bytes(*chunk))
                .collect();
            let interface = self.interfaces.get(&object).copied().unwrap_or("unknown");
            self.requests.push((interface, opcode));
            let new = |kind: &'static str, fake: &mut FakeCompositor, id: u32| {
                fake.interfaces.insert(id, kind);
            };
            match (interface, opcode) {
                ("wl_display", 0) => self.send(&event(words[0], 0, &[0], None)),
                ("wl_display", 1) => {
                    new("wl_registry", self, words[0]);
                    for (name, interface) in GLOBALS {
                        let mut global = event(words[0], 0, &[name], Some(interface));
                        // The version follows the name; the size in the header grows by it.
                        global.extend_from_slice(&5u32.to_ne_bytes());
                        let size = (global.len() as u32) << 16;
                        global[4..8].copy_from_slice(&size.to_ne_bytes());
                        self.send(&global);
                    }
                }
                ("wl_registry", 0) => {
                    let name = words[0];
                    let id = *words.last().unwrap();
                    let kind = match name {
                        1 => "wl_compositor",
                        2 => "wl_shm",
                        3 => "layer_shell",
                        4 => "alpha_modifier",
                        _ => "wl_output",
                    };
                    new(kind, self, id);
                    if kind == "wl_output" {
                        self.send(&event(id, OUTPUT_NAME, &[], Some("Virtual-0")));
                    }
                }
                ("wl_compositor", 0) => new("wl_surface", self, words[0]),
                ("wl_compositor", 1) => new("wl_region", self, words[0]),
                ("wl_shm", 0) => new("wl_shm_pool", self, words[0]),
                ("wl_shm_pool", 0) => new("wl_buffer", self, words[0]),
                ("layer_shell", 0) => {
                    new("layer_surface", self, words[0]);
                    self.layers.insert(words[0], words[1]);
                }
                ("alpha_modifier", 1) => new("alpha_surface", self, words[0]),
                ("alpha_surface", 1) => self.alphas.push(words[0]),
                ("layer_surface", 0) => {
                    self.unconfigured.insert(object);
                }
                ("wl_surface", 1) => {
                    if let Some(previous) = self.attached.insert(object, words[0]) {
                        self.send(&event(previous, BUFFER_RELEASE, &[], None));
                    }
                }
                ("wl_surface", 6) => {
                    let layers: Vec<u32> = self
                        .layers
                        .iter()
                        .filter(|(layer, surface)| **surface == object && self.unconfigured.contains(layer))
                        .map(|(layer, _)| *layer)
                        .collect();
                    for layer in layers {
                        self.unconfigured.remove(&layer);
                        self.serial += 1;
                        self.send(&event(layer, LAYER_CONFIGURE, &[self.serial, 200, 100], None));
                    }
                }
                _ => {}
            }
            true
        }
    }

    fn count(requests: &[(&str, u16)], wanted: (&str, u16)) -> usize {
        requests.iter().filter(|request| **request == wanted).count()
    }

    #[test]
    fn showing_and_hiding_change_alpha_and_never_open_or_close_the_surface() {
        let (client, server) = UnixStream::pair().unwrap();
        let fake = FakeCompositor::serve(server);
        let mut session = Session::on(Connection::on(client).unwrap()).unwrap();
        let target = Target {
            output: "Virtual-0".into(),
            origin: Point::new(0.0, 0.0),
            scale: 1.0,
        };
        let scene = |highlighted: usize, area: Rect| OverlayScene {
            work_area: area,
            zones: vec![OverlayZone {
                frame: area,
                number: 1,
                highlighted: highlighted == 1,
                caption: (highlighted == 1).then_some(Caption::Stack),
                swap_source: false,
            }],
        };
        let area = Rect::new(0.0, 0.0, 200.0, 100.0);

        session.prepare("main", &target, &scene(0, area)).unwrap();
        for cycle in 0..5 {
            session.show("main", &target, &scene(cycle % 2, area)).unwrap();
            session
                .show("main", &target, &scene((cycle + 1) % 2, area))
                .unwrap();
            session.hide().unwrap();
        }
        // Panels changed the work area: the same surface moves.
        session
            .show("main", &target, &scene(1, Rect::new(0.0, 40.0, 200.0, 60.0)))
            .unwrap();
        session.hide().unwrap();
        drop(session);
        let (requests, alphas) = fake.join().unwrap();

        assert_eq!(count(&requests, ("layer_shell", GET_LAYER_SURFACE)), 1);
        assert_eq!(count(&requests, ("layer_surface", LAYER_DESTROY)), 0);
        assert_eq!(count(&requests, ("wl_surface", SURFACE_DESTROY)), 0);
        // Prepared hidden, then each show draws and each hide only drops the alpha.
        assert_eq!(count(&requests, ("wl_surface", SURFACE_ATTACH)), 1 + 5 * 2 + 1);
        let mut expected = vec![HIDDEN];
        for _ in 0..5 {
            expected.extend([OPAQUE, OPAQUE, HIDDEN]);
        }
        expected.extend([OPAQUE, HIDDEN]);
        assert_eq!(alphas, expected);
        assert_eq!(count(&requests, ("layer_surface", LAYER_SET_SIZE)), 2);
    }

    #[test]
    fn fractional_scales_draw_at_the_next_whole_one() {
        assert_eq!(buffer_scale(1.0), 1);
        assert_eq!(buffer_scale(1.25), 2);
        assert_eq!(buffer_scale(2.0), 2);
        assert_eq!(buffer_scale(f64::NAN), 1);
        assert_eq!(buffer_scale(0.0), 1);
    }

    #[test]
    fn scenes_are_drawn_in_buffer_pixels() {
        let scene = OverlayScene {
            work_area: Rect::new(0.0, 40.0, 1920.0, 1000.0),
            zones: vec![OverlayZone {
                frame: Rect::new(10.0, 50.0, 900.0, 980.0),
                number: 1,
                highlighted: false,
                caption: None,
                swap_source: false,
            }],
        };
        let doubled = scaled(&scene, 2.0);
        assert_eq!(doubled.work_area, Rect::new(0.0, 80.0, 3840.0, 2000.0));
        assert_eq!(doubled.zones[0].frame, Rect::new(20.0, 100.0, 1800.0, 1960.0));
    }
}
