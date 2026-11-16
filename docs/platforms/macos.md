# macOS

`crates/macos` uses public APIs only: AppKit and Core Graphics through `objc2`, and hand-written FFI for Accessibility, Carbon hotkeys, and the HID event system (`src/ffi/`).

## Finding and moving windows

`src/windows/`. Windows are Accessibility elements.

- **The window under the pointer** comes from asking Accessibility for the element at that point and walking up to its window. Where Snappy has a window of its own at the point, such as its menu bar item or an overlay, it asks only the app beneath instead. Accessibility answers questions about Snappy's own windows inside the daemon, on the asking thread, and AppKit used off the main thread crashes.
- **Frames** are read as position and size together. Writes set one at a time, after checking that access is still granted, the app still runs, and the window isn't full screen.
- **Visible windows** are each app's windows matched against the window server's on-screen list, which gives owner, layer, and bounds only, never names.
- **Timeouts.** Every Accessibility call has a short timeout (`src/thresholds.rs`), so a hung app costs a fraction of a second.
- **Focusing** a window activates its app, raises the window, and makes it the app's focused window.

## Observing

`src/observers/`. Per app, Snappy watches for windows being created and focus changing. Per tracked window, it watches for the window being destroyed, moved, resized, and restored. NSWorkspace reports app activation, launch, and termination, Space changes, and sleep.

Registering an observer is a round trip to the app, so it never happens on the main thread.

## Input

`src/input.rs`. NSEvent monitors watch the left button's presses and releases while live control runs, and, only during a drag, drags and modifier changes. Modifiers come from each event's own flags, so a release carries the modifiers held when the button came up.

NSEvent monitors need only Accessibility. A listen-only keyboard tap would add an Input Monitoring prompt, so none is used.

## Shortcuts

`src/hotkeys.rs`, `src/system_shortcuts.rs`. Shortcuts are Carbon `RegisterEventHotKey` with physical key codes, so they stay on the same keys on every layout. The availability probe registers a chord under a separate signature and releases it at once.

Carbon accepts chords macOS consumes before any app, such as Mission Control's Control-arrows. So Snappy also reads the system's symbolic hotkeys and treats the enabled ones as taken.

One chord gets past both: the window server swallows Control-Option-Command-Delete from a real keyboard. Core lists it among the chords the OS keeps and never offers it, which is why clearing the selection uses Snappy + `\`.

## Overlay

`src/overlay/`. Each display gets a borderless, non-activating, click-through `NSPanel` at the floating level, joining every Space and full-screen apps. It's drawn with Core Animation layers that are updated only where they changed. Panels are made ahead of the first drag and kept. Zones are filled with the system accent color.

Window outlines get a second panel of the same kind per display, made the first time there's something to outline and hidden while there's nothing.

## Caps Lock as the Snappy key

`src/caps_lock/`.

- **The mapping.** Snappy adds one pair to the HID event system's user key mapping, the one `hidutil` writes, turning Caps Lock into F18 on every keyboard and leaving the user's own pairs alone. Caps Lock then never locks, lights, or repeats by itself.
- **Down and up.** A Carbon hotkey on F18 reports when it goes down and up.
- **While held.** An active event tap adds the Snappy key's modifier flags to every other key and click. So Caps Lock + J is Snappy + J, for Snappy's shortcuts and drags alike. The tap notes only that something else was used.
- **A tap** toggles Caps Lock and its light through the HID system.
- **Refusing.** Snappy refuses when the user's key mapping or System Settings' Modifier Keys already remap Caps Lock.
- **Undoing.** The mapping comes out when the setting goes off, at quit, when stopping for a restart, and as the daemon exits. macOS forgets it at reboot.

## Native tabs

`src/windows/tabs.rs`, `src/windows/tab_matching.rs`. macOS makes each tab of a window an Accessibility window of its own, lists only the selected tab of each group, and keeps answering for hidden tabs with the frame they last showed at. Nothing ties a hidden tab to its group but its title, which Snappy never reads.

So Snappy learns tabs as they're focused. A tracked window that has dropped out of the list, without being minimized or closed, is a possible hidden tab. It's matched to a group by where it was last seen, by having exactly the shown tab's frame, or by filling the only group with room, never more than the group's tab bar holds. The engine then moves each group as one window.

## Workspaces

- **Parking.** Windows are parked at an exposed lower corner of the screen with a sliver left showing, because macOS pulls windows that are fully off screen back.
- **Display changes.** When displays change, as when a Mac wakes with an external display, macOS moves windows itself, parked ones included. The engine parks them again once the displays settle.
- **Recovery key.** A window's recovery key is its process ID, the process's start time, and the hash of its Accessibility element, which stays the same while the window exists.
- **Spaces.** The desktop guard (`src/desktop_guard.rs`) keeps an invisible 1×1 window on each display and asks whether it's on the active Space, to tell when the user changed Spaces.

## Permissions

`src/permissions.rs`. Snappy needs Accessibility. While snapping is on and access is missing, Snappy checks once a second for a grant, and only then. A revocation is noticed without polling, from the system's accessibility notification and from Accessibility calls failing.

## Single instance, quitting, and login

`src/instance.rs`, `src/login_item.rs`.

- A lock on `daemon.lock` in the state folder keeps one daemon running. A second launch exits, and opening Snappy.app again reaches the running daemon as a reopen, which opens the app's window.
- `SIGTERM`, `SIGINT`, and `SIGHUP` quit like the menu's Quit, so parked windows return first. `SIGUSR2` stops for a restart.
- Both programs use the accessory activation policy: no Dock icon and no Command-Tab entry.
- The login item registers the launch agent bundled in the app (`packaging/macos`), which runs the daemon with `--background`.

## Limitations

- Chrome blocks frame changes while its enhanced accessibility is on. Snappy turns it off for one change, unless VoiceOver or Switch Control needs it, and turns it back on.
- Posted key events join the event stream past the HID remap and past where the window server drops Control-Option-Command-Delete. So the live suite can't show what a real keyboard does with either ([The live suite](../live-suite.md#what-posted-input-cant-show)).
