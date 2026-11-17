# Contributing to Snappy

Thanks for helping. This guide gets you from a fresh clone to a tested pull request.

To understand how the code fits together, read [the docs](docs/README.md). The code conventions and safety rules are in [AGENTS.md](AGENTS.md). They apply to everyone, not just coding agents.

## Reporting bugs and ideas

Bug reports and ideas are welcome as GitHub issues. For a bug, say which system and desktop you're on, what you did, and what you expected to happen.

If a drag didn't snap or a shortcut misbehaved, Snappy's diagnostic files help a lot. They never hold window titles, window contents, or keystrokes, so they're safe to attach. [Files and diagnostics](docs/files-and-diagnostics.md#when-something-didnt-work) says where they are and which to look at.

## Running Snappy safely

Snappy moves other apps' windows, registers global shortcuts, and can change keyboard settings. **Don't run a development build of the daemon on the desktop you're working at.** A broken build does all of that badly, and on GNOME or KDE it can replace the extension or script your installed copy of Snappy uses.

Instead, work on the app in a browser ([below](#working-on-the-app-in-a-browser)), and run the daemon in one of the [sandboxes](docs/sandboxes.md).

## Setting up

You need:

- [Rust](https://rustup.rs), stable;
- [Node.js](https://nodejs.org) 22 and pnpm 9;
- [Docker](https://docs.docker.com/engine/install/), for the app's end-to-end tests;
- on Linux, the WebKitGTK and GTK development packages. See the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#linux) for your distribution;
- on Windows, the Visual Studio Build Tools with the "Desktop development with C++" workload.

On Debian or Ubuntu, the Linux packages are:

```sh
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev
```

Then, from the repository, add what the app's WebAssembly build needs and install the UI's packages:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130
npm install -g pnpm@9
pnpm -C apps/app/ui install
```

Testing an adapter on a real desktop needs more, such as Docker or a VM. [Testing on a real desktop](docs/sandboxes.md) lists what each desktop needs.

## Building

Build the app's UI first, which also builds its WebAssembly core. Then build the daemon and the app:

```sh
pnpm -C apps/app/ui build
cargo build -p snappy -p snappy-app
```

The daemon looks for `snappy-app` next to its own executable, so keep the two together.

### On macOS

macOS ties the Accessibility permission to the app's code signature. So first, once, create a local signing identity that every build reuses:

```sh
tools/macos/signing-identity.sh
```

Then build and sign `dist/Snappy.app`:

```sh
tools/macos/bundle.sh
```

With the identity, the permission survives rebuilds. Without it, builds are signed ad hoc, and macOS asks for Accessibility again after each one. To sign with another identity, set `SNAPPY_SIGN_IDENTITY`.

### For Windows, from another system

To cross-compile the daemon for Windows on ARM, with the tools from [If you changed an adapter](#if-you-changed-an-adapter):

```sh
cargo xwin build --release --target aarch64-pc-windows-msvc -p snappy
```

## Working on the app in a browser

This serves the app at http://localhost:5179, against a fake daemon that runs in the browser:

```sh
pnpm -C apps/app/ui dev
```

- The page shows macOS's controls. Add `?platform=windows` or `?platform=linux` to the address for another platform's, and `&build=app-store` for the Mac App Store build's.
- In the browser console, `window.snappyFake` is the fake daemon.
- `SNAPPY_UI_PORT` picks another port.

The fake daemon applies changes with the same rules as the real one, so what you see is what Snappy's window would show.

## Testing

Before sending a change, run:

```sh
tools/check.sh
tools/e2e.sh
```

`tools/check.sh` runs formatting, lints, every Rust test, and the app's own checks. `tools/e2e.sh` runs the app's end-to-end tests in Chromium and WebKit, in Playwright's container, as CI does. [Testing](docs/testing.md) explains what each kind of test covers, and which one to write.

### If you changed an adapter

Lint the macOS and Windows adapters from any machine. That needs two more targets and `cargo-xwin`, once:

```sh
rustup target add aarch64-apple-darwin aarch64-pc-windows-msvc
cargo install --locked cargo-xwin
```

Then:

```sh
tools/cross-check.sh            # both
tools/cross-check.sh macos      # only macOS
tools/cross-check.sh windows    # only Windows
```

Then run the adapter's tests on a real desktop, in its sandbox. Each desktop's commands are in [Testing on a real desktop](docs/sandboxes.md#running-the-tests): [X11](docs/sandboxes.md#x11), [GNOME](docs/sandboxes.md#gnome-on-wayland), [KDE Plasma](docs/sandboxes.md#kde-plasma-on-wayland), [macOS](docs/sandboxes.md#macos), and [Windows](docs/sandboxes.md#windows).

## Sending a change

Open a pull request against `main`. Keep each commit to one logical change, with a message that says what changed and why.

On your first pull request, a bot asks you to accept the [Contributor License Agreement](CLA.md) by leaving a comment. You keep the copyright in your work. The agreement lets Snappy be offered under its [license](LICENSE.md) and in app stores. You only do this once.

CI runs on every pull request:

- on Linux, `tools/check.sh` and the app's end-to-end tests;
- on macOS and Windows, lints for that system's adapter, the daemon, and the live tests, and the adapter's unit tests.

## Changing the app icon

`assets/icon/snappy.svg` is the source of the app icon, and `assets/icon/snappy-glyph.svg` of the one-color glyph in the menu bar and tray. After changing either, regenerate the files made from them, and commit them together with the SVG:

```sh
tools/icon/generate.sh
```

It needs `rsvg-convert`, macOS's `iconutil`, and Node.

On Linux and Windows the tray glyph is also drawn in code, in `crates/platform/src/glyph.rs`, and a test checks that it matches the SVG. So change both together.

## Releasing

The maintainer releases from `main`, or from a release branch for a hotfix. [Releasing](docs/releasing/README.md) covers the whole process.
