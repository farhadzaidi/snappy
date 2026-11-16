# Adapters

An adapter connects the engine to one desktop. Every adapter implements the traits in `crates/platform` and nothing more: it reports events, answers questions about windows, and carries out moves. Every decision stays in core and the engine ([Overview](../overview.md)).

## How each adapter is split

- **`App`** runs on the main thread. It owns the OS event loop and whatever must live there: hooks or monitors, shortcuts, overlays, the tray.
- **A platform handle** (`MacPlatform`, `WindowsPlatform`, `X11Platform`, `GnomePlatform`, `KWinPlatform`) lives on the engine thread. It reads and moves windows, and hands main-thread work to `App`.

Callbacks on the main thread only forward events. They never call into other apps' windows, so a hung app can't freeze input or drawing.

## Laid out alike

The adapters use the same module names for the same jobs where they have them: `input`, `hotkeys`, `overlay`, `caps_lock`, `instance`, `login_item`, and `messages`. Each adapter keeps its user-facing messages in `messages.rs`, and the same situation reads the same on every platform.

Which windows Snappy manages is decided once, in `crates/core/src/eligibility.rs`, on facts each adapter reads from its OS. So the refusals read the same everywhere.

What tests need to read from an adapter, such as Snappy's own view of displays and key codes, lives in its `reference` module, built only with its `reference` feature ([Adapter tests](../adapter-tests.md)).

## How they compare

| | macOS | Windows | X11 | GNOME on Wayland | KDE Plasma on Wayland |
|---|---|---|---|---|---|
| Moves windows through | Accessibility | Win32 | EWMH | GNOME Shell extension | KWin script |
| Shortcuts | Carbon hotkeys | `RegisterHotKey` | `XGrabKey` | the extension | KDE's shortcut service |
| Overlay | `NSPanel` per display | layered window per display | ARGB window per display | St actors in GNOME Shell | layer-shell surface per display |
| Hides workspace windows | at a screen corner | off screen | on a desktop of Snappy's | on a workspace of Snappy's | on a desktop of Snappy's |
| Caps Lock as the Snappy key | HID key mapping + event tap | low-level keyboard hook | XKB key definition | XKB option | XKB option |
| Permission | Accessibility | none | none | none (the extension) | none |

## Each platform

- [macOS](macos.md)
- [Windows](windows.md)
- [Linux](linux.md), what the three Linux adapters share and how one is picked:
  - [X11](x11.md)
  - [GNOME on Wayland](gnome.md)
  - [KDE Plasma on Wayland](kde.md)
