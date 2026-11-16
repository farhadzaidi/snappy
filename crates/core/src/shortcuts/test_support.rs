//! Settings and chords the shortcut tests share.

use super::{NamedShortcut, ShortcutId};
use crate::keys::{KeyChord, Modifier, Mods, Os};
use crate::placement::navigate::Direction;
use crate::settings::{NavigationShortcut, Settings};

pub const MAC: Os = Os::MacOs;

pub fn snappy(code: &str) -> KeyChord {
    KeyChord::snappy(code, [])
}

pub fn left() -> ShortcutId {
    NavigationShortcut::Move(Direction::Left).into()
}

pub fn right() -> ShortcutId {
    NavigationShortcut::Move(Direction::Right).into()
}

pub fn named(id: ShortcutId, title: &str) -> Option<NamedShortcut> {
    Some(NamedShortcut {
        id,
        title: title.to_owned(),
    })
}

pub fn with_selection(settings: &mut Settings, layout_id: &str, chord: KeyChord) {
    let layout = settings.layout(layout_id).unwrap().clone();
    let mut options = settings.options(&layout);
    options.select_shortcut = Some(chord);
    settings.layouts.options.insert(layout_id.into(), options);
}

pub fn with_zone(settings: &mut Settings, zone_id: &str, chord: KeyChord) {
    let layout_id = zone_id.rsplit_once('-').map_or(zone_id, |(layout, _)| layout);
    let layout = settings.layout(layout_id).unwrap().clone();
    let mut options = settings.options(&layout);
    options.zone_shortcuts.insert(zone_id.into(), chord);
    settings.layouts.options.insert(layout_id.into(), options);
}

pub fn modifiers(held: &[Modifier]) -> Mods {
    held.iter().copied().collect()
}

pub const CONTROL_OPTION: [Modifier; 2] = [Modifier::Ctrl, Modifier::Alt];
