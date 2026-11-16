//! Panels like Plasma's, which reserve space along a display's edge.

use std::os::fd::AsFd;

use snappy_linux::wayland::{Arg, Connection};

use crate::support::WAIT;

/// A panel along a display's top or bottom edge, reserving its thickness the way Plasma's
/// panels do: a layer-shell surface with the dock namespace and an exclusive zone.
pub struct Panel {
    connection: Connection,
    surface: u32,
    layer: u32,
    _pixels: std::fs::File,
}

#[derive(Clone, Copy)]
pub enum Edge {
    Top,
    Bottom,
}

const ANCHOR_TOP: u32 = 1;
const ANCHOR_BOTTOM: u32 = 2;
const ANCHOR_LEFT_RIGHT: u32 = 4 | 8;

impl Panel {
    pub fn open(output: &str, edge: Edge, thickness: u32) -> Panel {
        let mut connection = Connection::open().unwrap();
        let (compositor, _) = connection.bind("wl_compositor", 4).unwrap();
        let (shm, _) = connection.bind("wl_shm", 1).unwrap();
        let (layer_shell, _) = connection.bind("zwlr_layer_shell_v1", 3).unwrap();
        let output = output_named(&mut connection, output);

        let surface = connection.new_id();
        connection.send(compositor, 0, &[Arg::Uint(surface)]).unwrap();
        let layer = connection.new_id();
        connection
            .send(
                layer_shell,
                0,
                &[
                    Arg::Uint(layer),
                    Arg::Uint(surface),
                    Arg::Uint(output),
                    Arg::Uint(2),
                    Arg::Str("dock"),
                ],
            )
            .unwrap();
        let anchor = match edge {
            Edge::Top => ANCHOR_TOP,
            Edge::Bottom => ANCHOR_BOTTOM,
        } | ANCHOR_LEFT_RIGHT;
        connection.send(layer, 1, &[Arg::Uint(anchor)]).unwrap();
        connection
            .send(layer, 0, &[Arg::Uint(0), Arg::Uint(thickness)])
            .unwrap();
        connection.send(layer, 2, &[Arg::Int(thickness as i32)]).unwrap();
        connection.send(surface, 6, &[]).unwrap();
        let configure = connection
            .read_until(WAIT, |message| message.object == layer && message.opcode == 0)
            .expect("KWin configures the panel")
            .pop()
            .unwrap();
        let mut reader = configure.reader();
        let (serial, width, height) = (
            reader.uint().unwrap(),
            reader.uint().unwrap(),
            reader.uint().unwrap(),
        );
        connection.send(layer, 6, &[Arg::Uint(serial)]).unwrap();

        let stride = width * 4;
        let pixels = scratch_file();
        pixels.set_len(u64::from(stride * height)).unwrap();
        let pool = connection.new_id();
        connection
            .send_with_fd(
                shm,
                0,
                &[Arg::Uint(pool), Arg::Int((stride * height) as i32)],
                pixels.as_fd(),
            )
            .unwrap();
        let buffer = connection.new_id();
        connection
            .send(
                pool,
                0,
                &[
                    Arg::Uint(buffer),
                    Arg::Int(0),
                    Arg::Int(width as i32),
                    Arg::Int(height as i32),
                    Arg::Int(stride as i32),
                    Arg::Uint(0),
                ],
            )
            .unwrap();
        connection
            .send(surface, 1, &[Arg::Uint(buffer), Arg::Int(0), Arg::Int(0)])
            .unwrap();
        connection.send(surface, 6, &[]).unwrap();
        connection.roundtrip().unwrap();
        Panel {
            connection,
            surface,
            layer,
            _pixels: pixels,
        }
    }

    /// Changes only the space the panel reserves, as switching it to auto-hide does, which
    /// KWin tells scripts nothing about.
    pub fn reserve(&mut self, thickness: i32) {
        self.connection
            .send(self.layer, 2, &[Arg::Int(thickness)])
            .unwrap();
        self.connection.send(self.surface, 6, &[]).unwrap();
        self.connection.roundtrip().unwrap();
    }
}

pub fn output_named(connection: &mut Connection, name: &str) -> u32 {
    let globals: Vec<_> = connection
        .globals()
        .iter()
        .filter(|global| global.interface == "wl_output")
        .cloned()
        .collect();
    let ids: Vec<u32> = globals
        .iter()
        .map(|global| connection.bind_global(global, 4).unwrap().0)
        .collect();
    connection
        .roundtrip()
        .unwrap()
        .into_iter()
        .find(|message| {
            ids.contains(&message.object)
                && message.opcode == 4
                && message.reader().string().as_deref() == Some(name)
        })
        .map(|message| message.object)
        .unwrap_or_else(|| panic!("no output named {name}"))
}

/// A file only this process can reach, for pixels KWin reads by descriptor.
pub fn scratch_file() -> std::fs::File {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let folder = std::env::var_os("XDG_RUNTIME_DIR").unwrap();
    let path = std::path::PathBuf::from(folder).join(format!("snappy-test-{}-{next}", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    file
}
