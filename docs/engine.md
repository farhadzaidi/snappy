# The rules and the engine

This page is for anyone changing what Snappy does, rather than how one OS does it. It explains how the rules in `crates/core` stay testable, where each rule lives, what the engine in `crates/engine` keeps track of, and how every window move is queued and checked.

## Why the rules live apart

Every decision Snappy makes is a rule in core, and core never does anything itself. It doesn't touch the OS, read or write files, start threads, or read a clock. Hand it a layout and a pointer position, and it tells you the zone. Hand it settings, and it tells you whether they're valid.

Some rules have to act along the way. Settling a window, for example, means writing a frame, waiting, and reading it back, possibly several times. For those, core defines a small trait describing just the effects it needs, and whoever runs the rule supplies them:

- `BoundsBackend` and `Clock`, in `placement/bounds.rs`, read and write one window's frame and wait.
- `WindowOps`, in `placement/operations.rs`, lets snaps, swaps, and undo act on windows.
- `WorkspaceIo`, in `workspace/session.rs`, lets workspace changes move windows and save their records.

In the daemon, the engine implements them on top of the adapter. In tests, scripted windows and an instant clock do. That's what lets every rule be tested in milliseconds, with no desktop.

The same code also runs in the app as WebAssembly, so the daemon and Snappy's window always agree.

## Where to find a rule

All paths are under `crates/core/src`.

| Rule | Where |
|---|---|
| Zones as fractions of the screen, gaps and padding, the zone under the pointer | `layout.rs` |
| Telling a move from a click or a resize | `placement/drag.rs` |
| Writing a frame and waiting for it to settle | `placement/bounds.rs` |
| Snap, swap, undo, and their history | `placement/operations.rs` |
| What happens when a zone is taken, and swap planning | `placement/swap.rs` |
| Moving between zones with the arrow shortcuts | `placement/navigate.rs` |
| Moving focus in a direction | `placement/focus.rs` |
| Sending a window to the back: what it goes behind, what takes focus, and how it's put back | `send_to_back/` |
| The nearest empty zone, and placing several windows at once | `placement/empty_zone.rs`, `placement/assignment.rs` |
| Which windows Snappy will manage | `eligibility.rs` |
| Key chords, the Snappy key, chords the OS keeps, Caps Lock presses | `keys/` |
| Shortcut names, clashes, which shortcuts to register | `shortcuts/` |
| The settings, their defaults, older settings files, validation, edits, resets | `settings/` |
| Workspaces: switching, hiding windows, focus after a switch, recognizing windows after a restart | `workspace/` |
| The layout editor | `editor/` |

## Which windows Snappy will manage

Snappy leaves dialogs, full-screen windows, and other windows it shouldn't move alone. The decision is made once, in `eligibility.rs`, from facts each adapter reads from its OS: whether it's an ordinary app window rather than a dialog or a panel, whether it's full screen, and whether the OS lets it be moved and resized.

Because the decision is in one place, so are the messages. "Choose a standard application window." and "Leave full screen before tiling this window." read the same on every platform.

## What the engine keeps track of

The engine is where Snappy's live state is: the settings, the displays, whether Snappy may act right now, the drag in progress, the registered shortcuts, the workspaces, the selected windows, the debug log, and what to send the app next. It all lives in one struct, `EngineState` in `crates/engine/src/state.rs`, with its behaviors split by topic under `state/`.

Only the engine thread touches it, so it needs no locks.

The rest of the engine is one module per feature, such as `drag.rs`, `actions.rs` for shortcuts, `focus.rs`, `selection.rs`, `send_to_back/` for sending the focused window to the back, `caps_lock.rs`, `commands.rs` for what the app asks, and `workspaces/`. `events.rs` routes each event from the adapter to the features that care about it. `store/` reads and writes every file ([Files and diagnostics](files-and-diagnostics.md)).

## When Snappy is "live"

Snappy only acts on windows while it's **live**, which the code calls live control. It's live when all three are true:

- snapping is turned on in settings;
- the OS has given Snappy the permission it needs, which only macOS asks for;
- the adapter says it can work. On GNOME and KDE, that means Snappy's extension or script inside the desktop is answering.

While Snappy isn't live, it watches no clicks and registers no shortcuts except the workspace ones. Snappy's window explains why it isn't live and what to do.

## Every move waits in one queue

Snaps, swaps, undo and redo, placing selected windows, switching workspaces, sending windows to another workspace, and sending a window to the back and putting it back all move windows or change their stacking. All of them go through one queue, in `crates/engine/src/ops.rs`, so they never fight over a window.

The queue follows four rules:

- **One at a time.** Operations never overlap.
- **The newest wins.** A new operation cancels the one in progress and waits for it to stop. The operation checks for cancellation before every read and write, so it stops within a step. Presses of send to back are the exception among themselves: a press while one is under way waits its turn, so quick presses each count, and so does putting back a window held back.
- **The user's hand wins.** A mouse press on any window cancels the operation in progress.
- **Hands off what the user grabbed.** A window pressed during an operation is marked as the user's. If the operation fails and puts windows back, it leaves that one alone.

## Why moves "settle"

Apps don't always do what they're told right away. Many report the new size before they've redrawn at it. Some insist on a minimum size. Terminals size themselves in whole character cells.

So a move doesn't just write a frame. The rule in `placement/bounds.rs` writes it and waits for it to settle:

1. It picks an order that keeps the window on its display: resizing first when the window is shrinking, moving first when growing would push it past a screen edge.
2. It reads the frame back after short waits.
3. If the app insists on a different size, it accepts that size.
4. It puts the window at its final position.

## Undo

When a move succeeds, the window's frame from before it goes onto the undo history, which keeps the last 50 moves. With workspaces on, each workspace has a history of its own. When a move fails, the window goes back where it was.

Undo and redo are shortcuts, with no key until the user records one.

## What happens when a setting changes

Every command from the app arrives in `crates/engine/src/commands.rs`. A settings change goes through four steps:

1. Copy the settings and apply the change to the copy. The change itself is a rule in core's `settings` module, so the app's fake daemon applies it the same way.
2. Check the result as a whole: tidy up layouts and look for shortcut clashes.
3. Save it, atomically.
4. Update everything that depends on settings: the debug log, workspaces, whether Snappy is live, shortcuts, overlays, and the tray menu.

If the check fails, the old settings stay, and the reply says why.
