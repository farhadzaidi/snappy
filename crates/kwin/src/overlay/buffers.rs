//! Each surface's shared-memory pool and its two buffers, and drawing into them in bands.

use std::fs::File;
use std::io;
use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;

use snappy_linux::overlay_image::{render_mark_rows, render_rows, to_argb};
use snappy_linux::wayland::Arg;

use super::protocol::{
    BUFFER_DESTROY, FORMAT_ARGB8888, POOL_CREATE_BUFFER, POOL_DESTROY, SHM_CREATE_POOL, int,
};
use super::session::{Content, Key, Session};

const PIXEL_BYTES: u32 = 4;
/// Rows drawn and written at a time, so no whole image is held in memory.
const BAND_ROWS: u32 = 128;
/// Two buffers per display: one KWin may still be showing, one to draw into.
const SLOTS: usize = 2;

pub struct Slot {
    pub buffer: u32,
    pub busy: bool,
}

/// A display's shared-memory pool and its buffers, for one size and scale.
pub struct Buffers {
    file: File,
    pool: u32,
    size: (u32, u32),
    pub slots: Vec<Slot>,
}

impl Session {
    /// Draws `content`, `factor` buffer pixels to KWin's, into a buffer KWin isn't using
    /// and returns it.
    pub fn draw(&mut self, key: &Key, content: &Content, factor: f64, size: (u32, u32)) -> io::Result<u32> {
        if self.buffers.get(key).is_none_or(|buffers| buffers.size != size) {
            self.drop_buffers(key)?;
            let buffers = self.allocate(size)?;
            self.buffers.insert(key.clone(), buffers);
        }
        let render = |top: u32, rows: u32| match content {
            Content::Zones(scene) => render_rows(scene, factor, top, rows),
            Content::Marks(marks) => render_mark_rows(marks, factor, top, rows),
        };
        let Some(buffers) = self.buffers.get_mut(key) else {
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
            let mut band =
                render(top, rows).ok_or_else(|| io::Error::other("the overlay couldn't be drawn"))?;
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

    pub fn drop_buffers(&mut self, key: &Key) -> io::Result<()> {
        let Some(buffers) = self.buffers.remove(key) else {
            return Ok(());
        };
        for slot in &buffers.slots {
            self.send(slot.buffer, BUFFER_DESTROY, &[])?;
        }
        self.send(buffers.pool, POOL_DESTROY, &[])
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
