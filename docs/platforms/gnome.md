# GNOME on Wayland

This page is for anyone working on Snappy's GNOME adapter, `crates/gnome`, and its GNOME Shell extension in `crates/gnome/extension`. It explains why GNOME needs an extension, how the daemon and the extension talk, and how each feature works on GNOME. It supports GNOME 46 to 49. What it shares with the other Linux adapters is in [Linux](linux.md).

## Why an extension

Wayland doesn't let an ordinary program move other apps' windows, watch input across the desktop, or own global shortcuts. On GNOME, only GNOME Shell can. So Snappy ships an extension that runs inside GNOME Shell and does those things when the daemon asks, and reports what happens.

The extension makes no decisions of its own. Calls that differ between GNOME versions go through `compat.js`.

## Installing the extension

The extension's files are built into the daemon. At every start, the daemon writes any that are missing or different into the user's extensions folder (`src/extension.rs`).

GNOME Shell on Wayland only loads a new extension at the next login. So the first time, Snappy turns the extension on in GNOME's settings, and Snappy's window asks the user to log out and back in once, with a **Log out now** button. After that, turning the extension on or off is up to the user.

## How the daemon and the extension talk

They talk over D-Bus (`src/protocol.rs`, `src/bus.rs`, and `service.js` in the extension). The extension offers an interface under GNOME Shell's own bus name, with three parts:

- **A method, `Call`**, which takes a request as JSON and returns `{"ok": …}` or `{"error": …}`.
- **A signal, `Event`**, which carries every event as JSON, in the order it happened.
- **A property, `Version`**, holding the protocol version. A daemon and an extension from different builds notice the mismatch, and Snappy's window asks the user to log out and back in.

The daemon's requests run on a bus thread of their own, because the engine thread runs an async runtime, where the D-Bus library, `zbus`, can't block. More threads pass on the extension's events, its state changing, GNOME Shell restarting, and display changes.

## When Snappy is live

Snappy is live on GNOME while the extension answers in the same protocol version (`src/extension.rs`). When it doesn't, GNOME's record of the extension's state says why, such as turned off, or failed to load, and Snappy's window tells the user, naming the desktop (`src/messages.rs`).

GNOME Shell can restart, and a fresh extension knows none of what the daemon set up. So the adapter sends it all again: shortcuts, input watching, outlines, the menu, and Caps Lock. Then it compares the window list with the one it had. Workspaces are suspended while the extension is away, and resume when it's back.

## When the daemon goes away

The extension watches the daemon's bus name (`service.js`). If the daemon disappears, as in a crash, the extension releases the shortcuts and Caps Lock, stops watching input, and removes the overlays and the menu. So no shortcut is left swallowing keys for a daemon that isn't there.

Hidden workspace windows stay where they are, for the next daemon to find.

## How Snappy finds and moves windows

The extension does the work in `windows.js`. Which windows Snappy manages is decided in Rust, from the facts the extension reports (`crates/linux/src/windows.rs`).

- **Which windows.** The extension lists the windows GNOME's window manager, Mutter, manages, top first, leaving out its own pop-ups and menus. Wayland and Xwayland windows are treated alike, and titles are never read.
- **IDs.** A window's ID is the stable sequence number Mutter gives it, which is also part of its recovery key.
- **Moves** take the window out of maximized or tiled state first.
- **Following moves.** The extension reports moves only for the windows the engine has asked it to follow.
- **Focusing** activates the window, which also raises it.

## Displays

Display names, scale, and layout mode come from Mutter's display settings, its `DisplayConfig` interface on D-Bus. Frames and work areas come from the extension (`src/displays.rs`).

GNOME has two layout modes. In the logical one, coordinates are points, so every display's scale is 1. In the physical one, coordinates are device pixels, at each monitor's scale.

A display's ID is a hash of the monitor's vendor, product, and serial number. That's different from X11, which hashes the monitor's whole EDID, so the same monitor has a different ID under each.

## How Snappy watches a drag

On Wayland, clicks go straight to their apps, so Snappy can't see them (`input.js`). Instead, as on Windows, the extension reports a press when GNOME starts moving or resizing a window with the pointer, naming the window, and a release when that ends.

While a drag is being watched, the extension samples the pointer and the modifier keys on a short timer. Nothing is sampled at any other time, and no keystroke is ever reported. The release's modifiers come from the release event itself, because asking afterwards could already see a modifier that was let go right after the button.

### Keeping the window under the pointer

GNOME pulls a window being dragged toward nearby screen and window edges while Shift is held, and has no setting to turn that off. Shift is also Snappy's default drag key.

So the engine tells the adapter the drag key. While it's held during a move, the extension puts the window back under the pointer every time it moves, before the frame is drawn. The window follows the pointer, GNOME's edge tiling stays out of the way, and Snappy's zones are the only guide. Moves without the drag key are GNOME's as usual.

## Shortcuts

Shortcuts are grabbed through GNOME Shell's `grab_accelerator`, by physical key, so they stay on the same keys whatever the layout (`src/keys.rs`, `shortcuts.js`). They ignore key repeat, and work only in the normal desktop, not in the overview or on the lock screen.

GNOME refuses a grab when it or another program already uses the chord, which is also how Snappy checks whether a chord is free.

## Drawing the zones

The daemon works out the finished shapes, placed and colored as on every desktop, and sends them to the extension (`crates/linux/src/scene.rs`). The extension draws them in a layer per display, above everything in GNOME Shell, that takes no clicks or focus (`overlay.js`). Only scenes that changed are sent.

Window outlines go in a second layer per display. Both are hidden on the lock screen.

## The menu

GNOME doesn't show tray icons, so a menu in the top bar stands in for one (`src/menu.rs`, `menu.js`). It's built from the same structure as the tray menu elsewhere. Its icon is the tray glyph, which GNOME recolors to match the bar.

Each choice carries the menu's version number, so a choice made in a menu that was replaced while it was open does nothing.

## Caps Lock as the Snappy key

Snappy writes an XKB option of its own to `~/.config/xkb` (`src/caps_lock.rs`, `capsLock.js`). With it, held Caps Lock sets the Snappy key's modifiers, doesn't repeat, and types no key, so GNOME's Super overview never sees one. Shift + Caps Lock is still a real Caps Lock.

- **Applying it.** The extension adds the option on top of the user's keyboard options, without changing their settings, and rebuilds the keymap.
- **Presses.** The extension grabs Caps Lock on its own to hear when it's pressed. Only while Caps Lock is down does it also grab other keys with the Snappy key's modifiers, and sample the pointer, to notice that something else was used, never what.
- **A tap** presses Caps Lock on a virtual keyboard, which flips the lock and every keyboard's light.
- **Undoing.** The option comes out when the setting is turned off, when the extension is turned off, when the daemon goes away, and while the screen is locked, so Caps Lock works as usual at the password prompt.
- **Refusing.** Snappy refuses when the user's keyboard options already remap Caps Lock, or the layout itself makes Caps Lock another key, as Colemak does. If the user adds such an option later, theirs wins, and Caps Lock is reported lost.

## Workspaces

Snappy hides workspace windows on a GNOME workspace of its own, frames untouched (`parking.js`). With dynamic workspaces, it's just before the empty last one GNOME always keeps. With a fixed number of workspaces, it's the last one.

Hidden windows are left out of Alt+Tab and the overview. Snappy's workspaces pause when the user switches to another GNOME workspace. Turning the extension off brings every hidden window back first.

## Limitations and settings that get in the way

- **Other tiling extensions.** Ubuntu's Enhanced Tiling, and the Tiling Assistant it comes from, also snap windows dragged to screen edges and act on Ctrl during drags. While one is running, Snappy's window says where to turn it off.
- **Workspaces on the primary display only.** With this turned on (Settings → Multitasking), GNOME shows windows on other displays on every workspace. Snappy can't hide them, so it refuses switches that would need to.
- **Keys GNOME keeps.** GNOME uses Super with Ctrl and a digit, and Ctrl+Alt with an F-key. The default Snappy key avoids both. A Snappy key with fewer modifiers may not: GNOME switches workspaces with Ctrl+Alt and the arrows and with Super+Alt and Left or Right, and moves windows between displays with Super+Shift and the arrows.
- **Cell grids.** Wayland apps can't ask to be sized in steps, so only Xwayland apps, such as some terminals, snap to a cell grid.
- **Activating a hidden window** from the dock or a notification makes GNOME switch to Snappy's hiding workspace. Snappy's workspaces pause there until you switch back.
- **Apps that capture desktop shortcuts**, such as virtual machines and remote desktops, get Caps Lock's modifiers but no tap.
