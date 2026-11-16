# Windows

`crates/windows` uses Win32 through the `windows` crate.

Coordinates are physical pixels, and each display's scale is its DPI over 96. The daemon declares per-monitor DPI awareness before making any window. The C runtime is linked statically (`.cargo/config.toml`), so Snappy needs no redistributable.

## The main thread

`src/app.rs`, `src/dispatch.rs`. A hidden top-level window and its message loop run on the main thread. It isn't a message-only window, because those get no broadcasts, and Snappy needs the display, work-area, power, session-end, and taskbar-restart ones.

The engine thread hands it work through a dispatcher and waits at most two seconds for an answer.

## Finding and moving windows

`src/managed/`.

- **Frames.** The visible frame is the DWM extended frame. Writes add back the invisible resize borders.
- **Writes** are asynchronous `SetWindowPos` calls, so a hung window can't block the engine, followed by a short wait for the window to change. A maximized window is restored first.
- **Reads** never send messages to the window.
- **Identity.** Window handles are reused, so a window's ID adds a serial tied to its process and that process's creation time (`src/managed/identity.rs`). Its recovery key is the process ID, the creation time, and the handle.
- **Elevated apps.** Windows of apps running as administrator are refused, since Windows won't let Snappy move them.

## Observing

`src/events.rs`. Out-of-context WinEvent hooks that skip Snappy's own process report foreground changes, moves and resizes starting and ending, minimizing, showing, destroying, and cloaking.

Location changes fire constantly, so they're hooked only for the processes whose windows the engine has asked about.

## Input

`src/input.rs`. Clicks cost nothing: a press is reported only when the system's move-or-resize loop starts on a window, and during it the window's location changes and the cursor position become the drag.

While drag monitoring is on, low-level keyboard and mouse hooks report modifier changes and the button's release. The release comes from the mouse hook, because by the time the move loop ends the user may have let go of the modifier.

## Shortcuts

`src/hotkeys.rs`, `src/keys.rs`. Shortcuts are `RegisterHotKey` on the hidden window.

Physical key codes become scan codes. Keys whose virtual key depends on the layout get it from the foreground window's keyboard layout, and shortcuts are registered again when a foreground change brings a different layout. The availability probe registers the chord under a spare ID and releases it.

## Overlay

`src/overlay/`. Each display gets a layered, click-through, topmost, non-activating tool window. Snappy's own rasterizer draws the zones into a bitmap, GDI renders the numbers and captions once into cached masks, and `UpdateLayeredWindow` hands the pixels to DWM, which keeps them. The accent is DWM's colorization color.

Window outlines get a second window of the same kind per display.

## Caps Lock as the Snappy key

`src/caps_lock/`. While the setting is on, the low-level keyboard hook stays installed.

- **Caps Lock's own presses** are swallowed, so Windows never turns it on, and reported.
- **While Caps Lock is down**, a key that completes one of Snappy's shortcuts with the Snappy key's modifiers fires that shortcut directly and is swallowed. No modifier key is injected, so Windows never sees a synthetic Win key and opens Start. Any other key is swallowed too, and modifier keys pass.
- **A tap** injects a Caps Lock press marked as Snappy's, which the hook lets through, so Windows toggles Caps Lock and its light.
- **Privacy.** A low-level hook can't ask for one key, so while the setting is on Windows calls it, on the main thread, for every key. It ignores everything but Caps Lock and what's pressed while Caps Lock is held, and writes nothing down.
- **Nothing to undo.** Nothing is changed in the system, so quitting or a crash leaves Caps Lock as it was.
- **Refusing.** Snappy refuses when the registry's `Scancode Map` remaps Caps Lock.

## Tray

`src/tray/`. A notification-area icon, added again when Explorer restarts, drawn from the tray glyph at the size the display's DPI asks for. Its menu is built from the latest content each time it opens.

## Workspaces

- **Parking.** Windows are parked entirely outside every display; Windows doesn't pull them back. Parked windows still show in the taskbar and Alt+Tab, and activating one switches to its workspace.
- **Virtual desktops.** The desktop guard (`src/desktop_guard.rs`) remembers the virtual desktop through a window on it, and asks Windows whether that window is on the current one, checking again when windows are cloaked or uncloaked.

## Permissions

None.

## Single instance, quitting, and login

`src/instance.rs`, `src/login_item.rs`.

- A named mutex keeps one daemon running. A second launch signals a named event that opens the app's window, and exits.
- Windows has no `SIGUSR2`, so `snappy.exe --stop-for-restart` sets another named event and waits for the daemon to exit.
- A close request to the hidden window, the session ending, and a console's Ctrl+C or close all quit like the tray's Quit.
- `taskkill` without `/F` can't close the daemon, since it only closes visible windows.
- The login item is a `Run` value under the current user's registry key, holding the daemon's path and `--background`.

## Limitations

- **Windows Snap** still runs during drags, because the drag is the system's own move loop. Dropping at a screen edge or corner lets Snap act first, and dragging to the top edge can show Snap Layouts over Snappy's overlay. Turning off "Snap windows" under Settings → System → Multitasking leaves drags to Snappy.
- **Keys Windows keeps.** The taskbar owns Win with Ctrl, Alt, or Shift and a digit, and Ctrl+Alt types AltGr characters on many layouts. The default Snappy key, Ctrl+Alt+Win, avoids both. A Snappy key of Ctrl+Alt alone registers, but on AltGr layouts the keys Snappy binds can stop typing their characters.
- **Elevated windows.** While a window of an app running as administrator is in front, Windows keeps its input from Snappy's hooks, so Caps Lock behaves as usual there.
- **Caps Lock's reach.** Caps Lock stands for the Snappy key only in Snappy's own shortcuts, since no modifier is injected. It doesn't count as the drag key, and the shortcut recorder never sees a chord pressed with it.
- **Untested setups.** More than one display, and scales other than 100%, are untested.
