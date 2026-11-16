# Platforms

How each desktop's adapter does its part. Every adapter implements the traits in `crates/platform` and nothing more: it reports events, answers questions about windows, and carries out moves, while every decision stays in `core` and the engine ([architecture.md](architecture.md)). The adapters are laid out alike, with the same module names for the same jobs where they have them (`input`, `hotkeys`, `overlay`, `caps_lock`, `instance`, `login_item`, `messages`), and each splits the same way:

- **`App`** runs on the main thread and owns the OS event loop and whatever must live there: hooks or monitors, shortcuts, overlays, the tray.
- **A platform handle** (`MacPlatform`, `WindowsPlatform`, `X11Platform`, `GnomePlatform`, `KWinPlatform`) lives on the engine thread, reads and moves windows, and hands main-thread work to `App`.

Callbacks on the main thread only forward events. They never call into other apps' windows, so a hung app can't freeze input or drawing. Which windows Snappy manages is decided once, in `crates/core/src/eligibility.rs`, on facts each adapter reads from its OS, so the refusals read the same everywhere ("Choose a standard application window.", "Leave full screen before tiling this window.").

| | macOS | Windows | X11 | GNOME on Wayland | KDE Plasma on Wayland |
|---|---|---|---|---|---|
| Moves windows through | Accessibility | Win32 | EWMH | GNOME Shell extension | KWin script |
| Shortcuts | Carbon hotkeys | `RegisterHotKey` | `XGrabKey` | the extension | KDE's shortcut service |
| Overlay | `NSPanel` per display | layered window per display | ARGB window per display | St actors in GNOME Shell | layer-shell surface per display |
| Hides workspace windows | at a screen corner | off screen | on a desktop of Snappy's | on a workspace of Snappy's | on a desktop of Snappy's |
| Caps Lock as the Snappy key | HID key mapping + event tap | low-level keyboard hook | XKB key definition | XKB option | XKB option |
| Permission | Accessibility | none | none | none (the extension) | none |

## macOS

`crates/macos`, on public APIs only: AppKit and Core Graphics through `objc2`, and hand-written FFI for Accessibility, Carbon hotkeys, and the HID event system (`src/ffi/`).

**Finding and moving windows** (`src/windows/`). Windows are Accessibility elements. The window under the pointer comes from asking Accessibility for the element at that point and walking up to its window. Where Snappy has a window of its own at the point, such as its menu bar item or an overlay, it asks only the app beneath instead: Accessibility answers questions about Snappy's own windows inside the daemon, on the asking thread, and AppKit used off the main thread crashes. Frames are read as position and size together; writes set one at a time, after checking access is still granted, the app still runs, and the window isn't full screen. Visible windows are each app's windows matched against the window server's on-screen list, which gives owner, layer, and bounds only, never names. Every Accessibility call has a short timeout (`src/thresholds.rs`), so a hung app costs a fraction of a second. Focusing a window activates its app, raises the window, and makes it the app's focused window.

**Observing** (`src/observers/`). Per app, window-created and focus-changed notifications; per tracked window, destroyed, moved, resized, and restored. NSWorkspace reports app activation, launch, and termination, Space changes, and sleep. Registering an observer is a round trip to the app, so it never happens on the main thread.

**Input** (`src/input.rs`). NSEvent monitors for the left button's presses and releases while live control runs, and, only during a drag, for drags and modifier changes. Modifiers come from each event's own flags, so a release carries the modifiers held when the button came up. NSEvent monitors need only Accessibility; a listen-only keyboard tap would add an Input Monitoring prompt, so none is used.

**Shortcuts** (`src/hotkeys.rs`, `src/system_shortcuts.rs`). Carbon `RegisterEventHotKey`, with physical key codes, so shortcuts stay on the same keys on every layout. The availability probe registers a chord under a separate signature and releases it at once. Carbon accepts chords macOS consumes before any app, such as Mission Control's Control-arrows, so Snappy also reads the system's symbolic hotkeys and treats the enabled ones as taken. One chord gets past both: the window server swallows Control-Option-Command-Delete from a real keyboard, so core lists it among the chords the OS keeps and never offers it; clearing the selection uses Snappy + `\` for that reason.

**Overlay** (`src/overlay/`). A borderless, non-activating, click-through `NSPanel` per display at the floating level, joining every Space and full-screen apps, drawn with Core Animation layers that are updated only where they changed. Panels are made ahead of the first drag and kept. Zones are filled with the system accent color. Window outlines get a second panel of the same kind per display, made the first time there is something to outline and hidden while there is nothing.

**Caps Lock as the Snappy key** (`src/caps_lock/`). Snappy adds one pair to the HID event system's user key mapping, the one `hidutil` writes, turning Caps Lock into F18 on every keyboard and leaving the user's own pairs alone. Caps Lock then never locks, lights, or repeats by itself. A Carbon hotkey on F18 reports when it goes down and up. While it is down, an active event tap adds the Snappy key's modifier flags to every other key and click, so Caps Lock + J is Snappy + J for Snappy's shortcuts and drags alike, and notes only that something else was used. A tap toggles Caps Lock and its light through the HID system. Snappy refuses when the user's key mapping or System Settings' Modifier Keys already remap Caps Lock. The mapping comes out when the setting goes off, at quit, when stopping for a restart, and as the daemon exits; macOS forgets it at reboot.

**Native tabs** (`src/windows/tabs.rs`, `src/windows/tab_matching.rs`). macOS makes each tab of a window an Accessibility window of its own, lists only the selected tab of each group, and keeps answering for hidden tabs with the frame they last showed at. Nothing ties a hidden tab to its group but its title, which Snappy never reads. So Snappy learns tabs as they are focused: a window it tracks that has dropped out of the list, without being minimized or closed, is a possible hidden tab, matched to a group by where it was last seen, by having exactly the shown tab's frame, or by filling the only group with room, never more than the group's tab bar holds. The engine then moves each group as one window.

**Workspaces.** Windows are parked at an exposed lower corner of the screen with a sliver left showing, because macOS pulls windows that are fully off screen back. When displays change, as when a Mac wakes with an external display, macOS moves windows itself, parked ones included, and the engine parks them again once the displays settle. A window's recovery key is its process ID, the process's start time, and the hash of its Accessibility element, which stays the same while the window exists. The desktop guard (`src/desktop_guard.rs`) keeps an invisible 1×1 window on each display and asks whether it's on the active Space to tell when the user changed Spaces.

**Permissions** (`src/permissions.rs`). Accessibility. While snapping is on and access is missing, Snappy checks once a second for a grant, and only then; a revocation is noticed without polling, from the system's accessibility notification and from Accessibility calls failing.

**Single instance and quitting** (`src/instance.rs`). A lock on `daemon.lock` in the state folder; a second launch exits, and opening Snappy.app again reaches the running daemon as a reopen, which opens the settings window. `SIGTERM`, `SIGINT`, and `SIGHUP` quit like the menu's Quit, so parked windows return first; `SIGUSR2` stops for a restart. Both programs use the accessory activation policy: no Dock icon and no Command-Tab entry. The login item (`src/login_item.rs`) registers the launch agent bundled in the app (`packaging/macos`), which runs the daemon with `--background`.

**Limitations.**

- Chrome blocks frame changes while its enhanced accessibility is on. Snappy turns it off for one change, unless VoiceOver or Switch Control needs it, and turns it back on.
- Posted key events join the event stream past the HID remap and past where the window server drops Control-Option-Command-Delete, so the live suite can't show what a real keyboard does with either.

## Windows

`crates/windows`, on Win32 through the `windows` crate. Coordinates are physical pixels, and each display's scale is its DPI over 96; the daemon declares per-monitor DPI awareness before making any window. The C runtime is linked statically (`.cargo/config.toml`), so Snappy needs no redistributable.

**The main thread** (`src/app.rs`, `src/dispatch.rs`). A hidden top-level window and its message loop. It isn't a message-only window, because those get no broadcasts, and Snappy needs display, work-area, power, session-end, and taskbar-restart ones. The engine thread hands it work through a dispatcher and waits at most two seconds for an answer.

**Finding and moving windows** (`src/managed/`). The visible frame is the DWM extended frame; writes add back the invisible resize borders. Writes are asynchronous `SetWindowPos` calls, so a hung window can't block the engine, followed by a short wait for the window to change; a maximized window is restored first. Reads never send messages to the window. Window handles are reused, so a window's ID adds a serial tied to its process and that process's creation time (`src/managed/identity.rs`); its recovery key is the process ID, the creation time, and the handle. Windows of elevated apps are refused, since Windows won't let Snappy move them.

**Observing** (`src/events.rs`). Out-of-context WinEvent hooks that skip Snappy's own process: foreground changes, moves and resizes starting and ending, minimizing, showing, destroying, and cloaking. Location changes fire constantly, so they are hooked only for the processes whose windows the engine has asked about.

**Input** (`src/input.rs`). Clicks cost nothing: a press is reported only when the system's move-or-resize loop starts on a window, and during it the window's location changes and the cursor position become the drag. While drag monitoring is on, low-level keyboard and mouse hooks report modifier changes and the button's release; the release comes from the mouse hook, because by the time the move loop ends the user may have let go of the modifier.

**Shortcuts** (`src/hotkeys.rs`, `src/keys.rs`). `RegisterHotKey` on the hidden window. Physical key codes become scan codes; keys whose virtual key depends on the layout get it from the foreground window's keyboard layout, and shortcuts are registered again when a foreground change brings a different layout. The availability probe registers the chord under a spare ID and releases it.

**Overlay** (`src/overlay/`). A layered, click-through, topmost, non-activating tool window per display. Snappy's own rasterizer draws the zones into a bitmap, GDI renders the numbers and captions once into cached masks, and `UpdateLayeredWindow` hands the pixels to DWM, which keeps them. The accent is DWM's colorization color. Window outlines get a second window of the same kind per display.

**Caps Lock as the Snappy key** (`src/caps_lock/`). While the setting is on, the low-level keyboard hook stays installed. It swallows Caps Lock's own presses, so Windows never turns it on, and reports them. While Caps Lock is down, a key that completes one of Snappy's shortcuts with the Snappy key's modifiers fires that shortcut directly and is swallowed, with no modifier key injected, so Windows never sees a synthetic Win key and opens Start; any other key is swallowed too, and modifier keys pass. A tap injects a Caps Lock press marked as Snappy's, which the hook lets through, so Windows toggles Caps Lock and its light. A low-level hook can't ask for one key, so while the setting is on Windows calls it, on the main thread, for every key; it ignores everything but Caps Lock and what's pressed while Caps Lock is held, and writes nothing down. Nothing is changed in the system, so quitting or a crash leaves Caps Lock as it was. Snappy refuses when the registry's `Scancode Map` remaps Caps Lock.

**Tray** (`src/tray/`). A notification-area icon, added again when Explorer restarts, drawn from the tray glyph at the size the display's DPI asks for. Its menu is built from the latest content each time it opens.

**Workspaces.** Windows are parked entirely outside every display; Windows doesn't pull them back. Parked windows still show in the taskbar and Alt+Tab, and activating one switches to its workspace. The desktop guard (`src/desktop_guard.rs`) remembers the virtual desktop through a window on it and asks Windows whether that window is on the current one, checking again when windows are cloaked or uncloaked.

**Permissions.** None.

**Single instance and quitting** (`src/instance.rs`). A named mutex; a second launch signals a named event that opens the settings window, and exits. Windows has no `SIGUSR2`, so `snappy.exe --stop-for-restart` sets another named event and waits for the daemon to exit. A close request to the hidden window, the session ending, and a console's Ctrl+C or close all quit like the tray's Quit. `taskkill` without `/F` can't close the daemon, since it only closes visible windows. The login item (`src/login_item.rs`) is a `Run` value under the current user's registry key, holding the daemon's path and `--background`.

**Limitations.**

- Windows Snap still runs during drags, because the drag is the system's own move loop: dropping at a screen edge or corner lets Snap act first, and dragging to the top edge can show Snap Layouts over Snappy's overlay. Turn off "Snap windows" under Settings → System → Multitasking to leave drags to Snappy.
- The taskbar owns Win with Ctrl, Alt, or Shift and a digit, and Ctrl+Alt types AltGr characters on many layouts. The default Snappy key, Ctrl+Alt+Win, avoids both. A Snappy key of Ctrl+Alt alone registers, but on AltGr layouts the keys Snappy binds can stop typing their characters.
- While a window of an app running as administrator is in front, Windows keeps its input from Snappy's hooks, so Caps Lock behaves as usual there.
- Caps Lock stands for the Snappy key only in Snappy's own shortcuts, since no modifier is injected: it doesn't count as the drag key, and the shortcut recorder never sees a chord pressed with it.
- More than one display, and scales other than 100%, are untested.

## Linux

Snappy picks one of three Linux adapters at launch from the session (`crates/linux/src/session.rs`): GNOME on Wayland and KDE Plasma on Wayland get adapters of their own, and X11 sessions get the X11 adapter. Any other Wayland session also gets the X11 adapter, through Xwayland, but X11 clients can't see other apps' windows there, so live control stays off and the settings window says why.

What doesn't depend on the display server is shared in `crates/linux`:

- **One daemon per session** (`instance.rs`). An exclusive lock on `snappy.lock` in `$XDG_RUNTIME_DIR`, holding the daemon's process ID. A second launch sends that process `SIGUSR1`, which opens the settings window, and exits. `SIGTERM`, `SIGINT`, and `SIGHUP` quit like the menu's Quit, so parked windows return first; `SIGUSR2` stops for a restart.
- **The login item** (`login_item.rs`) is `snappy.desktop` in `~/.config/autostart`, running the daemon with `--background`. It names the app icon, `io.github.farhadzaidi.Snappy`, which Snappy installs into the user's icon theme first (`app_icon.rs`), so nothing points at a path that moves with the program.
- **Recovery keys** combine the window's process ID, the process's start time from `/proc`, and the display server's or shell's ID for the window.
- **Overlays** share one renderer (`overlay_image/`), drawn with `tiny-skia` and a built-in stroke font for numbers and captions, so no font files are needed. X11 and KDE draw with it; for GNOME the same shapes are sent as data (`scene.rs`) and drawn in GNOME Shell.
- **The tray** (`tray.rs`, `menu.rs`) is a StatusNotifierItem with the tray glyph in white. Desktops need a tray host to show it; GNOME gets a top-bar menu of its own instead.
- **Caps Lock as the Snappy key** (`xkb/`) goes through XKB on every Linux desktop: reading which options and layouts already remap Caps Lock, and, on Wayland, an XKB option of Snappy's own, `snappy:caps`, written to `~/.config/xkb`.
- **A minimal Wayland client** (`wayland/`), enough to read the keymap the compositor hands every client and, on KDE, to draw overlays and read modifier keys.
- **For the desktop-shell adapters**, D-Bus plumbing (`dbus.rs`) and the rules for which of the windows a shell reports Snappy manages (`windows.rs`).

Apps draw their tabs inside one window on Linux, so every Linux adapter reports no tab groups. No Linux desktop asks for a permission.

### X11

`crates/x11`, on `x11rb` (pure Rust, no libxcb). It targets EWMH window managers and is tested with Xfwm4, with and without its compositor.

**Threads** (`src/app.rs`). X has no main-thread rule. The event loop reads X events on one connection, and the engine thread makes window requests on a second; key grabs and input selections go out on the event connection, so their events arrive where they're read. The overlay has a thread of its own, which also drives the tray.

**Finding and moving windows** (`src/windows/`). Managed windows come from `_NET_CLIENT_LIST_STACKING`, top first; the window under the pointer is the topmost visible one containing it. Frames include the window manager's decorations (`_NET_FRAME_EXTENTS`) and leave out client-side shadows (`_GTK_FRAME_EXTENTS`). Writes are `_NET_MOVERESIZE_WINDOW` requests, after taking a maximized window out of its maximized state, and focusing sends `_NET_ACTIVE_WINDOW` as a pager would. App identifiers come from `WM_CLASS`. A window is eligible when it isn't full screen, is a top-level window of normal type that isn't transient, and allows moving and resizing.

**Observing** (`src/events.rs`). Moves come from `ConfigureNotify` on the windows Snappy tracks, not on the root window, which would wake Snappy for every frame of every window. Root window properties report windows opening and closing, focus, and desktop changes; RandR, `_NET_WORKAREA`, and `Xft.dpi` report display changes, and work areas are worked out per monitor from panel struts.

**Input** (`src/input.rs`). XInput 2 raw button and motion events on the root window, which arrive even while the window manager grabs the pointer for a move, and grab nothing themselves. Raw key events are selected only while drag monitoring is on, to follow the modifier keys in event order from the keys down when it started, so a release reports the modifiers held when the button came up; asking the server afterwards could already see a modifier let go right behind the button.

**Shortcuts** (`src/hotkeys.rs`, `src/keymap.rs`). `XGrabKey` on the root window, once for each combination of Caps Lock and Num Lock. A refused grab means another client holds the chord, which doubles as the availability probe. On evdev keymaps physical key codes map straight to keycodes, so shortcuts stay on the same keys on every layout. Detectable auto-repeat makes a held shortcut fire once.

**Overlay** (`src/overlay/`). Override-redirect windows per display, one for zones and one for window outlines, with an ARGB visual and an empty input region, so they take no clicks. Without a compositing manager, the Shape extension cuts each down to the zone borders and the boxes behind numbers and captions. X11 has no accent color to read, so zones use the default blue. The outlines window is raised again whenever the active window changes, since window managers raise the window they focus.

**Caps Lock as the Snappy key** (`src/caps_lock/`). Snappy rewrites the Caps Lock key's XKB definition for the session: no symbol, an action that sets the Snappy key's modifiers while held, no repeat. Snappy's grabs and every other client see the modifiers themselves, and Caps Lock never locks. A passive grab on the key reports its press; only while it is down are raw key and button presses selected, to notice that something else was used. A tap toggles Caps Lock through XKB's lock state, which the light follows. The original definition is kept in the engine's undo record and written back, while the key still has Snappy's, when the setting goes off, at quit, when stopping for a restart, and after a crash at the next start. A keymap loaded over Snappy's change, as `setxkbmap` or a layout switch does, is noticed through XKB's own notices and reported as Caps Lock lost. Snappy refuses when the session's XKB options remap Caps Lock (`caps:escape`, `ctrl:nocaps`, and the like) or the key is no longer a plain Caps Lock, as after `xmodmap`.

**Workspaces** (`src/windows/desktops.rs`). Window managers such as Xfwm4 keep part of every window on screen, so moving windows off screen can't hide them. Instead Snappy adds a desktop named "Snappy" after the user's own and moves parked windows there, frames untouched. Its number is kept in a root window property, so a daemon restarted after a crash finds it again. The desktop guard compares the current desktop with the one workspaces started on.

**Limitations.**

- Only Xfwm4 is tested. Other window managers may differ in decorations, multi-monitor moves, and the tray.
- Grabs match modifiers exactly, so desktop shortcuts that use fewer of the Snappy key's modifiers don't collide with Snappy's, and GNOME, KDE Plasma, and Xfce bind nothing to Ctrl+Alt+Super. A Snappy key of fewer modifiers can reach theirs: Xfwm4 switches workspaces with Ctrl+Alt and the arrows, so a Snappy key of Ctrl+Alt is refused wherever Xfwm4 holds them.

### GNOME on Wayland

`crates/gnome` and the GNOME Shell extension in `apps/gnome-extension`, for GNOME 46 to 49. Wayland lets no ordinary program move other apps' windows, watch global input, or own global shortcuts; on GNOME only GNOME Shell can. So the extension does those things when the daemon asks and reports what happens, and makes no decisions of its own. Calls that differ between GNOME versions go through `compat.js`.

**Installing the extension** (`src/extension.rs`). The extension's files are built into the daemon, which writes any that are missing or differ into the user's extensions folder at every start. GNOME Shell on Wayland loads a new extension only at the next login, so a fresh install turns itself on in GNOME's settings and the settings window asks the user to log out and back in once, with a **Log out now** button. After that, turning it on or off is the user's choice.

**The protocol** (`src/protocol.rs`, `src/bus.rs`, `service.js`). The extension exports an interface on GNOME Shell's own bus name with one method, `Call`, which takes a JSON request and returns `{"ok": …}` or `{"error": …}`, and one signal, `Event`, which carries every event as JSON in the order it happened. A `Version` property holds the protocol version; a daemon and an extension from different builds notice the mismatch, and the settings window says to log out and back in. The daemon's requests run on a bus thread of their own, since the engine thread drives an async runtime in which `zbus` can't block, and further threads forward the extension's events, its state changes, GNOME Shell starting over, and Mutter's display changes.

**Live control** (`src/extension.rs`, `src/messages.rs`). Live control is available while the extension answers in this protocol version; otherwise GNOME's extension state says why, worded for the user and naming the desktop. When GNOME Shell starts over, a fresh extension holds none of Snappy's shortcuts, monitoring, outlines, menu, or Caps Lock hold, so the adapter sends them all again and compares the window list with the one it had. Workspaces are suspended while the extension is away and resume when it's back.

**When the daemon goes away** (`service.js`). The extension watches the bus name of the daemon that set things up. When it vanishes, as in a crash, the extension releases the shortcuts and Caps Lock, stops monitoring, and removes the overlays and the menu, so no dead shortcut swallows keys. Parked windows stay on the parking workspace for the next daemon to find.

**Finding and moving windows** (`windows.js`, `crates/linux/src/windows.rs`). Managed windows are Mutter's non-override-redirect windows, top first; Wayland and Xwayland windows are treated alike, and titles are never read. A window's ID is Mutter's stable sequence number, which also forms its recovery key. Eligibility is decided in Rust on the facts the extension reports. Writes are `move_frame` and `move_resize_frame`, after taking the window out of maximized or tiled state. Moves are reported only for windows the engine asked to follow. Focusing is `Meta.Window.activate`, which also raises the window.

**Displays** (`src/displays.rs`). Names, scale, and layout mode come from Mutter's `DisplayConfig`, frames and work areas from the extension. In Mutter's logical layout, coordinates are points, so every display's scale is 1; in the physical layout they're device pixels at the monitor's scale. Display IDs hash the monitor's vendor, product, and serial number, so the same monitor has a different ID under X11, which hashes the whole EDID.

**Input** (`input.js`). Clicks on Wayland windows go straight to their apps, so, as on Windows, a press is reported when a pointer-driven move or resize starts, naming the window, and the release when it ends. While a drag runs and drag monitoring is on, the pointer and modifiers are sampled on a short timer; nothing is sampled otherwise, and no keystroke is reported. The release's modifiers come from the release event itself, since asking afterwards can already see a modifier let go right behind the button.

**Keeping the window under the pointer.** GNOME pulls a moving window onto nearby screen and window edges while Shift is held, and has no setting for it. The engine tells the adapter the drag key, and while it's held during a move the extension puts the window back under the pointer on every position change, before the frame is drawn. The window follows the pointer, GNOME's edge tiling stays out of the way, and Snappy's zones are the only guide; moves without the drag key are GNOME's as usual.

**Shortcuts** (`src/keys.rs`, `shortcuts.js`). GNOME Shell's `grab_accelerator`, with physical keycodes, so shortcuts stay on the same keys on every layout, ignoring key repeat and allowed only in the normal session, not the overview or the lock screen. A grab is refused when GNOME or another program binds the chord, which doubles as the availability probe.

**Overlay** (`overlay.js`, `crates/linux/src/scene.rs`). The daemon sends finished shapes, placed and colored by the style every overlay shares, and the extension draws them in a non-reactive container per display above everything in GNOME Shell's UI, so it takes no clicks or focus. Only changed scenes cross the bus. Window outlines go in a second container per display. Both are hidden on the lock screen.

**Menu** (`src/menu.rs`, `menu.js`). A top-bar item instead of a tray icon, built from the same structure as the tray menu, with the tray glyph as a symbolic icon that GNOME recolors to match the bar. Each choice carries the menu's generation, so a choice from a menu that was replaced while open does nothing.

**Caps Lock as the Snappy key** (`src/caps_lock.rs`, `capsLock.js`). Snappy writes its XKB option to `~/.config/xkb`: held, Caps Lock sets the Snappy key's modifiers, doesn't repeat, and types no key, so GNOME's Super overview never sees one; Shift + Caps Lock stays a real Caps Lock. The extension adds the option by wrapping GNOME's keyboard options rather than changing the user's settings, and rebuilds the keymap. It grabs Caps Lock alone to learn of a press; only while Caps Lock is down does it grab the other keys with the Snappy key's modifiers and sample the pointer, to notice that something else was used, never what. A tap presses Caps Lock on a virtual keyboard, flipping the state and every keyboard's light. The option comes out when Caps Lock is restored, when the extension is turned off or the daemon goes away, and while the screen is locked, so Caps Lock works as usual at the password prompt. Snappy refuses when GNOME's options already remap Caps Lock or the layout itself makes Caps Lock another key, as Colemak does; when the user later adds such an option, theirs wins and Caps Lock is reported lost.

**Workspaces** (`parking.js`). Windows park on a GNOME workspace of Snappy's own, frames untouched: just before the empty last workspace GNOME keeps with dynamic workspaces, or last with a fixed number. Parked windows are left out of Alt+Tab and the overview. Workspaces belong to the GNOME workspace in use when they started and pause on any other. Turning the extension off returns every parked window first.

**Limitations and conflicting settings.**

- Ubuntu's Enhanced Tiling, and the Tiling Assistant it comes from, also snap windows dragged to screen edges and act on Ctrl during drags. While one runs, the settings window says where to turn it off.
- With "Workspaces on primary display only" on (Settings → Multitasking), GNOME shows windows on other displays on every workspace, so Snappy can't hide them and refuses switches that would need to.
- GNOME binds Super with Ctrl and a digit, and Ctrl+Alt with an F-key; the default Snappy key avoids both. A Snappy key of fewer modifiers can't: GNOME switches workspaces with Ctrl+Alt and the arrows and with Super+Alt and Left or Right, and moves windows between displays with Super+Shift and the arrows.
- Wayland has no way for an app to ask to be sized in steps, so only Xwayland apps, such as some terminals, snap to a cell grid.
- Activating a parked window from the dock or a notification makes GNOME switch to the parking workspace; workspaces pause there until you switch back.
- Apps that block desktop shortcuts, such as virtual machines and remote desktops, get Caps Lock's modifiers but no tap.

### KDE Plasma on Wayland

`crates/kwin` and the KWin script in `apps/kwin-script`, for Plasma 6.8. On Wayland only KWin can move other apps' windows or see them move, so the script does that when the daemon asks and reports what happens, and makes no decisions. Unlike GNOME, the overlay, shortcuts, and modifier keys don't go through the script: the daemon draws the overlay as a Wayland client, registers shortcuts with KDE's shortcut service, and reads modifiers from KWin directly.

**Loading the script** (`src/script.rs`). The script's files are built into the daemon. At every start it writes them to a folder in `$XDG_RUNTIME_DIR` named for their hash, since KWin caches compiled QML by path, and loads them over KWin's scripting D-Bus interface, replacing any copy a crashed daemon left loaded. Nothing is installed and there's no need to log out. Quitting unloads it, and when KWin starts over the daemon loads it again.

**The protocol** (`src/protocol.rs`, `src/bus.rs`, `service.js`). KWin scripts can call D-Bus methods but can't offer any, so the daemon serves the interface and answers only KWin's own connection. The script reports events by calling `Event`. For requests the daemon rings a doorbell: a shortcut action the script registers in KWin's shortcut component without a key, which the daemon triggers over D-Bus. The script then calls `Take` to collect the queued requests, and each `Answer` it sends returns any requests queued meanwhile, so a burst needs one ring. Nothing polls while nobody asks. A request the script doesn't answer within two seconds is reported as KWin not answering. Live control is available once a script of this protocol version reports it's ready. Requests block on a bus thread of their own, and further threads serve the script's calls and forward KDE's shortcut presses, KWin starting over, screen locking, and keyboard settings changes.

**Finding and moving windows** (`windows.js`, `crates/linux/src/windows.rs`). Managed windows are KWin's windows for apps, top first, leaving out its own and Plasma's surfaces: panels, the desktop, notifications, popups, and on-screen displays. Wayland and Xwayland windows are treated alike, and titles are never read. A window's ID is part of KWin's UUID for it, which outlives both the script and the daemon and forms its recovery key. Eligibility is decided in Rust, as on GNOME. Writes set the window's frame after taking it out of a maximized state or one of KWin's tiles. Focusing sets KWin's active window, which also raises it.

**Displays** (`src/displays.rs`). KWin's coordinates are logical pixels, so every display's scale is 1. Displays come in KWin's screen order with the area KWin maximizes windows into as the work area, so zones end where maximized windows do. KWin tells scripts nothing when a panel's reserved space changes, so the script compares work areas itself after a panel appears, goes, moves, or resizes, and at the start of every drag.

**Input** (`service.js`, `src/input/`). As on GNOME, a press is reported when KWin starts an interactive move or resize, and the release when it finishes; in between, pointer moves are reported at most every 8 ms, and only while drag monitoring is on. Scripts can't read the keyboard, so for the length of a drag the daemon connects to KWin as a Wayland client and binds `org_kde_kwin_keystate`, the protocol KDE's keyboard indicator uses, which reports only the modifier and lock keys, never others. Modifiers and the pointer arrive on different connections, and the D-Bus one is slower, so each modifier change waits 30 ms before it counts; a release arriving meanwhile carries the modifiers from before it. A modifier let go less than 30 ms before the button still counts as held, which no hand can tell apart.

**Shortcuts** (`src/hotkeys.rs`, `src/keys.rs`). The daemon registers its shortcuts with KDE's shortcut service directly, as the component `snappy`, so System Settings lists them under Snappy. KDE matches the symbol a key types, not the physical key, so keys are named by the symbol a US layout gives them. Snappy refuses a chord another component holds, which is also the availability probe, since the service would otherwise give one key to two actions. Keys are set without autoloading, so Snappy's settings decide them, not keys saved earlier.

**Overlay** (`src/overlay/`). The daemon draws the overlay itself, with the renderer the X11 adapter uses, and KWin only composites it: two layer-shell surfaces per display on the overlay layer, one for zones and one for window outlines, placed over the work area, taking no input or focus, with the on-screen-display type the script leaves out of managed windows. Images go through shared memory, drawn at the next whole scale on fractional displays. The surfaces are made before the first drag and kept: showing attaches a fresh image, and hiding sets the surface's alpha to 0 rather than destroying it, because KWin's effects fade windows that open and close, and a surface made for every drag would fade in and out. An invisible surface adds no damage and doesn't stop full-screen apps from scanning out directly. Nothing of Snappy's is drawn inside KWin itself, since a graphics failure there would take KWin down. The overlay has a thread of its own, so the engine never waits for drawing; failures are logged to the journal once until it works again.

**Caps Lock as the Snappy key** (`src/caps_lock/`). Snappy adds its XKB option `snappy:caps` to KDE's keyboard settings in `kxkbrc`, changing only those lines, and sends KDE's settings-changed notice so KWin rebuilds the keymap at once. Held, Caps Lock then sets the Snappy key's modifiers, and Shift + Caps Lock is a real Caps Lock. Nothing reports a Caps Lock press directly, so while the setting is on a keystate watch reports modifier states, never keys: the Snappy key's modifiers becoming held all at once is Caps Lock going down, since the physical modifier keys arrive one report at a time. A modifier-only shortcut on the Snappy key's modifiers fires when they come up with nothing else used, which after a Caps Lock press is a tap. To toggle, Snappy presses Shift + Caps Lock through KWin's fake-input protocol. The option comes out while the screen is locked and goes back after unlocking; if the keyboard settings changed meanwhile, or are saved without it, Caps Lock is reported lost. Snappy refuses when the settings or the layout already remap Caps Lock. KWin only looks in `~/.config/xkb` if the folder existed when it started, so Snappy creates the folder whenever it starts on KDE, which by the time most people turn the setting on has been there since their last login. If it wasn't, the option isn't found yet: Snappy checks the keymap KWin builds and backs out, the setting stays on with a notice that Caps Lock becomes the Snappy key after the next login, and the next run applies it.

**Workspaces** (`parking.js`). Windows park on a virtual desktop named "Snappy", added last when first needed and found again by name after a restart, frames untouched. While parked they skip the task manager, the pager, and the window switcher. Workspaces belong to the desktop in use when they started, and pause on any other. Stopping removes the parking desktop once it's empty.

**Quitting** also unloads the script, unregisters the shortcuts, and gives up the bus name, besides what every Linux adapter does.

**KWin settings that fight Snappy.** KWin's own tiling keeps running beside Snappy, and Snappy leaves KWin's window settings alone. The ones that act on the same drags and keys:

- *Shift while moving a window* snaps it into KWin's custom tiles and outlines the tile during the move, with no setting to turn it off. It does nothing when the custom tile layout is empty, so with Shift as the drag key, empty it in the tile editor (Meta+T) on every display and desktop.
- *Quick tiling and maximizing at screen edges* (System Settings → Screen Edges) act when a dragged window reaches an edge, where Snappy's edge zones are.
- *Meta+arrow quick tiling* and *Meta+T* tile windows KWin's way.

**Limitations.**

- Shortcuts follow the symbols of a US layout, so on other layouts a chord may land on another key.
- With a KWin whose keystate protocol is older than version 5, drags carry no modifiers and nothing snaps by dragging; shortcuts still work.
- The doorbell is listed among KWin's shortcuts in System Settings. Giving it a key does no harm.
- Plasma 6.0 has no modifier-only shortcuts, so there a Caps Lock tap doesn't toggle Caps Lock; holding it, and Shift + Caps Lock, still work.
- After a parked window comes back from a daemon restart, it shows in the task manager, pager, and switcher again even if its app had hidden it from them.
- Only Plasma 6.8 is tested, and only headless without compositing, so the tests check where the overlay's surfaces are, not how they look.
