//! Monitor names from EDID blocks.

const EDID_HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];
const EDID_DESCRIPTORS: [usize; 4] = [54, 72, 90, 108];
const EDID_NAME_TAG: u8 = 0xFC;

/// The monitor name descriptor of an EDID block, when it has one.
pub fn edid_name(edid: &[u8]) -> Option<String> {
    if edid.len() < 128 || edid[..8] != EDID_HEADER {
        return None;
    }
    EDID_DESCRIPTORS.iter().find_map(|&offset| {
        let descriptor = &edid[offset..offset + 18];
        if descriptor[..3] != [0, 0, 0] || descriptor[3] != EDID_NAME_TAG {
            return None;
        }
        let text: String = descriptor[5..]
            .iter()
            .take_while(|byte| **byte != b'\n')
            .map(|byte| char::from(*byte))
            .collect();
        let name = text.trim();
        (!name.is_empty()).then(|| name.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edid_with_name(name: &[u8]) -> Vec<u8> {
        let mut edid = vec![0u8; 128];
        edid[..8].copy_from_slice(&EDID_HEADER);
        edid[72..77].copy_from_slice(&[0, 0, 0, EDID_NAME_TAG, 0]);
        let mut text = [b' '; 13];
        text[..name.len()].copy_from_slice(name);
        if name.len() < 13 {
            text[name.len()] = b'\n';
        }
        edid[77..90].copy_from_slice(&text);
        edid
    }

    #[test]
    fn edid_names_come_from_the_name_descriptor() {
        assert_eq!(
            edid_name(&edid_with_name(b"DELL U2720Q")),
            Some("DELL U2720Q".into())
        );
        assert_eq!(
            edid_name(&edid_with_name(b"ABCDEFGHIJKLM")),
            Some("ABCDEFGHIJKLM".into())
        );
        assert_eq!(edid_name(&[0u8; 128]), None);
        assert_eq!(edid_name(&edid_with_name(b"X")[..100]), None);
    }
}
