# KDE Plasma on Wayland

`crates/kwin`, with the KWin script in `crates/kwin/script`, supports Plasma 6.8. What it shares with the other Linux adapters is in [Linux](linux.md).

On Wayland, only KWin can move other apps' windows or see them move. So the script does that when the daemon asks, reports what happens, and makes no decisions.

Unlike GNOME, the overlay, shortcuts, and modifier keys don't go through the script. The daemon draws the overlay as a Wayland client, registers shortcuts with KDE's shortcut service, and reads modifiers from KWin directly.

## Loading the script

`src/script.rs`. The script's files are built into the daemon. At every start, the daemon writes them to a folder in `$XDG_RUNTIME_DIR` named for their hash, since KWin caches compiled QML by path. It loads them over KWin's scripting D-Bus interface, replacing any copy a crashed daemon left loaded.

Nothing is installed, and there's no need to log out. Quitting unloads the script, and when KWin starts over the daemon loads it again.

## The protocol

`src/protocol.rs`, `src/bus.rs`, `service.js`. KWin scripts can call D-Bus methods but can't offer any. So the daemon serves the interface, and answers only KWin's own connection.

- **Events.** The script reports events by calling `Event`.
- **Requests.** The daemon rings a doorbell: a shortcut action the script registers in KWin's shortcut component without a key, which the daemon triggers over D-Bus. The script then calls `Take` to collect the queued requests, and each `Answer` it sends returns any requests queued meanwhile, so a burst needs one ring. Nothing polls while nobody asks.
- **Timeouts.** A request the script doesn't answer within two seconds is reported as KWin not answering.
- **Live control** is available once a script of this protocol version reports it's ready.

Requests block on a bus thread of their own. Further threads serve the script's calls and forward KDE's shortcut presses, KWin starting over, screen locking, and keyboard settings changes.

## Finding and moving windows

`windows.js`, `crates/linux/src/windows.rs`.

- **Managed windows** are KWin's windows for apps, top first, leaving out its own and Plasma's surfaces: panels, the desktop, notifications, popups, and on-screen displays. Wayland and Xwayland windows are treated alike, and titles are never read.
- **IDs.** A window's ID is part of KWin's UUID for it, which outlives both the script and the daemon and forms its recovery key.
- **Eligibility** is decided in Rust, as on GNOME.
- **Writes** set the window's frame after taking it out of a maximized state or one of KWin's tiles.
- **Focusing** sets KWin's active window, which also raises it.

## Displays

`src/displays.rs`. KWin's coordinates are logical pixels, so every display's scale is 1. Displays come in KWin's screen order, with the area KWin maximizes windows into as the work area, so zones end where maximized windows do.

KWin tells scripts nothing when a panel's reserved space changes. So the script compares work areas itself after a panel appears, goes, moves, or resizes, and at the start of every drag.

## Input

`service.js`, `src/input/`. As on GNOME, a press is reported when KWin starts an interactive move or resize, and the release when it finishes. In between, pointer moves are reported at most every 8 ms, and only while drag monitoring is on.

Scripts can't read the keyboard. So for the length of a drag, the daemon connects to KWin as a Wayland client and binds `org_kde_kwin_keystate`, the protocol KDE's keyboard indicator uses, which reports only the modifier and lock keys, never others.

Modifiers and the pointer arrive on different connections, and the D-Bus one is slower. So each modifier change waits 30 ms before it counts, and a release arriving meanwhile carries the modifiers from before it. A modifier let go less than 30 ms before the button still counts as held, which no hand can tell apart.

## Shortcuts

`src/hotkeys.rs`, `src/keys.rs`. The daemon registers its shortcuts with KDE's shortcut service directly, as the component `snappy`, so System Settings lists them under Snappy.

- KDE matches the symbol a key types, not the physical key, so keys are named by the symbol a US layout gives them.
- Snappy refuses a chord another component holds, which is also the availability probe, since the service would otherwise give one key to two actions.
- Keys are set without autoloading, so Snappy's settings decide them, not keys saved earlier.

## Overlay

`src/overlay/`. The daemon draws the overlay itself, with the renderer the X11 adapter uses, and KWin only composites it.

- **Surfaces.** Each display gets two layer-shell surfaces on the overlay layer, one for zones and one for window outlines, placed over the work area, taking no input or focus, with the on-screen-display type the script leaves out of managed windows.
- **Images** go through shared memory, drawn at the next whole scale on fractional displays.
- **Kept, not remade.** The surfaces are made before the first drag and kept. Showing attaches a fresh image, and hiding sets the surface's alpha to 0 rather than destroying it, because KWin's effects fade windows that open and close, and a surface made for every drag would fade in and out. An invisible surface adds no damage and doesn't stop full-screen apps from scanning out directly.
- **Outside KWin.** Nothing of Snappy's is drawn inside KWin itself, since a graphics failure there would take KWin down.
- **Its own thread.** The overlay has a thread of its own, so the engine never waits for drawing. Failures are logged to the journal once until it works again.

## Caps Lock as the Snappy key

`src/caps_lock/`. Snappy adds its XKB option `snappy:caps` to KDE's keyboard settings in `kxkbrc`, changing only those lines, and sends KDE's settings-changed notice, so KWin rebuilds the keymap at once. Held, Caps Lock then sets the Snappy key's modifiers, and Shift + Caps Lock is a real Caps Lock.

- **Presses.** Nothing reports a Caps Lock press directly. So while the setting is on, a keystate watch reports modifier states, never keys. The Snappy key's modifiers becoming held all at once is Caps Lock going down, since the physical modifier keys arrive one report at a time.
- **Taps.** A modifier-only shortcut on the Snappy key's modifiers fires when they come up with nothing else used, which after a Caps Lock press is a tap. To toggle, Snappy presses Shift + Caps Lock through KWin's fake-input protocol.
- **The lock screen.** The option comes out while the screen is locked and goes back after unlocking. If the keyboard settings changed meanwhile, or are saved without it, Caps Lock is reported lost.
- **Refusing.** Snappy refuses when the settings or the layout already remap Caps Lock.
- **The first login.** KWin only looks in `~/.config/xkb` if the folder existed when it started. So Snappy creates the folder whenever it starts on KDE, which by the time most people turn the setting on has been there since their last login. If it wasn't, the option isn't found yet: Snappy checks the keymap KWin builds and backs out, the setting stays on with a notice that Caps Lock becomes the Snappy key after the next login, and the next run applies it.

## Workspaces

`parking.js`. Windows park on a virtual desktop named "Snappy", added last when first needed and found again by name after a restart, frames untouched. While parked, they skip the task manager, the pager, and the window switcher.

Workspaces belong to the desktop in use when they started, and pause on any other. Stopping removes the parking desktop once it's empty.

## Quitting

Besides what every Linux adapter does, quitting unloads the script, unregisters the shortcuts, and gives up the bus name.

## KWin settings that fight Snappy

KWin's own tiling keeps running beside Snappy, and Snappy leaves KWin's window settings alone. These act on the same drags and keys:

- **Shift while moving a window** snaps it into KWin's custom tiles and outlines the tile during the move, with no setting to turn it off. It does nothing when the custom tile layout is empty, so with Shift as the drag key, empty it in the tile editor (Meta+T) on every display and desktop.
- **Quick tiling and maximizing at screen edges** (System Settings → Screen Edges) act when a dragged window reaches an edge, where Snappy's edge zones are.
- **Meta+arrow quick tiling** and **Meta+T** tile windows KWin's way.

## Limitations

- Shortcuts follow the symbols of a US layout, so on other layouts a chord may land on another key.
- With a KWin whose keystate protocol is older than version 5, drags carry no modifiers and nothing snaps by dragging. Shortcuts still work.
- The doorbell is listed among KWin's shortcuts in System Settings. Giving it a key does no harm.
- Plasma 6.0 has no modifier-only shortcuts, so there a Caps Lock tap doesn't toggle Caps Lock. Holding it, and Shift + Caps Lock, still work.
- After a parked window comes back from a daemon restart, it shows in the task manager, pager, and switcher again, even if its app had hidden it from them.
- Only Plasma 6.8 is tested, and only headless without compositing, so the tests check where the overlay's surfaces are, not how they look.
