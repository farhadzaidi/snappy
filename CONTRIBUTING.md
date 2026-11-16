# Contributing to Snappy

Thanks for helping. This guide covers setting up a development environment, building and running Snappy safely, testing, debugging, the icon, and releasing.

Two other places are worth knowing about:

- [docs/](docs/README.md) explains how the code works: the daemon and the app, the engine, each platform's adapter, and how the tests are layered.
- [AGENTS.md](AGENTS.md) holds the code conventions and the safety rules. They apply to everyone, not just coding agents.

## Reporting bugs and ideas

Bug reports and ideas are welcome as GitHub issues. For a bug, say which system and desktop you're on and what you expected to happen.

If a drag didn't snap or a shortcut misbehaved, Snappy's diagnostic files help a lot ([Debugging](#debugging)). They never hold window titles, window contents, or keystrokes, so they're safe to attach.

## Sending a change

Open a pull request against `main`. Keep each commit to one logical change, with a message that says what changed and why.

On your first pull request, a bot asks you to accept the [Contributor License Agreement](CLA.md) by leaving a comment. You keep the copyright in your work; the agreement lets Snappy be offered under its [license](LICENSE.md) and in app stores. You only do this once.

CI runs on every pull request:

- on Linux, `tools/check.sh` and the app's end-to-end tests;
- on macOS and Windows, lints for that system's adapter, the daemon, and the live harness, and the adapter's unit tests.

Run the same checks locally first ([Testing](#testing)).

## Setting up

### What you need

- [Rust](https://rustup.rs), stable.
- The `wasm32-unknown-unknown` target and `wasm-bindgen-cli` 0.2.130, for the app's WebAssembly core.
- [Node.js](https://nodejs.org) 22 and pnpm 9, for the app's UI.
- On Linux, the WebKitGTK and GTK development packages the app needs. See the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#linux) for your distribution.
- On Windows, the Visual Studio Build Tools with the "Desktop development with C++" workload.

On Debian or Ubuntu, the Linux packages are:

```sh
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev
```

Then, from the repository:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130
npm install -g pnpm@9
pnpm -C apps/app/ui install
```

For the app's end-to-end tests, also install the browsers they run in:

```sh
pnpm -C apps/app/ui exec playwright install chromium webkit
```

### Optional tools

Depending on what you work on, you may also want:

- **Cross-checking the macOS and Windows adapters** from any machine (`tools/cross-check.sh`):

  ```sh
  rustup target add aarch64-apple-darwin aarch64-pc-windows-msvc
  cargo install --locked cargo-xwin
  ```

- **Docker**, for the X11 and GNOME test sessions, and the KDE one on machines without KWin.
- **Bubblewrap and KWin 6**, for running the KDE tests against your own KWin, sealed off from your desktop.
- **[Tart](https://tart.run)**, on an Apple Silicon Mac, for the macOS test VM.
- **QEMU, wimlib, and xorriso**, on an Apple Silicon Mac, for the Windows test VM:

  ```sh
  brew install qemu wimlib xorriso
  ```

- **`rsvg-convert`, macOS's `iconutil`, and Node**, for regenerating the icon ([The app icon](#the-app-icon)).

## Building

Build the app's UI first, which also builds its WebAssembly core, then the daemon and the app:

```sh
pnpm -C apps/app/ui build
cargo build -p snappy -p snappy-app
```

The daemon looks for `snappy-app` beside its own executable, so keep the two together.

### macOS

On macOS, build and sign `dist/Snappy.app` with:

```sh
tools/macos/bundle.sh
```

macOS ties the Accessibility grant to the app's signature. So first, once, create a local signing identity that every build reuses:

```sh
tools/macos/signing-identity.sh
```

With it, the grant survives rebuilds. Without it, builds are signed ad hoc, and macOS asks for Accessibility again after each one. To sign with another identity, set `SNAPPY_SIGN_IDENTITY`.

### Windows from another system

To cross-compile the daemon for Windows on ARM:

```sh
cargo xwin build --release --target aarch64-pc-windows-msvc -p snappy
```

## Running safely

Snappy moves other apps' windows, registers global shortcuts, and can change keyboard settings. **Don't run a development build of the daemon on the desktop you're working at.** A broken build does all of that badly, and on GNOME or KDE it can replace the extension or script your installed copy uses.

Instead, work on the app in a browser, or run the daemon in a sandbox.

### Work on the app in a browser

This serves the app at http://localhost:5179 against a fake, in-memory daemon:

```sh
pnpm -C apps/app/ui dev
```

- Add `?platform=windows` or `?platform=linux` to the address to see another platform's controls. The default is macOS. Add `&build=app-store` to see the Mac App Store build's.
- In the browser console, `window.snappyFake` is the fake daemon.
- `SNAPPY_UI_PORT` picks another port.

### Run the daemon in a sandbox

The [Testing](#testing) section lists a sandbox for each desktop: containers for X11 and GNOME, a sealed-off KWin for KDE, and VMs for macOS and Windows. Each can also run any command you give it in its session, for example:

```sh
tools/live/x11/run.sh tools/live/x11/x11-session.sh xprop -root                # in the X11 session
tools/live/kwin/kwin-session.sh <command>                                       # in a sealed-off KWin, with KWin 6 and Bubblewrap
tools/live/windows/ssh.sh "Get-ComputerInfo | Select OsName, OsArchitecture"   # in the Windows VM
```

### Using your own build

Installing a build to use day to day is fine; that's what it's for. The README's install steps work for any build.

Quitting Snappy brings every hidden workspace window back on screen. To replace a running Snappy without that, stop it *for a restart* first. The next one puts each window back in its workspace.

The commands below assume a release build (`cargo build --release -p snappy -p snappy-app`), installed where the README puts it.

On macOS:

```sh
pkill -USR2 -x snappy; while pgrep -x snappy >/dev/null; do sleep 0.2; done
ditto dist/Snappy.app /Applications/Snappy.app
open -g /Applications/Snappy.app --args --background
```

On Linux:

```sh
pkill -USR2 -x snappy; while pgrep -x snappy >/dev/null; do sleep 0.2; done
install -Dm755 target/release/snappy target/release/snappy-app -t ~/.local/bin/
~/.local/bin/snappy --background
```

On Windows, in PowerShell:

```powershell
$dest = "$env:LOCALAPPDATA\Programs\Snappy"
Start-Process -Wait "$dest\snappy.exe" -ArgumentList "--stop-for-restart"
Copy-Item target\release\snappy.exe, target\release\snappy-app.exe $dest
& "$dest\snappy.exe" --background
```

## Testing

[docs/testing.md](docs/testing.md) explains what each layer of tests checks. This section is how to run them.

### Before sending a change

```sh
tools/check.sh              # formatting, lints, every Rust test, and the app's checks
pnpm -C apps/app/ui e2e     # the app end to end, in Chromium and WebKit
```

### If you changed an adapter

Lint it for its own system, and run it in its sandbox below.

```sh
tools/cross-check.sh            # the macOS and Windows adapters, with the daemon and the live harness
tools/cross-check.sh macos      # only macOS
tools/cross-check.sh windows    # only Windows
```

Every sandbox runs on its own desktop, never on yours. The live suites take a scenario name prefix to run only some scenarios, and the GNOME, KDE, and Windows test scripts take a test name filter. Results land under `target/live/<desktop>/`. [docs/sandboxes.md](docs/sandboxes.md) explains how each one works.

### X11

Runs in Docker: a headless X server with Xfwm4 and two virtual monitors.

```sh
tools/live/x11/run.sh tools/live/x11/x11-session.sh    # the adapter's tests
tools/live/x11/live.sh                                  # the live suite
```

### GNOME on Wayland

Runs in Docker: a headless GNOME Shell of the version you ask for, with two virtual monitors. GNOME 46 is the default.

```sh
tools/live/gnome/test.sh                              # the adapter's tests
tools/live/gnome/test.sh --gnome 46,47,48,49          # on several versions
tools/live/gnome/live.sh                              # the live suite
tools/live/gnome/live.sh --static-workspaces          # with a fixed number of GNOME workspaces
```

### KDE Plasma on Wayland

Runs your own KWin inside Bubblewrap when you have KWin 6 and Bubblewrap, and otherwise a KWin in Docker. KWin runs headless, without a GPU, sealed off from your session.

```sh
tools/live/kwin/test.sh                  # the adapter's tests, with the daemon end to end
tools/live/kwin/live.sh                  # the live suite
tools/live/kwin/live.sh --container      # in Docker, even with KWin installed
```

### macOS

Runs in a Tart VM named `snappy-macos`, cloned from `ghcr.io/cirruslabs/macos-sequoia-base` with SIP off, so the script can grant Accessibility inside it. Create the VM once; the script starts it when it isn't running.

```sh
tools/live/macos/live.sh
```

There's no separate adapter test script for macOS; its unit tests run on a Mac with:

```sh
cargo test -p snappy-macos --lib
```

### Windows

Runs in a Windows 11 ARM64 VM under QEMU on an Apple Silicon Mac. Create it once, unattended; it takes about 12 minutes after 5.5 GB of downloads.

```sh
tools/live/windows/create.sh
```

Then start it, run the tests, and stop it:

```sh
tools/live/windows/start.sh
tools/live/windows/test.sh          # the adapter's unit and integration tests
tools/live/windows/live.sh          # the live suite
tools/live/windows/stop.sh
```

To look at its screen, or put it back as it was right after the install:

```sh
tools/live/windows/screenshot.sh
tools/live/windows/reset.sh
```

## Debugging

Snappy's diagnostics never hold window titles, window contents, or keystrokes. [docs/files-and-diagnostics.md](docs/files-and-diagnostics.md) has the details and where each file lives.

- **The debug log** records what Snappy saw, decided, and did. Turn it on in Settings → Debug, or start the daemon with `SNAPPY_DEBUG=1`. It's `debug.jsonl` in Snappy's log folder, and Settings shows its path while it's on.
- **`drops.json`**, in the same folder, keeps the last 20 drags made with the drag key and how each ended, even with the debug log off. It's the first place to look when a drop didn't snap.
- **`SNAPPY_HOME=<folder>`** makes the daemon keep its settings, state, and logs in that folder instead of the usual places, which is how the tests run it against scratch settings. The single-instance lock stays where it always is on Windows, and on Linux while `$XDG_RUNTIME_DIR` is set; on macOS it moves into that folder too.

## The app icon

`assets/icon/snappy.svg` is the source of the app icon, and `assets/icon/snappy-glyph.svg` of the monochrome menu bar and tray glyph. After changing either, regenerate the files made from them:

```sh
tools/icon/generate.sh
```

It needs `rsvg-convert`, macOS's `iconutil`, and Node. Commit the SVG together with the files it writes.

The tray glyph on Linux and Windows is also drawn in code (`crates/platform/src/glyph.rs`), and a test checks it matches the SVG, so change both together.

## Releasing

The maintainer releases from `main` once CI passes, or from a release branch for a hotfix. `tools/release/bump.sh patch` (or `minor`, `major`, or a version) sets the version everywhere it's written. Pushing the tag `v<version>` runs the release workflow, which builds every package and drafts a GitHub release to check and publish.

[docs/releasing/](docs/releasing/README.md) covers the whole process, the packages, hotfixes, and the signing secrets.
