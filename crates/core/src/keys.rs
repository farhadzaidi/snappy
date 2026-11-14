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

impl Platform {
    /// The system's name, as messages show it.
    pub fn system_name(self) -> &'static str {
        match self {
            Platform::MacOs => "macOS",
            Platform::Windows => "Windows",
            Platform::Linux => "Linux",
        }
    }
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

    const fn bit(self) -> u8 {
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

    /// What the Snappy key is on `platform`, for example "Control + Option + Command".
    pub fn label(self, platform: Platform) -> String {
        self.0
            .in_display_order()
            .map(|modifier| modifier.name(platform))
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
    snappy: bool,
    others: Mods,
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

    /// Modifier names in display order, the Snappy key first.
    fn names(self, name: impl Fn(Modifier) -> &'static str) -> impl Iterator<Item = &'static str> {
        self.snappy
            .then_some(SNAPPY_KEY_NAME)
            .into_iter()
            .chain(self.others.in_display_order().map(name))
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

    /// Every Snappy key holds a command modifier: it has at least two modifiers, and only
    /// one of them can be Shift.
    pub fn is_valid(&self) -> bool {
        self.code.is_known()
            && (self.mods.has_snappy_key() || self.mods.others().has_command_modifier())
            && self
                .label
                .as_ref()
                .is_none_or(|label| !label.is_empty() && label.chars().count() <= MAX_LABEL_LENGTH)
    }

    /// The key's own label, without modifiers.
    pub fn key_label(&self) -> String {
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

/// A chord the OS consumes before any app sees it, though registering it succeeds and
/// nothing reports it as taken, so only this list can refuse it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptChord {
    pub combo: Combo,
    /// Why it can't be a shortcut, as conflicts show it.
    pub message: &'static str,
}

/// The chords the OS keeps for itself on `platform`. macOS's window server swallows
/// Control-Option-Command-Delete: the keyboard sends it, but no app or hotkey receives it.
pub fn chords_the_system_keeps(platform: Platform) -> Vec<KeptChord> {
    match platform {
        Platform::MacOs => vec![KeptChord {
            combo: Combo::new("Backspace", [Modifier::Ctrl, Modifier::Alt, Modifier::Meta]),
            message: "macOS reserves Control + Option + Command + Delete; it never reaches apps.",
        }],
        Platform::Windows | Platform::Linux => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HYPER: [Modifier; 3] = [Modifier::Ctrl, Modifier::Alt, Modifier::Meta];

    const DEFAULT: SnappyKey = SnappyKey::DEFAULT;

    fn key(modifiers: &[Modifier]) -> SnappyKey {
        SnappyKey::new(modifiers.iter().copied().collect()).unwrap()
    }

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
        assert_eq!(DEFAULT.label(Platform::MacOs), "Control + Option + Command");
        assert_eq!(DEFAULT.label(Platform::Windows), "Ctrl + Alt + Win");
        assert_eq!(DEFAULT.label(Platform::Linux), "Ctrl + Alt + Super");
    }

    #[test]
    fn a_chosen_snappy_key_is_labelled_by_its_own_modifiers_and_chords_still_say_snappy() {
        let chosen = key(&[Modifier::Shift, Modifier::Alt]);
        assert_eq!(chosen.label(Platform::MacOs), "Option + Shift");
        assert_eq!(chosen.label(Platform::Windows), "Alt + Shift");
        let chord = KeyChord::new("KeyJ", [Modifier::Alt, Modifier::Shift]).recorded(chosen);
        assert_eq!(chord.long_label(Platform::MacOs), "Snappy + J");
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
        assert_eq!(snappy_shift.long_label(Platform::MacOs), "Snappy + Shift + J");
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
        assert_eq!(symbolic.combo(DEFAULT), Combo::new("Digit1", HYPER));
        assert_eq!(
            KeyChord::snappy("Digit1", [Modifier::Ctrl]).combo(DEFAULT),
            Combo::new("Digit1", HYPER),
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
        assert_eq!(a.combo(DEFAULT), b.combo(DEFAULT));
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

    #[test]
    fn macos_keeps_control_option_command_delete_and_other_platforms_keep_nothing() {
        let kept: Vec<Combo> = chords_the_system_keeps(Platform::MacOs)
            .into_iter()
            .map(|kept| kept.combo)
            .collect();
        assert_eq!(kept, [Combo::new("Backspace", HYPER)]);
        assert!(chords_the_system_keeps(Platform::Windows).is_empty());
        assert!(chords_the_system_keeps(Platform::Linux).is_empty());
    }
}
