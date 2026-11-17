# Linux

This page is for anyone working on Snappy for Linux. Snappy has three Linux adapters, for [X11](x11.md), [GNOME on Wayland](gnome.md), and [KDE Plasma on Wayland](kde.md). This page covers how Snappy picks one, and the code they share in `crates/linux`.

## Picking an adapter

Wayland doesn't let ordinary programs move other apps' windows. So on Wayland, Snappy needs help from inside the desktop itself, and that's different for each desktop. At launch, Snappy looks at the session (`crates/linux/src/session.rs`) and picks:

- the GNOME adapter for GNOME on Wayland;
- the KDE adapter for KDE Plasma on Wayland;
- the X11 adapter for an X11 session.

Any other Wayland session also gets the X11 adapter, running through Xwayland. There, X11 programs can't see other apps' windows, so Snappy can't act on them. Snappy's window says why.

## What the three share

Everything that doesn't depend on the display server is in `crates/linux`.

### One daemon per session

An exclusive lock on `snappy.lock` in `$XDG_RUNTIME_DIR` holds the running daemon's process ID (`instance.rs`). A second launch sends that process `SIGUSR1`, which opens Snappy's window, and exits.

`SIGTERM`, `SIGINT`, and `SIGHUP` quit as the menu's **Quit** does, so hidden workspace windows come back first. `SIGUSR2` stops for a restart, leaving them hidden ([Quitting, or stopping for a restart](../daemon-and-app.md#quitting-or-stopping-for-a-restart)).

### Starting at login

The login item is `snappy.desktop` in `~/.config/autostart`, which runs the daemon with `--background` (`login_item.rs`).

It names the app icon, `io.github.farhadzaidi.Snappy`, rather than a file path, so it doesn't break when the program moves. Snappy installs the icon into the user's icon theme first (`app_icon.rs`).

### Recognizing windows after a restart

A window's recovery key combines its process ID, when that process started, read from `/proc`, and the display server's or shell's ID for the window.

### Drawing the zones

All three adapters draw with one renderer (`overlay_image/`), built on `tiny-skia`, with a built-in stroke font for numbers and captions, so no font files are needed. X11 and KDE draw images with it. GNOME draws the same shapes inside GNOME Shell, so for GNOME they're sent as data instead (`scene.rs`).

### The tray

The tray icon is a StatusNotifierItem with the tray glyph in white (`tray.rs`, `menu.rs`). A desktop needs a tray host to show it. GNOME gets a top-bar menu of its own instead.

### Caps Lock

Caps Lock as the Snappy key goes through XKB, the keyboard layer every Linux desktop shares (`xkb/`). Snappy reads which of the user's options and layouts already remap Caps Lock, so it can refuse. On Wayland, it adds an XKB option of its own, `snappy:caps`, written to `~/.config/xkb`.

### A small Wayland client

`wayland/` is a minimal Wayland client. It reads the keymap the compositor hands every program, and, on KDE, draws the overlay and reads the modifier keys.

### Talking to GNOME and KDE

For GNOME and KDE, `dbus.rs` holds the D-Bus plumbing, and `windows.rs` the rules for which of the windows the desktop reports Snappy manages.

## True of all three

Linux apps draw their tabs inside one window, so no Linux adapter reports tab groups. No Linux desktop asks Snappy for a permission.
