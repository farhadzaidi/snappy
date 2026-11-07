//! A 32-bit top-down DIB section selected into its own memory DC: pixels Snappy draws
//! into directly, which GDI can also draw text into and layered windows can present.

use std::ffi::c_void;

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GdiFlush, HBITMAP, HDC, HGDIOBJ, SelectObject,
};

pub struct Dib {
    pub dc: HDC,
    pub bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u32,
    pub width: usize,
    pub height: usize,
}

impl Dib {
    pub fn new(width: usize, height: usize) -> Option<Dib> {
        let (Ok(w), Ok(h)) = (i32::try_from(width), i32::try_from(height)) else {
            return None;
        };
        if w == 0 || h == 0 {
            return None;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                // Negative: rows run top to bottom.
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        // SAFETY: a memory DC compatible with the screen.
        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.is_invalid() {
            return None;
        }
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: `info` describes a 32-bit bitmap; `bits` receives its pixels.
        let Ok(bitmap) = (unsafe { CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) })
        else {
            // SAFETY: the DC was created above.
            let _ = unsafe { DeleteDC(dc) };
            return None;
        };
        // SAFETY: both handles were created above.
        let previous = unsafe { SelectObject(dc, bitmap.into()) };
        Some(Dib {
            dc,
            bitmap,
            previous,
            bits: bits.cast(),
            width,
            height,
        })
    }

    /// The pixels, after any GDI drawing has finished.
    pub fn pixels(&mut self) -> &mut [u32] {
        // SAFETY: flushes this thread's GDI batch so drawn text is in the pixels.
        let _ = unsafe { GdiFlush() };
        // SAFETY: the section holds width × height pixels for the bitmap's lifetime,
        // which this borrow can't outlive.
        unsafe { std::slice::from_raw_parts_mut(self.bits, self.width * self.height) }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        // SAFETY: restores the DC's original bitmap, then frees what `new` created.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}
