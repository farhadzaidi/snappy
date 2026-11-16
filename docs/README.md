# Snappy's docs

These pages explain how Snappy's code works: what each part does, how the parts talk, and why it's built that way. Each one takes a few minutes to read.

For setting up, building, running, and testing, see [CONTRIBUTING.md](../CONTRIBUTING.md). For code conventions and the safety rules, see [AGENTS.md](../AGENTS.md).

## Start here

- [Overview](overview.md): the two programs, the layers, and the ground rules that shape the code.
- [Code map](code-map.md): every crate, how they depend on each other, and where a change goes.

## How it fits together

- [The daemon and the app](daemon-and-app.md): starting, threads, one daemon per user, quitting, and the macOS bundle.
- [Talking to the app](ipc.md): the pipe between the daemon and the app, and the messages on it.
- [The app](app.md): the Tauri shell, the React UI, the WebAssembly core, and the fake daemon.

## Inside the daemon

- [The rules](core.md): what `crates/core` holds and how it stays free of the OS.
- [The engine](engine.md): runtime state, live control, and how commands and settings changes are handled.
- [Moving windows](window-operations.md): the window-operation queue, settling, and undo.
- [Drags and shortcuts](drags-and-shortcuts.md): the path a drag and a shortcut take, plus focus, the selection, and outlines.
- [Workspaces](workspaces.md): switching, parking, containment, display changes, and restarts.
- [The Snappy key and Caps Lock](snappy-key.md): how the Snappy key is stored and expanded, and how Caps Lock stands in for it.
- [Files and diagnostics](files-and-diagnostics.md): where Snappy keeps its files, how it writes them, and what it logs.

## Platforms

- [Adapters](platforms/adapters.md): what every adapter does, how they're laid out, and how they compare.
- [macOS](platforms/macos.md)
- [Windows](platforms/windows.md)
- [Linux](platforms/linux.md): what the Linux adapters share, and how Snappy picks one.
- [X11](platforms/x11.md)
- [GNOME on Wayland](platforms/gnome.md)
- [KDE Plasma on Wayland](platforms/kde.md)

## Testing

- [Testing](testing.md): the layers of tests, where a test goes, and how tests are named.
- [Engine scenarios](engine-scenarios.md): end-to-end engine tests over the fake platform.
- [Adapter tests](adapter-tests.md): unit tests, the fake GNOME Shell, and integration tests against real desktops.
- [The live suite](live-suite.md): the real daemon, real windows, and real input.
- [Sandboxes](sandboxes.md): the containers, sealed-off KWin, and VMs the desktop tests run in.
