//! Every shortcut by what it is, who owns which chord, and the shortcuts to register.
//! Chords are compared by their combos, with the Snappy key expanded to the modifiers the
//! settings choose, so Snappy + 1 and the same key with the Snappy key's modifiers held are
//! one shortcut.

mod bound;
mod conflicts;
pub mod guide;
mod hotkeys;
mod id;
mod probes;
mod sections;
#[cfg(test)]
mod test_support;

pub use bound::{
    Bound, NamedShortcut, bound, bound_combos, bound_global, bound_select, bound_workspace, bound_zones,
    give_defaults,
};
pub use conflicts::{
    Refusal, conflict_for, conflict_with_layout, first_conflict, recording_conflict, snappy_key_conflict,
};
pub use hotkeys::{HotKey, HotKeyAction, desired_hotkeys};
pub use id::ShortcutId;
pub(crate) use id::for_each_chord;
pub use probes::{Probe, availability_probes, unavailable_refusal};
pub use sections::ShortcutSection;
