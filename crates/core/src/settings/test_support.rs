//! What the settings tests share.

use crate::keys::{Combo, KeyChord, Os, SnappyKey};

pub const MAC: Os = Os::MacOs;

pub fn snappy(code: &str) -> Option<Combo> {
    Some(KeyChord::snappy(code, []).combo(SnappyKey::DEFAULT))
}
