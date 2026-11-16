//! A shortcut's chord: a key with its modifiers, as stored and as the OS sees it.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::{ChordMods, KeyCode, Modifier, Mods, Os, SNAPPY_KEY_NAME, SnappyKey};

/// Labels never exceed this many characters.
pub const MAX_LABEL_LENGTH: usize = 24;

/// A shortcut: one physical key plus modifiers, and the label recorded with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: ChordMods,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Why a chord can't be a shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidChord {
    /// A key Snappy has no name for.
    UnknownKey,
    /// Neither the Snappy key nor Control, Option/Alt, or Command/Win/Super, so it would take
    /// a key press from every app.
    NoCommandModifier,
    /// A label that is empty or longer than `MAX_LABEL_LENGTH`.
    BadLabel,
}

impl InvalidChord {
    /// What to tell someone who pressed it while recording a shortcut.
    pub fn hint(self, platform: Os) -> String {
        match self {
            InvalidChord::NoCommandModifier => format!(
                "Include {}, {}, or {}.",
                Modifier::Ctrl.name(platform),
                Modifier::Alt.name(platform),
                Modifier::Meta.name(platform)
            ),
            InvalidChord::UnknownKey | InvalidChord::BadLabel => {
                "Snappy can't use that key in a shortcut.".to_owned()
            }
        }
    }
}

/// What identifies a shortcut for conflicts and registration: the key and the concrete
/// modifiers to hold, never the label. This is the form adapters register and probe, so
/// they never see the Snappy key itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Combo {
    pub code: KeyCode,
    pub mods: Mods,
}

impl Combo {
    pub fn new(code: &str, mods: impl IntoIterator<Item = Modifier>) -> Self {
        Self {
            code: KeyCode::new(code),
            mods: mods.into_iter().collect(),
        }
    }
}

/// The chord that holds exactly the combo's modifiers, none of them as the Snappy key.
impl From<Combo> for KeyChord {
    fn from(combo: Combo) -> Self {
        Self {
            code: combo.code,
            mods: ChordMods {
                snappy: false,
                others: combo.mods,
            },
            label: None,
        }
    }
}

impl KeyChord {
    /// A chord of concrete modifiers, as given, which stays so whatever the Snappy key.
    pub fn new(code: &str, mods: impl IntoIterator<Item = Modifier>) -> Self {
        Self {
            code: KeyCode::new(code),
            mods: ChordMods {
                snappy: false,
                others: mods.into_iter().collect(),
            },
            label: None,
        }
    }

    /// The Snappy key plus `others` and `code`.
    pub fn snappy(code: &str, others: impl IntoIterator<Item = Modifier>) -> Self {
        Self {
            code: KeyCode::new(code),
            mods: ChordMods::snappy_with(others),
            label: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// This key pressed with this chord's modifiers held, as recording it with `key` as the
    /// Snappy key stores it (`ChordMods::recorded`).
    pub fn recorded(mut self, key: SnappyKey) -> Self {
        self.mods = ChordMods::recorded(self.mods.expanded(key), key);
        self
    }

    /// The key with the Snappy key expanded to the modifiers `key` stands for.
    pub fn combo(&self, key: SnappyKey) -> Combo {
        Combo {
            code: self.code.clone(),
            mods: self.mods.expanded(key),
        }
    }

    /// Whether the chord can be a shortcut, and why not. Every Snappy key holds a command
    /// modifier: it has at least two modifiers, and only one of them can be Shift.
    pub fn check(&self) -> Result<(), InvalidChord> {
        if !self.code.is_known() {
            return Err(InvalidChord::UnknownKey);
        }
        if !self.mods.has_snappy_key() && !self.mods.others().has_command_modifier() {
            return Err(InvalidChord::NoCommandModifier);
        }
        let label_fits = |label: &String| !label.is_empty() && label.chars().count() <= MAX_LABEL_LENGTH;
        if !self.label.as_ref().is_none_or(label_fits) {
            return Err(InvalidChord::BadLabel);
        }
        Ok(())
    }

    pub fn is_valid(&self) -> bool {
        self.check().is_ok()
    }

    /// The key's own label, without modifiers.
    pub fn key_label(&self) -> String {
        self.label.clone().unwrap_or_else(|| self.code.default_label())
    }

    /// "Snappy + Shift ⇧ + 1" or "Control ⌃ + Option ⌥ + L" on macOS, "Ctrl + Alt + L"
    /// elsewhere: how messages, recorders, and cards all name a chord. The Snappy key is
    /// always named as itself, never as the modifiers it stands for.
    pub fn long_label(&self, platform: Os) -> String {
        let parts = self.label_parts(platform);
        if parts.snappy_key {
            format!("{SNAPPY_KEY_NAME} + {}", parts.keys)
        } else {
            parts.keys
        }
    }

    /// The long label split where the Snappy key ends, so a UI can draw the key its own way.
    pub fn label_parts(&self, platform: Os) -> LabelParts {
        let mut keys: Vec<String> = self.mods.other_labels(platform).collect();
        keys.push(self.key_label());
        LabelParts {
            snappy_key: self.mods.has_snappy_key(),
            keys: keys.join(" + "),
        }
    }
}

/// A chord's long label in two parts. The Snappy key has no character of its own, so a UI
/// names it first, with its glyph, as macOS names ⌘ Command, and joins it to `keys` with a
/// plus: "Snappy + Shift ⇧ + 1".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelParts {
    pub snappy_key: bool,
    /// What follows the Snappy key, or the whole label without it: "Shift ⇧ + 1" on macOS
    /// and "Shift + 1" elsewhere.
    pub keys: String,
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.long_label(Os::MacOs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::test_support::{DEFAULT, DEFAULT_MODIFIERS};

    #[test]
    fn labels_name_the_snappy_key_then_modifiers_in_display_order() {
        let chord = KeyChord::snappy("Digit1", []);
        assert_eq!(chord.long_label(Os::MacOs), "Snappy + 1");
        let shifted = KeyChord::snappy("BracketLeft", [Modifier::Shift]);
        assert_eq!(shifted.long_label(Os::Linux), "Snappy + Shift + [");
        let arrow = KeyChord::new("ArrowLeft", [Modifier::Meta, Modifier::Ctrl, Modifier::Shift]);
        assert_eq!(arrow.long_label(Os::MacOs), "Control ⌃ + Shift ⇧ + Command ⌘ + ←");
        assert_eq!(DEFAULT.label(Os::MacOs), "Control ⌃ + Option ⌥ + Command ⌘");
        assert_eq!(DEFAULT.label(Os::Windows), "Ctrl + Alt + Win");
        assert_eq!(DEFAULT.label(Os::Linux), "Ctrl + Alt + Super");
    }

    #[test]
    fn label_parts_name_modifiers_with_their_symbols_on_macos_and_by_name_elsewhere() {
        let parts = |chord: &KeyChord, platform| {
            let label = chord.label_parts(platform);
            (label.snappy_key, label.keys)
        };
        let shifted = KeyChord::snappy("KeyJ", [Modifier::Shift]);
        assert_eq!(parts(&shifted, Os::MacOs), (true, "Shift ⇧ + J".to_owned()));
        assert_eq!(parts(&shifted, Os::Windows), (true, "Shift + J".to_owned()));
        let digit = KeyChord::snappy("Digit1", []);
        assert_eq!(parts(&digit, Os::Linux), (true, "1".to_owned()));

        let arrow = KeyChord::new("ArrowLeft", [Modifier::Meta, Modifier::Ctrl, Modifier::Shift]);
        assert_eq!(
            parts(&arrow, Os::MacOs),
            (false, "Control ⌃ + Shift ⇧ + Command ⌘ + ←".to_owned())
        );
        assert_eq!(
            parts(&arrow, Os::Windows),
            (false, "Ctrl + Shift + Win + ←".to_owned())
        );
        let relabelled = KeyChord::new("KeyQ", [Modifier::Meta]).with_label("A");
        assert_eq!(parts(&relabelled, Os::MacOs), (false, "Command ⌘ + A".to_owned()));
    }

    #[test]
    fn chords_need_a_command_modifier_and_a_known_key() {
        assert!(KeyChord::snappy("KeyZ", []).is_valid());
        assert_eq!(
            KeyChord::new("KeyZ", [Modifier::Shift]).check(),
            Err(InvalidChord::NoCommandModifier)
        );
        assert_eq!(
            KeyChord::new("NotAKey", DEFAULT_MODIFIERS).check(),
            Err(InvalidChord::UnknownKey)
        );
        assert_eq!(
            KeyChord::new("KeyZ", DEFAULT_MODIFIERS).with_label("").check(),
            Err(InvalidChord::BadLabel)
        );
    }

    #[test]
    fn the_hint_for_a_chord_without_a_command_modifier_names_the_platforms_modifiers() {
        assert_eq!(
            InvalidChord::NoCommandModifier.hint(Os::MacOs),
            "Include Control, Option, or Command."
        );
        assert_eq!(
            InvalidChord::NoCommandModifier.hint(Os::Windows),
            "Include Ctrl, Alt, or Win."
        );
    }

    #[test]
    fn labels_never_affect_identity() {
        let a = KeyChord::new("Digit1", DEFAULT_MODIFIERS).with_label("&");
        let b = KeyChord::new("Digit1", DEFAULT_MODIFIERS);
        assert_eq!(a.combo(DEFAULT), b.combo(DEFAULT));
    }
}
