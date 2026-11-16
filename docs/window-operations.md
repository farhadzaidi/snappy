# Moving windows

Every window move goes through one queue (`crates/engine/src/ops.rs`): snaps, swaps, undo and redo, placing the selection, workspace switches, and sends. The queue keeps moves from fighting each other, and settling makes them land where they were meant to.

## The window-operation queue

- **One at a time.** Operations never interleave, so two never fight over a window.
- **The newest wins.** Submitting cancels the operation in flight through its cancel token and waits for it to stop before starting. Settling checks the token before every read and write, so a stale operation stops within a step.
- **A press cancels.** While live control is on, a mouse press on any window cancels the operation in flight, so the user's hand beats Snappy's.
- **Protected windows.** A window pressed while an operation is in flight is marked protected, and the operation's recovery (putting windows back after a failure) leaves it to the user.

## Settling

Apps don't always do what they're told right away. They often acknowledge a position or size before their redraw applies it, insist on a minimum size, or size in character cells.

`core::placement::bounds` writes a frame and waits for it to settle:

1. It resizes first when shrinking on one display, and moves first when growing would cross a screen edge.
2. It re-reads the frame after short waits.
3. It accepts the size an app insists on.
4. It puts the window at its final position.

The engine drives this through the platform. The rule itself is tested against scripted windows and an instant clock.

## Undo

On success, the window's original frame goes onto the undo history. The history keeps 50 moves, with one history per workspace when workspaces are on, or one overall. On failure, the window goes back where it was.

Undo and redo are shortcuts too, with no key until the user records one.
