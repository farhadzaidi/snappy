# Adapters

This page is for anyone changing how Snappy works on a particular desktop. It explains what every adapter does, how each one is laid out, and how the desktops compare. The pages for each desktop go into the details.

## What an adapter does

An adapter connects the engine to one desktop. It does three things:

- **reports what happens**, such as a mouse press, a window moving, or a display being added;
- **answers questions**, such as which window is under the pointer, or what a window's frame is;
- **carries out requests**, such as moving a window, registering a shortcut, or drawing the zones.

It never decides anything. Whether a drop snaps, which zone a shortcut means, which windows Snappy manages: all of that is in core and the engine ([How Snappy is built](../overview.md#who-decides-what)). What an adapter must be able to do is a set of traits in `crates/platform`, and every adapter implements all of them.

## Two halves, on two threads

Each adapter is split in two, because most OSes insist that their event loop runs on the main thread, while the engine runs on a thread of its own.

**The main-thread half**, called `App` in every adapter, runs the OS's event loop and everything that must live there: input hooks or monitors, shortcuts, the overlay, the tray. Its callbacks only turn what happened into events and pass them on. They never call into other apps' windows, so a hung app can't freeze input or drawing.

**The engine-thread half**, such as `MacPlatform` or `X11Platform`, is what the engine calls. It reads and moves windows itself, and hands anything that has to happen on the main thread over to `App`.

## Laid out alike

Where two adapters do the same job, they use the same module name for it, such as `input`, `hotkeys`, `overlay`, `caps_lock`, `instance`, or `login_item`. So once you know one adapter, you can find your way around the others.

Each adapter keeps the messages it shows users in `messages.rs`. The same situation reads the same on every desktop.

Which windows Snappy manages is decided in core, from facts each adapter reads ([Which windows Snappy will manage](../engine.md#which-windows-snappy-will-manage)). So the refusals read the same everywhere too.

What tests need to read from an adapter, such as its view of displays and key codes, is in its `reference` module, compiled only with its `reference` feature ([Testing on a real desktop](../sandboxes.md#adapter-integration-tests)).

## How the desktops compare

| | macOS | Windows | X11 | GNOME on Wayland | KDE Plasma on Wayland |
|---|---|---|---|---|---|
| Moves windows through | Accessibility | Win32 | the window manager (EWMH) | Snappy's GNOME Shell extension | Snappy's KWin script |
| Shortcuts | Carbon hotkeys | `RegisterHotKey` | `XGrabKey` | the extension | KDE's shortcut service |
| Draws the zones in | a panel per display | a layered window per display | a window per display | GNOME Shell itself, through the extension | a layer-shell surface per display |
| Hides workspace windows | at a screen corner | off screen | on a desktop of Snappy's | on a workspace of Snappy's | on a desktop of Snappy's |
| Caps Lock as the Snappy key | a key remapping and an event tap | a keyboard hook | a changed XKB key | an XKB option | an XKB option |
| Permission it needs | Accessibility | none | none | none | none |

## Each desktop

- [macOS](macos.md)
- [Windows](windows.md)
- [Linux](linux.md): how one of the three Linux adapters is picked, and what they share.
  - [X11](x11.md)
  - [GNOME on Wayland](gnome.md)
  - [KDE Plasma on Wayland](kde.md)
