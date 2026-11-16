# GNOME on Wayland

`crates/gnome`, with the GNOME Shell extension in `crates/gnome/extension`, supports GNOME 46 to 49. What it shares with the other Linux adapters is in [Linux](linux.md).

Wayland lets no ordinary program move other apps' windows, watch global input, or own global shortcuts. On GNOME, only GNOME Shell can. So the extension does those things when the daemon asks, reports what happens, and makes no decisions of its own. Calls that differ between GNOME versions go through `compat.js`.

## Installing the extension

`src/extension.rs`. The extension's files are built into the daemon, which writes any that are missing or differ into the user's extensions folder at every start.

GNOME Shell on Wayland loads a new extension only at the next login. So a first install turns itself on in GNOME's settings, and the app's window asks the user to log out and back in once, with a **Log out now** button. After that, turning it on or off is the user's choice.

## The protocol

`src/protocol.rs`, `src/bus.rs`, `service.js`. The extension exports an interface on GNOME Shell's own bus name, with:

- one method, `Call`, which takes a JSON request and returns `{"ok": …}` or `{"error": …}`;
- one signal, `Event`, which carries every event as JSON in the order it happened;
- a `Version` property holding the protocol version. A daemon and an extension from different builds notice the mismatch, and the app's window says to log out and back in.

The daemon's requests run on a bus thread of their own, since the engine thread drives an async runtime in which `zbus` can't block. Further threads forward the extension's events, its state changes, GNOME Shell starting over, and Mutter's display changes.

## Live control

`src/extension.rs`, `src/messages.rs`. Live control is available while the extension answers in this protocol version. Otherwise, GNOME's extension state says why, worded for the user and naming the desktop.

When GNOME Shell starts over, a fresh extension holds none of Snappy's shortcuts, monitoring, outlines, menu, or Caps Lock hold. So the adapter sends them all again and compares the window list with the one it had. Workspaces are suspended while the extension is away and resume when it's back.

## When the daemon goes away

`service.js`. The extension watches the bus name of the daemon that set things up. When it vanishes, as in a crash, the extension releases the shortcuts and Caps Lock, stops monitoring, and removes the overlays and the menu, so no dead shortcut swallows keys. Parked windows stay on the parking workspace for the next daemon to find.

## Finding and moving windows

`windows.js`, `crates/linux/src/windows.rs`.

- **Managed windows** are Mutter's non-override-redirect windows, top first. Wayland and Xwayland windows are treated alike, and titles are never read.
- **IDs.** A window's ID is Mutter's stable sequence number, which also forms its recovery key.
- **Eligibility** is decided in Rust, on the facts the extension reports.
- **Writes** are `move_frame` and `move_resize_frame`, after taking the window out of maximized or tiled state.
- **Moves** are reported only for windows the engine asked to follow.
- **Focusing** is `Meta.Window.activate`, which also raises the window.

## Displays

`src/displays.rs`. Names, scale, and layout mode come from Mutter's `DisplayConfig`; frames and work areas from the extension. In Mutter's logical layout, coordinates are points, so every display's scale is 1. In the physical layout, they're device pixels at the monitor's scale.

Display IDs hash the monitor's vendor, product, and serial number. So the same monitor has a different ID under X11, which hashes the whole EDID.

## Input

`input.js`. Clicks on Wayland windows go straight to their apps. So, as on Windows, a press is reported when a pointer-driven move or resize starts, naming the window, and the release when it ends.

While a drag runs and drag monitoring is on, the pointer and modifiers are sampled on a short timer. Nothing is sampled otherwise, and no keystroke is reported. The release's modifiers come from the release event itself, since asking afterwards can already see a modifier let go right behind the button.

## Keeping the window under the pointer

GNOME pulls a moving window onto nearby screen and window edges while Shift is held, and has no setting for it. So the engine tells the adapter the drag key, and while it's held during a move, the extension puts the window back under the pointer on every position change, before the frame is drawn.

The window follows the pointer, GNOME's edge tiling stays out of the way, and Snappy's zones are the only guide. Moves without the drag key are GNOME's as usual.

## Shortcuts

`src/keys.rs`, `shortcuts.js`. Shortcuts use GNOME Shell's `grab_accelerator` with physical keycodes, so they stay on the same keys on every layout. They ignore key repeat and are allowed only in the normal session, not the overview or the lock screen.

A grab is refused when GNOME or another program binds the chord, which doubles as the availability probe.

## Overlay

`overlay.js`, `crates/linux/src/scene.rs`. The daemon sends finished shapes, placed and colored by the style every overlay shares. The extension draws them in a non-reactive container per display, above everything in GNOME Shell's UI, so it takes no clicks or focus. Only changed scenes cross the bus.

Window outlines go in a second container per display. Both are hidden on the lock screen.

## Menu

`src/menu.rs`, `menu.js`. A top-bar item stands in for the tray icon. It's built from the same structure as the tray menu, with the tray glyph as a symbolic icon that GNOME recolors to match the bar. Each choice carries the menu's generation, so a choice from a menu that was replaced while open does nothing.

## Caps Lock as the Snappy key

`src/caps_lock.rs`, `capsLock.js`. Snappy writes its XKB option to `~/.config/xkb`. Held, Caps Lock sets the Snappy key's modifiers, doesn't repeat, and types no key, so GNOME's Super overview never sees one. Shift + Caps Lock stays a real Caps Lock.

- **Applying it.** The extension adds the option by wrapping GNOME's keyboard options, rather than changing the user's settings, and rebuilds the keymap.
- **Presses.** It grabs Caps Lock alone to learn of a press. Only while Caps Lock is down does it grab the other keys with the Snappy key's modifiers and sample the pointer, to notice that something else was used, never what.
- **A tap** presses Caps Lock on a virtual keyboard, flipping the state and every keyboard's light.
- **Undoing.** The option comes out when Caps Lock is restored, when the extension is turned off or the daemon goes away, and while the screen is locked, so Caps Lock works as usual at the password prompt.
- **Refusing.** Snappy refuses when GNOME's options already remap Caps Lock, or the layout itself makes Caps Lock another key, as Colemak does. When the user later adds such an option, theirs wins and Caps Lock is reported lost.

## Workspaces

`parking.js`. Windows park on a GNOME workspace of Snappy's own, frames untouched: just before the empty last workspace GNOME keeps with dynamic workspaces, or last with a fixed number.

Parked windows are left out of Alt+Tab and the overview. Workspaces belong to the GNOME workspace in use when they started, and pause on any other. Turning the extension off returns every parked window first.

## Limitations and conflicting settings

- **Other tiling extensions.** Ubuntu's Enhanced Tiling, and the Tiling Assistant it comes from, also snap windows dragged to screen edges and act on Ctrl during drags. While one runs, the app says where to turn it off.
- **Workspaces on the primary display only.** With this on (Settings → Multitasking), GNOME shows windows on other displays on every workspace. Snappy can't hide them, and refuses switches that would need to.
- **Keys GNOME keeps.** GNOME binds Super with Ctrl and a digit, and Ctrl+Alt with an F-key; the default Snappy key avoids both. A Snappy key of fewer modifiers can't: GNOME switches workspaces with Ctrl+Alt and the arrows and with Super+Alt and Left or Right, and moves windows between displays with Super+Shift and the arrows.
- **Cell grids.** Wayland has no way for an app to ask to be sized in steps, so only Xwayland apps, such as some terminals, snap to a cell grid.
- **Activating a parked window** from the dock or a notification makes GNOME switch to the parking workspace. Workspaces pause there until you switch back.
- **Apps that block desktop shortcuts**, such as virtual machines and remote desktops, get Caps Lock's modifiers but no tap.
