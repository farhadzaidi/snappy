# The Snappy key and Caps Lock

The Snappy key is one held key that stands for two or more modifiers the user chooses (`core::keys::SnappyKey`). By default it's Control, Option, and Command on macOS, and Ctrl, Alt, and Win or Super elsewhere. Every default shortcut is the Snappy key plus one other key.

## How shortcuts store it

Shortcuts store the Snappy key as a symbol, not as the modifiers it stands for. A chord's `ChordMods` say either "the Snappy key, plus these" or name concrete modifiers, and recording decides which, once.

Only `KeyChord::combo` expands the symbol. So conflicts, reserved chords, availability probes, and registration all compare concrete combos, and labels always say "Snappy".

## Changing it

Changing the Snappy key is checked as a whole before anything is saved:

1. every binding, with the new expansion;
2. each chord that moved, against the OS (`core::shortcuts::availability_probes`).

Then the shortcuts are registered again at once.

## Caps Lock as the Snappy key

Caps Lock can stand for the Snappy key while it's held. The work is split three ways.

### The adapter makes the key

Each adapter does it with what its OS offers (`CapsLock::hold_caps_lock_as`): held, Caps Lock acts as the Snappy key's modifiers. The adapter refuses when something else already remaps Caps Lock. It reports when the key goes down, when something else is used while it's down (never what), and when it comes up.

The platform pages describe how each OS does it.

### Core decides what a press was

`core::keys::caps_lock` calls a press a tap when nothing else was used and the key came up quickly. The engine answers a tap by toggling Caps Lock. Anything else is a hold.

### The engine keeps it in step

`crates/engine/src/caps_lock.rs` keeps Caps Lock in step with the setting, the permission, whether the platform can control the desktop, and the Snappy key.

While a change is in place, the engine keeps what undoes it in `caps-lock.json`. So a crashed run's change is undone at the next start, before anything else.

When the system's keyboard settings change under Snappy, the adapter reports Caps Lock lost. The engine holds it again from the new settings, unless they now remap Caps Lock themselves.
