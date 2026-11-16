# Overview

Snappy arranges other apps' windows into zones. It's a Rust workspace with a Tauri and React app, and an adapter for each desktop it supports: macOS, Windows, X11, GNOME on Wayland, and KDE Plasma on Wayland.

## Two programs

- **The daemon**, `snappy` (`apps/snappy`), runs all the time. It watches input, moves windows, draws the zone overlays, owns the menu bar or tray item, and is the only program that writes Snappy's files.
- **The app**, `snappy-app` (`apps/app`), is the window people open to pick layouts, edit them, manage workspaces, and change settings. The daemon starts it on demand, and it quits when its window closes. It never touches other apps' windows: it sends commands and shows the state the daemon sends back.

```
                 ┌──────────────────────── snappy (daemon) ───────────────────────────┐
  OS events      │  main thread                     engine thread                     │
 ──────────────► │  adapter: hooks, monitors,  ───► engine ──► window-operation queue ─┼──► other apps'
                 │  hotkeys, overlays, tray   ◄───  (core's rules)        │            │    windows
                 │                                        └──► settings, state, logs   │
                 └───────────────────────────────▲────────────────────────────────────┘
                                                 │ piped stdin/stdout, length-prefixed JSON
                 ┌───────────────────────────────┴──── snappy-app ────────────────────┐
                 │  Tauri shell ◄──► React UI ──► snappy-core as WebAssembly           │
                 └────────────────────────────────────────────────────────────────────┘
```

[The daemon and the app](daemon-and-app.md) covers how they start and stop, and [Talking to the app](ipc.md) the messages between them.

## Four layers

Each layer has one job, and the crate boundaries keep it that way.

- **Core** (`crates/core`) holds every rule: geometry, snapping, swaps, navigation, settings and their validation, shortcut conflicts, workspace bookkeeping. It has no OS dependencies and does no I/O. See [The rules](core.md).
- **The engine** (`crates/engine`) owns all runtime state and decides when things happen. Every window move goes through its queue, and it's the only writer of Snappy's files. See [The engine](engine.md).
- **The adapters** (`crates/macos`, `crates/windows`, `crates/x11`, `crates/gnome`, `crates/kwin`) translate between the engine and the OS. They report events and carry out requests, and make no decisions. See [Adapters](platforms/adapters.md).
- **The app** shows what the daemon sends and asks core, compiled to WebAssembly, for anything that's a rule. See [The app](app.md).

## Ground rules

A few rules shape the whole codebase. [AGENTS.md](../AGENTS.md) lists them all.

- **Privacy.** No network access of any kind. Snappy never reads, logs, or stores window titles, URLs, window contents, or keystrokes. A test (`apps/snappy/tests/privacy.rs`) fails the build if the daemon links a network client.
- **Quiet when idle.** Nothing polls or wakes up while Snappy is idle. Keyboard events are watched only during a drag or a window operation, and, with Caps Lock as the Snappy key, Caps Lock itself.
- **Responsive.** No blocking OS call on a UI thread, so a hung app can't freeze input or drawing.
- **No test code in what ships.** Tests use fakes, and read-only helpers for tests live in each adapter's `reference` module, built only with its `reference` feature.

## Read next

- [Code map](code-map.md) for every crate and where a change goes.
- [Drags and shortcuts](drags-and-shortcuts.md) and [Workspaces](workspaces.md) for how the main features flow through the layers.
