//! Zone numbers and captions rendered by GDI into coverage masks, once per string, size,
//! and weight, then reused for every redraw.

use std::collections::HashMap;

use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DeleteObject,
    GetTextExtentPoint32W, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
};
use windows::core::w;

use super::canvas::Mask;
use super::dib::Dib;
use super::paint::Glyphs;

/// Room around the text so antialiasing at the edges isn't cut off.
const PADDING: usize = 2;

#[derive(Default)]
pub struct GdiGlyphs {
    cache: HashMap<(String, u32, u32, u32), Mask>,
}

impl Glyphs for GdiGlyphs {
    fn text(&mut self, text: &str, size: f64, weight: u32, blur: f64) -> Option<&Mask> {
        let key = (text.to_owned(), size.round() as u32, weight, blur.round() as u32);
        if !self.cache.contains_key(&key) {
            let mask = render(text, key.1, weight)?;
            let mask = if key.3 > 0 {
                mask.blurred(key.3 as usize / 2)
            } else {
                mask
            };
            self.cache.insert(key.clone(), mask);
        }
        self.cache.get(&key)
    }
}

/// White text on black, read back as coverage from the green channel. GDI writes no
/// alpha, so this is how antialiased text becomes a mask.
fn render(text: &str, size: u32, weight: u32) -> Option<Mask> {
    let units: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: a font by face name; the height is negative to ask for the em size.
    let font = unsafe {
        CreateFontW(
            -(size as i32),
            0,
            0,
            0,
            weight as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            u32::from(DEFAULT_PITCH.0),
            w!("Segoe UI"),
        )
    };
    if font.is_invalid() {
        return None;
    }
    let measure = Dib::new(1, 1)?;
    let mut extent = SIZE::default();
    // SAFETY: the font is valid and selected only for the measurement.
    unsafe {
        let previous = SelectObject(measure.dc, font.into());
        let _ = GetTextExtentPoint32W(measure.dc, &units, &mut extent);
        SelectObject(measure.dc, previous);
    }
    let width = usize::try_from(extent.cx).ok()? + 2 * PADDING;
    let height = usize::try_from(extent.cy).ok()? + 2 * PADDING;
    let mut dib = Dib::new(width, height);
    if let Some(dib) = dib.as_mut() {
        // SAFETY: the DIB's DC is valid; the font is restored before it's deleted.
        unsafe {
            let previous = SelectObject(dib.dc, font.into());
            SetBkMode(dib.dc, TRANSPARENT);
            SetTextColor(dib.dc, COLORREF(0x00FF_FFFF));
            let _ = TextOutW(dib.dc, PADDING as i32, PADDING as i32, &units);
            SelectObject(dib.dc, previous);
        }
    }
    // SAFETY: the font is no longer selected anywhere.
    let _ = unsafe { DeleteObject(font.into()) };
    let mut dib = dib?;
    let coverage = dib
        .pixels()
        .iter()
        .map(|pixel| ((pixel >> 8) & 0xff) as u8)
        .collect();
    Some(Mask {
        width,
        height,
        coverage,
    })
}
