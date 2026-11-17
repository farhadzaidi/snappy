# macOS

This page is for anyone working on Snappy's macOS adapter, `crates/macos`. It explains how Snappy finds and moves windows on macOS, watches drags, registers shortcuts, draws zones, and handles Caps Lock, Spaces, and native tabs, and what's different in the Mac App Store build.

The adapter uses public APIs only. AppKit and Core Graphics come through the `objc2` crates. Accessibility, Carbon hotkeys, and the HID event system, which have no such crates, are declared by hand in `src/ffi/`.

## How Snappy finds and moves windows

On macOS, other apps' windows are reached through Accessibility, the API screen readers use. That's why Snappy needs the Accessibility permission. The code is in `src/windows/`.

**The window under the pointer** comes from asking Accessibility which element is at that point, then walking up to its window. One catch: when one of Snappy's own windows is at that point, such as the menu bar item or an overlay, Accessibility would answer inside Snappy itself, on the asking thread, and AppKit crashes when used off the main thread. So in that case Snappy asks only the app underneath.

**Frames** are read as position and size together. Writes set one at a time, after checking that access is still granted, the app is still running, and the window isn't full screen.

**Which windows are on screen** comes from matching each app's windows against the window server's list. That list gives each window's owner, layer, and bounds, never its name.

**Hung apps** can't hold Snappy up for long: every Accessibility call has a short timeout (`src/thresholds.rs`).

**Focusing** a window activates its app, raises the window, and makes it the app's focused window.

## How Snappy notices changes

Snappy asks each app to tell it when a window is created and when focus changes. For each window it tracks, it also asks to hear when the window closes, moves, resizes, or comes back from the Dock (`src/observers/`).

The system tells Snappy when apps launch, activate, and quit, when the Space changes, and when the Mac sleeps.

Setting up each of these is a round trip to the app, which can be slow, so it never happens on the main thread.

## How Snappy watches a drag

Snappy watches the left mouse button with AppKit's event monitors, while it's live (`src/input.rs`). Only during a drag does it also watch the pointer moving and the modifier keys. Each event carries the modifiers held at that moment, so a release reports exactly what was held when the button came up.

Event monitors need only the Accessibility permission. A keyboard event tap would add an Input Monitoring prompt, so Snappy doesn't use one for drags.

## Shortcuts

Shortcuts are Carbon hotkeys, registered by physical key, so they stay on the same keys whatever the keyboard layout (`src/hotkeys.rs`). To check whether a chord is free, Snappy registers it under a separate signature and releases it at once.

Carbon happily accepts some chords macOS uses before any app sees them, such as Mission Control's Control-arrows. So Snappy also reads the system's own shortcuts, and treats the ones that are turned on as taken (`src/system_shortcuts.rs`).

One chord gets past both checks. The window server swallows Control-Option-Command-Delete from a real keyboard. Core lists it among the chords the OS keeps and never offers it, which is why clearing the selection uses Snappy + `\` instead.

## Drawing the zones

Each display gets a panel of Snappy's own that never takes focus or clicks, floats above other windows, and shows on every Space and over full-screen apps (`src/overlay/`). It's drawn with Core Animation layers, updated only where something changed, and filled with the system accent color.

Panels are made before the first drag and kept, so the first drag doesn't wait for them. Window outlines get a second panel of the same kind per display, made the first time something needs an outline and hidden while nothing does.

## Caps Lock as the Snappy key

Making Caps Lock act as the Snappy key takes four pieces on macOS (`src/caps_lock/`).

1. **A key remapping.** Snappy adds one entry to the system's key remapping, the same one the `hidutil` command edits, turning Caps Lock into F18 on every keyboard. The user's own entries are left alone. Caps Lock then never locks, lights up, or repeats by itself.
2. **A hotkey on F18**, which tells Snappy when Caps Lock goes down and comes up.
3. **An event tap** that, while Caps Lock is held, adds the Snappy key's modifiers to every other key and click. So Caps Lock + J is Snappy + J, for shortcuts and drags alike. The tap notes only that something else was used, never what.
4. **A toggle.** On a tap, Snappy flips Caps Lock and its light through the HID system.

Snappy refuses to set this up when the user's own key remapping, or the Modifier Keys setting in System Settings, already changes Caps Lock.

The remapping comes out when the setting is turned off, when Snappy quits or stops for a restart, and as the daemon exits. macOS also forgets it at reboot.

## Native tabs

On macOS, each tab of a window is a separate Accessibility window. Accessibility lists only the tab that's showing in each group, and keeps answering for hidden tabs with the frame they last had. Nothing links a hidden tab to its group except its title, which Snappy never reads.

So Snappy learns which tabs belong together as the user switches between them (`src/windows/tabs.rs`, `src/windows/tab_matching.rs`). A tracked window that drops out of the list without being minimized or closed may be a hidden tab. Snappy matches it to a group:

- where it was last seen, or
- because it has exactly the showing tab's frame, or
- because only one group has room for it.

A group never gets more tabs than its tab bar holds. The engine then moves each group as one window.

## Workspaces

**Hiding windows.** macOS pulls windows that are fully off screen back on. So Snappy hides workspace windows at an exposed bottom corner of the screen, with a sliver still showing.

**Display changes.** When displays change, as when a Mac wakes up with an external monitor, macOS moves windows itself, hidden ones included. Snappy hides them again once the displays settle.

**Recognizing windows after a restart.** A window's recovery key is its process ID, when that process started, and a hash of its Accessibility element, which stays the same for as long as the window exists.

**Spaces.** Workspaces pause when the user switches to another Space. To notice, Snappy keeps an invisible 1×1 window on each display and asks whether it's on the active Space (`src/desktop_guard.rs`).

## Permissions

Snappy needs Accessibility (`src/permissions.rs`). While snapping is on and the permission is missing, Snappy checks for it once a second, and only then. When the permission is taken away, Snappy notices without polling: from the system's accessibility notification, and from Accessibility calls failing.

## Starting, quitting, and starting at login

- **One daemon.** A lock on `daemon.lock` in the state folder keeps a second daemon from running (`src/instance.rs`). Opening Snappy.app again reaches the running daemon instead, which opens Snappy's window.
- **Quitting.** `SIGTERM`, `SIGINT`, and `SIGHUP` quit as the menu's **Quit** does, so hidden workspace windows come back first. `SIGUSR2` stops for a restart, leaving them hidden.
- **Quit Apple Events**, which logging out and scripts send, and Homebrew's `quit` when it uninstalls, quit the same way. The delegate answers `applicationShouldTerminate:` with "later", the engine quits, and `request_termination` then lets AppKit end the process, tearing down first (`src/delegate.rs`, `src/app.rs`).
- **No Dock icon.** Neither the daemon nor the app shows a Dock icon or appears in Command-Tab.
- **Open at login** registers the launch agent inside the app (`packaging/macos`), which runs the daemon with `--background` (`src/login_item.rs`).

## The Mac App Store build

Apps on the Mac App Store run in Apple's App Sandbox. The `mac-app-store` feature builds the adapter for it, and `tools/macos/app-store.sh` builds the app. [The Mac App Store](../releasing/mac-app-store.md) goes through what the sandbox allows, the signing, and the release. In the code, five things differ.

**Caps Lock can't be the Snappy key.** The sandbox doesn't let apps remap keys. So `src/caps_lock/unavailable.rs` stands in for the Caps Lock module, and none of the remapping, event tap, or F18 code is compiled. The adapter says up front that Caps Lock is unavailable, with a sentence for the user, and Snappy's window shows the setting disabled with that sentence underneath ([Builds where it can't work](../features.md#builds-where-it-cant-work)).

**Open at login** registers the app itself, rather than the launch agent, which the store build leaves out. macOS then opens Snappy at login without `--background`. So when the app is opened, the daemon checks whether the launch was a login, and opens its window only if it wasn't (`src/login_launch.rs`).

**The app** is signed to run inside the daemon's sandbox, so the pipes between them work unchanged.

**Files** live in the app's sandbox container, where a sandboxed program's home folder points.

**Import and export.** The file picker grants the chosen file only to the program that showed it, which is the app ([Why the app opens import and export files](../daemon-and-app.md#why-the-app-opens-import-and-export-files)). Showing the picker needs the `com.apple.security.files.user-selected.read-write` entitlement. That entitlement goes on the daemon, in `AppStore.entitlements`, not on the app: a helper signed to run inside another program's sandbox may hold only `app-sandbox` and `com.apple.security.inherit`, or it won't start, and it gets the daemon's entitlements. So `AppStoreApp.entitlements` stays as it is.

## Limitations

- **Chrome** blocks moves while its enhanced accessibility is on. Snappy turns it off for each move and back on afterwards, unless VoiceOver or Switch Control needs it.
- **The live tests' limits.** Key presses the live suite sends join the event stream after the point where Caps Lock is remapped, and after the point where the window server swallows Control-Option-Command-Delete. So the live suite can't show what a real keyboard does with either ([Testing on a real desktop](../sandboxes.md#the-live-suite)).
