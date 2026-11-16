//! Chords Snappy never takes: standard app commands, and chords the OS keeps for itself.

use super::{Combo, Modifier, Os};

/// Standard app commands that can never become global shortcuts.
pub fn reserved_app_commands(platform: Os) -> Vec<Combo> {
    let chord = |code: &str, mods: &[Modifier]| Combo::new(code, mods.iter().copied());
    match platform {
        Os::MacOs => [
            "KeyA", "KeyC", "KeyV", "KeyX", "KeyZ", "KeyW", "KeyM", "Comma", "KeyQ", "KeyH",
        ]
        .into_iter()
        .map(|code| chord(code, &[Modifier::Meta]))
        .chain([chord("KeyZ", &[Modifier::Shift, Modifier::Meta])])
        .collect(),
        Os::Windows | Os::Linux => ["KeyA", "KeyC", "KeyV", "KeyX", "KeyZ", "KeyY", "KeyW"]
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
pub fn chords_the_system_keeps(platform: Os) -> Vec<KeptChord> {
    match platform {
        Os::MacOs => vec![KeptChord {
            combo: Combo::new("Backspace", [Modifier::Ctrl, Modifier::Alt, Modifier::Meta]),
            message: "macOS reserves Control ⌃ + Option ⌥ + Command ⌘ + Delete; it never reaches apps.",
        }],
        Os::Windows | Os::Linux => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::test_support::DEFAULT_MODIFIERS;

    #[test]
    fn reserved_commands_per_platform() {
        let mac = reserved_app_commands(Os::MacOs);
        assert!(mac.contains(&Combo::new("KeyQ", [Modifier::Meta])));
        assert_eq!(mac.len(), 11);
        assert!(reserved_app_commands(Os::Windows).contains(&Combo::new("F4", [Modifier::Alt])));
    }

    #[test]
    fn macos_keeps_control_option_command_delete_and_other_platforms_keep_nothing() {
        let kept: Vec<Combo> = chords_the_system_keeps(Os::MacOs)
            .into_iter()
            .map(|kept| kept.combo)
            .collect();
        assert_eq!(kept, [Combo::new("Backspace", DEFAULT_MODIFIERS)]);
        assert!(chords_the_system_keeps(Os::Windows).is_empty());
        assert!(chords_the_system_keeps(Os::Linux).is_empty());
    }
}
