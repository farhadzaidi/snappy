# Snappy

Snappy arranges your windows into zones. Pick a layout for each display, then drag a window with a key held, or press a shortcut, and it snaps into place.

It runs quietly in the background on macOS, Windows, and Linux, with a menu bar or tray icon, and a window you open when you want to change something.

Snappy never uses the network, and never reads window titles, window contents, or what you type.

## Features

- **Layouts and zones.** Eight ready-made layouts (halves, three columns, four corners, center stage, and more), or draw your own in the layout editor. Each display has its own layout.
- **Drag to snap.** Hold Shift while dragging a window by its title bar, and the zones appear. Drop the window on one to snap it there; let go of Shift first to cancel.
- **Keyboard shortcuts.** Move windows between zones, move focus, and switch workspaces without the mouse.
- **Swaps.** Dropping a window on a zone that's taken trades places with the window there, or stacks on top if you prefer. Hold the reverse key (Command on macOS, Ctrl elsewhere) as you drop to do the other, just this once.
- **Workspaces.** Groups of windows, one shown at a time. Switching hides the others and brings the chosen group back exactly where it was.
- **Selection.** Select several windows to move them together, or take them along to another workspace.
- **Undo and redo** for window moves.

## Shortcuts

Every default shortcut is the **Snappy key** plus one other key. The Snappy key is Control+Option+Command on macOS, and Ctrl+Alt+Win (Ctrl+Alt+Super on Linux) elsewhere. You can choose other modifiers in Settings, and you can also have Caps Lock act as the Snappy key while you hold it.

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

A few things to know:

- Workspaces are off until you turn them on in the Workspaces tab of Snappy's window. Snappy starts you with three.
- Undo and redo have no shortcut until you record one in Settings.
- Every shortcut can be changed or cleared in Settings, and zones, layouts, and workspaces can have shortcuts of their own.

## Supported systems

| System | Notes |
|---|---|
| macOS 14 or later | Needs Accessibility permission. |
| Windows 11 | Windows of apps running as administrator can't be moved. |
| Linux on X11 | GNOME on Xorg, KDE Plasma on X11, Xfce, Cinnamon, and other EWMH window managers. |
| Linux, GNOME on Wayland | GNOME 46 to 49, through a GNOME Shell extension Snappy installs itself. |
| Linux, KDE Plasma on Wayland | Plasma 6.8, through a KWin script Snappy loads itself. |

Other Wayland desktops aren't supported yet. Snappy starts there, but can't move windows, and says so.

## Install

There are no prebuilt releases yet, so Snappy is built from source.

### What you need

- [Rust](https://rustup.rs)
- [Node.js](https://nodejs.org) with pnpm 9
- The WebAssembly tools for Snappy's window

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130
npm install -g pnpm@9
```

Then get the code:

```sh
git clone https://github.com/farhadzaidi/snappy.git
cd snappy
pnpm -C apps/app/ui install
```

### macOS

Build Snappy.app and copy it to Applications:

```sh
tools/macos/signing-identity.sh    # once: lets the Accessibility grant survive updates
tools/macos/bundle.sh              # builds dist/Snappy.app
cp -R dist/Snappy.app /Applications/
open /Applications/Snappy.app
```

### Linux

First install the libraries Snappy's window needs. On Debian or Ubuntu:

```sh
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev
```

On other distributions, install the equivalents listed in the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#linux). Then build and install:

```sh
pnpm -C apps/app/ui build
cargo build --release -p snappy -p snappy-app
install -Dm755 target/release/snappy target/release/snappy-app -t ~/.local/bin/
~/.local/bin/snappy
```

Snappy is two files, `snappy` and `snappy-app`. Keep them in the same folder.

### Windows

Install the Visual Studio Build Tools with the "Desktop development with C++" workload. Then, in PowerShell:

```powershell
pnpm -C apps/app/ui build
cargo build --release -p snappy -p snappy-app
$dest = "$env:LOCALAPPDATA\Programs\Snappy"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item target\release\snappy.exe, target\release\snappy-app.exe $dest
& "$dest\snappy.exe"
```

Keep `snappy.exe` and `snappy-app.exe` in the same folder.

## First run

When Snappy starts, its icon appears in the menu bar or the tray, and Snappy's window opens. From the icon's menu you can pick a layout for each display, switch workspaces, open Snappy's window again, or quit.

To have Snappy start when you log in, turn on **Open at login** in Settings.

### macOS

Snappy asks for Accessibility permission. Allow it in System Settings → Privacy & Security → Accessibility. Snappy notices the grant and starts working.

### Windows

Windows Snap also acts on windows you drag to screen edges and corners, and dragging to the top edge can show Snap Layouts over Snappy's zones. To leave drags to Snappy, turn off **Snap windows** under Settings → System → Multitasking.

### GNOME on Wayland

The first launch installs Snappy's GNOME Shell extension. GNOME only loads extensions at login, so Snappy asks you to log out and back in once, with a **Log out now** button. An update to Snappy can ask for this again.

Snappy puts its menu in the top bar. Two things get in the way:

- Ubuntu's Enhanced Tiling, and the Tiling Assistant extension, also snap windows dragged to screen edges. While one is on, Snappy says where to turn it off.
- With "Workspaces on primary display only" on (Settings → Multitasking), GNOME shows windows on other displays on every workspace, so Snappy can't hide them for a workspace switch.

### KDE Plasma on Wayland

Snappy works straight away, with no log out.

KWin's own tiling keeps running beside Snappy. If Shift-dragging also snaps windows into KWin's tiles, empty the tile layout in KWin's tile editor (Meta+T) on every display and desktop. Quick tiling at screen edges (System Settings → Screen Edges) can also act where Snappy's edge zones are.

If you turn on Caps Lock as the Snappy key in the same session you first started Snappy in, it starts working after your next login.

### Linux on X11

Snappy is tested with Xfce's window manager. Others should work, but may differ in window borders, moving windows between monitors, and the tray. The tray icon needs a desktop that shows tray icons.

## Updating

Quit Snappy from its menu, pull the latest code, and repeat the build and copy steps for your system. On GNOME, Snappy may ask you to log out and back in once more.

## Uninstalling

1. In Settings, turn off **Open at login**, and **Also use Caps Lock as the Snappy key** if you turned it on.
2. Quit Snappy from its menu.
3. Delete Snappy: `/Applications/Snappy.app` on macOS, the `%LOCALAPPDATA%\Programs\Snappy` folder on Windows, or on Linux `~/.local/bin/snappy`, `~/.local/bin/snappy-app`, and Snappy's icon, `~/.local/share/icons/hicolor/scalable/apps/io.github.farhadzaidi.Snappy.svg`.
4. On GNOME, remove Snappy's extension in the Extensions app.

To remove your settings too, delete Snappy's folders:

- macOS: `~/Library/Application Support/Snappy` and `~/Library/Logs/Snappy`
- Windows: `%APPDATA%\Snappy` and `%LOCALAPPDATA%\Snappy`
- Linux: `~/.config/snappy` and `~/.local/state/snappy`

## Contributing

Bug reports and ideas are welcome as GitHub issues. [CONTRIBUTING.md](CONTRIBUTING.md) explains how to set up a development environment, build, test, and send a change.
