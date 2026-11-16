# Contributing to Snappy

Thanks for helping. This guide covers setting up a development environment, building and running Snappy safely, testing, and releasing. For how the code is put together, see [docs/architecture.md](docs/architecture.md); for how each platform's adapter works, [docs/platforms.md](docs/platforms.md); for how the tests are organized, [docs/testing.md](docs/testing.md).

Bug reports and ideas are welcome as GitHub issues. For a code change, open a pull request against `main`; CI runs the same checks described below.

## Setting up

You need:

| Tool | For |
|---|---|
| [Rust](https://rustup.rs), stable | everything |
| The `wasm32-unknown-unknown` target and `wasm-bindgen-cli` 0.2.130 | the app's WebAssembly core |
| Node.js 22 and pnpm 9 | the app's UI |
| On Linux, WebKitGTK and GTK development packages | the app ([Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#linux)) |

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130
npm install -g pnpm@9
pnpm -C apps/app/ui install
pnpm -C apps/app/ui exec playwright install chromium webkit   # for the UI's end-to-end tests
```

Optional, depending on what you work on:

| Tool | For |
|---|---|
| `rustup target add aarch64-apple-darwin aarch64-pc-windows-msvc` and `cargo install --locked cargo-xwin` | linting the macOS and Windows adapters from another system (`tools/cross-check.sh`) |
| Docker | the X11 and GNOME test sessions, and the KDE one on machines without KWin |
| Bubblewrap and KWin 6 | running the KDE tests against your own KWin, sealed off from your desktop |
| [Tart](https://tart.run) (Apple Silicon) | the macOS test VM |
| QEMU, wimlib, and xorriso (Apple Silicon) | the Windows test VM |

## The code at a glance

| Path | What it holds |
|---|---|
| `crates/core` | Every rule: zones, layouts, the layout editor, snapping, swaps, shortcuts, settings, workspaces. Pure logic, no I/O. |
| `crates/engine` | Snappy's runtime state and the order things happen in; the only writer of Snappy's files. |
| `crates/platform` | The interfaces every OS adapter implements, and the events they report. |
| `crates/macos`, `crates/windows`, `crates/x11`, `crates/gnome`, `crates/kwin` | One adapter per desktop. `crates/linux` holds what the Linux ones share. |
| `crates/gnome/extension`, `crates/kwin/script` | The parts that run inside GNOME Shell and KWin on Wayland. |
| `apps/snappy` | The daemon: the program that runs in the background. |
| `apps/app` | The app: a Tauri shell (`src-tauri`) around a React UI (`ui`). |
| `crates/ipc` | Messages between the daemon and the app. |
| `crates/wasm` | Core's rules compiled to WebAssembly for the app. |
| `crates/fake`, `crates/live` | Test support: an in-memory desktop, and the live test runner and fixture app. |
| `tools`, `packaging` | Scripts for checks, bundling, and the test sandboxes; files that ship inside the macOS app. |
| `assets` | The app icon and its sources. |

## Building

```sh
pnpm -C apps/app/ui build   # the WebAssembly core, then the UI
cargo build -p snappy -p snappy-app
```

On macOS, `tools/macos/bundle.sh` builds and signs `dist/Snappy.app`. macOS ties the Accessibility grant to the app's signature, so run `tools/macos/signing-identity.sh` once first: it creates a local signing identity that every build reuses, so the grant survives rebuilds. Without it, builds are signed ad hoc and macOS asks for Accessibility again each time.

For Windows from another system, `cargo xwin build --release --target aarch64-pc-windows-msvc -p snappy` cross-compiles the daemon.

## Running safely

Snappy moves other apps' windows, registers global shortcuts, and can change keyboard settings, so **don't run a development build of the daemon on the desktop you're working at.** A broken build does all of that badly, and on GNOME or KDE it can replace the extension or script your installed copy uses. Instead:

- **Work on the app in a browser.** `pnpm -C apps/app/ui dev` serves it at http://localhost:5179 against a fake, in-memory daemon. Add `?platform=windows` or `?platform=linux` to see another platform's controls. In the browser console, `window.snappyFake` is the fake daemon.
- **Run the daemon in a sandbox** ([Testing](#testing)): containers for X11 and GNOME, a sealed-off KWin for KDE, VMs for macOS and Windows. Each can also run any command you give it in its session.

Installing a build to use day to day is fine; that's what it's for.

### Installing a new build without losing workspaces

Quitting Snappy brings every hidden workspace window back on screen. To replace a running Snappy without that, stop it *for a restart* first; the new one puts each window back in its workspace.

```sh
# macOS
pkill -USR2 -x snappy; while pgrep -x snappy >/dev/null; do sleep 0.2; done
ditto dist/Snappy.app /Applications/Snappy.app
open -g /Applications/Snappy.app --args --background

# Linux: the same pkill and wait, then replace the binaries and run `snappy --background`.
# Windows: run `snappy.exe --stop-for-restart`, replace the files, then run `snappy.exe --background`.
```

## Testing

Before sending a change:

```sh
tools/check.sh                    # formatting, lints, every Rust test, and the app's checks
pnpm -C apps/app/ui e2e           # the app end to end, in Chromium and WebKit
```

If you changed an adapter, also lint it for its own system and run it in its sandbox:

```sh
tools/cross-check.sh              # lint the macOS and Windows adapters from any machine
```

| Desktop | Adapter tests | Live suite (the real daemon, real windows) |
|---|---|---|
| X11 | `tools/live/x11/run.sh tools/live/x11/x11-session.sh` | `tools/live/x11/live.sh` |
| GNOME on Wayland | `tools/live/gnome/test.sh [--gnome 46,47,48,49]` | `tools/live/gnome/live.sh [--gnome …]` |
| KDE Plasma on Wayland | `tools/live/kwin/test.sh [--container]` | `tools/live/kwin/live.sh [--container]` |
| macOS | — | `tools/live/macos/live.sh` |
| Windows | `tools/live/windows/test.sh` | `tools/live/windows/live.sh` |

The live suites take a scenario name prefix to run only some scenarios. Every one of them runs in a sandbox, never on your desktop:

- **X11 and GNOME** run in Docker: a headless X server with Xfwm4, or a headless GNOME Shell of the version you ask for, two virtual monitors each.
- **KDE** runs your own KWin inside Bubblewrap when you have KWin 6 and Bubblewrap, and otherwise, or with `--container`, a KWin in Docker. KWin runs headless, without a GPU, sealed off from your session.
- **macOS** runs in a Tart VM named `snappy-macos`, cloned from `ghcr.io/cirruslabs/macos-sequoia-base` with SIP off so the script can grant Accessibility inside it.
- **Windows** runs in a Windows 11 ARM64 VM under QEMU: `tools/live/windows/create.sh` downloads and installs it once, unattended, and `start.sh` and `stop.sh` boot and stop it.

Results land in `target/live/<desktop>/`. [docs/testing.md](docs/testing.md) explains what each layer checks and how the sandboxes work.

## Debugging

- **The debug log** records what Snappy saw, decided, and did, without window titles or keystrokes. Turn it on in Settings → Debug, or start the daemon with `SNAPPY_DEBUG=1`. It's `debug.jsonl` in Snappy's log folder, which Settings shows.
- **`drops.json`** in the same folder keeps the last 20 drags made with the drag key and how each ended, even with the debug log off. It's the first place to look when a drop didn't snap.
- **`SNAPPY_HOME=<folder>`** makes the daemon keep its settings, state, and logs there instead of the usual places, which is how the tests run it against scratch settings. Its single-instance lock stays where it always is.

## The app icon

`assets/icon/snappy.svg` is the source of the app icon and `assets/icon/snappy-glyph.svg` of the monochrome menu bar and tray glyph. After changing either, run `tools/icon/generate.sh` (it needs `rsvg-convert`, macOS's `iconutil`, and Node) and commit the SVG together with the files it writes. The tray glyph on Linux and Windows is also drawn in code (`crates/platform/src/glyph.rs`), and a test checks it matches the SVG, so change both together.

## Releasing

Releases are made by the maintainer from `main`, once CI passes:

1. Set the new version in `[workspace.package]` in `Cargo.toml`, the macOS property lists in `packaging/macos`, the GNOME extension's `metadata.json`, and the app's `package.json`. Tests check they all match.
2. Commit, tag the commit `v<version>`, and push the tag.
3. Publish a GitHub release for the tag, with notes on what changed since the last one.
