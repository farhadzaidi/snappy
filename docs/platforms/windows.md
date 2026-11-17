# Windows

This page is for anyone working on Snappy's Windows adapter, `crates/windows`. It explains how Snappy finds and moves windows on Windows, watches drags, registers shortcuts, draws zones, and handles Caps Lock, virtual desktops, and the Microsoft Store package.

The adapter calls Win32 through the `windows` crate. A few things hold throughout:

- **Coordinates are physical pixels**, and each display's scale is its DPI divided by 96. The daemon declares itself aware of each monitor's DPI before it makes any window.
- **No runtime to install.** The C runtime is linked statically (`.cargo/config.toml`), so Snappy needs no redistributable package.

## The main thread

Windows sends programs most of their news as messages to a window. So the main thread runs a hidden window and its message loop (`src/app.rs`). It's an ordinary top-level window rather than a message-only one, because message-only windows don't get broadcasts. Snappy needs the ones about displays, work areas, power, the session ending, and the taskbar restarting.

The engine thread hands the main thread work through a dispatcher (`src/dispatch.rs`), and waits at most two seconds for an answer.

## How Snappy finds and moves windows

The code is in `src/managed/`.

**Frames.** The frame Snappy works with is the one people see, which Windows calls the extended frame. Windows also gives most windows invisible borders for resizing, so writes add those back.

**Writes** are asynchronous, so a hung window can't block the engine. Snappy then waits briefly for the window to change. A maximized window is restored first.

**Reads** never send the window a message, so a hung app can't make them wait.

**Telling windows apart.** Windows reuses window handles. So a window's ID in Snappy adds a serial number tied to its process and when that process started (`src/managed/identity.rs`). Its recovery key, used after a restart, is the process ID, when the process started, and the handle.

**Apps running as administrator** are refused, since Windows doesn't let Snappy move their windows.

## How Snappy notices changes

Windows reports system-wide events through hooks that run outside the app being watched, and Snappy uses those (`src/events.rs`). They report focus changes, moves and resizes starting and ending, and windows being minimized, shown, closed, or hidden by the system.

Location changes fire constantly, for every window that moves. So Snappy hooks them only for the processes whose windows the engine has asked about.

## How Snappy watches a drag

A click costs Snappy nothing (`src/input.rs`). When someone drags a window by its title bar, Windows runs a move loop of its own, and Snappy is told when that starts. Only then does it report a press, and while the loop runs, the window's position and the cursor make up the drag.

While a drag is being watched, low-level keyboard and mouse hooks report modifier changes and the button's release. The release comes from the mouse hook, because by the time the move loop ends, the user may already have let go of the modifier.

## Shortcuts

Shortcuts are registered with `RegisterHotKey` on the hidden window (`src/hotkeys.rs`, `src/keys.rs`).

Snappy stores keys by their physical position, and turns them into scan codes. Some keys mean different things on different layouts, so their virtual key comes from the keyboard layout of the window in front. When the user switches to a window with a different layout, the shortcuts are registered again.

To check whether a chord is free, Snappy registers it under a spare ID and releases it.

## Drawing the zones

Each display gets a window of Snappy's own that sits on top, lets clicks through, and never takes focus (`src/overlay/`). Snappy draws the zones into an image, with the numbers and captions drawn once by GDI and kept. Then it hands the finished pixels to Windows' compositor, which keeps showing them without Snappy redrawing. Zones use the system's accent color.

Window outlines get a second window of the same kind per display.

## Caps Lock as the Snappy key

Windows offers no way to remap Caps Lock for one program, so Snappy watches the keyboard directly with a low-level hook, installed while the setting is on (`src/caps_lock/`).

- **Caps Lock's own presses** are swallowed, so Windows never turns Caps Lock on, and reported to the engine.
- **A key pressed while Caps Lock is down** that completes one of Snappy's shortcuts fires that shortcut directly, and is swallowed. Snappy never sends a fake modifier key, so Windows never sees a stray Win key and opens Start. Other keys pressed while Caps Lock is down are swallowed too. Modifier keys pass through.
- **A tap** sends a Caps Lock press marked as Snappy's own. The hook lets it through, so Windows toggles Caps Lock and its light.

**Privacy.** A low-level hook can't ask for just one key. So while the setting is on, Windows calls the hook for every key, on the main thread. The hook ignores everything except Caps Lock and what's pressed while Caps Lock is held, and writes nothing down.

**Nothing to undo.** Nothing in the system is changed, so quitting or a crash leaves Caps Lock as it was.

**Refusing.** Snappy refuses to set this up when the registry's `Scancode Map` already remaps Caps Lock.

## The tray

The tray icon sits in the notification area, drawn from the tray glyph at the size the display's DPI asks for (`src/tray/`). It's added again when Explorer restarts. Its menu is rebuilt each time it opens.

## Workspaces

**Hiding windows.** Windows leaves off-screen windows where they are, so Snappy hides workspace windows entirely outside every display. Hidden windows still show in the taskbar and Alt+Tab, and activating one switches to its workspace.

**Virtual desktops.** Workspaces pause when the user switches to another virtual desktop. To notice, Snappy keeps a window on the desktop workspaces started on, and asks Windows whether that window is on the current desktop (`src/desktop_guard.rs`). It asks again whenever the system hides or shows windows.

## Permissions

Windows asks for none.

## Starting, quitting, and starting at login

- **One daemon.** A named mutex keeps a second daemon from running (`src/instance.rs`). A second launch signals a named event, which opens Snappy's window, and exits.
- **Stopping for a restart.** Windows has no `SIGUSR2`, so `snappy.exe --stop-for-restart` sets another named event and waits for the daemon to exit.
- **Quitting.** A close request to the hidden window, the session ending, and Ctrl+C or closing a console all quit as the tray's **Quit** does. `snappy --uninstall` sends that close request, then waits on the mutex for the daemon to exit.
- **Uninstalling.** `snappy --uninstall` also removes the `Run` value when it starts any copy of Snappy (`src/uninstall.rs`). Caps Lock changes nothing in the system, so there's nothing to give back. The daemon is a windowed program with no console, so for a command it attaches to the terminal it was run from (`src/console.rs`). The Microsoft Store's copy can't be run from outside its package, so `tools/uninstall/uninstall.ps1` quits it with the same close request, then removes the package, which takes its files and startup task with it.
- **`taskkill`** without `/F` can't close the daemon, because it only closes visible windows.
- **Open at login** is a `Run` value under the current user's registry key, holding the daemon's path and `--background` (`src/login_item/`). Installed from the Microsoft Store, Snappy uses the package's startup task instead (below).

## The MSIX package

The Microsoft Store installs Snappy as an MSIX package ([The Microsoft Store](../releasing/microsoft-store.md)). Run from the package, the daemon and the app have the package's identity, which Snappy checks for with `GetCurrentPackageFullName` (`src/package.rs`). Three things change.

**Open at login** is the package's startup task, `SnappyStartup`, which the package declares off. Snappy turns it on and off through Windows' startup task API (`src/login_item/`). It shows under Settings → Apps → Startup, where the user, or a policy, can hold it off or on. Snappy can't override that, so its switch says where to change it.

A startup task can't pass `--background`. So the daemon asks Windows how it was started, and starts quietly when it was the startup task.

**Files.** Windows quietly redirects a package's writes to `%APPDATA%` and `%LOCALAPPDATA%` into a private folder, which Explorer doesn't show at those paths. The debug log's path in Settings would then lead nowhere. So a packaged Snappy keeps every file in the package's own `LocalState` folder instead, laid out as with `SNAPPY_HOME`. Uninstalling removes it.

**The registry.** Writes to the current user's registry go to the package's private copy too, which is why the `Run` value isn't used.

The named mutex and events are the same in both, so a packaged and an unpackaged Snappy never run at the same time. The app starts from next to the daemon, inside the package's install folder, as anywhere else.

## Limitations

- **Windows' own Snap still runs during drags**, because the drag is Windows' own move loop. Dropping at a screen edge or corner lets Windows snap first, and dragging to the top edge can show Snap Layouts over Snappy's zones. Turning off "Snap windows" under Settings → System → Multitasking leaves drags to Snappy.
- **Keys Windows keeps.** The taskbar uses Win with Ctrl, Alt, or Shift and a digit, and on many layouts Ctrl+Alt types AltGr characters. The default Snappy key, Ctrl+Alt+Win, avoids both. A Snappy key of just Ctrl+Alt registers, but on AltGr layouts the keys Snappy uses can stop typing their characters.
- **Windows running as administrator.** While one is in front, Windows keeps its input from Snappy's hooks, so Caps Lock behaves as usual there.
- **What Caps Lock can do.** Since Snappy sends no fake modifier keys, Caps Lock stands in for the Snappy key only in Snappy's own shortcuts. It doesn't count as the drag key, and the shortcut recorder never sees a chord pressed with it.
- **Untested setups.** More than one display, and scales other than 100%, haven't been tested.
