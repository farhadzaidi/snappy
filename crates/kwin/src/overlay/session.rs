//! A connection to KWin with the overlay's surfaces on it: showing, hiding, and marking
//! draw on them and change their alpha, and what KWin says back is taken in here.

use std::collections::HashMap;
use std::io;

use snappy_core::geometry::Rect;
use snappy_linux::journal;
use snappy_linux::overlay_image::pixel_size;
use snappy_linux::wayland::{Arg, Connection, Message};
use snappy_platform::{OverlayMarks, OverlayScene};

use super::buffers::Buffers;
use super::protocol::{
    ALPHA_MODIFIER, ALPHA_SET_MULTIPLIER, BUFFER_RELEASE, COMPOSITOR, COMPOSITOR_VERSION, HIDDEN,
    LAYER_ACK_CONFIGURE, LAYER_CLOSED, LAYER_CONFIGURE, LAYER_SHELL, LAYER_SHELL_VERSION, OPAQUE, OUTPUT,
    OUTPUT_NAME, OUTPUT_VERSION, SHM, SURFACE_ATTACH, SURFACE_COMMIT, SURFACE_DAMAGE_BUFFER, int,
};
use super::scale::{buffer_scale, scaled, scaled_marks};
use super::surfaces::Surface;
use super::thread::Target;

/// Which of a display's two surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    Zones,
    Marks,
}

/// A surface: the display it covers and what it shows.
pub type Key = (String, Layer);

fn key(display: &str, layer: Layer) -> Key {
    (display.to_owned(), layer)
}

/// What a surface shows.
pub enum Content {
    Zones(OverlayScene),
    Marks(OverlayMarks),
}

impl Content {
    fn work_area(&self) -> Rect {
        match self {
            Content::Zones(scene) => scene.work_area,
            Content::Marks(marks) => marks.work_area,
        }
    }

    /// The content in buffer pixels: every length times `factor`.
    fn scaled(&self, factor: f64) -> Content {
        match self {
            Content::Zones(scene) => Content::Zones(scaled(scene, factor)),
            Content::Marks(marks) => Content::Marks(scaled_marks(marks, factor)),
        }
    }
}

struct Output {
    id: u32,
    name: Option<String>,
}

pub struct Session {
    pub connection: Connection,
    pub compositor: u32,
    pub shm: u32,
    pub layer_shell: u32,
    pub alpha_modifier: Option<u32>,
    outputs: Vec<Output>,
    pub surfaces: HashMap<Key, Surface>,
    /// The display whose zones are showing.
    pub visible: Option<String>,
    pub buffers: HashMap<Key, Buffers>,
}

impl Session {
    pub fn open() -> io::Result<Session> {
        Session::on(Connection::open()?)
    }

    pub fn on(mut connection: Connection) -> io::Result<Session> {
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
    pub fn roundtrip(&mut self) -> io::Result<()> {
        for message in self.connection.roundtrip()? {
            self.event(&message)?;
        }
        Ok(())
    }

    /// Output names, buffers KWin is done with, and configures, which are acknowledged at
    /// once, so the next commit answers the latest.
    pub fn event(&mut self, message: &Message) -> io::Result<()> {
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
        let Some(key) = self
            .surfaces
            .iter()
            .find(|(_, surface)| surface.layer == message.object)
            .map(|(key, _)| key.clone())
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
            LAYER_CLOSED => self.destroy_surface(&key)?,
            _ => {}
        }
        Ok(())
    }

    pub fn output(&self, name: &str) -> io::Result<u32> {
        self.outputs
            .iter()
            .find(|output| output.name.as_deref() == Some(name))
            .map(|output| output.id)
            .ok_or_else(|| io::Error::other(format!("KWin has no output named {name}")))
    }

    /// Draws the scene on the display's surface, invisibly, unless it is showing.
    pub fn prepare(&mut self, display: &str, target: &Target, scene: &OverlayScene) -> io::Result<()> {
        if self.visible.as_deref() == Some(display) {
            return Ok(());
        }
        let content = Content::Zones(scene.clone());
        self.draw_on(&key(display, Layer::Zones), target, &content, HIDDEN)
    }

    pub fn show(&mut self, display: &str, target: &Target, scene: &OverlayScene) -> io::Result<()> {
        if self.visible.as_deref().is_some_and(|visible| visible != display) {
            self.hide()?;
        }
        let content = Content::Zones(scene.clone());
        self.draw_on(&key(display, Layer::Zones), target, &content, OPAQUE)?;
        self.visible = Some(display.to_owned());
        Ok(())
    }

    /// Outlines the marked windows on the display's marks surface, or hides the surface
    /// when there are none.
    pub fn mark(&mut self, display: &str, target: &Target, marks: &OverlayMarks) -> io::Result<()> {
        let key = key(display, Layer::Marks);
        if marks.windows.is_empty() {
            return self.conceal(&key);
        }
        self.draw_on(&key, target, &Content::Marks(marks.clone()), OPAQUE)
    }

    /// Draws `content` on the surface at `alpha`, making or moving the surface first when
    /// needed. Without an alpha control a hidden scene isn't drawn ahead.
    fn draw_on(&mut self, key: &Key, target: &Target, content: &Content, alpha: u32) -> io::Result<()> {
        if alpha == HIDDEN && self.alpha_modifier.is_none() {
            return Ok(());
        }
        self.roundtrip()?;
        let factor = buffer_scale(target.scale);
        let surface = self.place(key, target, content.work_area(), factor)?;
        let scaled = content.scaled(f64::from(factor));
        let (width, height) =
            pixel_size(scaled.work_area()).ok_or_else(|| io::Error::other("the work area has no pixels"))?;
        let buffer = self.draw(key, &scaled, f64::from(factor), (width, height))?;
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

    /// Hides the zones that show.
    pub fn hide(&mut self) -> io::Result<()> {
        let Some(display) = self.visible.take() else {
            return Ok(());
        };
        self.conceal(&key(&display, Layer::Zones))
    }

    /// Hides a surface by making it invisible, so KWin sees no window close.
    fn conceal(&mut self, key: &Key) -> io::Result<()> {
        let Some(surface) = self.surfaces.get(key) else {
            return Ok(());
        };
        match surface.alpha {
            Some(control) => {
                let target = surface.surface;
                self.send(control, ALPHA_SET_MULTIPLIER, &[Arg::Uint(HIDDEN)])?;
                self.send(target, SURFACE_COMMIT, &[])?;
                self.roundtrip()
            }
            None => self.destroy_surface(key),
        }
    }

    pub fn retain(&mut self, displays: &[String]) -> io::Result<()> {
        let gone: Vec<Key> = self
            .surfaces
            .keys()
            .chain(self.buffers.keys())
            .filter(|(display, _)| !displays.contains(display))
            .cloned()
            .collect();
        for key in gone {
            self.destroy_surface(&key)?;
            self.drop_buffers(&key)?;
        }
        Ok(())
    }

    pub fn send(&mut self, object: u32, opcode: u16, args: &[Arg]) -> io::Result<()> {
        self.connection.send(object, opcode, args)
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixStream;

    use snappy_core::geometry::Point;
    use snappy_platform::{Caption, MarkStyle, MarkedWindow, OverlayZone};

    use super::super::fake_compositor::{FakeCompositor, count};
    use super::super::protocol::{GET_LAYER_SURFACE, LAYER_DESTROY, LAYER_SET_SIZE, SURFACE_DESTROY};
    use super::*;

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
    fn marks_get_a_surface_of_their_own_that_stays_while_the_zones_hide() {
        let (client, server) = UnixStream::pair().unwrap();
        let fake = FakeCompositor::serve(server);
        let mut session = Session::on(Connection::on(client).unwrap()).unwrap();
        let target = Target {
            output: "Virtual-0".into(),
            origin: Point::new(0.0, 0.0),
            scale: 1.0,
        };
        let area = Rect::new(0.0, 0.0, 200.0, 100.0);
        let scene = OverlayScene {
            work_area: area,
            zones: Vec::new(),
        };
        let marks = |windows: Vec<MarkedWindow>| OverlayMarks {
            work_area: area,
            windows,
        };
        let outlined = |style| MarkedWindow {
            frame: Rect::new(20.0, 10.0, 80.0, 60.0),
            style,
        };

        session.prepare("main", &target, &scene).unwrap();
        session
            .mark("main", &target, &marks(vec![outlined(MarkStyle::Selected)]))
            .unwrap();
        session
            .mark("main", &target, &marks(vec![outlined(MarkStyle::Focused)]))
            .unwrap();
        session.show("main", &target, &scene).unwrap();
        session.hide().unwrap();
        session.mark("main", &target, &marks(Vec::new())).unwrap();
        drop(session);
        let (requests, alphas) = fake.join().unwrap();

        assert_eq!(count(&requests, ("layer_shell", GET_LAYER_SURFACE)), 2);
        assert_eq!(count(&requests, ("layer_surface", LAYER_DESTROY)), 0);
        assert_eq!(alphas, [HIDDEN, OPAQUE, OPAQUE, OPAQUE, HIDDEN, HIDDEN]);
    }
}
