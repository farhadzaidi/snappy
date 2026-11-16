# Engine scenarios

`crates/engine/tests/scenarios/` drives a real `Engine` over the fake platform, end to end. Platform events and app commands go in; window frames, overlays, outlines, registered shortcuts, files, debug log records, and status messages come out.

This is the layer for anything about order and state: what a refused move does to a switch, how a restart rejoins windows, what a burst of display changes settles to, which shortcuts are registered after a Snappy key change.

## How they're organized

It's one test program with a module per feature: dragging, swaps, history, shortcuts, the nearest empty zone, layouts, focus, the selection, workspaces, containment, sending, restarts, desktop parking, tabs, the Snappy key, Caps Lock, live control, and settings. `support.rs` holds what they share.

- **No real time.** Each scenario runs on a single-threaded runtime with paused time, so settle waits, the focus flash, and display settling take no real time.
- **Helpers act like the user.** They start an engine in a known state, such as snapping live on the halves layout, or workspaces on with every default shortcut. Then they drag, release, press a shortcut, focus a window, switch, and send.
- **Scratch folders.** Each test gets a scratch folder from its name, which is why names must be unique across the modules. Reopening the same folder simulates a restart or a crash.

## The fake platform

`crates/fake` is an in-memory desktop implementing every platform trait.

Its windows can behave like real apps. They can have a minimum size, resize only while fully on one display, ignore every write, be hidden by their app, be full screen, show a sheet, be minimized, sit on the parking desktop, or be hidden as a macOS-style tab behind another.

Tests can:

- add, remove, and rearrange displays;
- set focus and the pointer;
- grant or withhold permission;
- have another app hold a chord;
- refuse Caps Lock;
- advance the clock.

The fake records every overlay call, outline, window write, shortcut registration, and tray menu. So a scenario can check what Snappy showed as well as what it did.

A capability every adapter must provide is implemented by `crates/fake` too ([Code map](code-map.md#where-a-change-goes)).
