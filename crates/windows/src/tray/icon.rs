//! The icon itself: drawing it, and adding, updating, and removing it in the notification
//! area.

use snappy_platform::{TrayMenu, glyph};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, HICON, ICONINFO, SM_CXSMICON,
};
use windows::core::BOOL;

use crate::dispatch::WM_TRAY;
use crate::overlay::{Canvas, Color, Dib, Mask};
use crate::wide::copy_into;

/// The only icon Snappy adds.
pub const ICON_ID: u32 = 1;
const MINT: u32 = 0x00C7BE;

#[derive(Default)]
pub struct Tray {
    window: Option<HWND>,
    icon: Option<HICON>,
    /// The latest menu content, built into a menu each time it opens.
    pub(super) menu: TrayMenu,
    added: bool,
}

/// The glyph in the overlay's highlight color, which reads on light and dark taskbars
/// alike.
fn draw_icon(size: usize) -> Option<HICON> {
    let mut dib = Dib::new(size, size)?;
    let mut canvas = Canvas::new(size, size, dib.pixels());
    let shape = Mask {
        width: size,
        height: size,
        coverage: glyph::mask(size),
    };
    canvas.draw_mask(&shape, 0, 0, Color::rgb(MINT, 1.0));
    let side = i32::try_from(size).ok()?;
    // A cleared mask, so the color bitmap's alpha alone decides. Monochrome rows are
    // padded to whole 16-bit words.
    let mask_bits = vec![0u8; size.div_ceil(16) * 2 * size];
    // SAFETY: the bits hold a monochrome bitmap of the icon's size, and the call copies them.
    let mask = unsafe { CreateBitmap(side, side, 1, 1, Some(mask_bits.as_ptr().cast())) };
    dib.deselect();
    let info = ICONINFO {
        fIcon: BOOL(1),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: dib.bitmap,
    };
    // SAFETY: the icon copies both bitmaps; the mask is freed after.
    let icon = unsafe { CreateIconIndirect(&info) }.ok();
    // SAFETY: the mask was created above and isn't selected anywhere.
    let _ = unsafe { DeleteObject(mask.into()) };
    icon
}

impl Tray {
    fn data(&self, window: HWND) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: window,
            uID: ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: WM_TRAY,
            hIcon: self.icon.unwrap_or_default(),
            ..NOTIFYICONDATAW::default()
        };
        let tooltip = if self.menu.tooltip.is_empty() {
            "Snappy"
        } else {
            &self.menu.tooltip
        };
        copy_into(&mut data.szTip, tooltip);
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        data
    }

    /// Adds the icon; again after Explorer restarts, which drops every icon.
    pub fn add(&mut self, window: HWND) {
        self.window = Some(window);
        if self.icon.is_none() {
            // SAFETY: plain queries for a window this thread owns.
            let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForWindow(window)) };
            self.icon = draw_icon(usize::try_from(size).unwrap_or(16).max(16));
        }
        let data = self.data(window);
        // SAFETY: `data` is fully initialized for this window.
        self.added = unsafe {
            Shell_NotifyIconW(NIM_ADD, &data).as_bool() && Shell_NotifyIconW(NIM_SETVERSION, &data).as_bool()
        };
    }

    pub fn update(&mut self, menu: &TrayMenu) {
        let tooltip_changed = menu.tooltip != self.menu.tooltip;
        self.menu = menu.clone();
        if let (true, true, Some(window)) = (tooltip_changed, self.added, self.window) {
            let data = self.data(window);
            // SAFETY: as in `add`.
            let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
        }
    }

    pub fn remove(&mut self) {
        if let (true, Some(window)) = (self.added, self.window) {
            let data = self.data(window);
            // SAFETY: as in `add`.
            let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
            self.added = false;
        }
        if let Some(icon) = self.icon.take() {
            // SAFETY: an icon this module created, destroyed once.
            let _ = unsafe { DestroyIcon(icon) };
        }
    }
}
