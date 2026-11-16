//! A shortcut's modifiers as stored: the Snappy key or not, plus concrete modifiers.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{Modifier, Mods, Os, SnappyKey};

/// A modifier as a shortcut stores it: the Snappy key, or a concrete modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoredModifier {
    Snappy,
    Shift,
    Ctrl,
    Alt,
    Meta,
}

impl From<Modifier> for StoredModifier {
    fn from(modifier: Modifier) -> Self {
        match modifier {
            Modifier::Shift => StoredModifier::Shift,
            Modifier::Ctrl => StoredModifier::Ctrl,
            Modifier::Alt => StoredModifier::Alt,
            Modifier::Meta => StoredModifier::Meta,
        }
    }
}

/// A shortcut's modifiers: the Snappy key or not, plus the concrete modifiers held with
/// it. Serialized as a list such as `["snappy","shift"]` or `["ctrl","alt","meta"]`.
///
/// Recording decides which, once (`recorded`): holding every modifier of the Snappy key
/// records the key plus whatever else is held, and anything else records exactly the
/// modifiers held. Nothing changes the form afterwards. A Snappy key chord follows the
/// key wherever it goes, keeping the modifiers recorded with it; a concrete chord keeps
/// its modifiers whatever the key becomes, even ones the key comes to match, where the
/// two are told apart only by the conflict between them (their combos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ChordMods {
    pub(super) snappy: bool,
    pub(super) others: Mods,
}

impl ChordMods {
    /// What recording a chord with `held` down stores while the Snappy key stands for
    /// `key`'s modifiers: the Snappy key plus the rest when every one of them is held.
    pub fn recorded(held: Mods, key: SnappyKey) -> Self {
        let snappy = key.modifiers();
        if held.includes(snappy) {
            Self {
                snappy: true,
                others: held.without(snappy),
            }
        } else {
            Self {
                snappy: false,
                others: held,
            }
        }
    }

    /// The Snappy key plus `others`, whatever the key stands for.
    pub fn snappy_with(others: impl IntoIterator<Item = Modifier>) -> Self {
        Self {
            snappy: true,
            others: others.into_iter().collect(),
        }
    }

    pub fn has_snappy_key(self) -> bool {
        self.snappy
    }

    /// The concrete modifiers held besides the Snappy key.
    pub fn others(self) -> Mods {
        self.others
    }

    /// The concrete modifiers to hold: the Snappy key's, plus the others.
    pub fn expanded(self, key: SnappyKey) -> Mods {
        if self.snappy {
            key.modifiers().union(self.others)
        } else {
            self.others
        }
    }

    fn with_other(self, modifier: Modifier) -> Self {
        Self {
            others: self.others.with(modifier),
            ..self
        }
    }

    fn stored(self) -> impl Iterator<Item = StoredModifier> {
        self.snappy
            .then_some(StoredModifier::Snappy)
            .into_iter()
            .chain(self.others.iter().map(StoredModifier::from))
    }

    /// The modifiers besides the Snappy key, labelled, in display order.
    pub(super) fn other_labels(self, platform: Os) -> impl Iterator<Item = String> {
        self.others
            .in_display_order()
            .map(move |modifier| modifier.label(platform))
    }
}

impl Serialize for ChordMods {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.stored())
    }
}

/// Read as written: only recording decides whether a chord holds the Snappy key.
impl<'de> Deserialize<'de> for ChordMods {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = Vec::<StoredModifier>::deserialize(deserializer)?;
        Ok(stored
            .into_iter()
            .fold(ChordMods::default(), |mods, modifier| match modifier {
                StoredModifier::Snappy => ChordMods { snappy: true, ..mods },
                StoredModifier::Shift => mods.with_other(Modifier::Shift),
                StoredModifier::Ctrl => mods.with_other(Modifier::Ctrl),
                StoredModifier::Alt => mods.with_other(Modifier::Alt),
                StoredModifier::Meta => mods.with_other(Modifier::Meta),
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::test_support::{DEFAULT, DEFAULT_MODIFIERS, key};
    use crate::keys::{Combo, KeyChord};

    #[test]
    fn recording_every_snappy_key_modifier_records_the_snappy_key() {
        let recorded = KeyChord::new(
            "Digit1",
            [Modifier::Shift, Modifier::Ctrl, Modifier::Alt, Modifier::Meta],
        )
        .recorded(DEFAULT);
        assert!(recorded.mods.has_snappy_key());
        assert_eq!(recorded.mods.others(), Mods::from_iter([Modifier::Shift]));
        assert_eq!(recorded, KeyChord::snappy("Digit1", [Modifier::Shift]));

        let partial = KeyChord::new("Digit1", [Modifier::Ctrl, Modifier::Alt]).recorded(DEFAULT);
        assert!(!partial.mods.has_snappy_key());
    }

    #[test]
    fn recording_follows_the_chosen_snappy_key() {
        let control_option = key(&[Modifier::Ctrl, Modifier::Alt]);
        let held = KeyChord::new("KeyJ", [Modifier::Ctrl, Modifier::Alt, Modifier::Shift]);
        assert_eq!(
            held.clone().recorded(control_option),
            KeyChord::snappy("KeyJ", [Modifier::Shift])
        );
        assert_eq!(
            held.clone().recorded(DEFAULT),
            held,
            "the default key also needs Command"
        );
    }

    #[test]
    fn a_snappy_key_chord_keeps_the_modifiers_recorded_with_it_whatever_the_key() {
        let snappy_shift = KeyChord::snappy("KeyJ", [Modifier::Shift]);
        let control_shift = key(&[Modifier::Ctrl, Modifier::Shift]);
        assert_eq!(
            snappy_shift.combo(control_shift),
            Combo::new("KeyJ", [Modifier::Ctrl, Modifier::Shift]),
            "a modifier the key holds adds nothing to the keys pressed"
        );
        assert_eq!(snappy_shift.long_label(Os::MacOs), "Snappy + Shift ⇧ + J");
        assert_eq!(
            snappy_shift.combo(DEFAULT),
            Combo::new(
                "KeyJ",
                [Modifier::Shift, Modifier::Ctrl, Modifier::Alt, Modifier::Meta]
            ),
            "back on the default key, Shift is held again"
        );
    }

    #[test]
    fn the_snappy_key_expands_to_the_chosen_modifiers_wherever_chords_are_compared() {
        let symbolic = KeyChord::snappy("Digit1", []);
        assert_eq!(symbolic.combo(DEFAULT), Combo::new("Digit1", DEFAULT_MODIFIERS));
        assert_eq!(
            KeyChord::snappy("Digit1", [Modifier::Ctrl]).combo(DEFAULT),
            Combo::new("Digit1", DEFAULT_MODIFIERS),
            "a modifier the Snappy key already holds adds nothing"
        );
        let chosen = key(&[Modifier::Alt, Modifier::Meta]);
        assert_eq!(
            symbolic.combo(chosen),
            Combo::new("Digit1", [Modifier::Alt, Modifier::Meta])
        );
        assert_eq!(
            KeyChord::new("Digit1", [Modifier::Ctrl]).combo(chosen),
            Combo::new("Digit1", [Modifier::Ctrl]),
            "a concrete chord is the same whatever the key"
        );
    }

    #[test]
    fn chord_modifiers_are_read_and_written_as_stored() {
        let json = serde_json::to_string(&KeyChord::snappy("Digit1", [Modifier::Shift])).unwrap();
        assert_eq!(json, r#"{"code":"Digit1","mods":["snappy","shift"]}"#);
        let written = r#"{"code":"Digit1","mods":["ctrl","alt","meta"]}"#;
        let concrete: KeyChord = serde_json::from_str(written).unwrap();
        assert!(
            !concrete.mods.has_snappy_key(),
            "the default key's modifiers by hand stay so"
        );
        assert_eq!(
            concrete.combo(DEFAULT),
            KeyChord::snappy("Digit1", []).combo(DEFAULT)
        );
        assert_eq!(serde_json::to_string(&concrete).unwrap(), written);
        let doubled: ChordMods = serde_json::from_str(r#"["ctrl","snappy"]"#).unwrap();
        assert_eq!(doubled, ChordMods::snappy_with([Modifier::Ctrl]));
        assert_eq!(serde_json::to_string(&doubled).unwrap(), r#"["snappy","ctrl"]"#);
        assert!(serde_json::from_str::<ChordMods>(r#"["hyper"]"#).is_err());
    }

    proptest::proptest! {
        #[test]
        fn recording_keeps_the_held_modifiers_and_survives_a_save(bits in 0u8..16, key_bits in 0u8..16) {
            let of = |bits: u8| -> Mods {
                Modifier::ALL
                    .into_iter()
                    .filter(|modifier| bits & modifier.bit() != 0)
                    .collect()
            };
            let held = of(bits);
            let key = SnappyKey::new(of(key_bits)).unwrap_or(DEFAULT);
            let mods = ChordMods::recorded(held, key);
            proptest::prop_assert_eq!(mods.expanded(key), held);
            proptest::prop_assert_eq!(mods.has_snappy_key(), held.includes(key.modifiers()));
            proptest::prop_assert_eq!(ChordMods::recorded(mods.expanded(key), key), mods);
            let saved = serde_json::to_string(&mods).unwrap();
            proptest::prop_assert_eq!(serde_json::from_str::<ChordMods>(&saved).unwrap(), mods);
        }
    }
}
