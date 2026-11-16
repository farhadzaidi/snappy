# Snappy

Snappy arranges your windows into zones. Pick a layout for each display, then drag a window with a key held, or press a shortcut, and it snaps into place. It runs quietly in the background on macOS, Windows, and Linux, with a menu bar or tray icon and a settings window you open when you need it.

Snappy never uses the network, and never reads window titles, window contents, or what you type.

## What it does

- **Layouts and zones.** Eight ready-made layouts (halves, thirds, quarters, center stage, and more), or draw your own in the layout editor. Each display has its own layout.
- **Drag to snap.** Hold Shift while dragging a window by its title bar, and the zones appear. Drop the window on one to snap it there; let go of Shift first to cancel.
- **Keyboard shortcuts.** Every default shortcut is the *Snappy key* plus one other key. The Snappy key is Control+Option+Command on macOS and Ctrl+Alt+Win (Super on Linux) elsewhere, and you can choose other modifiers. You can also have Caps Lock act as the Snappy key while you hold it.
- **Swaps.** Moving a window into a zone that's taken trades places with the window there, or stacks on top if you prefer.
- **Workspaces.** Groups of windows, one shown at a time. Switching hides the others and brings the chosen group back exactly where it was.
- **Selection.** Select several windows to move them together, or take them along to another workspace.
- **Undo and redo** for window moves.

| Snappy + | Does |
|---|---|
| ← → ↑ ↓ | Move the focused window to the neighboring zone |
| I J K L | Focus the next window up, left, down, or right |
| Space | Move the focused window, or the selection, into the nearest empty zone |
| Return | Select or unselect the focused window |
| `\` | Clear the selection |
| 1 to 9 | Switch to that workspace |
| `;` `'` | Previous or next workspace |
| `.` `/` | Send the focused window, or the selection, to the previous or next workspace |

Every shortcut can be changed or cleared in Settings, and zones, layouts, and workspaces can have shortcuts of their own.

## Platforms

| System | Notes |
|---|---|
| macOS 14 or later | Needs Accessibility permission. |
| Windows 11 | Windows of apps running as administrator can't be moved. |
| Linux on X11 | GNOME on Xorg, KDE Plasma on X11, Xfce, Cinnamon, and other EWMH window managers. |
| Linux, GNOME on Wayland | GNOME 46 to 49, through a GNOME Shell extension Snappy installs itself. |
| Linux, KDE Plasma on Wayland | Plasma 6.8, through a KWin script Snappy loads itself. |

Other Wayland desktops aren't supported yet.

## Install

There are no prebuilt releases yet, so Snappy is built from source. You need [Rust](https://rustup.rs), [Node.js](https://nodejs.org) with pnpm 9, and the WebAssembly tools for the settings window:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130
npm install -g pnpm@9

git clone https://github.com/farhadzaidi/snappy.git
cd snappy
pnpm -C apps/settings/ui install
```

### macOS

```sh
tools/macos/signing-identity.sh    # once: a local signing identity, so the Accessibility grant survives updates
tools/macos/bundle.sh              # builds dist/Snappy.app
cp -R dist/Snappy.app /Applications/
open /Applications/Snappy.app
```

Allow Snappy in System Settings → Privacy & Security → Accessibility when it asks.

### Linux

First install the settings window's libraries. On Debian or Ubuntu:

```sh
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev
```

On other distributions, install the equivalents listed in the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#linux). Then:

```sh
pnpm -C apps/settings/ui build
cargo build --release -p snappy -p snappy-settings
install -Dm755 target/release/snappy target/release/snappy-settings -t ~/.local/bin/
~/.local/bin/snappy
```

- **GNOME on Wayland:** the first launch installs Snappy's GNOME Shell extension. GNOME only loads new extensions at login, so Snappy asks you to log out and back in once.
- **KDE Plasma on Wayland:** Snappy works straight away. If you turn on Caps Lock as the Snappy key in the same session you first started Snappy in, it starts working after your next login. KWin's own tiling keeps running beside it: if Shift-dragging also snaps windows into KWin's tiles, empty the tile layout in KWin's tile editor (Meta+T).

### Windows

Install the Visual Studio Build Tools with the "Desktop development with C++" workload. Then, in PowerShell:

```powershell
pnpm -C apps/settings/ui build
cargo build --release -p snappy -p snappy-settings
$dest = "$env:LOCALAPPDATA\Programs\Snappy"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item target\release\snappy.exe, target\release\snappy-settings.exe $dest
& "$dest\snappy.exe"
```

On every system, turn on **Open at login** in Snappy's settings to start it automatically. To update, pull and repeat the build and copy steps.

## Contributing

[CONTRIBUTING.md](CONTRIBUTING.md) explains how to set up a development environment, build, test, and release.
