# KDE Plasma on Wayland

This page is for anyone working on Snappy's KDE adapter, `crates/kwin`, and its KWin script in `crates/kwin/script`. It explains why KDE needs a script, how the daemon and the script talk, and how each feature works on KDE. It supports Plasma 6.8. What it shares with the other Linux adapters is in [Linux](linux.md).

## Why a script, and what it doesn't do

On Wayland, only the compositor, KWin on KDE, can move other apps' windows or see them move. So Snappy loads a small script into KWin that does those things when the daemon asks, and reports what happens. It makes no decisions.

Unlike GNOME's extension, the script does nothing else. The daemon draws the zones itself as an ordinary Wayland program, registers shortcuts with KDE's shortcut service, and reads the modifier keys from KWin directly.

## Loading the script

The script's files are built into the daemon (`src/script.rs`). At every start, the daemon writes them to a folder in `$XDG_RUNTIME_DIR` named after their hash, because KWin caches compiled scripts by path. Then it loads them through KWin's scripting interface on D-Bus, replacing any copy a crashed daemon left behind.

Nothing is installed, and nobody has to log out. Quitting unloads the script, and when KWin restarts, the daemon loads it again.

## How the daemon and the script talk

KWin scripts can call D-Bus methods, but can't offer any. So the conversation runs backwards: the daemon offers the D-Bus interface, and the script calls it (`src/protocol.rs`, `src/bus.rs`, and `service.js` in the script). The daemon answers only KWin's own connection.

- **Events.** The script reports events by calling `Event`.
- **Requests.** The daemon can't call the script, so it rings a doorbell. The doorbell is a shortcut action the script registers with KWin, with no key, which the daemon triggers over D-Bus. The script then calls `Take` to collect the waiting requests. Each `Answer` it sends back returns any requests that arrived meanwhile, so a burst of requests needs only one ring. Nothing polls while nobody's asking.
- **Timeouts.** If the script doesn't answer a request within two seconds, it's reported as KWin not answering.
- **Live.** Snappy is live once a script of the same protocol version reports that it's ready.

Requests wait on a bus thread of their own. More threads answer the script's calls, and pass on KDE's shortcut presses, KWin restarting, the screen locking, and keyboard settings changing.

## How Snappy finds and moves windows

The script does the work in `windows.js`. Which windows Snappy manages is decided in Rust, as on GNOME (`crates/linux/src/windows.rs`).

- **Which windows.** KWin's windows for apps, top first. KWin's and Plasma's own surfaces are left out: panels, the desktop, notifications, pop-ups, and on-screen displays. Wayland and Xwayland windows are treated alike, and titles are never read.
- **IDs.** A window's ID is part of the UUID KWin gives it, which outlives both the script and the daemon, so it's also part of the window's recovery key.
- **Moves** take the window out of a maximized state or one of KWin's tiles first.
- **Focusing** makes the window KWin's active window, which also raises it.

## Displays

KWin's coordinates are logical pixels, so every display's scale is 1 (`src/displays.rs`). Displays come in KWin's screen order. Each one's work area is the area KWin maximizes windows into, so zones end exactly where maximized windows do.

KWin doesn't tell scripts when the space a panel reserves changes. So the script compares work areas itself after a panel appears, goes, moves, or resizes, and at the start of every drag.

## How Snappy watches a drag

As on GNOME, the script reports a press when KWin starts moving or resizing a window, and a release when that ends (`service.js`, `src/input/`). In between, it reports pointer moves at most every 8 ms, and only while a drag is being watched.

Scripts can't read the keyboard. So for the length of a drag, the daemon connects to KWin as a Wayland program and asks for the state of the modifier and lock keys, through the same protocol KDE's keyboard indicator uses (`org_kde_kwin_keystate`). It reports only those keys, never others.

The modifiers and the pointer arrive on different connections, and the pointer's, over D-Bus, is slower. So each modifier change waits 30 ms before it counts, and a release that arrives meanwhile carries the modifiers from before the change. A modifier let go less than 30 ms before the button still counts as held. No hand can tell the difference.

## Shortcuts

The daemon registers its shortcuts with KDE's shortcut service itself, as the component `snappy`, so System Settings lists them under Snappy (`src/hotkeys.rs`, `src/keys.rs`).

- **By symbol, not position.** KDE matches the character a key types, not the physical key. So Snappy names keys by what they type on a US layout.
- **No sharing.** The service would give one key to two actions, so Snappy itself refuses a chord another component holds. That's also how it checks whether a chord is free.
- **Snappy's settings decide.** Keys are set without loading ones saved earlier, so the keys are always the ones in Snappy's settings.

## Drawing the zones

The daemon draws the zones itself, with the same renderer as the X11 adapter, and KWin just puts them on screen (`src/overlay/`).

- **Surfaces.** Each display gets two surfaces in the layer above everything else, one for zones and one for window outlines. They cover the work area, take no input or focus, and are marked as on-screen displays, which the script leaves out of managed windows.
- **Images** are passed through shared memory, drawn at the next whole scale on displays with a fractional one.
- **Kept, not remade.** The surfaces are made before the first drag and kept. Showing the zones attaches a new image, and hiding them makes the surface fully transparent rather than destroying it. That's because KWin's effects fade windows in and out as they open and close, and a surface made for every drag would fade. An invisible surface costs KWin nothing to draw, and doesn't stop full-screen apps from going straight to the screen.
- **Nothing inside KWin.** Snappy draws nothing inside KWin itself, since a graphics failure there would take KWin down.
- **A thread of its own.** The overlay has its own thread, so the engine never waits for drawing. If drawing fails, it's logged to the system journal once, until it works again.

## Caps Lock as the Snappy key

Snappy adds its XKB option, `snappy:caps`, to KDE's keyboard settings in `kxkbrc`, changing only those lines (`src/caps_lock/`). Then it tells KDE the settings changed, and KWin rebuilds the keymap at once. Held, Caps Lock then sets the Snappy key's modifiers. Shift + Caps Lock is still a real Caps Lock.

- **Presses.** Nothing reports a Caps Lock press directly. So while the setting is on, Snappy watches the modifier states, never keys. All of the Snappy key's modifiers becoming held at once means Caps Lock went down, because real modifier keys arrive one at a time.
- **Taps.** A modifier-only shortcut on the Snappy key's modifiers fires when they're let go with nothing else used. After a Caps Lock press, that's a tap. To toggle the lock, Snappy presses Shift + Caps Lock through KWin's fake-input protocol.
- **The lock screen.** The option comes out while the screen is locked, and goes back after unlocking. If the keyboard settings changed meanwhile, or were saved without it, Caps Lock is reported lost.
- **Refusing.** Snappy refuses when the user's keyboard settings or layout already remap Caps Lock.

### The first login

KWin only looks in `~/.config/xkb` for options like Snappy's if that folder existed when KWin started. So Snappy creates the folder whenever it starts on KDE. For most people, by the time they turn the setting on, it's been there since their last login.

If it wasn't, KWin can't find the option yet. Snappy checks the keymap KWin builds, sees the option missing, and backs out. The setting stays on, with a notice that Caps Lock becomes the Snappy key after the next login, and the next run applies it.

## Workspaces

Snappy hides workspace windows on a virtual desktop named "Snappy", added last when it's first needed and found again by name after a restart (`parking.js`). Frames stay untouched. While hidden, windows are left out of the task manager, the pager, and the window switcher.

Snappy's workspaces pause when the user switches to another desktop. When Snappy stops, it removes the "Snappy" desktop once it's empty.

## Quitting

Besides what every Linux adapter does, quitting unloads the script, unregisters the shortcuts, and gives up the bus name.

## Uninstalling

Quitting already takes Snappy's option out of `kxkbrc`. After it, `snappy --uninstall` removes what's left (`src/uninstall.rs`):

- **Keyboard settings.** An option a run that couldn't quit left in `kxkbrc` comes out, and a running KWin hears of it. So does the applications entry that lets Snappy toggle Caps Lock.
- **Shortcuts.** KDE's shortcut service keeps a component's shortcuts in `kglobalshortcutsrc` after the program lets go of them, and the script's doorbell among KWin's. With the service running, Snappy has it clean up its component and forget the doorbell.

## KWin settings that get in the way

KWin's own tiling keeps running alongside Snappy, and Snappy leaves KWin's settings alone. These act on the same drags and keys:

- **Shift while moving a window** snaps it into KWin's custom tiles, with no setting to turn that off. It does nothing when the custom tile layout is empty. So with Shift as the drag key, empty it in the tile editor (Meta+T) on every display and desktop.
- **Quick tiling and maximizing at screen edges** (System Settings → Screen Edges) act when a dragged window reaches an edge, which is where Snappy's edge zones are.
- **Meta and the arrows**, and **Meta+T**, tile windows KWin's way.

## Limitations

- **US key symbols.** Shortcuts follow what keys type on a US layout, so on other layouts a chord may land on a different key.
- **Older KWin.** If KWin's keyboard state protocol is older than version 5, drags carry no modifiers, so nothing snaps by dragging. Shortcuts still work.
- **The doorbell** shows among KWin's shortcuts in System Settings. Giving it a key does no harm.
- **Plasma 6.0** has no modifier-only shortcuts, so there a Caps Lock tap doesn't toggle Caps Lock. Holding it, and Shift + Caps Lock, still work.
- **Hidden-from-taskbar windows.** After a hidden window comes back from a daemon restart, it shows in the task manager, pager, and switcher again, even if its app had hidden it from them.
- **Testing.** Only Plasma 6.8 is tested, and only headless, without anything drawn on screen. So the tests check where the overlay's surfaces are, not how they look.
