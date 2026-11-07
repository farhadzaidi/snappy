# Temporary spec: GNOME on Wayland

This is a working spec for adding GNOME on Wayland support. Delete it once the support lands, after folding what's still true into `platforms.md`, `architecture.md`, `development.md`, and `testing.md`.

Read `AGENTS.md`, `README.md`, and every file in `docs/` before starting. Everything there applies: clean code, no `unwrap`/`expect` outside tests, comments that explain why, descriptive test names, platform code only in adapters.

## Ground rules

- Work on a branch named `gnome-adapter`, made from the latest `main`. Rebase onto `main` before handing it over, because `main` keeps moving.
- Commit as the owner. In this clone: `git config user.name "Syed Zaidi"` and `git config user.email "zaidi.farhad03@gmail.com"`. Never add `Co-Authored-By` or any attribution lines.
- Commit locally in logical steps. Push only the `gnome-adapter` branch, and only when the owner asks.
- Never run a development build of the daemon, `snappy-live`, or `snappy-fixture` in the owner's logged-in session, and never enable the extension there unless asked. Develop and test in a nested or headless GNOME Shell (see Testing).

## Goal

Snappy works on GNOME Shell under Wayland, GNOME 46 (Ubuntu 24.04) and newer, including GNOME 48 and 49, with the same behavior as macOS, Windows, and X11:

- drag to snap, with the zone overlay and the swap and stack captions;
- Snappy-key shortcuts: zones, moving between zones, workspace steps, the nearest empty zone, and undo and redo when assigned;
- swaps, with the "When the zone is taken" setting;
- workspaces, hiding and showing windows seamlessly;
- a menu equivalent to the tray menu;
- the settings app, open at login, and single instance.

The live suite should pass in full.

Out of scope: KDE Plasma, wlroots compositors (Sway, Hyprland), and other Wayland desktops. Those keep today's "can't control windows in a Wayland session yet" message.

## Why an extension

Wayland doesn't let one client move another's windows, watch global input, or own global shortcuts. On GNOME only GNOME Shell can, and code runs inside GNOME Shell as an extension. That's why GNOME window tilers ship as extensions.

Snappy stays a separate daemon with all of its logic. The extension is only the adapter's hands: it does what the daemon asks and reports what happens, over D-Bus. To the engine it's one more adapter, like `crates/macos`, `crates/windows`, and `crates/x11`. Core and the engine don't change, except for anything the adapter genuinely needs, which must be minimal and justified.

## Architecture

### 1. The GNOME Shell extension: `apps/gnome-extension/`

- **Format:** the ESM extension format (GNOME 45 and newer), with UUID `snappy@farhadzaidi.github.io` and `shell-version` covering 46, 47, 48, and 49.
- **Interface:** it exports a session D-Bus interface, for example `io.github.farhadzaidi.Snappy.Shell` at `/io/github/farhadzaidi/Snappy/Shell`, with methods for requests and signals for events. Keep it thin: no snapping rules in JavaScript.

What it covers:

- **Windows** (`Meta.Window`, through `global.display` and `global.workspace_manager`): list the managed normal windows and give each window's frame (`get_frame_rect`), pid, app ID (`Shell.WindowTracker`, else `WM_CLASS`), stable sequence, and minimized, full-screen, and maximized state. Never read titles.
  - Move and resize with `move_resize_frame`, un-maximizing first.
  - Focus with `activate`.
  - The window at a point: the topmost window on the active workspace whose frame contains it.
  - Both Wayland and XWayland windows are `Meta.Window`s; treat them the same.
- **Events,** as signals: window created, destroyed, moved or resized, and focus changed; app launched and terminated; display changes; workspace changes.
- **Drags:** `global.display` `grab-op-begin` and `grab-op-end` for window moves. While a move is in progress, report the pointer position and modifiers (`global.get_pointer()`), Escape, and modifier changes, all in event order. Report modifiers as they were when the button came up; X11 had a bug here (`platforms.md`).
- **Shortcuts:** `global.display.grab_accelerator`, with `accelerator-activated` reported as a signal carrying Snappy's ID. The daemon only ever sends expanded chords, never the Snappy key itself.
  - A grab that returns no action means the chord is taken: that's the availability probe. Release the probe grab right away.
  - Release everything on disable.
- **Overlay:** draw the scene the daemon sends (zones, numbers, highlight, caption, source outline) as non-reactive actors above windows, one per monitor. It must be click-through and must never take focus. Match the look the other adapters draw; `crates/core` and the other overlays define the colors and sizes.
- **Menu:** a top-bar indicator in place of a tray icon, holding the same items as the tray menu (`TrayMenu`). Menu choices go back to the daemon as signals.
- **Workspace parking:** a dedicated GNOME workspace for parked windows, the same idea as X11's parking desktop.
  - Hide by moving a window to it (`change_workspace`) without changing its frame. Show by moving it back to the active workspace.
  - With dynamic workspaces, GNOME may remove an empty workspace or add its own trailing one. Handle both, and keep the parking workspace last.
  - Report each window's `on_parking_desktop` state.
- **Displays:** monitor geometry and work areas (`workspace.get_work_area_for_monitor`), plus the scale. Stable display IDs may come from Mutter's `org.gnome.Mutter.DisplayConfig` (connector, vendor, product, and serial), read by the Rust side.

### 2. The adapter crate: `crates/gnome`, package `snappy-gnome`

- Implements every trait in `crates/platform` by calling the extension's D-Bus interface with `zbus`. Compile it only on Linux, gated the way the other adapters are.
- Mirrors `crates/x11`'s structure, including a `reference` module for the live harness (displays, key codes, and the window list).
- Uses `ParkingPolicy::Desktop` and `WindowState.on_parking_desktop`, as X11 does.
- The recovery key is the pid, the process start time from `/proc`, and the window's stable sequence.
- Coordinates are Mutter's logical layout pixels. Document how they relate to the work area and scale.
- Capabilities: no permission step. Live control is available when the extension answers.
- The Linux-generic pieces X11 already has (the single-instance lock and `SIGUSR1`, `SIGTERM` quitting, the XDG autostart login item, the `/proc` start time) should move into one small shared crate, for example `crates/linux`, used by both adapters. Don't copy them.

### 3. Choosing the adapter: `apps/snappy`

- On a Wayland session with GNOME (`XDG_SESSION_TYPE=wayland`, `XDG_CURRENT_DESKTOP` containing `GNOME`), use `snappy-gnome`.
- On an X11 session, keep `snappy-x11`.
- Other Wayland desktops keep the current unsupported message.

### 4. Installing the extension

- The daemon installs the bundled extension into `~/.local/share/gnome-shell/extensions/<uuid>/` and enables it (`gnome-extensions enable`), or tells the user exactly what to do.
- Wayland only loads a newly installed extension after logging out and back in. The settings app must explain that, and say plainly when the extension is missing, disabled, or the wrong version.
- Version the D-Bus interface, so an old extension and a new daemon detect the mismatch and say so.
- Update the Linux section of `README.md`'s install steps.

## Testing

- **Unit tests** in `crates/gnome` against a fake D-Bus service (`zbus` can serve one in-process).
- **Integration tests** against a real GNOME Shell, run only when an opt-in environment variable is set, as `crates/x11` does. Use a headless shell inside its own D-Bus session:

  ```sh
  dbus-run-session -- gnome-shell --headless --wayland --virtual-monitor 1920x1080
  ```

  For looking at it by hand, use `gnome-shell --nested --wayland`, or `--devkit` on GNOME 49.
- **The live suite** gets a `tools/live/gnome/live.sh`, analogous to the other platforms' scripts, that starts a headless shell with the extension installed and runs `snappy-live` against the daemon.
  - **Input:** Wayland clients can't inject input, so give the extension a test-only interface, enabled only when the shell is started with an environment variable such as `SNAPPY_GNOME_TEST=1`. It uses a Clutter virtual input device for pointer and keyboard events, and reports window frames as ground truth. Mutter's RemoteDesktop D-Bus API is an alternative if it works headless.
  - **Fixture windows** open normally. On Wayland they can't place or read their own positions, so the harness places and reads them through that test interface.
  - **All scenarios pass.** Report any that can't apply, with the reason.
- `tools/check.sh` still passes, and covers the new crate on Linux.

## Docs

When done, move what's true into `docs/platforms.md` (a GNOME section next to Linux X11), `docs/architecture.md` (the adapter list and the extension), `docs/development.md` (building and installing the extension), and `docs/testing.md` (the GNOME live suite). Then delete this file.

## Hand-off report

- What's implemented, and anything stubbed.
- Unit, integration, and live results.
- The GNOME versions verified.
- Known limitations.
