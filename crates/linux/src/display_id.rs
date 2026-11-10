//! Display IDs that stay the same across reconnects and restarts: the connector name, plus
//! a hash of what identifies the monitor itself, so a different monitor on the same port
//! gets a different ID.

/// `connector` alone when nothing identifies the monitor, else `connector-<hash>`.
pub fn display_id(connector: &str, identity: Option<&[u8]>) -> String {
    match identity.filter(|bytes| !bytes.is_empty()) {
        Some(bytes) => format!("{connector}-{:016x}", fnv1a(bytes)),
        None => connector.to_owned(),
    }
}

/// 64-bit FNV-1a, which is stable across Rust releases, unlike the standard hasher.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_ids_combine_the_connector_and_a_hash_of_the_monitor() {
        assert_eq!(display_id("DP-1", None), "DP-1");
        assert_eq!(display_id("DP-1", Some(&[])), "DP-1");
        let a = display_id("DP-1", Some(&[1, 2, 3]));
        assert_eq!(a, display_id("DP-1", Some(&[1, 2, 3])));
        assert_ne!(a, display_id("DP-1", Some(&[1, 2, 4])));
        assert_ne!(a, display_id("DP-2", Some(&[1, 2, 3])));
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
