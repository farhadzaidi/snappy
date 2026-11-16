//! Physical keys by their W3C codes, and the labels they get on a US layout.

use serde::{Deserialize, Serialize};

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
