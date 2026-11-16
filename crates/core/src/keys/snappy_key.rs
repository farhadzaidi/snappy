//! The Snappy key: the modifiers the user chose to stand for it.

use serde::{Deserialize, Serialize};

use super::{Modifier, Mods, Os};

/// The Snappy key's name in labels and messages.
pub const SNAPPY_KEY_NAME: &str = "Snappy";

/// What the Snappy key stands for: the concrete modifiers held for it, at least two of
/// Control, Option/Alt, Command/Win/Super, and Shift. The user chooses them; the default,
/// the same on every platform, is Control+Option+Command (Ctrl+Alt+Win, Ctrl+Alt+Super).
/// Shortcuts store the Snappy key as a symbol and are expanded through the settings' key
/// wherever they meet the OS or each other, so changing it moves every shortcut that uses
/// it without rewriting them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Mods", into = "Mods")]
pub struct SnappyKey(Mods);

/// Why a set of modifiers can't be the Snappy key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SnappyKeyError {
    #[error("The Snappy key needs at least two modifiers.")]
    TooFewModifiers,
}

impl SnappyKey {
    pub const DEFAULT: SnappyKey = SnappyKey(Mods(
        Modifier::Ctrl.bit() | Modifier::Alt.bit() | Modifier::Meta.bit(),
    ));
    /// Fewer would make Snappy shortcuts everyday app shortcuts, such as Ctrl+C.
    pub const MIN_MODIFIERS: u32 = 2;

    pub fn new(modifiers: Mods) -> Result<SnappyKey, SnappyKeyError> {
        if modifiers.count() < Self::MIN_MODIFIERS {
            return Err(SnappyKeyError::TooFewModifiers);
        }
        Ok(SnappyKey(modifiers))
    }

    /// The concrete modifiers held for the Snappy key.
    pub fn modifiers(self) -> Mods {
        self.0
    }

    /// What the Snappy key is on `platform`, for example "Control ⌃ + Option ⌥ + Command ⌘".
    pub fn label(self, platform: Os) -> String {
        self.0
            .in_display_order()
            .map(|modifier| modifier.label(platform))
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

impl Default for SnappyKey {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl TryFrom<Mods> for SnappyKey {
    type Error = SnappyKeyError;

    fn try_from(modifiers: Mods) -> Result<Self, Self::Error> {
        SnappyKey::new(modifiers)
    }
}

impl From<SnappyKey> for Mods {
    fn from(key: SnappyKey) -> Self {
        key.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyChord;
    use crate::keys::test_support::{DEFAULT, key};

    #[test]
    fn a_chosen_snappy_key_is_labelled_by_its_own_modifiers_and_chords_still_say_snappy() {
        let chosen = key(&[Modifier::Shift, Modifier::Alt]);
        assert_eq!(chosen.label(Os::MacOs), "Option ⌥ + Shift ⇧");
        assert_eq!(chosen.label(Os::Windows), "Alt + Shift");
        let chord = KeyChord::new("KeyJ", [Modifier::Alt, Modifier::Shift]).recorded(chosen);
        assert_eq!(chord.long_label(Os::MacOs), "Snappy + J");
    }

    #[test]
    fn the_snappy_key_needs_at_least_two_modifiers() {
        assert_eq!(
            SnappyKey::new(Mods::from_iter([Modifier::Ctrl])),
            Err(SnappyKeyError::TooFewModifiers)
        );
        assert_eq!(SnappyKey::new(Mods::NONE), Err(SnappyKeyError::TooFewModifiers));
        assert!(SnappyKey::new(Mods::from_iter([Modifier::Ctrl, Modifier::Shift])).is_ok());
        assert_eq!(
            DEFAULT.modifiers(),
            Mods::from_iter([Modifier::Ctrl, Modifier::Alt, Modifier::Meta])
        );
        assert_eq!(
            serde_json::to_string(&DEFAULT).unwrap(),
            r#"["ctrl","alt","meta"]"#
        );
        assert_eq!(
            serde_json::from_str::<SnappyKey>(r#"["shift","alt"]"#).unwrap(),
            key(&[Modifier::Alt, Modifier::Shift])
        );
        assert!(serde_json::from_str::<SnappyKey>(r#"["meta"]"#).is_err());
    }
}
