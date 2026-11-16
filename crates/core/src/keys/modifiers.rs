//! Modifier keys, one at a time and as sets.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::Os;

/// A modifier key as the operating system reports and registers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    Shift,
    Ctrl,
    Alt,
    Meta,
}

impl Modifier {
    pub(super) const ALL: [Modifier; 4] = [Modifier::Shift, Modifier::Ctrl, Modifier::Alt, Modifier::Meta];
    /// The order modifiers are shown in labels.
    const DISPLAY_ORDER: [Modifier; 4] = [Modifier::Ctrl, Modifier::Alt, Modifier::Shift, Modifier::Meta];

    pub(super) const fn bit(self) -> u8 {
        match self {
            Modifier::Shift => 1,
            Modifier::Ctrl => 2,
            Modifier::Alt => 4,
            Modifier::Meta => 8,
        }
    }

    pub fn name(self, platform: Os) -> &'static str {
        match (self, platform) {
            (Modifier::Shift, _) => "Shift",
            (Modifier::Ctrl, Os::MacOs) => "Control",
            (Modifier::Ctrl, _) => "Ctrl",
            (Modifier::Alt, Os::MacOs) => "Option",
            (Modifier::Alt, _) => "Alt",
            (Modifier::Meta, Os::MacOs) => "Command",
            (Modifier::Meta, Os::Windows) => "Win",
            (Modifier::Meta, Os::Linux) => "Super",
        }
    }

    /// The modifier's symbol on macOS ("⌘"), its name elsewhere.
    pub fn symbol(self, platform: Os) -> &'static str {
        match (self, platform) {
            (Modifier::Shift, Os::MacOs) => "⇧",
            (Modifier::Ctrl, Os::MacOs) => "⌃",
            (Modifier::Alt, Os::MacOs) => "⌥",
            (Modifier::Meta, Os::MacOs) => "⌘",
            _ => self.name(platform),
        }
    }

    /// The name with its symbol, as every label shows it: "Shift ⇧" on macOS, where the
    /// symbols are what keyboards print, and the name alone elsewhere.
    pub fn label(self, platform: Os) -> String {
        match platform {
            Os::MacOs => format!("{} {}", self.name(platform), self.symbol(platform)),
            _ => self.name(platform).to_owned(),
        }
    }
}

/// A set of concrete modifiers, as the OS reports and registers them. Serialized as a list
/// in a fixed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Mods(pub(super) u8);

impl Mods {
    pub const NONE: Mods = Mods(0);

    pub fn contains(self, modifier: Modifier) -> bool {
        self.0 & modifier.bit() != 0
    }

    pub fn with(self, modifier: Modifier) -> Mods {
        Mods(self.0 | modifier.bit())
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// How many modifiers the set holds.
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// True when every modifier in `other` is in this set too.
    pub fn includes(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn union(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }

    pub fn without(self, other: Mods) -> Mods {
        Mods(self.0 & !other.0)
    }

    /// True when the set includes Control, Option/Alt, or Command/Win/Super, which every
    /// assignable shortcut needs.
    pub fn has_command_modifier(self) -> bool {
        self.contains(Modifier::Ctrl) || self.contains(Modifier::Alt) || self.contains(Modifier::Meta)
    }

    pub fn iter(self) -> impl Iterator<Item = Modifier> {
        Modifier::ALL
            .into_iter()
            .filter(move |modifier| self.contains(*modifier))
    }

    /// The modifiers in the order labels show them.
    pub fn in_display_order(self) -> impl Iterator<Item = Modifier> {
        Modifier::DISPLAY_ORDER
            .into_iter()
            .filter(move |modifier| self.contains(*modifier))
    }
}

impl FromIterator<Modifier> for Mods {
    fn from_iter<I: IntoIterator<Item = Modifier>>(iter: I) -> Self {
        iter.into_iter().fold(Mods::NONE, Mods::with)
    }
}

impl Serialize for Mods {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for Mods {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Vec::<Modifier>::deserialize(deserializer)?.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mods_serialize_in_fixed_order() {
        let json = serde_json::to_string(&Mods::from_iter([Modifier::Meta, Modifier::Shift])).unwrap();
        assert_eq!(json, r#"["shift","meta"]"#);
        let mods: Mods = serde_json::from_str(r#"["alt","ctrl"]"#).unwrap();
        assert!(mods.contains(Modifier::Ctrl) && mods.contains(Modifier::Alt));
    }
}
