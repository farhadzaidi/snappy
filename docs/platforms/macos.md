# macOS

This page is for anyone working on Snappy's macOS adapter, `crates/macos`. It explains how Snappy finds and moves windows on macOS, watches drags, registers shortcuts, draws zones, and handles Caps Lock, Spaces, and native tabs, and what's different in the Mac App Store build.

The adapter uses public APIs only. AppKit and Core Graphics come through the `objc2` crates. Accessibility, Carbon hotkeys, and the HID event system, which have no such crates, are declared by hand in `src/ffi/`.

## How Snappy finds and moves windows

On macOS, other apps' windows are reached through Accessibility, the API screen readers use. That's why Snappy needs the Accessibility permission. The code is in `src/windows/`.

**The window under the pointer** comes from asking Accessibility which element is at that point, then walking up to its window. One catch: when one of Snappy's own windows is at that point, such as the menu bar item or an overlay, Accessibility would answer inside Snappy itself, on the asking thread, and AppKit crashes when used off the main thread. So in that case Snappy asks only the app underneath.

**Frames** are read as position and size together. Writes set one at a time, after checking that access is still granted, the app is still running, and the window isn't full screen.

**Which windows are on screen** comes from matching each app's windows against the window server's list, by owner and bounds. Only apps the list shows a window of are asked, and each window's frame is asked first, since every question is a round trip to the app and the frame alone rules out the windows on other Spaces. That list gives each window's owner, number, layer, and bounds, never its name. It runs front to back, so the visible windows come in its order. An app's windows with the same frame can't be told apart in it: they count only when the list shows that many of them, as when they're stacked in one zone, and take its places for that frame in the order Accessibility lists them, the app's own front to back. So another app's window stacked between them stays between them.

**Hung apps** can't hold Snappy up for long: every Accessibility call has a short timeout (`src/thresholds.rs`).

**Focusing** a window raises it and makes it its app's main and focused window, and only then activates the app. Activating brings the app's main window forward, so activating first would bring whichever window was main forward too, and sending a window to the back would raise one more window than it means to.

## How Snappy notices changes

Snappy asks each app to tell it when a window is created and when focus changes. For each window it tracks, it also asks to hear when the window closes, moves, resizes, or comes back from the Dock (`src/observers/`).

The system tells Snappy when apps launch, activate, and quit, when the Space changes, and when the Mac sleeps.

Setting up each of these is a round trip to the app, which can be slow, so it never happens on the main thread.

## How Snappy watches a drag

Snappy watches the left mouse button with AppKit's event monitors, while it's live (`src/input.rs`). Only during a drag does it also watch the pointer moving and the modifier keys. Each event carries the modifiers held at that moment, so a release reports exactly what was held when the button came up.

Event monitors need only the Accessibility permission. A keyboard event tap would add an Input Monitoring prompt, so Snappy doesn't use one for drags.

## Shortcuts

Shortcuts are Carbon hotkeys, registered by physical key, so they stay on the same keys whatever the keyboard layout (`src/hotkeys.rs`). To check whether a chord is free, Snappy registers it under a separate signature and releases it at once.

Carbon reports each hotkey's release as well as its press, when its key comes up, even after the Snappy key or Caps Lock did. Snappy passes on the release only for a shortcut whose release matters, as send to back's does, once for each press. Carbon never repeats a held hotkey, so a press while one is down means its release went unseen, and Snappy reports the press alone, as a new one.

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

On macOS, each tab of a window is a separate Accessibility window. Accessibility lists only the tab that's showing in each group, and only that tab has the tab bar, which says how many tabs the group holds. Hidden tabs keep answering with the frame they had when they last showed. Nothing links a hidden tab to its group except its title, which Snappy never reads.

A window on another Space drops out of the list too, and the window server reports it off screen, just as it reports a hidden tab. No public API tells the two apart. Taking a window for a hidden tab when it isn't one is worse than missing a tab: the window can't be focused with the focus shortcuts, and it moves with another window. So Snappy puts a window in a group only on evidence it saw itself, learned as the user opens and switches tabs (`src/windows/tabs.rs`, `src/windows/tab_matching.rs`):

- **It was in the group before**, and the group still shows a tab with room for it. This keeps tabs together after their group moves and their frames go stale.
- **Snappy saw it go.** It was listed earlier while the same Spaces were showing, then dropped out of the list, not minimized and without a tab bar of its own, right where exactly one group now shows. Right where means the group's frame, or the same left edge and width with the top and bottom off by no more than a tab bar's height: a group's first new tab adds the tab bar, and a new tab can push the group up to keep it on screen.

Every change of Space or display starts a new visit (`src/space_visits.rs`), counted before the engine hears of the change. A window Snappy last saw listed on an earlier visit may be on another Space, so its place never puts it in a group, however exactly it fits. A window that still has a tab bar is showing in a group of its own somewhere else, so it's never a hidden tab.

A group never gets more tabs than its tab bar holds. When more windows fit a group than it has room for, none of them joins, since they can't be told apart. A window shown on its own leaves its group, and a group whose tab bar goes away lets all its tabs go.

The engine then moves each group as one window. It logs each window that starts or stops counting as a hidden tab as `tabsChanged`, with the evidence and the group's tab count. Focus shortcuts don't ask about tab groups: a window Accessibility lists as showing can always be focused.

## Sending a window to the back

Accessibility can raise another app's window but has no way to lower one, and macOS offers no way to hold the screen still or to order another app's window directly. So Snappy sends a window to the back by raising windows over it, never the window sent back itself. Core picks which windows to raise (`crates/core/src/send_to_back/raised_over.rs`), and on a busy desktop that's one to three raises.

macOS places a raise by its app, and core plans the steps around where each lands (`crates/core/src/send_to_back/raises.rs`): the window that takes focus is focused first, so the menu bar changes once, and the others are raised under it. The adapter only raises one window at a time and reads the window server's list (`src/windows/restacking.rs`).

Where core says, the engine awaits the order so far in the window server's list, read about every 8 milliseconds without holding up the engine (`crates/engine/src/send_to_back/raising.rs`). Windows are matched in the list by their number there, which `visible_windows` reads off the row each window takes (`src/windows/stack_order.rs`), so two windows of one app with one frame, as in a zone, are told apart; a window not yet matched to a row is matched by owner and the frame the engine knows it by.

- **The window focused** has to show in front of the window sent back within a quarter of a second, or the press stops and says so, where its focusing is awaited before anything else is raised, as it is for a window of another app. A window of the same app as the window sent back goes out with its app's raises, unawaited, and once those have gone out the press sends every raise left whatever shows, so none ends out of order.
- **Each wait** lasts a quarter of a second at most. A window that can't be raised, as one that just closed or a hung app's can't, is left out, with every other window of its app, as each would hold the engine up for the Accessibility timeout again. Focusing a window likewise asks a hung app once, not three times.
- **The whole press,** reading the windows included, waits 800 milliseconds at most. After that, or once another operation takes the queue, the raises left go out at once without being awaited, and a window focused that hasn't shown yet no longer stops it: stopping partway would leave the windows raised so far over windows that were in front of them, so the order ends right, though the raises may show in turn. A press whose last order never showed is logged as `reached: false`.

How it looks depends on the stack:

- **One window behind,** or windows stacked in one zone with the same frame: only the window focused is raised, so nothing shows but the result.
- **Windows that only partly overlap:** the raises of different apps show in turn, over a frame or two each. Each shows only what the press ends with, except where a window raised lands over one still to be raised that's to end in front of it.
- **A raised window of the focused window's own app** lands in front of it, as that app is active, so the focused window is raised again straight after it. The two usually show in one frame; when they don't, the raised window shows for a frame.

## Workspaces

**Hiding windows.** macOS pulls windows that are fully off screen back on. So Snappy hides workspace windows at an exposed bottom corner of the screen, with a sliver still showing.

**Display changes.** When displays change, as when a Mac wakes up with an external monitor, macOS moves windows itself, hidden ones included. Snappy hides them again once the displays settle.

**Recognizing windows after a restart.** A window's recovery key is its process ID, when that process started, and a hash of its Accessibility element, which stays the same for as long as the window exists.

**Spaces.** Workspaces pause when the user switches to another Space. To notice, Snappy keeps an invisible 1×1 window on each display and asks whether it's on the active Space (`src/desktop_guard.rs`).

## Permissions

Snappy needs Accessibility (`src/permissions.rs`). While snapping is on and the permission is missing, Snappy checks for it once a second, and only then. When the permission is taken away, Snappy notices without polling: from the system's accessibility notification, and from Accessibility calls failing.

## Starting, quitting, and starting at login

- **One daemon.** A lock on `daemon.lock` in the state folder keeps a second daemon from running (`src/instance.rs`). The file holds the daemon's process ID, which `snappy --uninstall` sends `SIGTERM`. Opening Snappy.app again reaches the running daemon instead, which opens Snappy's window.
- **Quitting.** `SIGTERM`, `SIGINT`, and `SIGHUP` quit as the menu's **Quit** does, so hidden workspace windows come back first. `SIGUSR2` stops for a restart, leaving them hidden.
- **Quit Apple Events**, which logging out and scripts send, and Homebrew's `quit` when it uninstalls, quit the same way. The delegate answers `applicationShouldTerminate:` with "later", the engine quits, and `request_termination` then lets AppKit end the process, tearing down first (`src/delegate.rs`, `src/app.rs`).
- **Uninstalling.** `snappy --uninstall`, run from inside the app, unregisters the login item, which only a program in the app can do (`src/uninstall.rs`). Quitting has already removed Caps Lock's key remapping.
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
