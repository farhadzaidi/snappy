//! Zone overlays: one layered, click-through, topmost popup per display, covering
//! its work area. Scenes are drawn in software into a DIB and handed to the window with
//! `UpdateLayeredWindow`; DWM keeps the result, so the pixels are freed right after and an
//! idle overlay costs Snappy no memory beyond its window. A surface redraws only when its
//! scene changes. Marked windows' outlines go on a second popup per display of the same
//! kind, so they stay while the zones show and hide.

mod canvas;
mod dib;
mod paint;
mod style;
mod text;

use std::collections::HashMap;

use snappy_core::geometry::Rect;
use snappy_platform::{OverlayMarks, OverlayScene};
use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Dwm::DwmGetColorizationColor;
use windows::Win32::Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, HWND_TOPMOST, SW_HIDE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SetWindowPos, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};
use windows::core::{BOOL, w};

use crate::class;

pub use self::canvas::{Canvas, Color};
pub use self::dib::Dib;
use self::paint::{paint, paint_marks};
use self::style::DEFAULT_ACCENT;
use self::text::GdiGlyphs;
use crate::frames::{extent, pixel};

const CLASS: windows::core::PCWSTR = w!("Snappy.Overlay");

struct Surface {
    window: HWND,
    drawn: Option<OverlayScene>,
}

struct MarkSurface {
    window: HWND,
    drawn: Option<OverlayMarks>,
}

#[derive(Default)]
pub struct Overlays {
    surfaces: HashMap<String, Surface>,
    marks: HashMap<String, MarkSurface>,
    glyphs: GdiGlyphs,
    class_registered: bool,
}

/// The system accent, from DWM's colorization color.
fn accent() -> u32 {
    let (mut color, mut opaque) = (0u32, BOOL(0));
    // SAFETY: valid out pointers.
    match unsafe { DwmGetColorizationColor(&mut color, &mut opaque) } {
        Ok(()) => color & 0x00FF_FFFF,
        Err(_) => DEFAULT_ACCENT,
    }
}

/// A hidden overlay window: layered and transparent to clicks, never activated, above
/// normal windows, and kept off the taskbar and Alt+Tab.
fn create_window() -> Option<HWND> {
    let style = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
    class::popup(CLASS, style, 0, 0, 1, 1)
}

/// Draws `scene` and hands it to the layered window.
fn present(window: HWND, scene: &OverlayScene, scale: f64, glyphs: &mut GdiGlyphs) -> bool {
    present_drawn(window, scene.work_area, |canvas| {
        paint(canvas, scene, scale, accent(), glyphs)
    })
}

/// Draws the outlines of `marks` and hands them to the layered window.
fn present_marks(window: HWND, marks: &OverlayMarks, scale: f64) -> bool {
    present_drawn(window, marks.work_area, |canvas| {
        paint_marks(canvas, marks, scale)
    })
}

/// Draws an image covering `area` with `draw` and hands it to the layered window.
fn present_drawn(window: HWND, area: Rect, draw: impl FnOnce(&mut Canvas)) -> bool {
    let (Some(x), Some(y), Some(width), Some(height)) = (
        pixel(area.x),
        pixel(area.y),
        extent(area.width),
        extent(area.height),
    ) else {
        return false;
    };
    let Some(mut dib) = Dib::new(width as usize, height as usize) else {
        return false;
    };
    let mut canvas = Canvas::new(dib.width, dib.height, dib.pixels());
    draw(&mut canvas);
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    // SAFETY: the DIB's DC holds premultiplied pixels of the given size.
    unsafe {
        UpdateLayeredWindow(
            window,
            None,
            Some(&POINT { x, y }),
            Some(&SIZE {
                cx: width,
                cy: height,
            }),
            Some(dib.dc),
            Some(&POINT::default()),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        )
    }
    .is_ok()
}

fn hide_window(window: HWND) {
    // SAFETY: a window this thread owns.
    let _ = unsafe { ShowWindow(window, SW_HIDE) };
}

/// Shows a window above the others without activating it.
fn show_topmost(window: HWND) {
    // SAFETY: a window this thread owns; it is shown without activation.
    let _ = unsafe {
        SetWindowPos(
            window,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    };
}

fn destroy_window(window: HWND) {
    // SAFETY: a window this thread owns, destroyed once.
    let _ = unsafe { DestroyWindow(window) };
}

impl Overlays {
    /// A new hidden overlay window, registering the class the first time.
    fn new_window(&mut self) -> Option<HWND> {
        if !self.class_registered {
            class::register(CLASS, Some(class::default_procedure));
            self.class_registered = true;
        }
        create_window()
    }

    fn surface(&mut self, display: &str) -> Option<&mut Surface> {
        if !self.surfaces.contains_key(display) {
            let window = self.new_window()?;
            self.surfaces
                .insert(display.to_owned(), Surface { window, drawn: None });
        }
        self.surfaces.get_mut(display)
    }

    /// Outlines `marks` on `display` above every window, or hides the outlines when there
    /// are none.
    pub fn mark(&mut self, display: &str, marks: &OverlayMarks, scale: f64) {
        if marks.frames.is_empty() {
            if let Some(surface) = self.marks.get_mut(display) {
                hide_window(surface.window);
                surface.drawn = None;
            }
            return;
        }
        if !self.marks.contains_key(display) {
            let Some(window) = self.new_window() else {
                return;
            };
            self.marks
                .insert(display.to_owned(), MarkSurface { window, drawn: None });
        }
        let Some(surface) = self.marks.get_mut(display) else {
            return;
        };
        if surface.drawn.as_ref() != Some(marks) {
            surface.drawn = present_marks(surface.window, marks, scale).then(|| marks.clone());
        }
        if surface.drawn.is_some() {
            show_topmost(surface.window);
        }
    }

    /// Draws `scene` for `display` ahead of time, so showing it later costs nothing.
    pub fn prepare(&mut self, display: &str, scene: &OverlayScene, scale: f64) -> Option<HWND> {
        let mut glyphs = std::mem::take(&mut self.glyphs);
        let window = self.surface(display).and_then(|surface| {
            if surface.drawn.as_ref() != Some(scene) {
                surface.drawn = present(surface.window, scene, scale, &mut glyphs).then(|| scene.clone());
            }
            surface.drawn.is_some().then_some(surface.window)
        });
        self.glyphs = glyphs;
        window
    }

    /// Shows `display`'s overlay and hides every other one.
    pub fn show(&mut self, display: &str, scene: &OverlayScene, scale: f64) {
        for (id, surface) in &self.surfaces {
            if id != display {
                hide_window(surface.window);
            }
        }
        if let Some(window) = self.prepare(display, scene, scale) {
            show_topmost(window);
        }
    }

    pub fn hide(&mut self) {
        for surface in self.surfaces.values() {
            hide_window(surface.window);
        }
    }

    /// Releases surfaces of displays not in `displays`.
    pub fn retain(&mut self, displays: &[String]) {
        self.surfaces.retain(|id, surface| {
            let keep = displays.contains(id);
            if !keep {
                destroy_window(surface.window);
            }
            keep
        });
        self.marks.retain(|id, surface| {
            let keep = displays.contains(id);
            if !keep {
                destroy_window(surface.window);
            }
            keep
        });
    }

    pub fn close_all(&mut self) {
        self.retain(&[]);
    }
}
