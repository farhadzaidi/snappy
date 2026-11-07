# Platforms

Each adapter implements the traits in `crates/platform/src/lib.rs` and nothing else: it reports events, answers questions about windows, and carries out moves. Policy lives in `core` and the engine. Every adapter splits the same way:

- `App`, on the main thread, owns the OS event loop and everything that must live there: monitors or hooks, hotkeys, overlays, the tray.
- A platform handle (`MacPlatform`, `WindowsPlatform`, `X11Platform`) lives on the engine thread. It reads and moves windows directly and hands main-thread work to `App`.

Callbacks on the main thread only forward events. They never call window APIs, so a hung app can't freeze input or drawing.

Each adapter keeps its user-facing refusals in `src/messages.rs`, worded the same on every platform where the situation is the same, for example "Choose a standard application window." and "Leave full screen before tiling this window."

## macOS

`crates/macos`. Public APIs only; Accessibility and Carbon hotkeys through hand-written FFI in `src/ffi/`.

**Finding and moving windows.** `src/windows/` wraps `AXUIElement`s (`src/ax/`). A `WindowId` maps to a retained element. The window under the pointer comes from `AXUIElementCopyElementAtPosition`, walking up at most 12 parents to the window. Frames are `AXPosition` and `AXSize`, read together; writes set one attribute at a time and first check that access is still granted, the app runs, and the window isn't full screen. Visible windows are each app's `AXWindows` matched against the window server's on-screen list (`src/windows/server.rs`, `src/occupancy.rs`), which reads only owner, layer, and bounds, never names. Messaging timeouts are in `src/thresholds.rs`.

**Eligibility** (`src/eligibility.rs`). Regular apps only, plus the accessory settings app (`local.snappy.settings`); subrole `AXStandardWindow`; not full screen; position and size settable.

**Observing.** `src/observers/`: per app, window-created and focus-changed notifications; per tracked window, destroyed, moved, resized, and deminiaturized. NSWorkspace activation, launch, termination, Space change, and sleep notifications, and screen parameter changes. Registering an observer is a round trip to the app, so it never happens on the main thread.

**Drag detection** (`src/monitors.rs`). Global and local NSEvent monitors for left mouse down and up. While drag monitoring is on, they add left-mouse-dragged and flags-changed. Modifiers come from each event's own flags, so the mouse-up carries those held when the button came up. NSEvent monitors need only Accessibility; a keyboard event tap would add an Input Monitoring prompt, so none is used.

**Shortcuts** (`src/hotkeys.rs`, `src/keycodes.rs`). Carbon `RegisterEventHotKey`, with W3C key codes mapped to virtual key codes. The availability probe registers the chord under a separate signature and releases it at once. Carbon accepts chords that macOS itself consumes first, such as Mission Control's Control-arrows, so `src/system_shortcuts.rs` also reads the enabled entries of `com.apple.symbolichotkeys` and reports those chords as taken. Accessibility's increase and decrease contrast shortcuts, Control-Option-Command with period and comma, ship turned off; a user who turns them on loses the default previous move-window step (period) to them, and the probe says so.

**Overlays** (`src/overlay/`). One borderless, non-activating, click-through `NSPanel` per display at the floating level, joining every Space and full-screen apps. Content is Core Animation layers: one shape and two text layers per zone, redrawn only where they changed. Panels are created ahead of the first drag and kept. Zones use the system accent color.

**Menu bar** (`src/tray/`). An `NSStatusItem` whose `NSMenu` is rebuilt each time it opens (`src/delegate.rs`): the status heading, Open Snappy…, a layout submenu per display, Workspaces, and Quit.

**Workspaces.** Corner-sliver parking: windows move to an exposed lower corner, leaving about a point on screen, because macOS pulls fully off-screen windows back. The desktop guard (`src/desktop_guard.rs`) keeps an invisible, stationary 1×1 window per display and checks `isOnActiveSpace` to tell whether the user changed Spaces.

**Permissions** (`src/permissions.rs`). `AXIsProcessTrusted` answers whether access is granted; the request shows the system prompt. A grant is detected by polling once a second, only while snapping is on and access is missing. A revocation is detected without polling, from the `com.apple.accessibility.api` distributed notification and from Accessibility calls failing with `kAXErrorAPIDisabled`.

**Single instance and quitting.** The daemon holds a lock on `daemon.lock` in its state folder; a second launch exits, and opening Snappy.app reaches the running daemon as a reopen event, which opens the settings window. `SIGTERM` is delivered to a dispatch source on the main queue (`src/signals.rs`) and handled like Quit, so parked windows return first. Both programs use the accessory activation policy: no Dock icon, no Command-Tab entry.

**Login item** (`src/login_item.rs`). `SMAppService` registers the launch agent bundled at `Contents/Library/LaunchAgents/local.snappy.agent.plist`, which runs the daemon with `--background`.

**Known quirks.**

- Chrome blocks bounds changes while enhanced accessibility (`AXEnhancedUserInterface`) is on. `src/windows/chrome.rs` turns it off for one bounds change, unless VoiceOver or Switch Control needs it, turns it back on, and waits 25 ms before rereading the frame. Outstanding changes are undone at quit.
- Terminals size in character cells, and some apps refuse a size until the whole window is on the destination display; the settle algorithm in `core` handles both.
- Apps that move their own parked windows back on screen are parked again.

**Building.** `tools/macos/bundle.sh` builds the UI and both binaries and assembles `dist/Snappy.app` with the settings app inside it at `Contents/Helpers/Snappy Settings.app`. See [development.md](development.md#building-and-bundling) for signing.

## Windows

`crates/windows`, on Win32 through the `windows` crate. Coordinates are physical pixels; each display's scale is its effective DPI over 96. The daemon declares per-monitor DPI awareness v2 before creating any window, and the C runtime is linked statically (`.cargo/config.toml`).

**The main thread** (`src/app.rs`). A hidden top-level window and its message loop. It isn't message-only, because message-only windows receive no broadcasts, and Snappy needs display, work-area, power, session-end, and taskbar-restart broadcasts. The engine thread hands it work through a dispatcher; hotkey registration and the desktop guard's queries wait up to 2 seconds for an answer.

**Finding and moving windows** (`src/managed/`, `src/frames.rs`). The visible frame is `DWMWA_EXTENDED_FRAME_BOUNDS`; writes add the difference to `GetWindowRect`, whose invisible resize borders are wider. Writes use `SetWindowPos` with `SWP_ASYNCWINDOWPOS`, so a hung window can't block the engine, then reread for up to 50 ms until the window changes. A maximized window is restored first with `ShowWindowAsync`. `WindowFromPoint` finds the window under the pointer, `GetForegroundWindow` the focused one, and `EnumWindows` the rest. HWNDs are reused, so a `WindowId` adds a serial that changes when a handle shows up with a different process (`src/identity.rs`).

**Eligibility** (`src/eligibility.rs`), in order: full screen (covers its monitor without a caption); not a standard window (hidden, owned, cloaked, tool window, or no caption); elevated (a higher integrity level than Snappy, which Windows won't let it move); fixed size.

**Observing** (`src/events.rs`). Out-of-context WinEvent hooks that skip Snappy's own process: foreground changes, move/size start and end, minimize end, show and destroy, and cloak changes. `EVENT_OBJECT_LOCATIONCHANGE` fires constantly, so it is hooked only for processes with a window the engine asked about.

**Drag detection** (`src/input_hooks.rs`). Move/size start becomes `PointerDown`, so ordinary clicks cost nothing. During a drag, location changes plus `GetCursorPos` become `PointerDragged`. While drag monitoring is on, `WH_KEYBOARD_LL` reports modifier changes and `WH_MOUSE_LL` reports the button release. The release comes from the mouse hook, not the end of the move loop: by the time the loop ends, the user may have let go of the modifier.

**Shortcuts** (`src/hotkeys.rs`, `src/keys.rs`). `RegisterHotKey` on the hidden window. W3C codes become scan codes; layout-dependent keys get their virtual key from the foreground window's keyboard layout, and shortcuts are registered again when a foreground change brings a different layout. The availability probe registers the chord under a spare ID and releases it; a refusal means another app or Windows holds it. Snappy's own registrations count as free.

**Overlays** (`src/overlay/`). One layered, click-through, topmost, non-activating tool window per display. Snappy's own rasterizer draws zones into a DIB, GDI renders the numbers once into cached masks, and `UpdateLayeredWindow` hands the pixels to DWM, which keeps them, so the DIB is freed straight away. The accent is DWM's colorization color.

**Notification area** (`src/tray/`). A `Shell_NotifyIconW` icon drawn at runtime, added again when Explorer restarts. Clicking opens a menu built with `TrackPopupMenuEx` from the latest content.

**Workspaces.** Off-screen parking: Windows doesn't clamp positions, so a parked window stays fully outside every display. Parked windows still appear in the taskbar and Alt+Tab; activating one switches to its workspace. The desktop guard (`src/desktop_guard.rs`) records the current virtual desktop from a visible window on it (`GetWindowDesktopId`) and asks `IsWindowOnCurrentVirtualDesktop`, rechecking when windows are cloaked or uncloaked.

**Permissions.** None. Windows of elevated apps are refused instead.

**Single instance and quitting** (`src/instance.rs`). The named mutex `Local\Snappy.Daemon`; a second launch signals the event `Local\Snappy.OpenSettings` and exits. A close request to the hidden window, the end of the session, and a console's Ctrl+C or close all ask the engine to quit, and the session-end and console paths wait for parked windows to return. `taskkill` without `/F` can't close the daemon, since it only closes visible windows.

**Login item** (`src/login_item.rs`). The value `Snappy` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, holding the quoted daemon path and `--background`.

**Known quirks.**

- Windows Snap still runs during drags, because the drag is the system's own move loop. Dropping inside a zone works, but dropping at a screen edge or corner lets Windows Snap act first, and dragging to the top edge can show the Snap Layouts flyout over Snappy's overlay. Users who want only Snappy can turn off "Snap windows" under Settings, System, Multitasking.
- The taskbar owns Win with Ctrl, Alt, or Shift plus a digit, and Ctrl+Alt without Win types AltGr characters on many layouts. The Snappy key, Ctrl+Alt+Win, avoids both. Win+period and Win+semicolon (the emoji panel) and Win+slash (IME reconversion) don't collide with the workspace steps, which add Ctrl and Alt.
- While an elevated window is in front, Windows drops injected input; the live tests wait for such windows to close.
- Two displays, scales other than 100%, and mixed DPI are untested, since the test VM has one display.

**Building.** Cross-compiled on the Mac with `cargo-xwin` for `aarch64-pc-windows-msvc` and copied into the test VM ([development.md](development.md#windows)). The settings app's Tauri shell isn't part of these builds.

## Linux (X11)

`crates/x11`, on `x11rb` (pure Rust, no libxcb). What doesn't depend on the display server lives in `crates/linux` and is shared with any other Linux adapter: the single-instance lock and signals, the autostart entry, `/proc` facts, recognizing the session, physical key codes, the menu's structure, the overlay's look, and stable display IDs. It targets EWMH-compliant stacking window managers and has been verified against Xorg with Xfwm4, with and without its compositor.

**Threads** (`src/app.rs`). X has no main-thread rule. The event thread reads X events on one connection; the engine thread makes window requests on a second. Key grabs and input selections go out on the event connection so their events arrive where they are read. An overlay thread draws and drives the tray, and the tray's D-Bus service runs inside `ksni`.

**Wayland** (`crates/linux/src/session.rs`). When `XDG_SESSION_TYPE` is `wayland`, or `WAYLAND_DISPLAY` is set without an X11 session type, X11 clients can't see other apps' windows, so live control and workspaces stay off and the settings app says why. With no X server at all the daemon exits with a message.

**Finding and moving windows** (`src/windows/`, `src/frames.rs`, `crates/linux/src/clients.rs`). Managed windows are `_NET_CLIENT_LIST`. Frames include `_NET_FRAME_EXTENTS` decorations and exclude client-side shadows from `_GTK_FRAME_EXTENTS`. Writes are `_NET_MOVERESIZE_WINDOW` client messages; maximized windows have their maximized state removed first. The window under the pointer is the topmost visible client in `_NET_CLIENT_LIST_STACKING`. Recovery keys use `_NET_WM_PID` and the process start time from `/proc`; app identifiers come from `WM_CLASS`.

**Eligibility** (`src/eligibility.rs`): not full screen; a top-level client without `WM_TRANSIENT_FOR` whose window type is normal or unset; and, when `_NET_WM_ALLOWED_ACTIONS` is present, move and resize allowed (skipped for maximized and minimized windows, which window managers report without them).

**Observing.** Moves from `ConfigureNotify` on tracked clients only, rather than on the root window, which would wake Snappy for every frame of every window. Creation and destruction from changes to `_NET_CLIENT_LIST`; focus from `_NET_ACTIVE_WINDOW`; desktop changes from `_NET_CURRENT_DESKTOP`; displays from RandR, `_NET_WORKAREA`, and `Xft.dpi`. `WillSleep` isn't reported.

**Drag detection** (`src/input.rs`). XInput 2 raw button and motion events on the root window, which arrive even while the window manager grabs the pointer for a move, and never grab anything themselves. Raw key events are selected only while drag monitoring is on, and serve only to follow the modifier keys: the event thread follows them in event order (`HeldModifiers`), starting from the keys `QueryKeymap` reports down when monitoring turns on and mapping keycodes to modifiers through the modifier mapping, so a release reports the modifiers held when the button came up. Asking the server instead (`QueryPointer`) would answer for the time the release is handled, after a drag modifier let go right behind the button.

**Shortcuts** (`src/hotkeys.rs`, `src/keys/`). `XGrabKey` on the root window, once per combination of Caps Lock and Num Lock. A `BadAccess` error means another client holds the chord, which doubles as the availability probe. On evdev keymaps, W3C codes map straight to physical keycodes, so shortcuts follow the physical key on every layout. Detectable auto-repeat is turned on so a held shortcut fires once.

**Overlays** (`src/overlay/`). One override-redirect window per display with a 32-bit ARGB visual and an empty XFixes input region, so it is click-through. `tiny-skia` renders the scene in bands into the window's background pixmap, and a built-in stroke font draws the numbers and the "Swap" and "Stack" captions, so no font files are needed. Without a compositing manager, the Shape extension cuts each overlay down to zone borders and boxes behind the numbers and captions. X11 has no readable accent color, so zones use `#007AFF`.

**Tray** (`src/tray/`). A StatusNotifierItem through `ksni`. Desktops need a tray host to show it; GNOME needs the AppIndicator extension.

**Workspaces.** Desktop parking (`src/windows/desktops.rs`): binding adds a native desktop named "Snappy" after the user's own and parks windows there, leaving their frames untouched. Window managers such as Xfwm4 keep part of every window on screen, so moving windows off screen can't hide them. The desktop's number is kept in a root window property, so a daemon restarted after a crash finds it again. The desktop guard compares `_NET_CURRENT_DESKTOP` with the desktop workspaces started on; window managers without desktops count as one.

**Permissions.** None.

**Single instance and quitting** (`crates/linux/src/instance.rs`). An exclusive lock on `snappy.lock` in `$XDG_RUNTIME_DIR` (else the state folder) holding the daemon's process ID. A second launch sends that process `SIGUSR1`, which opens the settings window, and exits. `SIGTERM` quits like the tray's Quit, so parked windows return first.

**Login item** (`crates/linux/src/login_item.rs`). `snappy.desktop` in `$XDG_CONFIG_HOME/autostart` (default `~/.config/autostart`), running the daemon with `--background`.

**Known quirks.**

- Only Xfwm4 has been verified. Mutter and KWin may differ in decorations, multi-monitor moves, and tray behavior.
- Desktop bindings that use a subset of Snappy's modifiers don't collide, because grabs match modifiers exactly. A chord the desktop has grabbed is reported unavailable. GNOME, KDE Plasma, and Xfce bind nothing to Ctrl+Alt+Super by default; their nearest bindings, such as Plasma's Meta+period emoji picker, use fewer modifiers.

**Building.** In the Linux development container: `tools/live/linux/run.sh cargo build -p snappy` ([development.md](development.md#linux)).
