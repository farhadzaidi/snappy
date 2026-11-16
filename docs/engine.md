# The engine

`crates/engine` owns Snappy's runtime state and decides the order things happen in. It runs on a thread of its own ([The daemon and the app](daemon-and-app.md#threads)), takes in platform events and the app's commands, and drives the adapter through the traits in `crates/platform`.

## Its parts

- **`engine.rs`** is what the daemon sees: start, handle an input, shut down, or stop for a restart.
- **`events.rs`** routes each platform event to the features that care about it.
- **`state.rs`** holds `EngineState`, all the runtime state in one place. Its behaviors are split by concern under `state/`: what's on screen, live control, shortcut registration, the overlay, the tray menu, logging, the status message, and what the app is sent.
- **`ops.rs`** holds the window-operation queue and the undo histories. See [Moving windows](window-operations.md).
- **One module per feature**: `drag`, `actions` (shortcuts), `focus`, `selection`, `marks` (window outlines), `caps_lock`, `commands` (from the app), and `workspaces/`.
- **`store/`** reads and writes every file. See [Files and diagnostics](files-and-diagnostics.md).

## State

`EngineState` (`crates/engine/src/state.rs`) holds the settings, the displays, whether live control runs, the drag in progress, the registered shortcuts, the workspaces, the selection, the debug log, and the outbox to the app. Because only the engine thread touches it, it needs no locks.

## Live control

Live control is whether Snappy acts on windows right now. It's on when:

- snapping is on,
- the platform's permission is granted, and
- the platform says it can. On GNOME and KDE, that means its extension or script answers.

Press monitoring, and every shortcut but the workspace ones, follow it.

## Settings changes

Every command from the app lands in `crates/engine/src/commands.rs`. A settings change:

1. copies the settings and applies the edit. The edits themselves are rules in `core::settings`, which the app's fake daemon runs too;
2. validates the whole, normalizing layouts and checking every shortcut conflict;
3. saves atomically;
4. reconfigures what depends on settings: the debug log, workspaces, live control, shortcuts, overlays, and the tray.

A rejected change leaves the old settings in place and replies with the reason.

## Read next

- [Moving windows](window-operations.md): the queue every move goes through.
- [Drags and shortcuts](drags-and-shortcuts.md) and [Workspaces](workspaces.md): how the features use it.
- [Engine scenarios](engine-scenarios.md): how the engine is tested end to end.
