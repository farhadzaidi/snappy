//! The system's own key remapping: the `Scancode Map` value under
//! `HKLM\SYSTEM\CurrentControlSet\Control\Keyboard Layout`, which the keyboard driver
//! applies before anything else sees a key. Snappy only reads it.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_BINARY, RegGetValueW};
use windows::core::w;

/// Caps Lock's set-1 scan code.
const CAPS_LOCK: u16 = 0x3A;
/// A version, flags, and the entry count come before the entries.
const HEADER: usize = 12;
/// The map is a few entries; a larger value is read only this far.
const MAX_BYTES: usize = 4096;

/// Whether `map`, a `Scancode Map` value, turns the Caps Lock key into another key or none.
/// Each entry is the new scan code then the original, 16 bits each, little-endian; the
/// count includes the zero entry that ends the list.
pub fn remaps_caps_lock(map: &[u8]) -> bool {
    let Some(count) = map
        .get(8..HEADER)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
    else {
        return false;
    };
    let (entries, _) = map.get(HEADER..).unwrap_or_default().as_chunks::<4>();
    entries
        .iter()
        .take(count as usize)
        .any(|&[new_low, new_high, low, high]| {
            let new = u16::from_le_bytes([new_low, new_high]);
            let original = u16::from_le_bytes([low, high]);
            original == CAPS_LOCK && new != CAPS_LOCK
        })
}

/// Whether the system's `Scancode Map` remaps Caps Lock. Without the value nothing is
/// remapped.
pub fn system_remaps_caps_lock() -> bool {
    let mut buffer = vec![0u8; MAX_BYTES];
    let mut size = buffer.len() as u32;
    // SAFETY: the buffer and its byte size match.
    let read = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SYSTEM\CurrentControlSet\Control\Keyboard Layout"),
            w!("Scancode Map"),
            RRF_RT_REG_BINARY,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    read == ERROR_SUCCESS && remaps_caps_lock(buffer.get(..size as usize).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map with `entries` of (new, original) and its terminating zero.
    fn map(entries: &[(u16, u16)]) -> Vec<u8> {
        let mut bytes = vec![0u8; 8];
        bytes.extend((entries.len() as u32 + 1).to_le_bytes());
        for (new, original) in entries {
            bytes.extend(new.to_le_bytes());
            bytes.extend(original.to_le_bytes());
        }
        bytes.extend([0u8; 4]);
        bytes
    }

    #[test]
    fn caps_lock_made_into_another_key_or_none_is_a_remapping() {
        assert!(remaps_caps_lock(&map(&[(0x1D, CAPS_LOCK)])), "Caps Lock as Ctrl");
        assert!(
            remaps_caps_lock(&map(&[(0x01, 0x46), (0x00, CAPS_LOCK)])),
            "Caps Lock off"
        );
    }

    #[test]
    fn other_keys_remapped_even_onto_caps_lock_leave_it_alone() {
        assert!(!remaps_caps_lock(&map(&[(CAPS_LOCK, 0x1D)])), "Ctrl as Caps Lock");
        assert!(!remaps_caps_lock(&map(&[(CAPS_LOCK, CAPS_LOCK)])));
        assert!(!remaps_caps_lock(&map(&[])));
    }

    #[test]
    fn a_short_or_truncated_value_remaps_nothing() {
        assert!(!remaps_caps_lock(&[]));
        assert!(!remaps_caps_lock(&[0; 10]));
        let mut truncated = map(&[(0x1D, CAPS_LOCK)]);
        truncated.truncate(14);
        assert!(!remaps_caps_lock(&truncated));
    }

    #[test]
    fn entries_past_the_count_are_ignored() {
        let mut bytes = map(&[]);
        bytes.extend(0x1Du16.to_le_bytes());
        bytes.extend(CAPS_LOCK.to_le_bytes());
        assert!(!remaps_caps_lock(&bytes));
    }
}
