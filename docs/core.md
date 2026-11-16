# The rules

`crates/core` holds every rule Snappy follows. The daemon runs it natively, and the app runs it as WebAssembly, so both always agree.

## What keeps it pure

Core has no OS dependencies and does no I/O, threads, or clocks. Where a rule needs an effect, core defines a small trait, and the engine or a test implements it:

- `BoundsBackend` and `Clock` (`placement/bounds.rs`) let settling read and write a window's frame and wait.
- `WindowOps` (`placement/operations.rs`) lets snaps, swaps, and undo act on windows.
- `WorkspaceIo` (`workspace/session.rs`) lets workspace sessions move windows and save their journal.

This is what lets every rule be tested with no OS and no real time, against scripted windows and an instant clock. See [Testing](testing.md#core).

## Where each rule lives

All paths are under `crates/core/src`.

| Rule | Module |
|---|---|
| Zones as fractions of the work area, gaps and padding, the zone under the pointer | `layout.rs` |
| Telling a move from a click or a resize | `placement/drag.rs` |
| Writing a frame and waiting for it to settle | `placement/bounds.rs` |
| Snap, swap, and undo, and their history | `placement/operations.rs` |
| When a window fills a zone, swap planning, "When the zone is taken", the swap key | `placement/swap.rs` |
| Moving between zones with the arrows | `placement/navigate.rs` |
| Moving focus in a direction | `placement/focus.rs` |
| The nearest empty zone, and placing the selection | `placement/empty_zone.rs`, `placement/assignment.rs` |
| Which windows may be managed | `eligibility.rs` |
| Chords, the Snappy key, reserved chords, Caps Lock presses | `keys/` |
| Shortcut IDs and titles, conflicts, what to register, probes, help | `shortcuts/` |
| The settings model, defaults, reading old files, validation, edits, resets | `settings/` |
| Workspace sessions, parking, focus after a switch, relaunch | `workspace/` |
| The layout editor | `editor/` |

## Eligibility

Which windows Snappy manages is decided once, in `eligibility.rs`, on facts each adapter reads from its OS. So the refusals read the same everywhere: "Choose a standard application window.", "Leave full screen before tiling this window."

## Used by the app

`crates/wasm` wraps core in a JSON API for the app, and `apps/app/ui/src/core/snappyCore.ts` loads it. A rule the app needs goes in core and is exported through both. See [The app](app.md#the-webassembly-core).
