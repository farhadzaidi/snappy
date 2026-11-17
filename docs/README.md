# Snappy's docs

These pages explain how Snappy's code works and why it's built that way. For setting up, building, and sending a change, start with [CONTRIBUTING.md](../CONTRIBUTING.md). The code conventions and safety rules are in [AGENTS.md](../AGENTS.md).

## If you want to…

- **Get the big picture**, or find where a change goes: [How Snappy is built](overview.md).
- **Change how Snappy starts, stops, or talks to its window**, or work on the app: [The daemon and the app](daemon-and-app.md).
- **Change a rule**, or understand how moves are queued and settled: [The rules and the engine](engine.md).
- **Follow a drag, a shortcut, a workspace switch, or the Snappy key** through the code: [How the features work](features.md).
- **Find out why something didn't work**, or change what Snappy saves: [Files and diagnostics](files-and-diagnostics.md).
- **Write a test**: [Testing](testing.md).
- **Run the tests that need a real desktop**: [Testing on a real desktop](sandboxes.md).
- **Change how one desktop does something**: start with [Adapters](platforms/adapters.md), then the page for that desktop.
- **Cut a release**, or publish to a store: [Releasing](releasing/README.md).

## The desktops

- [Adapters](platforms/adapters.md): what every adapter does, and how the desktops compare.
- [macOS](platforms/macos.md)
- [Windows](platforms/windows.md)
- [Linux](platforms/linux.md): how Snappy picks one of its three Linux adapters, and what they share.
  - [X11](platforms/x11.md)
  - [GNOME on Wayland](platforms/gnome.md)
  - [KDE Plasma on Wayland](platforms/kde.md)

## Releasing

- [Releasing](releasing/README.md): setting the version, the release workflow, the packages, and signing.
- [Package managers](releasing/package-managers.md): Homebrew and winget.
- [The Microsoft Store](releasing/microsoft-store.md)
- [The Mac App Store](releasing/mac-app-store.md)
