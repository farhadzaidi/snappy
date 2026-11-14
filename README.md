# Snappy

Snappy arranges windows into zones on macOS, Windows, and Linux (X11, and GNOME or KDE Plasma on Wayland). A layout divides each display into zones, and Snappy moves windows into them when you drag with a modifier held or press a shortcut.

It runs as a small background program with a menu bar item (a notification-area or tray icon on Windows and Linux). The settings window opens on demand to pick and design layouts, set shortcuts, and manage workspaces. Snappy never reads window titles or contents, and never touches the network.

## Features

- **Zones.** Eight preset layouts (halves, thirds, center stage, quarters, and more) plus your own. Each display has its own layout, with an even 12-point gap between zones and around the edges.
- **Drag to snap.** Hold Shift (or Option or Control) while dragging a window by its title bar. The zones appear on the display under the pointer; drop on one to snap the window into it. Let go of the modifier before dropping to cancel.
- **Snappy-key shortcuts.** The Snappy key is Control+Option+Command on macOS, Ctrl+Alt+Win on Windows, and Ctrl+Alt+Super on Linux, unless you choose other modifiers for it; shortcuts show it as "Snappy". Every default shortcut is the Snappy key and one other key, the same on every platform:

  | Snappy + | Does |
  |---|---|
  | ← → ↑ ↓ | Move the focused window to the neighboring zone |
  | I J K L | Focus the next window up, left, down, or right |
  | Return | Select the focused window, or unselect it |
  | `\` | Clear the selection |
  | Space | Move the focused window into the nearest empty zone; with a selection, move the selected windows into empty zones |
  | 1 to 9 | Switch to workspace 1 to 9, taking the selection along |
  | `;` `'` | Go to the previous or next workspace, taking the selection along |
  | `.` `/` | Send the focused window, or the selection, to the previous or next workspace, and stay |

  Anything else is a shortcut you assign: each zone, each layout (applied to the display under the pointer), sending the focused window to a particular workspace, with or without switching there, and undo and redo of window moves. You can change or clear any shortcut.
- **Choosing the Snappy key.** In Settings, under Snappy key, turn on at least two of Control, Option, Command, and Shift (Ctrl, Alt, Win or Super, and Shift on Windows and Linux). Every shortcut shown as Snappy follows at once, with no restart, and still reads "Snappy + …". A shortcut you recorded without the Snappy key keeps exactly the keys you pressed, whatever the Snappy key becomes, even when they are the new key's modifiers; only holding the Snappy key while recording makes a Snappy shortcut. A choice that would make two shortcuts the same, such as a Snappy shortcut landing on one you recorded by hand, or land one on a chord the system or another app already uses, is refused, and Settings names both shortcuts or the one that clashed. Reset to default brings back Control+Option+Command.
- **Caps Lock as the Snappy key.** Turn on "Use Caps Lock as the Snappy key" under Snappy key in Settings, and holding Caps Lock does whatever holding the Snappy key does: Caps Lock + J is Snappy + J, with whichever modifiers the Snappy key is. Tapped alone, Caps Lock still turns Caps Lock on and off, its light included; holding it never does, and it doesn't repeat. The modifiers themselves keep working too. There is nothing to install. Snappy refuses, and says why, when your system already remaps Caps Lock, and gives Caps Lock back its usual behavior when you turn the setting off or quit, or at its next start after a crash. [platforms.md](docs/platforms.md) has how each system does it and its limits.
- **Keyboard focus.** Snappy + I, J, K, or L focuses and raises the next window in that direction, on any display, and outlines it in orange for a moment so you can see where focus went. While any window is in a zone, focus moves from zone to zone the way the arrows move a window, skipping empty zones and windows that aren't in a zone; of several windows in one zone, the one used last gets focus. With no window in a zone, focus goes to the nearest window.
- **Swaps.** A window moved into a zone another window fills, by shortcut or by drag, trades places with it. A swap trades zones: when the moving window isn't in a zone, the other window goes to the zone under the moving window's center, and takes its exact spot only when that zone can't take it. The "When the zone is taken" setting can make it stack on top instead, and holding the extra key (Command on macOS, Ctrl elsewhere) during a drag does the opposite of the setting for that drop.
- **Workspaces.** Groups of windows, one visible at a time. Switching hides the other groups' windows and brings the chosen group back exactly where it was. Each workspace can use its own layouts. When Snappy restarts while your apps keep running, after an update or a crash, every window stays in its workspace and nothing moves; quitting brings hidden windows back.
- **Selection.** Snappy + Return selects the focused window, and a mint outline shows it is selected; select as many as you like, with or without workspaces. Snappy + Space puts the selected windows that aren't in a zone into empty zones, on the display under the pointer first, pairing windows and zones so they travel least, as one step of undo. The next workspace switch, however you make it, brings the selection along, where it is, and the last window selected gets focus; Snappy + `.` and `/` send the whole selection away instead. Snappy + Return on a selected window unselects it, and Snappy + `\` clears the selection.
- **Layout editor.** Draw, cut, move, and resize zones on a canvas shaped like your display, with snapping guides, pixel-exact coordinate fields, and undo.
- **Debug log.** An opt-in record of what Snappy saw, decided, and did, for finding out why a window or shortcut didn't behave as expected.

## Platforms

| Platform | Notes |
|---|---|
| macOS 14 or later | Needs Accessibility permission. |
| Windows 11 | Tested on ARM64. Windows of apps running as administrator can't be moved. |
| Linux, X11 | EWMH window managers such as GNOME on Xorg, KDE Plasma on X11, Xfce, and Cinnamon. |
| Linux, GNOME on Wayland | GNOME 46 to 49, through a GNOME Shell extension Snappy installs itself. |
| Linux, KDE Plasma on Wayland | Plasma 6.8, through a KWin script Snappy loads at every start. Other Wayland desktops are recognized and live control stays off. |

## Install from source

There are no prebuilt releases yet. Building takes a few minutes the first time.

**Every platform needs:**

- Rust, through [rustup](https://rustup.rs), plus the WebAssembly target and its bindings generator:
  ```sh
  rustup target add wasm32-unknown-unknown
  cargo install wasm-bindgen-cli --version 0.2.130
  ```
- Node.js and pnpm 9 (`npm install -g pnpm@9`).

Then clone the repository and build the settings UI once:

```sh
git clone https://github.com/farhadzaidi/snappy.git
cd snappy
pnpm -C apps/settings/ui install
```

### macOS

```sh
tools/macos/signing-identity.sh        # once per Mac: a local signing identity, so Accessibility stays granted across rebuilds
tools/macos/bundle.sh                  # builds and signs dist/Snappy.app
cp -R dist/Snappy.app /Applications/
open /Applications/Snappy.app
```

Allow Snappy in System Settings → Privacy & Security → Accessibility when asked. Snapping starts as soon as access is granted.

To install a newer build over a running Snappy without bringing every workspace's windows onto the screen, stop it for a restart first:

```sh
pkill -USR2 -x snappy; while pgrep -x snappy >/dev/null; do sleep 0.2; done
ditto dist/Snappy.app /Applications/Snappy.app
open -g /Applications/Snappy.app --args --background
```

This also works the first time, over a Snappy too old to know the request: `SIGUSR2` ends it at once without bringing windows back, and the new build reads the windows it left hidden. Linux and Windows have the same: see [development.md](docs/development.md#installing-a-new-build-without-losing-workspaces).

### Linux

Install the settings window's system libraries first. On Debian or Ubuntu:

```sh
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev
```

Other distributions: see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#linux) for the equivalent packages.

```sh
pnpm -C apps/settings/ui build
cargo build --release -p snappy -p snappy-settings
install -Dm755 target/release/snappy target/release/snappy-settings -t ~/.local/bin/
~/.local/bin/snappy
```

On GNOME on Wayland, the first launch installs Snappy's GNOME Shell extension into `~/.local/share/gnome-shell/extensions/` and turns it on. GNOME loads new extensions only when you log in, so the settings window asks you to log out of your desktop and sign back in once, with a **Log out now** button; Snappy starts snapping after that. Updates that change the extension ask the same way. On KDE Plasma on Wayland, Snappy loads its KWin script when it starts and snapping works straight away; KWin's own tiling still runs alongside it, so see [platforms.md](docs/platforms.md#linux-kde-plasma-on-wayland) for the KDE settings that fight it. On X11 there is nothing more to do. Other Wayland desktops aren't supported yet.

Turn on "Open at login" in Snappy's settings to start it automatically.

### Windows

Install the Visual Studio Build Tools with the "Desktop development with C++" workload, which Rust needs on Windows. Then, in PowerShell:

```powershell
pnpm -C apps/settings/ui build
cargo build --release -p snappy -p snappy-settings
$dest = "$env:LOCALAPPDATA\Programs\Snappy"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item target\release\snappy.exe, target\release\snappy-settings.exe $dest
& "$dest\snappy.exe"
```

Turn on "Open at login" in Snappy's settings to start it automatically.

### Updating

Pull and repeat the build and copy steps for your platform:

```sh
git pull
```

## Repository map

| Path | What it holds |
|---|---|
| `crates/core` | Every rule about zones, layouts, editing, snapping, swaps, shortcuts, settings, and workspaces. Pure logic, no I/O. |
| `crates/platform` | The traits each OS adapter implements, and the events adapters report. |
| `crates/engine` | The engine: the single owner of runtime state, the window-operation queue, and the files Snappy writes. |
| `crates/ipc` | Messages between the daemon and the settings app, and their framing. |
| `crates/fake` | An in-memory OS for engine tests. |
| `crates/macos` | macOS adapter (Accessibility, AppKit, Carbon hotkeys). |
| `crates/windows` | Windows adapter (Win32). |
| `crates/x11` | Linux X11 adapter (x11rb). |
| `crates/gnome` | GNOME on Wayland adapter: talks to the extension over D-Bus (zbus). |
| `crates/linux` | What the Linux adapters share: single instance, login item, session, key codes, menu, overlay style. |
| `apps/gnome-extension` | The GNOME Shell extension that carries out the GNOME adapter's requests. |
| `crates/kwin` | KDE Plasma on Wayland adapter: talks to its KWin script and KDE's shortcut service over D-Bus (zbus). |
| `apps/kwin-script` | The KWin script that carries out the KWin adapter's requests. |
| `crates/wasm` | `core`'s editor and shortcut rules compiled to WebAssembly for the settings app. |
| `crates/live` | Live tests: a runner and a fixture app that drive the real daemon in a sandbox. |
| `apps/snappy` | The daemon binary. |
| `apps/settings` | The settings app: `src-tauri` (Tauri shell) and `ui` (React and TypeScript). |
| `tools` | The full check, macOS bundling and signing, and the Linux (X11, GNOME, and KWin), macOS, and Windows sandboxes. |

Package names keep the `snappy-` prefix: `crates/core` is `snappy-core`, and so on.

## Development

Set up the toolchain first ([docs/development.md](docs/development.md#toolchain)). Then:

```sh
tools/check.sh                          # format, lint, every Rust test, and the settings UI's checks
pnpm -C apps/settings/ui e2e            # settings UI end-to-end tests in a headless browser
pnpm -C apps/settings/ui dev            # the settings UI in a browser, against a fake daemon
tools/live/linux/live.sh                # the real daemon against real windows, in a Linux container
tools/live/gnome/live.sh                # the same on GNOME Shell under Wayland, in containers
tools/live/kwin/test.sh                 # the KWin adapter and the daemon against a sealed-off headless KWin
```

While developing, never run a development or test build of the daemon on the desktop you work at: it moves windows and registers global shortcuts, and a broken build does that badly. Run it in the Linux container or in one of the test VMs ([docs/testing.md](docs/testing.md#live-tests)). Installing Snappy from source to use it day to day is what it's for.

## Documentation

- [docs/architecture.md](docs/architecture.md): the two programs, the crates, the engine, and how a drag, a shortcut, and a workspace switch flow through them.
- [docs/platforms.md](docs/platforms.md): how each OS adapter finds, moves, and watches windows, and its quirks.
- [docs/development.md](docs/development.md): toolchain, building, running safely, where code goes, and the code style.
- [docs/testing.md](docs/testing.md): the test layers, the fake platform, and the live sandboxes.
- [AGENTS.md](AGENTS.md): the hard rules for anyone changing this repository.

## License

Snappy is licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at your option.
