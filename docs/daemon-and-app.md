# The daemon and the app

How the two programs start, run, and stop, and how they find each other.

## Starting the daemon

The daemon, `snappy`, reads its launch flags and environment from `crates/platform/src/launch.rs`:

- **`--background`** starts it quietly, as the login item does. Started any other way, it also opens the app's window.
- **`--stop-for-restart`** (Windows only) asks the running daemon to stop for a restart and waits for it to exit. See [Quitting and restarting](#quitting-and-restarting).
- **`SNAPPY_HOME`** names a folder to hold every file instead of the usual places. See [Files and diagnostics](files-and-diagnostics.md).
- **`SNAPPY_DEBUG=1`** turns the debug log on, whatever the settings say.

On Linux, `apps/snappy/src/main.rs` also picks the adapter from the session: GNOME on Wayland, KDE Plasma on Wayland, or X11 for everything else. [Linux](platforms/linux.md) explains how.

## One daemon per user

Only one daemon runs at a time. A second launch hands off to the running one, which opens the app's window, and then exits. Each platform does this its own way:

- **macOS**: a lock on `daemon.lock` in the state folder. Opening Snappy.app again reaches the running daemon as a reopen.
- **Linux**: an exclusive lock on `snappy.lock` in `$XDG_RUNTIME_DIR`. A second launch sends the running daemon `SIGUSR1`.
- **Windows**: a named mutex. A second launch signals a named event.

The platform pages have the details.

## Threads

The adapter runs the OS event loop on the main thread, which AppKit and Win32 require, and forwards every event over a channel. The engine runs on a thread of its own, in a single-threaded Tokio runtime with a `LocalSet` (`apps/snappy/src/runtime.rs`). Neither blocks the other, and the engine's state needs no locks.

If the engine thread ends for any reason, a panic included, the adapter's loop is stopped too. The daemon exits rather than holding windows and keys with nothing behind them, and the next start picks up from the saved files.

## Starting the app

The daemon starts the app with piped standard input and output (`apps/snappy/src/app_process.rs`), passing it `--daemon`. It looks for `snappy-app` beside its own executable, or, inside the macOS bundle, at `Contents/Helpers/Snappy App.app`. So the two programs are always installed together.

The pipes are the only channel between them. There's no socket to find or secure, and the app can't outlive the daemon that started it. [Talking to the app](ipc.md) covers what goes over them.

The app quits when its window closes. Asking for it, from the menu's "Open Snappy…" or with a second launch, starts it when it isn't running. Either way the engine then brings its window forward with a `Focus` message, switching first to the workspace the window is in when that isn't the one showing.

## Quitting and restarting

There are two ways for the daemon to stop, and they differ in what happens to hidden workspace windows:

- **Quitting** brings every parked window back on screen first. The menu's Quit does this, and so do `SIGTERM`, `SIGINT`, and `SIGHUP` on macOS and Linux, and a close request, the session ending, or a console's Ctrl+C on Windows.
- **Stopping for a restart** leaves every window where it is. `SIGUSR2` does this on macOS and Linux, and `snappy --stop-for-restart` on Windows, which has no such signal. The next start puts each window back in its workspace without moving it.

A crash leaves things as stopping for a restart does. [Workspaces](workspaces.md#restarts) explains how windows are recognized again.

## The macOS bundle

`tools/macos/bundle.sh` builds `dist/Snappy.app`:

- the daemon at `Contents/MacOS/snappy`, with `packaging/macos/Snappy.plist` as its `Info.plist`;
- the app as a helper bundle at `Contents/Helpers/Snappy App.app`, with `packaging/macos/SnappyApp.plist`;
- the launch agent `io.github.farhadzaidi.Snappy.Agent.plist` in `Contents/Library/LaunchAgents`, which the login item registers to run the daemon with `--background`.

Both programs use the accessory activation policy: no Dock icon and no Command-Tab entry. macOS ties the Accessibility grant to the bundle's code signature, which is why [CONTRIBUTING.md](../CONTRIBUTING.md#building) asks for a local signing identity.
