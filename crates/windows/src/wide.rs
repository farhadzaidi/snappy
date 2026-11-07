//! UTF-16 strings for Win32.

/// `text` as a null-terminated UTF-16 string.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The text before the first null, or the whole buffer when there is none.
pub fn from_wide(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|unit| *unit == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// Copies `text` into a fixed-size field, truncated so the terminating null fits.
pub fn copy_into(field: &mut [u16], text: &str) {
    let Some(capacity) = field.len().checked_sub(1) else {
        return;
    };
    let units: Vec<u16> = text.encode_utf16().take(capacity).collect();
    field[..units.len()].copy_from_slice(&units);
    field[units.len()] = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_round_trip_through_utf16() {
        assert_eq!(wide("ab"), vec![97, 98, 0]);
        assert_eq!(from_wide(&[97, 98, 0, 99]), "ab");
        assert_eq!(from_wide(&[97, 98]), "ab");
    }

    #[test]
    fn fixed_fields_keep_room_for_the_null() {
        let mut field = [1u16; 4];
        copy_into(&mut field, "abcdef");
        assert_eq!(field, [97, 98, 99, 0]);
    }
}
