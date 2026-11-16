//! The HID event system's user key mapping, what `hidutil property --set` writes: pairs of
//! HID usages, each making one key arrive as another before anything else sees it. Snappy
//! adds one pair, Caps Lock to F18, beside the user's own, and takes out only that pair.

/// HID usages: keyboard page 7 in the upper bits, the key's usage in the lower ones.
pub const CAPS_LOCK: u64 = 0x7_0000_0039;
pub const F18: u64 = 0x7_0000_006D;
/// The W3C code of the key `F18` is, which the tap watches for Caps Lock.
pub const F18_CODE: &str = "F18";

/// One key arriving as another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyMapping {
    pub source: u64,
    pub destination: u64,
}

/// Caps Lock arriving as F18, a key no Mac keyboard has, so nothing else reacts to it.
pub const SNAPPY: KeyMapping = KeyMapping {
    source: CAPS_LOCK,
    destination: F18,
};

/// How a later run names Snappy's pair to undo it.
pub const UNDO_NAME: &str = "f18";

/// `existing` with Snappy's pair added, or `None` when another pair already remaps Caps
/// Lock: the user's own remapping wins and is never replaced.
pub fn with_snappy(existing: &[KeyMapping]) -> Option<Vec<KeyMapping>> {
    if existing
        .iter()
        .any(|mapping| mapping.source == CAPS_LOCK && *mapping != SNAPPY)
    {
        return None;
    }
    let mut mappings = existing.to_vec();
    if !mappings.contains(&SNAPPY) {
        mappings.push(SNAPPY);
    }
    Some(mappings)
}

/// `existing` without Snappy's pair, the user's own pairs as they were.
pub fn without_snappy(existing: &[KeyMapping]) -> Vec<KeyMapping> {
    existing
        .iter()
        .copied()
        .filter(|mapping| *mapping != SNAPPY)
        .collect()
}

/// Whether a keyboard's modifier key pairs, the ones System Settings sets under Modifier
/// Keys, make Caps Lock do something else.
pub fn modifier_keys_remap_caps_lock(pairs: &[KeyMapping]) -> bool {
    pairs
        .iter()
        .any(|pair| pair.source == CAPS_LOCK && pair.destination != CAPS_LOCK)
}

/// The pair a run kept as `name` to undo later, if this build knows it.
pub fn named(name: &str) -> Option<KeyMapping> {
    (name == UNDO_NAME).then_some(SNAPPY)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A user's own mapping of Right Option to Right Command.
    const USERS: KeyMapping = KeyMapping {
        source: 0x7_0000_00E6,
        destination: 0x7_0000_00E7,
    };

    #[test]
    fn snappys_pair_joins_the_users_own_once() {
        let merged = with_snappy(&[USERS]).unwrap();
        assert_eq!(merged, [USERS, SNAPPY]);
        assert_eq!(with_snappy(&merged).unwrap(), merged, "adding again adds nothing");
        assert_eq!(with_snappy(&[]).unwrap(), [SNAPPY]);
    }

    #[test]
    fn a_caps_lock_the_user_already_maps_is_left_alone() {
        let to_escape = KeyMapping {
            source: CAPS_LOCK,
            destination: 0x7_0000_0029,
        };
        assert_eq!(with_snappy(&[USERS, to_escape]), None);
    }

    #[test]
    fn taking_snappys_pair_out_keeps_the_users_own() {
        assert_eq!(without_snappy(&[USERS, SNAPPY]), [USERS]);
        assert_eq!(without_snappy(&[USERS]), [USERS]);
        assert!(without_snappy(&[SNAPPY]).is_empty());
    }

    #[test]
    fn modifier_keys_that_change_caps_lock_count_and_identity_pairs_do_not() {
        let to_control = KeyMapping {
            source: CAPS_LOCK,
            destination: 0x7_0000_00E0,
        };
        let identity = KeyMapping {
            source: CAPS_LOCK,
            destination: CAPS_LOCK,
        };
        assert!(modifier_keys_remap_caps_lock(&[USERS, to_control]));
        assert!(!modifier_keys_remap_caps_lock(&[USERS, identity]));
        assert!(!modifier_keys_remap_caps_lock(&[]));
    }

    #[test]
    fn a_kept_pair_is_found_again_by_its_name_only() {
        assert_eq!(named(UNDO_NAME), Some(SNAPPY));
        assert_eq!(named("f19"), None);
        assert_eq!(named(""), None);
    }
}
