# Code map

What each crate and folder holds, how the crates depend on each other, and where a change belongs.

## Crates

- **`core`**: every rule. Geometry, layouts and zones, the layout editor, drag intent, snapping and settling, swaps, navigation and focus, filling empty zones, shortcuts and their conflicts, the Snappy key and Caps Lock, settings and their validation, undo, workspace sessions, and which windows may be managed. No I/O, threads, or clocks.
- **`platform`**: the traits every adapter implements (`WindowSystem`, `Input`, `HotKeys`, `CapsLock`, `Overlay`, `Permissions`, `DesktopGuard`, `Tray`, `LoginItem`, gathered as `Platform`), the `PlatformEvent`s adapters report, the overlay's shared look (`overlay_style`), the tray glyph, and the daemon's launch flags.
- **`engine`**: runtime state, the order things happen in, the window-operation queue, and every file Snappy writes.
- **`ipc`**: the messages between the daemon and the app, and their framing.
- **`macos`, `windows`, `x11`, `gnome`, `kwin`**: one adapter per desktop. Each one's dependencies sit behind a `cfg` for its OS, so the crate is empty elsewhere. The GNOME Shell extension lives in `crates/gnome/extension` and the KWin script in `crates/kwin/script`.
- **`linux`**: what the Linux adapters share. One daemon per session and its signals, the autostart entry, `/proc` facts, recognizing the session, key codes, the menu, the overlay renderer, display IDs, the tray, XKB and a minimal Wayland client for Caps Lock and overlays, and D-Bus plumbing for the desktop-shell adapters.
- **`wasm`**: a JSON API over `core` for the app.
- **`fake`** and **`live`**: test support. An in-memory desktop for engine tests, and the live test runner and fixture app. Never linked into the daemon.

## Apps and other folders

- **`apps/snappy`**: the daemon.
- **`apps/app`**: the app, a Tauri shell (`src-tauri`) around a React UI (`ui`).
- **`tools`**: scripts for checks (`check.sh`, `cross-check.sh`), the macOS bundle (`macos/`), the icon (`icon/`), and the test sandboxes (`live/`).
- **`packaging/macos`**: the property lists and launch agent that ship inside the macOS app.
- **`assets/icon`**: the app icon, the menu bar and tray glyph, and the files generated from them.

## How the crates depend on each other

```
core ◄── platform ◄── engine ◄── apps/snappy
 ▲          ▲  ▲                    │
 │          │  └── macos, windows, x11, gnome, kwin ◄──┘
 │          │      linux ◄── x11, gnome, kwin
 │          ├── ipc ◄── engine, apps/app/src-tauri
 │          ├── fake        (engine tests)
 │          └── live        (live tests; also uses engine and the adapters)
 └── wasm ◄── apps/app/ui
```

They're separate crates for three reasons:

- **OS dependencies stay contained.** `objc2`, `windows`, `x11rb`, and `zbus` appear only in the adapters, `crates/linux`, and the live harness, so `core` and `engine` build and test on any machine.
- **Core compiles to WebAssembly.** The app's editor and shortcut recorder call the same Rust the daemon uses. A zone drawn in the editor lands exactly where the daemon puts the window, and a conflict the recorder reports is the one the daemon would.
- **The compiler enforces the layering.** Core can't reach the engine, the engine can't call an OS API, and an adapter can't make a policy decision without depending on something it doesn't.

## Where a change goes

- **A rule** (geometry, snapping, swaps, navigation, settings validation, shortcut conflicts, workspace bookkeeping): `crates/core`, with unit tests beside it. [The rules](core.md) lists which module holds which.
- **When something happens, or what state changes as a result**: `crates/engine`.
- **How an OS does something**: its adapter. What more than one Linux adapter needs goes in `crates/linux`.
- **A capability every adapter must provide**: a trait method in `crates/platform`, implemented by each adapter and by `crates/fake`.
- **Something the app can ask for**: a `Command` in `crates/ipc`, handled in `crates/engine/src/commands.rs`.
- **A rule the app needs too**: `crates/core`, exported through `crates/wasm` and `apps/app/ui/src/core/snappyCore.ts`.
- **A page of the app**: its folder under `apps/app/ui/src` (`layouts`, `editor`, `workspaces`, `settings`).
