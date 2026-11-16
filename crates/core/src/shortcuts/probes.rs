//! Asking the OS whether the chords a Snappy key change moves are free.

use std::collections::BTreeSet;

use super::{NamedShortcut, Refusal, bound};
use crate::keys::{Combo, KeyChord, Os};
use crate::settings::Settings;

/// A chord to ask the OS about before the Snappy key changes, and what to say when the OS
/// or another app already holds it, naming the shortcut that would have it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub combo: Combo,
    pub refusal: Refusal,
}

/// What to say when the OS or another app already holds `chord` for `shortcut`, naming that
/// shortcut so the app can point to it. The Snappy key's probes and a recorder's
/// say the same.
pub fn unavailable_refusal(chord: &KeyChord, shortcut: NamedShortcut, platform: Os) -> Refusal {
    Refusal {
        message: format!(
            "{} for {} is already used by {} or another app.",
            chord.long_label(platform),
            shortcut.title,
            platform.system_name()
        ),
        shortcut: Some(shortcut),
    }
}

/// One probe for each distinct chord that holds the Snappy key in `settings`, which have
/// the new key. Disabled layouts and workspaces count, as they do for conflicts, so turning
/// them on later can't fail on a chord the change took.
pub fn availability_probes(settings: &Settings, platform: Os) -> Vec<Probe> {
    let key = settings.snappy_key;
    let mut seen = BTreeSet::new();
    bound(settings)
        .into_iter()
        .filter(|bound| bound.chord.mods.has_snappy_key())
        .filter_map(|bound| {
            let combo = bound.chord.combo(key);
            if !seen.insert(combo.clone()) {
                return None;
            }
            let refusal = unavailable_refusal(&bound.chord, NamedShortcut::from(&bound), platform);
            Some(Probe { combo, refusal })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifier;
    use crate::settings::{NavigationShortcut, WorkspaceShortcut};
    use crate::shortcuts::test_support::*;
    use crate::shortcuts::{ShortcutId, bound_combos};

    #[test]
    fn availability_probes_ask_once_for_each_snappy_key_chord_and_say_what_it_would_be() {
        let mut settings = Settings::defaults(MAC)
            .with_snappy_key(modifiers(&CONTROL_OPTION), MAC)
            .unwrap();
        settings.history.undo = Some(KeyChord::new("KeyU", [Modifier::Ctrl]));
        settings.history.redo = Some(snappy("KeyR"));
        let probes = availability_probes(&settings, MAC);
        assert!(
            probes
                .iter()
                .all(|probe| probe.combo.mods == modifiers(&CONTROL_OPTION))
        );
        assert_eq!(
            probes.len(),
            bound_combos(&settings).len() - 1,
            "all but the concrete undo"
        );
        let space = probes
            .iter()
            .find(|probe| probe.combo.code.as_str() == "Space")
            .unwrap();
        assert_eq!(
            space.refusal,
            Refusal {
                message: "Snappy + Space for Move to the nearest empty zone is already used by macOS or another app."
                    .into(),
                shortcut: named(NavigationShortcut::NearestEmptyZone.into(), "Move to the nearest empty zone"),
            }
        );
        let redo = probes
            .iter()
            .find(|probe| probe.combo.code.as_str() == "KeyR")
            .unwrap();
        assert_eq!(
            redo.refusal.message,
            "Snappy + R for Redo window move is already used by macOS or another app."
        );
    }

    #[test]
    fn availability_probes_name_each_workspace_by_its_id_when_two_share_a_name() {
        let mut settings = Settings::defaults(MAC);
        for item in &mut settings.workspaces.items {
            item.name = "Work".to_owned();
        }
        let probes = availability_probes(&settings, MAC);
        for (code, workspace) in [("Digit1", "workspace-1"), ("Digit2", "workspace-2")] {
            let probe = probes
                .iter()
                .find(|probe| probe.combo.code.as_str() == code)
                .unwrap();
            assert_eq!(
                probe.refusal.shortcut,
                named(
                    ShortcutId::workspace(workspace, WorkspaceShortcut::Switch),
                    "Switch to Work"
                ),
                "{code}"
            );
        }
    }
}
