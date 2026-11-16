//! The Snappy keys and modifiers the keys tests share.

use super::{Modifier, SnappyKey};

pub const DEFAULT_MODIFIERS: [Modifier; 3] = [Modifier::Ctrl, Modifier::Alt, Modifier::Meta];

pub const DEFAULT: SnappyKey = SnappyKey::DEFAULT;

pub fn key(modifiers: &[Modifier]) -> SnappyKey {
    SnappyKey::new(modifiers.iter().copied().collect()).unwrap()
}
