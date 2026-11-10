//! Overlay windows: per display, one override-redirect window over its work area for the
//! zones and another for the marked windows' outlines, so the outlines stay while zones
//! come and go. Both are 32-bit ARGB when the server offers it and click-through through
//! an empty XFixes input region. Each drawing becomes the window's background pixmap, so
//! the server repaints exposures without a round trip to Snappy.

use std::collections::HashMap;
use std::sync::Arc;

use snappy_core::geometry::Rect;
use snappy_platform::{OverlayMarks, OverlayScene};
use tiny_skia::Pixmap;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, ImageFormat, PropMode, Rectangle, StackMode, Window, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;

use crate::shared::Shared;
use snappy_linux::overlay_image::{self as render, mark_outline, outline, pixel_size, to_argb};

const CLASS: &[u8] = b"snappy-overlay\0Snappy\0";
/// Bytes per pixel in a 24- or 32-bit Z-format image.
const PIXEL_BYTES: usize = 4;
/// Room for a `PutImage` request's header.
const REQUEST_HEADER: usize = 64;
/// Rows rendered and sent at a time; a 4K-wide band is about 1 MB.
const BAND_ROWS: u32 = 64;

/// What an overlay window shows.
#[derive(Clone, PartialEq)]
enum Content {
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

    fn render_rows(&self, scale: f64, top: u32, rows: u32) -> Option<Pixmap> {
        match self {
            Content::Zones(scene) => render::render_rows(scene, scale, top, rows),
            Content::Marks(marks) => render::render_mark_rows(marks, scale, top, rows),
        }
    }

    /// The parts that show without a compositor.
    fn outline(&self, scale: f64) -> Vec<Rect> {
        match self {
            Content::Zones(scene) => outline(scene, scale),
            Content::Marks(marks) => mark_outline(marks, scale),
        }
    }
}

/// Which of a display's two overlay windows.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Layer {
    Zones,
    Marks,
}

struct Surface {
    window: Window,
    area: Rect,
    /// What the background shows, and whether it was cut to outlines.
    drawn: Option<(Content, bool)>,
}

pub struct Overlays {
    shared: Arc<Shared>,
    /// Depth and colormap of the ARGB visual, when there is one.
    argb: Option<(u8, u32)>,
    compositor_atom: Option<u32>,
    surfaces: HashMap<(String, Layer), Surface>,
    /// The display whose zones show.
    visible: Option<String>,
    /// The displays whose marks show.
    marked: Vec<String>,
}

impl Overlays {
    pub fn new(shared: Arc<Shared>) -> Overlays {
        let argb = shared.screen.argb_visual.and_then(|visual| {
            let colormap = shared.conn.generate_id().ok()?;
            shared
                .conn
                .create_colormap(ColormapAlloc::NONE, colormap, shared.screen.root, visual)
                .ok()?
                .check()
                .ok()?;
            Some((32, colormap))
        });
        let name = format!("_NET_WM_CM_S{}", shared.screen.number);
        let compositor_atom = shared
            .conn
            .intern_atom(false, name.as_bytes())
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.atom);
        Overlays {
            shared,
            argb,
            compositor_atom,
            surfaces: HashMap::new(),
            visible: None,
            marked: Vec::new(),
        }
    }

    /// Readies a display's surface so the first drag never waits for it.
    pub fn prepare(&mut self, display: &str, scene: &OverlayScene) {
        self.draw_layer(display, Layer::Zones, Content::Zones(scene.clone()));
        let _ = self.shared.conn.flush();
    }

    /// Shows one display's zones above every window and hides the others'.
    pub fn show(&mut self, display: &str, scene: &OverlayScene) {
        self.prepare(display, scene);
        if self.visible.as_deref() != Some(display) {
            self.hide();
        }
        self.raise(display, Layer::Zones);
        let _ = self.shared.conn.flush();
        self.visible = Some(display.to_owned());
    }

    pub fn hide(&mut self) {
        let Some(display) = self.visible.take() else {
            return;
        };
        self.unmap(&display, Layer::Zones);
        let _ = self.shared.conn.flush();
    }

    /// Outlines `marks` on `display`, above every window, or takes the outlines down when
    /// there are none.
    pub fn mark(&mut self, display: &str, marks: &OverlayMarks) {
        if marks.frames.is_empty() {
            self.unmap(display, Layer::Marks);
            self.marked.retain(|marked| marked != display);
        } else {
            self.draw_layer(display, Layer::Marks, Content::Marks(marks.clone()));
            self.raise(display, Layer::Marks);
            if !self.marked.iter().any(|marked| marked == display) {
                self.marked.push(display.to_owned());
            }
        }
        let _ = self.shared.conn.flush();
    }

    /// Puts the outlines back on top, after the window manager raised a window over them.
    pub fn raise_marks(&mut self) {
        for display in self.marked.clone() {
            self.raise(&display, Layer::Marks);
        }
        let _ = self.shared.conn.flush();
    }

    /// Destroys the surfaces of displays no longer connected.
    pub fn retain(&mut self, displays: &[String]) {
        let gone: Vec<(String, Layer)> = self
            .surfaces
            .keys()
            .filter(|(display, _)| !displays.contains(display))
            .cloned()
            .collect();
        for key in gone {
            self.close(&key);
        }
        let _ = self.shared.conn.flush();
    }

    pub fn close_all(&mut self) {
        let keys: Vec<(String, Layer)> = self.surfaces.keys().cloned().collect();
        for key in keys {
            self.close(&key);
        }
        if let Some((_, colormap)) = self.argb.take() {
            let _ = self.shared.conn.free_colormap(colormap);
        }
        let _ = self.shared.conn.flush();
    }

    fn close(&mut self, key: &(String, Layer)) {
        if let Some(surface) = self.surfaces.remove(key) {
            let _ = self.shared.conn.destroy_window(surface.window);
        }
        let (display, layer) = key;
        match layer {
            Layer::Zones if self.visible.as_ref() == Some(display) => self.visible = None,
            Layer::Zones => {}
            Layer::Marks => self.marked.retain(|marked| marked != display),
        }
    }

    /// Draws `content` on the display's surface for `layer` unless it already shows it.
    fn draw_layer(&mut self, display: &str, layer: Layer, content: Content) {
        let outlines = !self.has_compositor();
        let Some(window) = self.surface(display, layer, content.work_area()) else {
            return;
        };
        let key = (display.to_owned(), layer);
        let up_to_date = self
            .surfaces
            .get(&key)
            .and_then(|surface| surface.drawn.as_ref())
            .is_some_and(|(drawn, cut)| *drawn == content && *cut == outlines);
        if !up_to_date
            && self.draw(window, display, &content, outlines)
            && let Some(surface) = self.surfaces.get_mut(&key)
        {
            surface.drawn = Some((content, outlines));
        }
    }

    fn raise(&self, display: &str, layer: Layer) {
        let Some(surface) = self.surfaces.get(&(display.to_owned(), layer)) else {
            return;
        };
        let conn = &self.shared.conn;
        let _ = conn.map_window(surface.window);
        let _ = conn.configure_window(
            surface.window,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        );
    }

    fn unmap(&self, display: &str, layer: Layer) {
        if let Some(surface) = self.surfaces.get(&(display.to_owned(), layer)) {
            let _ = self.shared.conn.unmap_window(surface.window);
        }
    }

    /// Translucency needs a compositing manager, which owns `_NET_WM_CM_S<screen>`.
    fn has_compositor(&self) -> bool {
        let Some(atom) = self.compositor_atom else {
            return false;
        };
        self.argb.is_some()
            && self
                .shared
                .conn
                .get_selection_owner(atom)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .is_some_and(|reply| reply.owner != x11rb::NONE)
    }

    /// The display's window for `layer`, created or moved to cover `area`.
    fn surface(&mut self, display: &str, layer: Layer, area: Rect) -> Option<Window> {
        let (width, height) = pixel_size(area)?;
        let key = (display.to_owned(), layer);
        if let Some(surface) = self.surfaces.get_mut(&key) {
            if surface.area != area {
                let geometry = ConfigureWindowAux::new()
                    .x(area.x as i32)
                    .y(area.y as i32)
                    .width(width)
                    .height(height);
                let _ = self.shared.conn.configure_window(surface.window, &geometry);
                surface.area = area;
                surface.drawn = None;
            }
            return Some(surface.window);
        }
        let window = self.create_window(area, width, height)?;
        self.surfaces.insert(
            key,
            Surface {
                window,
                area,
                drawn: None,
            },
        );
        Some(window)
    }

    fn create_window(&self, area: Rect, width: u32, height: u32) -> Option<Window> {
        let conn = &self.shared.conn;
        let window = conn.generate_id().ok()?;
        let mut attributes = CreateWindowAux::new().override_redirect(1).border_pixel(0);
        let (depth, visual) = match (self.argb, self.shared.screen.argb_visual) {
            (Some((depth, colormap)), Some(visual)) => {
                attributes = attributes.colormap(colormap);
                (depth, visual)
            }
            _ => (x11rb::COPY_DEPTH_FROM_PARENT, x11rb::COPY_FROM_PARENT),
        };
        conn.create_window(
            depth,
            window,
            self.shared.screen.root,
            area.x as i16,
            area.y as i16,
            width as u16,
            height as u16,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &attributes,
        )
        .ok()?
        .check()
        .ok()?;

        let _ = conn.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            CLASS,
        );
        let atoms = &self.shared.atoms;
        let _ = conn.change_property32(
            PropMode::REPLACE,
            window,
            atoms._NET_WM_WINDOW_TYPE,
            AtomEnum::ATOM,
            &[atoms._NET_WM_WINDOW_TYPE_DND],
        );
        self.make_click_through(window);
        Some(window)
    }

    /// An empty input region lets every click through to the windows below.
    fn make_click_through(&self, window: Window) {
        let conn = &self.shared.conn;
        let Ok(region) = conn.generate_id() else { return };
        if conn.xfixes_create_region(region, &[]).is_ok() {
            let _ = conn.xfixes_set_window_shape_region(window, shape::SK::INPUT, 0, 0, region);
            let _ = conn.xfixes_destroy_region(region);
        }
    }

    /// Draws `content` into a fresh background pixmap; true on success.
    fn draw(&self, window: Window, display: &str, content: &Content, outlines: bool) -> bool {
        let scale = self.shared.scale(display);
        let Some((width, height)) = pixel_size(content.work_area()) else {
            return false;
        };
        let depth = self.argb.map_or(self.root_depth(), |(depth, _)| depth);
        if self
            .upload(window, depth, (width, height), content, scale)
            .is_none()
        {
            return false;
        }
        self.cut(window, content, scale, outlines);
        true
    }

    fn root_depth(&self) -> u8 {
        self.shared
            .conn
            .setup()
            .roots
            .get(self.shared.screen.number)
            .map_or(24, |screen| screen.root_depth)
    }

    /// Renders the content band by band into a new pixmap, sets it as the window's
    /// background, and frees Snappy's handle; the window keeps the pixmap alive.
    fn upload(
        &self,
        window: Window,
        depth: u8,
        (width, height): (u32, u32),
        content: &Content,
        scale: f64,
    ) -> Option<()> {
        let conn = &self.shared.conn;
        let pixmap = conn.generate_id().ok()?;
        conn.create_pixmap(depth, pixmap, window, width as u16, height as u16)
            .ok()?
            .check()
            .ok()?;
        let gc = conn.generate_id().ok()?;
        conn.create_gc(gc, pixmap, &CreateGCAux::new()).ok()?;

        let row_bytes = width as usize * PIXEL_BYTES;
        let request_rows = conn.maximum_request_bytes().saturating_sub(REQUEST_HEADER) / row_bytes;
        let band = (request_rows as u32).clamp(1, BAND_ROWS);
        for top in (0..height).step_by(band as usize) {
            let Some(mut image) = content.render_rows(scale, top, band) else {
                break;
            };
            to_argb(image.data_mut(), self.shared.screen.msb_first);
            let _ = conn.put_image(
                ImageFormat::Z_PIXMAP,
                pixmap,
                gc,
                width as u16,
                image.height() as u16,
                0,
                top as i16,
                0,
                depth,
                image.data(),
            );
        }

        let background = ChangeWindowAttributesAux::new().background_pixmap(pixmap);
        let _ = conn.change_window_attributes(window, &background);
        let _ = conn.free_gc(gc);
        let _ = conn.free_pixmap(pixmap);
        let _ = conn.clear_area(false, window, 0, 0, 0, 0);
        Some(())
    }

    /// Without a compositor, cuts the window down to outlines; otherwise restores the
    /// full rectangle.
    fn cut(&self, window: Window, content: &Content, scale: f64, outlines: bool) {
        let conn = &self.shared.conn;
        if !outlines {
            let _ = conn.shape_mask(shape::SO::SET, shape::SK::BOUNDING, window, 0, 0, x11rb::NONE);
            return;
        }
        let rectangles: Vec<Rectangle> = content
            .outline(scale)
            .into_iter()
            .filter(|rect| rect.width >= 1.0 && rect.height >= 1.0)
            .map(|rect| Rectangle {
                x: rect.x.round() as i16,
                y: rect.y.round() as i16,
                width: rect.width.round() as u16,
                height: rect.height.round() as u16,
            })
            .collect();
        let _ = conn.shape_rectangles(
            shape::SO::SET,
            shape::SK::BOUNDING,
            x11rb::protocol::xproto::ClipOrdering::UNSORTED,
            window,
            0,
            0,
            &rectangles,
        );
    }
}

/// XFixes requests need the version negotiated first, once per connection.
pub fn negotiate(shared: &Shared) {
    if let Ok(cookie) = shared.conn.xfixes_query_version(5, 0) {
        let _ = cookie.reply();
    }
    if let Ok(cookie) = shared.conn.shape_query_version() {
        let _ = cookie.reply();
    }
}
