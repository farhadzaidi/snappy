//! Keyboard shortcuts as physical keys plus modifiers, independent of any platform.
//! A chord names the key's physical position, so it stays on the same key on every
//! keyboard layout.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Labels never exceed this many characters.
pub const MAX_LABEL_LENGTH: usize = 24;

/// The operating system, for display names, defaults, and reserved shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

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
    const ALL: [Modifier; 4] = [Modifier::Shift, Modifier::Ctrl, Modifier::Alt, Modifier::Meta];
    /// The order modifiers are shown in labels.
    const DISPLAY_ORDER: [Modifier; 4] = [Modifier::Ctrl, Modifier::Alt, Modifier::Shift, Modifier::Meta];

    fn bit(self) -> u8 {
        match self {
            Modifier::Shift => 1,
            Modifier::Ctrl => 2,
            Modifier::Alt => 4,
            Modifier::Meta => 8,
        }
    }

    pub fn name(self, platform: Platform) -> &'static str {
        match (self, platform) {
            (Modifier::Shift, _) => "Shift",
            (Modifier::Ctrl, Platform::MacOs) => "Control",
            (Modifier::Ctrl, _) => "Ctrl",
            (Modifier::Alt, Platform::MacOs) => "Option",
            (Modifier::Alt, _) => "Alt",
            (Modifier::Meta, Platform::MacOs) => "Command",
            (Modifier::Meta, Platform::Windows) => "Win",
            (Modifier::Meta, Platform::Linux) => "Super",
        }
    }

    pub fn short_name(self, platform: Platform) -> &'static str {
        match (self, platform) {
            (Modifier::Ctrl, Platform::MacOs) => "Ctrl",
            (Modifier::Alt, Platform::MacOs) => "Opt",
            (Modifier::Meta, Platform::MacOs) => "Cmd",
            _ => self.name(platform),
        }
    }

    /// The name with its symbol, as messages show it: "Shift ⇧".
    pub fn label(self, platform: Platform) -> String {
        match platform {
            Platform::MacOs => {
                let symbol = match self {
                    Modifier::Shift => "⇧",
                    Modifier::Ctrl => "⌃",
                    Modifier::Alt => "⌥",
                    Modifier::Meta => "⌘",
                };
                format!("{} {symbol}", self.name(platform))
            }
            _ => self.name(platform).to_owned(),
        }
    }
}

/// What the Snappy key is: the concrete modifiers it stands for, Control+Option+Command on
/// macOS, Ctrl+Alt+Win on Windows, and Ctrl+Alt+Super on Linux. This is its only
/// definition. Shortcuts store the Snappy key as a symbol and are expanded through this
/// function wherever they meet the OS or each other, so changing it changes every shortcut
/// that uses it without touching settings files.
pub fn snappy_key_modifiers() -> Mods {
    Mods::from_iter([Modifier::Ctrl, Modifier::Alt, Modifier::Meta])
}

/// The Snappy key's name in labels and messages.
pub const SNAPPY_KEY_NAME: &str = "Snappy";

/// What the Snappy key is on `platform`, for example "Control + Option + Command".
pub fn snappy_key_label(platform: Platform) -> String {
    snappy_key_modifiers()
        .in_display_order()
        .map(|modifier| modifier.name(platform))
        .collect::<Vec<_>>()
        .join(" + ")
}

/// A set of concrete modifiers, as the OS reports and registers them. Serialized as a list
/// in a fixed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Mods(u8);

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

    fn in_display_order(self) -> impl Iterator<Item = Modifier> {
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
/// it. Always normalized: a set holding every modifier the Snappy key stands for is the
/// Snappy key plus the rest, so `["ctrl","alt","meta"]` and `["snappy"]` are one shortcut.
/// Serialized as a list such as `["snappy","shift"]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ChordMods {
    snappy: bool,
    others: Mods,
}

impl ChordMods {
    /// The normalized form of a set of concrete modifiers.
    pub fn from_concrete(mods: Mods) -> Self {
        let snappy = snappy_key_modifiers();
        if !snappy.is_empty() && mods.includes(snappy) {
            Self {
                snappy: true,
                others: mods.without(snappy),
            }
        } else {
            Self {
                snappy: false,
                others: mods,
            }
        }
    }

    /// The Snappy key plus `others`.
    pub fn snappy_with(others: impl IntoIterator<Item = Modifier>) -> Self {
        Self::from_concrete(snappy_key_modifiers().union(others.into_iter().collect()))
    }

    pub fn has_snappy_key(self) -> bool {
        self.snappy
    }

    /// The concrete modifiers held besides the Snappy key.
    pub fn others(self) -> Mods {
        self.others
    }

    /// The concrete modifiers to hold: the Snappy key's, plus the others.
    pub fn expanded(self) -> Mods {
        if self.snappy {
            snappy_key_modifiers().union(self.others)
        } else {
            self.others
        }
    }

    pub fn with(self, modifier: Modifier) -> ChordMods {
        Self::from_concrete(self.expanded().with(modifier))
    }

    fn stored(self) -> impl Iterator<Item = StoredModifier> {
        self.snappy
            .then_some(StoredModifier::Snappy)
            .into_iter()
            .chain(self.others.iter().map(StoredModifier::from))
    }

    /// Modifier names in display order, the Snappy key first.
    fn names(self, name: impl Fn(Modifier) -> &'static str) -> impl Iterator<Item = &'static str> {
        self.snappy
            .then_some(SNAPPY_KEY_NAME)
            .into_iter()
            .chain(self.others.in_display_order().map(name))
    }
}

impl FromIterator<Modifier> for ChordMods {
    fn from_iter<I: IntoIterator<Item = Modifier>>(iter: I) -> Self {
        Self::from_concrete(iter.into_iter().collect())
    }
}

impl Serialize for ChordMods {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.stored())
    }
}

impl<'de> Deserialize<'de> for ChordMods {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = Vec::<StoredModifier>::deserialize(deserializer)?;
        let mods = stored
            .into_iter()
            .fold(Mods::NONE, |mods, modifier| match modifier {
                StoredModifier::Snappy => mods.union(snappy_key_modifiers()),
                StoredModifier::Shift => mods.with(Modifier::Shift),
                StoredModifier::Ctrl => mods.with(Modifier::Ctrl),
                StoredModifier::Alt => mods.with(Modifier::Alt),
                StoredModifier::Meta => mods.with(Modifier::Meta),
            });
        Ok(Self::from_concrete(mods))
    }
}

/// A physical key, named by its W3C `KeyboardEvent.code`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeyCode(String);

impl KeyCode {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_known(&self) -> bool {
        KNOWN_CODES.contains(&self.0.as_str())
    }

    /// The label for this key on a US layout, used when a chord has no recorded label.
    pub fn default_label(&self) -> String {
        let code = self.0.as_str();
        if let Some(letter) = code.strip_prefix("Key") {
            return letter.to_owned();
        }
        if let Some(digit) = code.strip_prefix("Digit") {
            return digit.to_owned();
        }
        if let Some(digit) = code.strip_prefix("Numpad").filter(|rest| rest.len() == 1) {
            return format!("Num {digit}");
        }
        let label = match code {
            "ArrowLeft" => "←",
            "ArrowRight" => "→",
            "ArrowUp" => "↑",
            "ArrowDown" => "↓",
            "Space" => "Space",
            "Enter" => "↩",
            "Tab" => "⇥",
            "Backspace" => "⌫",
            "Delete" => "⌦",
            "Escape" => "⎋",
            "Minus" => "-",
            "Equal" => "=",
            "BracketLeft" => "[",
            "BracketRight" => "]",
            "Backslash" => "\\",
            "Semicolon" => ";",
            "Quote" => "'",
            "Comma" => ",",
            "Period" => ".",
            "Slash" => "/",
            "Backquote" => "`",
            other => other,
        };
        label.to_owned()
    }
}

/// Every key code a shortcut may use.
const KNOWN_CODES: &[&str] = &[
    "KeyA",
    "KeyB",
    "KeyC",
    "KeyD",
    "KeyE",
    "KeyF",
    "KeyG",
    "KeyH",
    "KeyI",
    "KeyJ",
    "KeyK",
    "KeyL",
    "KeyM",
    "KeyN",
    "KeyO",
    "KeyP",
    "KeyQ",
    "KeyR",
    "KeyS",
    "KeyT",
    "KeyU",
    "KeyV",
    "KeyW",
    "KeyX",
    "KeyY",
    "KeyZ",
    "Digit0",
    "Digit1",
    "Digit2",
    "Digit3",
    "Digit4",
    "Digit5",
    "Digit6",
    "Digit7",
    "Digit8",
    "Digit9",
    "Minus",
    "Equal",
    "BracketLeft",
    "BracketRight",
    "Backslash",
    "Semicolon",
    "Quote",
    "Comma",
    "Period",
    "Slash",
    "Backquote",
    "IntlBackslash",
    "IntlYen",
    "IntlRo",
    "Enter",
    "Tab",
    "Space",
    "Backspace",
    "Delete",
    "Escape",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Help",
    "ContextMenu",
    "ArrowLeft",
    "ArrowRight",
    "ArrowUp",
    "ArrowDown",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    "F13",
    "F14",
    "F15",
    "F16",
    "F17",
    "F18",
    "F19",
    "F20",
    "Numpad0",
    "Numpad1",
    "Numpad2",
    "Numpad3",
    "Numpad4",
    "Numpad5",
    "Numpad6",
    "Numpad7",
    "Numpad8",
    "Numpad9",
    "NumpadDecimal",
    "NumpadMultiply",
    "NumpadAdd",
    "NumpadSubtract",
    "NumpadDivide",
    "NumpadEnter",
    "NumpadEqual",
    "NumpadComma",
    "NumLock",
    "Lang1",
    "Lang2",
    "AudioVolumeUp",
    "AudioVolumeDown",
    "AudioVolumeMute",
];

/// A shortcut: one physical key plus modifiers, and the label recorded with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: ChordMods,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
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

impl KeyChord {
    /// A chord of concrete modifiers, normalized so that holding all of the Snappy key's
    /// modifiers makes it a Snappy key chord.
    pub fn new(code: &str, mods: impl IntoIterator<Item = Modifier>) -> Self {
        Self {
            code: KeyCode::new(code),
            mods: mods.into_iter().collect(),
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

    /// The key with the Snappy key expanded to its concrete modifiers.
    pub fn combo(&self) -> Combo {
        Combo {
            code: self.code.clone(),
            mods: self.mods.expanded(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.code.is_known()
            && self.mods.expanded().has_command_modifier()
            && self
                .label
                .as_ref()
                .is_none_or(|label| !label.is_empty() && label.chars().count() <= MAX_LABEL_LENGTH)
    }

    fn key_label(&self) -> String {
        self.label.clone().unwrap_or_else(|| self.code.default_label())
    }

    fn label_with(&self, name: impl Fn(Modifier) -> &'static str) -> String {
        let mut parts: Vec<String> = self.mods.names(name).map(str::to_owned).collect();
        parts.push(self.key_label());
        parts.join(" + ")
    }

    /// "Snappy + Shift + 1" or "Control + Option + L", as messages show it. The Snappy key
    /// is always named as itself, never as the modifiers it stands for.
    pub fn long_label(&self, platform: Platform) -> String {
        self.label_with(|modifier| modifier.name(platform))
    }

    /// "Snappy + Shift + 1" or "Ctrl + Opt + L", used on recorders and cards.
    pub fn compact_label(&self, platform: Platform) -> String {
        self.label_with(|modifier| modifier.short_name(platform))
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.long_label(Platform::MacOs))
    }
}

/// Standard app commands that can never become global shortcuts.
pub fn reserved_app_commands(platform: Platform) -> Vec<Combo> {
    let chord = |code: &str, mods: &[Modifier]| Combo::new(code, mods.iter().copied());
    match platform {
        Platform::MacOs => [
            "KeyA", "KeyC", "KeyV", "KeyX", "KeyZ", "KeyW", "KeyM", "Comma", "KeyQ", "KeyH",
        ]
        .into_iter()
        .map(|code| chord(code, &[Modifier::Meta]))
        .chain([chord("KeyZ", &[Modifier::Shift, Modifier::Meta])])
        .collect(),
        Platform::Windows | Platform::Linux => ["KeyA", "KeyC", "KeyV", "KeyX", "KeyZ", "KeyY", "KeyW"]
            .into_iter()
            .map(|code| chord(code, &[Modifier::Ctrl]))
            .chain([chord("F4", &[Modifier::Alt])])
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HYPER: [Modifier; 3] = [Modifier::Ctrl, Modifier::Alt, Modifier::Meta];

    #[test]
    fn labels_name_the_snappy_key_then_modifiers_in_display_order() {
        let chord = KeyChord::snappy("Digit1", []);
        assert_eq!(chord.long_label(Platform::MacOs), "Snappy + 1");
        assert_eq!(chord.compact_label(Platform::Windows), "Snappy + 1");
        let shifted = KeyChord::snappy("BracketLeft", [Modifier::Shift]);
        assert_eq!(shifted.long_label(Platform::Linux), "Snappy + Shift + [");
        let arrow = KeyChord::new("ArrowLeft", [Modifier::Meta, Modifier::Ctrl, Modifier::Shift]);
        assert_eq!(arrow.long_label(Platform::MacOs), "Control + Shift + Command + ←");
        assert_eq!(arrow.compact_label(Platform::MacOs), "Ctrl + Shift + Cmd + ←");
        assert_eq!(arrow.compact_label(Platform::Windows), "Ctrl + Shift + Win + ←");
        assert_eq!(
            KeyChord::new("KeyQ", [Modifier::Meta])
                .with_label("A")
                .compact_label(Platform::MacOs),
            "Cmd + A"
        );
        assert_eq!(snappy_key_label(Platform::MacOs), "Control + Option + Command");
        assert_eq!(snappy_key_label(Platform::Linux), "Ctrl + Alt + Super");
    }

    #[test]
    fn holding_every_snappy_key_modifier_is_the_snappy_key() {
        let recorded = KeyChord::new(
            "Digit1",
            [Modifier::Shift, Modifier::Ctrl, Modifier::Alt, Modifier::Meta],
        );
        assert!(recorded.mods.has_snappy_key());
        assert_eq!(recorded.mods.others(), Mods::from_iter([Modifier::Shift]));
        assert_eq!(recorded, KeyChord::snappy("Digit1", [Modifier::Shift]));

        let partial = KeyChord::new("Digit1", [Modifier::Ctrl, Modifier::Alt]);
        assert!(!partial.mods.has_snappy_key());
        assert_eq!(partial.mods.with(Modifier::Meta), ChordMods::snappy_with([]));
    }

    #[test]
    fn the_snappy_key_expands_to_its_modifiers_wherever_chords_are_compared() {
        let symbolic = KeyChord::snappy("Digit1", []);
        assert_eq!(symbolic.combo(), Combo::new("Digit1", HYPER));
        assert_eq!(
            KeyChord::snappy("Digit1", [Modifier::Ctrl]).combo(),
            Combo::new("Digit1", HYPER),
            "a modifier the Snappy key already holds adds nothing"
        );
    }

    #[test]
    fn chord_modifiers_store_the_snappy_key_as_a_symbol() {
        let json = serde_json::to_string(&KeyChord::snappy("Digit1", [Modifier::Shift])).unwrap();
        assert_eq!(json, r#"{"code":"Digit1","mods":["snappy","shift"]}"#);
        let concrete: KeyChord =
            serde_json::from_str(r#"{"code":"Digit1","mods":["ctrl","alt","meta"]}"#).unwrap();
        assert_eq!(concrete, KeyChord::snappy("Digit1", []));
        assert_eq!(serde_json::to_string(&concrete.mods).unwrap(), r#"["snappy"]"#);
        let doubled: ChordMods = serde_json::from_str(r#"["ctrl","snappy"]"#).unwrap();
        assert_eq!(doubled, ChordMods::snappy_with([]));
        assert!(serde_json::from_str::<ChordMods>(r#"["hyper"]"#).is_err());
    }

    proptest::proptest! {
        #[test]
        fn normalizing_keeps_the_held_modifiers_and_survives_a_save(bits in 0u8..16) {
            let held: Mods = Modifier::ALL
                .into_iter()
                .filter(|modifier| bits & modifier.bit() != 0)
                .collect();
            let mods = ChordMods::from_concrete(held);
            proptest::prop_assert_eq!(mods.expanded(), held);
            proptest::prop_assert_eq!(mods.has_snappy_key(), held.includes(snappy_key_modifiers()));
            let saved = serde_json::to_string(&mods).unwrap();
            proptest::prop_assert_eq!(serde_json::from_str::<ChordMods>(&saved).unwrap(), mods);
        }
    }

    #[test]
    fn chords_need_a_command_modifier_and_a_known_key() {
        assert!(KeyChord::snappy("KeyZ", []).is_valid());
        assert!(!KeyChord::new("KeyZ", [Modifier::Shift]).is_valid());
        assert!(!KeyChord::new("NotAKey", HYPER).is_valid());
    }

    #[test]
    fn labels_never_affect_identity() {
        let a = KeyChord::new("Digit1", HYPER).with_label("&");
        let b = KeyChord::new("Digit1", HYPER);
        assert_eq!(a.combo(), b.combo());
    }

    #[test]
    fn mods_serialize_in_fixed_order() {
        let json = serde_json::to_string(&Mods::from_iter([Modifier::Meta, Modifier::Shift])).unwrap();
        assert_eq!(json, r#"["shift","meta"]"#);
        let mods: Mods = serde_json::from_str(r#"["alt","ctrl"]"#).unwrap();
        assert!(mods.contains(Modifier::Ctrl) && mods.contains(Modifier::Alt));
    }

    #[test]
    fn reserved_commands_per_platform() {
        let mac = reserved_app_commands(Platform::MacOs);
        assert!(mac.contains(&Combo::new("KeyQ", [Modifier::Meta])));
        assert_eq!(mac.len(), 11);
        assert!(reserved_app_commands(Platform::Windows).contains(&Combo::new("F4", [Modifier::Alt])));
    }
}
