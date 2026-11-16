# Linux

Snappy has three Linux adapters: [X11](x11.md), [GNOME on Wayland](gnome.md), and [KDE Plasma on Wayland](kde.md). This page covers how one is picked and what they share in `crates/linux`.

## Picking an adapter

At launch, Snappy picks an adapter from the session (`crates/linux/src/session.rs`):

- GNOME on Wayland gets the GNOME adapter.
- KDE Plasma on Wayland gets the KDE adapter.
- X11 sessions get the X11 adapter.
- Any other Wayland session also gets the X11 adapter, through Xwayland. X11 clients can't see other apps' windows there, so live control stays off and the app's window says why.

## What they share

What doesn't depend on the display server lives in `crates/linux`.

### One daemon per session

`instance.rs`. An exclusive lock on `snappy.lock` in `$XDG_RUNTIME_DIR` holds the daemon's process ID. A second launch sends that process `SIGUSR1`, which opens the app's window, and exits.

`SIGTERM`, `SIGINT`, and `SIGHUP` quit like the menu's Quit, so parked windows return first. `SIGUSR2` stops for a restart.

### The login item

`login_item.rs`. The login item is `snappy.desktop` in `~/.config/autostart`, running the daemon with `--background`.

It names the app icon, `io.github.farhadzaidi.Snappy`, which Snappy first installs into the user's icon theme (`app_icon.rs`). So nothing points at a path that moves with the program.

### Recovery keys

A window's recovery key combines its process ID, the process's start time from `/proc`, and the display server's or shell's ID for the window.

### Overlays

The adapters share one renderer (`overlay_image/`), drawn with `tiny-skia` and a built-in stroke font for numbers and captions, so no font files are needed. X11 and KDE draw with it. For GNOME, the same shapes are sent as data (`scene.rs`) and drawn in GNOME Shell.

### The tray

`tray.rs`, `menu.rs`. The tray is a StatusNotifierItem with the tray glyph in white. Desktops need a tray host to show it. GNOME gets a top-bar menu of its own instead.

### Caps Lock

`xkb/`. Caps Lock as the Snappy key goes through XKB on every Linux desktop. Snappy reads which options and layouts already remap Caps Lock, and, on Wayland, uses an XKB option of its own, `snappy:caps`, written to `~/.config/xkb`.

### Wayland

`wayland/`. A minimal Wayland client, enough to read the keymap the compositor hands every client and, on KDE, to draw overlays and read modifier keys.

### The desktop-shell adapters

For GNOME and KDE, `dbus.rs` holds the D-Bus plumbing, and `windows.rs` the rules for which of the windows a shell reports Snappy manages.

## Across all three

Apps draw their tabs inside one window on Linux, so every Linux adapter reports no tab groups. No Linux desktop asks for a permission.
