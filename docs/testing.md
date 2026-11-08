# Testing

## Layers

| Layer | Where | Runs |
|---|---|---|
| Core unit tests | `mod tests` in each `crates/core/src` module, and `crates/core/src/workspace/tests.rs` | anywhere, `cargo test` |
| Engine scenarios | `crates/engine/tests/scenarios.rs`, plus unit tests in `crates/engine/src` | anywhere, `cargo test` |
| WebAssembly API | `crates/wasm/src/lib.rs`, `crates/wasm/src/gestures.rs` | anywhere, `cargo test` |
| Adapter unit tests | `mod tests` in each adapter module | on the adapter's OS |
| Adapter integration tests | `crates/x11/tests/x11.rs`, `crates/gnome/tests/gnome.rs`, `crates/windows/tests/windows.rs` | the Linux container, the GNOME containers, the Windows VM |
| Daemon checks | `apps/snappy/tests/bundle.rs` (bundle and permissions), `apps/snappy/tests/privacy.rs` (no network, no window content) | anywhere, `cargo test` |
| Settings UI unit tests | `apps/settings/ui/src/**/*.test.ts`, Vitest | anywhere, `pnpm -C apps/settings/ui test` |
| Settings UI end to end | `apps/settings/ui/e2e/*.spec.ts`, Playwright | anywhere, `pnpm -C apps/settings/ui e2e` |
| Live suites | `crates/live` | the Linux and GNOME containers and the macOS and Windows VMs |

### Core

Every rule in `crates/core` is tested next to the code, with no OS and no real time: settling runs against a scripted window and an instant clock (`InstantClock` in `crates/core/src/bounds.rs`), and workspace sessions against an in-memory `WorkspaceIo`.

Property tests (`proptest`, a dev-dependency of `snappy-core`) check invariants over generated inputs:

- `crates/core/src/editor/properties.rs`: any sequence of moves, edge drags, resizes, draws, and cuts, from a preset or an empty canvas, never leaves overlapping or undersized zones.
- `crates/core/src/layout.rs`: `zone_frame` stays inside the work area, within half a point of rounding, for any valid zone, work area, and scale.
- `crates/core/src/settings.rs`: validating settings that already passed changes nothing, and saving and loading them gives them back.
- `crates/core/src/keys.rs`: normalizing a chord's modifiers to the Snappy key keeps exactly the modifiers held, and survives a save.

### Engine scenarios

`crates/engine/tests/scenarios.rs` drives a real `Engine` over `FakePlatform` end to end: events in, window frames, overlays, shortcuts, files, and status messages out. Helpers at the top of the file set things up:

- `scenario` runs a test on a single-threaded runtime with paused time, so settle waits and the focus gate take no real time.
- `live_engine(name, fake)` starts an engine on the halves layout with snapping live. `started(name, fake)` starts one without that setup.
- Each test gets a scratch folder from its `name`, so names must be unique across the file. `paths_kept` reopens the same folder to simulate a restart or a crash.
- `drag`, `release`, `press`, `hotkey_id`, `enable_workspaces`, `switch_to`, and `send_to` act like the user.

### The fake platform

`crates/fake` is an in-memory OS implementing every platform trait:

- **Displays** you can add, remove, and reorder; the main one is 1440×900 at scale 2 with a menu bar.
- **Windows** (`FakeWindow`) with an app ID, process ID, frame, and stacking order, plus behavior: a minimum size, resizing only while fully on one display (`strict_resize`), ignoring every write (`frozen`), hidden by its app, full screen, an open sheet, minimized, or on the parking desktop.
- **Focus and the pointer**, which tests set directly.
- **Permissions**: whether access is granted, how often access was requested and the settings page opened, and whether a grant is being waited for.
- **Shortcuts**: what is registered, how many times, and which chords another app holds (`taken_elsewhere`).
- **Recorders** for overlay calls, the tray menu, window writes and bounds-change brackets in order, occupancy lookups, self-activation, and the login item, with an injectable login item error.
- **Desktop and time**: whether the managed desktop is current, the parking policy and platform name to imitate, a manual clock, and the delay a bounds change reports.

Apps that lag, ignore one axis, or size in cells are scripted in `core`'s own settle tests rather than in the fake.

### Adapter tests

Adapter unit tests cover the pure parts: eligibility rules, key maps, display ordering, menu structure, overlay styles and rendering. The GNOME adapter's also run anywhere `dbus-daemon` is installed against a fake GNOME Shell on a private bus (`crates/gnome/tests/fake_shell.rs`): what it asks the extension, how it reads the replies, and how the extension's events become engine events, in order. A test that expects nothing more waits for a marker event the fake sends last, not for a set time. The fake bus gets its own temporary home and an in-memory settings backend, so nothing reaches the session the tests run in. They only compile on their own OS, so `cargo test` on a Mac runs the macOS ones. On macOS they also check the default shortcuts against the system shortcuts macOS ships with (`src/system_shortcuts.rs`).

The integration tests drive a real window system and real input, and skip themselves (printing "skipped") unless their sandbox is present:

- `crates/x11/tests/x11.rs` runs when `SNAPPY_X11_TEST_DISPLAY` names a display, which `tools/live/linux/x11-session.sh` sets: `tools/live/linux/run.sh tools/live/linux/x11-session.sh`. `SNAPPY_X11_COMPOSITOR=off` runs the window manager without its compositor.
- `crates/gnome/tests/gnome.rs` runs when `SNAPPY_GNOME_TEST=1`, which `tools/live/gnome/test.sh` sets inside a throwaway GNOME Shell with Snappy's extension and the test helper loaded: `tools/live/gnome/test.sh [--gnome 46,47,48,49]`.
- `crates/windows/tests/windows.rs` runs when `SNAPPY_WINDOWS_TEST=1`, which `tools/live/windows/desktop.sh` sets: `tools/live/windows/test.sh [test name filter]`.

They run one test at a time, since the tests share one screen, pointer, and keyboard. Each probes every default shortcut (`the_default_shortcuts_are_free` and `the_windows_default_shortcuts_are_free`), so a default the window manager or Windows itself holds fails the suite.

### Settings UI

Unit tests run in Node with the WebAssembly core loaded from disk (`apps/settings/ui/src/test/loadWasm.ts`). End-to-end tests run the UI in headless Chromium against the fake daemon (`apps/settings/ui/src/bridge/fake/`): `openApp(page, "windows")` in `e2e/support.ts` picks the platform, and `sentCommands` and `window.snappyFake` let a test check what the UI sent or change the daemon's state. Both need the WebAssembly core built first (`pnpm -C apps/settings/ui wasm`, which `tools/check.sh` runs).

## Naming tests

Name a test for what it checks, as a short sentence in plain words:

- Rust: `fn a_refused_move_reverses_the_switch()`, `fn resizing_outside_the_title_bar_cancels_for_good()`.
- TypeScript: `it("errors stay until dismissed, and dismissing tells the daemon")`, `test("deleting a custom layout asks first")`.
- Live scenarios: `("switching_workspaces_parks_and_returns_windows", workspace_switch)`.

Names must be unique within a file. Don't put ticket numbers or other IDs in names.

## Live tests

`crates/live` runs the real daemon against real windows with real input and checks where the windows actually end up. It has two programs:

- `snappy-fixture` opens plain windows on request over standard input and output (JSON lines): normal ones, ones with a minimum size, and ones that size in steps like a terminal. Each window reports its own frame, which is the ground truth.
- `snappy-live` writes scratch settings (the platform's defaults with snapping on, the halves layout everywhere, and workspaces on with a shortcut for each), starts the daemon with `SNAPPY_HOME` pointing at a scratch folder and `SNAPPY_DEBUG=1`, posts pointer and keyboard input the way hardware does, runs the scenarios in `crates/live/src/scenarios.rs`, and writes `results.json` plus a screenshot of each failure. The overlay scenario also saves `overlay_caption.png`, a drag with the swap key held, to show the caption each platform draws. `--only <prefix>` runs the scenarios whose names start with it.

Scenarios take every shortcut and modifier from the run's settings, with the Snappy key expanded, so the same scenario presses Control+Option+Command+1 on macOS and Ctrl+Alt+Super+1 on Linux. The runs use the default "When the zone is taken", swap, so plain shortcuts and drags into an occupied zone swap, and a drag holding the swap key stacks. Drags let go of the modifiers right after the button, as quickly as input allows, so an adapter that reads modifiers late loses the drop. They read the daemon's debug log for what Snappy measured and decided itself, such as settle times and the overlay's caption.

Every suite runs in a sandbox, never on the machine you're working on. Each takes an optional scenario name prefix.

### Linux

```sh
tools/live/linux/live.sh [scenario prefix]
```

Needs Docker. In the development container it builds the daemon, runner, and fixture, starts a headless X desktop (`x11-session.sh`: Xorg on the dummy driver with two side-by-side 1920×1080 outputs, the evdev US keymap, Xfwm4, and a D-Bus session), runs the scenarios on the primary output, and copies `results.json`, screenshots, the daemon's output (`home/daemon.log`), and its logs to `target/live/linux/out/`.

- Input comes from XTest, which the server treats like a device, so the window manager's moves, Snappy's raw input events, and its key grabs all see it.
- The container runs no tray host, so the menu bar scenario checks that the daemon registered its StatusNotifierItem.
- Screenshots are of the whole X screen, through `xwd` and netpbm.

### GNOME on Wayland

```sh
tools/live/gnome/live.sh [--gnome 46,47,48,49] [--static-workspaces] [scenario prefix]
```

Needs Docker. It builds the daemon, runner, and fixture in an Ubuntu 24.04 container (`Dockerfile.build`), then for each GNOME version asked for (46 by default) starts a container from `Dockerfile` (Ubuntu 24.04 for GNOME 46, Fedora 41, 42, and 43 for 47, 48, and 49) and runs `suite.sh` inside a fresh headless GNOME Shell (`gnome-session.sh`: two side-by-side 1920×1080 virtual monitors, Xwayland, its own system and session buses, animations off). Results, screenshots, the daemon's logs, and GNOME Shell's log land in `target/live/gnome/<version>/`.

- **Installing is part of the run.** The daemon's first launch must install its extension and turn it on; then `restart-shell` stops GNOME Shell and starts it again in the same session, as logging out and back in would, and the extension must be running and speak this protocol version (`install.txt`).
- **Twice over.** The scenarios run with the fixture's windows on Wayland (`wayland/`), then through Xwayland (`xwayland/`). Wayland has no way to ask to size in steps, so the grid scenario is skipped, with that reason, in the Wayland run.
- **Input and frames** come from a test-only extension, `tools/live/gnome/test-extension`, loaded only in these containers: Clutter virtual devices inject pointer and key events that GNOME Shell handles like hardware, and it places and reads fixture windows by their titles, since Wayland apps can't. It also lists windows and Snappy's overlays front to back, checks the top bar item, and takes screenshots. The shipped extension has no test-only code.
- `--static-workspaces` gives GNOME a fixed number of workspaces instead of dynamic ones, which changes where the parking workspace goes.
- **Input like a hand's.** Drags rest on the title bar before the press, take a first step just past GTK's 8-pixel drag threshold (GNOME draws Xwayland title bars with GTK, which ignores a press whose first movement jumps far away), and wait until GNOME Shell is moving the window before covering the rest. Fixture windows are focused by activating and then clicking them, and a freshly started GNOME Shell gets one throwaway window clicked and dragged before the scenarios: GNOME 49 ignores a Wayland app's title bar drags until the session has handled some input. Between scenarios the button and every modifier key the tests press are let go, so a failure doesn't leave Shift held for the next. A drag GNOME Shell never starts moving, or a window that never takes focus, fails the scenario instead of carrying on.
- `gnome-session.sh` refuses to run outside a container: it starts a GNOME Shell and rewrites GNOME settings.
- To look around by hand, run a shell in the session: `docker run --rm -it --init -v "$PWD:/work" snappy-gnome:46 /work/tools/live/gnome/gnome-session.sh bash`.

### macOS

```sh
tools/live/macos/run.sh [scenario prefix]
```

Needs Tart and a VM named `snappy-macos` (`SNAPPY_MACOS_VM` overrides the name), cloned from `ghcr.io/cirruslabs/macos-sequoia-base` with SIP disabled, so the script can grant Accessibility through the VM's TCC database. The script bundles the signed app, builds the runner and fixture, starts the VM headless if needed, copies everything in, grants Accessibility to this exact build by its code signature, runs the scenarios through `tart exec`, and copies `results.json`, screenshots, and the daemon's logs to `target/live/macos/out/`. Nothing appears on your screen.

### Windows

```sh
tools/live/windows/create.sh                  # once: download Windows 11 ARM64 and install it unattended
tools/live/windows/start.sh                   # boot headless and wait for SSH
tools/live/windows/test.sh [test filter]      # the adapter's unit and integration tests, on the VM's desktop
tools/live/windows/live.sh [scenario prefix]  # the live scenarios
tools/live/windows/stop.sh
```

Needs QEMU, wimlib, xorriso, and `cargo-xwin`. The VM is Windows 11 Pro ARM64 under QEMU with Hypervisor.framework; its disk, ISOs, and SSH key live in `~/VMs/snappy-windows/` (`SNAPPY_WINDOWS_VM_DIR` moves them). The user is `snappy`, password `snappy`, logged on to the console automatically. SSH listens on `127.0.0.1:2222`.

- `live.sh` cross-compiles the daemon, runner, and fixture, copies them in, runs the scenarios on the VM's desktop, and copies `results.json`, screenshots, `home/daemon.log`, and the daemon's logs to `target/live/windows/out/`.
- Programs started over SSH run in session 0, which has no desktop. `desktop.sh '<command>' [timeout]` runs a command on the console desktop through a scheduled task and returns its output; `test.sh` and `live.sh` use it.
- `ssh.sh` runs a PowerShell command in the VM, `copy.sh` copies files in and out (`vm:` paths are in the VM), `screenshot.sh` saves the VM's display as a PNG, and `reset.sh` returns the VM to the freshly installed snapshot. A VNC server on `127.0.0.1:5915` lets a person look.
- Input is injected with `SendInput` by scan code, which Windows handles like a hardware keyboard.
- A tool killed while it holds a synthetic key down leaves that key down for the whole session. Release it, or restart the VM.

## Before you commit

```sh
tools/check.sh
pnpm -C apps/settings/ui e2e
```

`tools/check.sh` runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, and the settings UI's `check` script (WebAssembly build, type check, ESLint, Prettier, Vitest, and a production build). Both must pass.

When you change an adapter, also lint and test it where it compiles. The GNOME adapter compiles on any Linux machine, and `tools/live/gnome/test.sh` runs its integration tests against real GNOME Shells: `tools/live/linux/run.sh cargo clippy -p snappy-x11 --all-targets -- -D warnings` and `tools/live/linux/run.sh tools/live/linux/x11-session.sh` for X11, `cargo xwin clippy --target aarch64-pc-windows-msvc -p snappy-windows --all-targets -- -D warnings` and `tools/live/windows/test.sh` for Windows. Run the live suite for the platform when a change affects how windows really move.
