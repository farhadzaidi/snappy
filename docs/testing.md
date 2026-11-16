# Testing

How Snappy's tests are layered, what each layer covers, and how the ones that need a real desktop stay off yours. The commands to run them are in [CONTRIBUTING.md](../CONTRIBUTING.md#testing).

## Layers

| Layer | Where | Needs |
|---|---|---|
| Core unit and property tests | `mod tests` in `crates/core/src`, and `crates/core/src/workspace/tests.rs` | nothing |
| Engine scenarios | `crates/engine/tests/scenarios/`, plus unit tests in `crates/engine/src` | nothing |
| WebAssembly API | `crates/wasm/src/exports.rs` | nothing |
| Daemon checks | `apps/snappy/tests/` | nothing |
| Adapter unit tests | `mod tests` in each adapter and in `crates/linux` | the adapter's OS |
| GNOME adapter against a fake GNOME Shell | `crates/gnome/tests/fake_shell/` | Linux with `dbus-daemon` |
| Adapter integration tests | `crates/x11/tests/x11/`, `crates/gnome/tests/gnome/`, `crates/kwin/tests/kwin/`, `crates/windows/tests/windows.rs` | the adapter's sandbox |
| Live suite | `crates/live` | a sandbox for each desktop |
| App unit tests | `apps/app/ui/src/**/*.test.ts` (Vitest) | Node |
| App end to end | `apps/app/ui/e2e/` (Playwright) | Chromium and WebKit |

Everything that needs nothing runs in `cargo test` on any machine, and `tools/check.sh` runs it along with formatting, lints, and the UI's checks. The rule of thumb for where a test goes: a rule is unit-tested in core; behavior across events, files, and time is an engine scenario; what an OS actually does is an adapter integration test or a live scenario.

### Core

Every rule in `crates/core` is tested next to its code, with no OS and no real time: settling runs against scripted windows that lag, refuse, and size in cells, with an instant clock (`placement/bounds.rs`), and workspace sessions run against an in-memory store and window system. Property tests (`proptest`) check invariants over generated inputs:

- any sequence of editor operations leaves a layout sound: no overlaps, no zone under the minimum size, no more than 32 zones (`editor/properties.rs`);
- a zone's frame stays inside the work area for any zone, work area, and scale (`layout.rs`);
- validating settings that already passed changes nothing, and saving and loading gives them back (`settings/validation.rs`);
- recording any modifiers with any Snappy key keeps exactly what was held, and survives a save (`keys/chord_mods.rs`).

### Engine scenarios

`crates/engine/tests/scenarios/` drives a real `Engine` over the fake platform, end to end: platform events and app commands in; window frames, overlays, outlines, registered shortcuts, files, debug log records, and status messages out. It is one test program with a module per feature (dragging, swaps, history, shortcuts, the nearest empty zone, layouts, focus, the selection, workspaces, containment, sending, restarts, desktop parking, tabs, the Snappy key, Caps Lock, live control, and settings), and `support.rs` holds what they share:

- Each scenario runs on a single-threaded runtime with paused time, so settle waits, the focus flash, and display settling take no real time.
- Helpers start an engine in a known state (snapping live on the halves layout, or workspaces on with every default shortcut), and act like the user: drag, release, press a shortcut, focus a window, switch, send.
- Each test gets a scratch folder from its name, which is why names must be unique across the modules; reopening the same folder simulates a restart or a crash.

This is the layer for anything about order and state: what a refused move does to a switch, how a restart rejoins windows, what a burst of display changes settles to, which shortcuts are registered after a Snappy key change.

### The fake platform

`crates/fake` is an in-memory desktop implementing every platform trait. Its windows can behave like real apps: a minimum size, resizing only while fully on one display, ignoring every write, hidden by their app, full screen, showing a sheet, minimized, on the parking desktop, or hidden as a macOS-style tab behind another. Tests can add, remove, and rearrange displays, set focus and the pointer, grant or withhold permission, have another app hold a chord, refuse Caps Lock, and advance the clock. It records every overlay call, outline, window write, shortcut registration, and tray menu, so a scenario can check what Snappy showed as well as what it did.

### Adapters

Adapter unit tests cover what's pure in each adapter: eligibility facts, key maps, display ordering, menu structure, overlay rendering, and the Caps Lock rules (merging Snappy's key mapping with the user's on macOS, the hook's decisions and the `Scancode Map` on Windows, `kxkbrc` on KDE, XKB options and keymaps in `crates/linux`). On macOS they also check every default shortcut against the shortcuts macOS ships with. They compile only on their own OS; `tools/cross-check.sh` lints the macOS and Windows adapters from any machine.

The GNOME adapter also runs against a fake GNOME Shell on a private bus (`crates/gnome/tests/fake_shell/`): what it asks the extension, how it reads the replies, and how the extension's events become engine events, in order. The fake bus gets its own home folder and an in-memory settings backend, so nothing reaches the session the tests run in. A test that expects nothing more waits for a marker event the fake sends last rather than for a set time.

The integration tests drive a real window system with real input: windows, frames, eligibility, displays, drags, shortcuts, overlays, parking, and Caps Lock. Each skips itself, printing "skipped", unless an environment variable its sandbox sets says it's inside one (`SNAPPY_X11_TEST_DISPLAY`, `SNAPPY_GNOME_TEST`, `SNAPPY_KWIN_TEST`, `SNAPPY_WINDOWS_TEST`). They run one test at a time, since they share one screen, pointer, and keyboard. Each suite probes every default shortcut, so a default the desktop itself holds fails the suite. The KDE suite also runs the real daemon in its sandbox end to end: dragging a window into a zone around panels, moving it by shortcut, and switching workspaces with the default shortcuts.

What the tests and the live harness read from an adapter, such as Snappy's own view of displays and key codes, so that expectations match it, lives in each adapter's `reference` module, compiled only with its `reference` feature, so the shipped daemon carries no test code.

### The live suite

`crates/live` runs the real daemon against real windows with real input and checks where the windows actually end up. It has two programs:

- **`snappy-fixture`** opens plain windows on request over its standard input and output: ordinary ones, ones with a minimum size, ones that size in steps like a terminal, and, on macOS, native tabs. Each window reports its own frame, which is the ground truth.
- **`snappy-live`** writes scratch settings, starts the daemon with `SNAPPY_HOME` pointing at a scratch folder and the debug log on, posts pointer and keyboard input the way hardware does, runs the scenarios, and writes `results.json` with a screenshot of each failure. It refuses to start unless a sandbox script has set `SNAPPY_LIVE_SANDBOX=1`.

The scenarios (`crates/live/src/scenarios/`) are grouped by feature: moves, swaps, the overlay and menu bar item, workspaces (switching, stepping, sending, restarting, quitting, display changes), the selection, tabs, focus, the Snappy key, and Caps Lock. How each desktop is driven, its input, reading frames, and screenshots, is in `crates/live/src/desktop/`. Scenarios take every shortcut from the run's settings with the Snappy key expanded, so one scenario presses Control+Option+Command+1 on macOS and Ctrl+Alt+Super+1 on Linux. They read the daemon's debug log for what Snappy measured and decided, such as settle times and overlay captions. Drags let go of the modifier right after the button, as fast as input allows, so an adapter that reads modifiers late loses the drop.

A scenario that can't apply to a desktop skips with the reason, which counts as a pass: tabs and display-size changes run only on macOS, and so does the check that the menu opens every time; snapping to a cell grid is skipped for Wayland windows, which can't ask to size in steps.

Posted input doesn't pass every point a real keyboard's does. On macOS it joins the event stream after the HID remap and after the window server drops Control-Option-Command-Delete, so the Caps Lock scenario presses F18 there, and a passing scenario doesn't prove a chord reaches apps ([platforms.md](platforms.md#macos)).

### The app

Unit tests run in Node with the WebAssembly core loaded from disk, so the UI's logic is tested against the real rules. End-to-end tests run the UI in headless Chromium and WebKit against the fake daemon, which applies commands with the same core rules: a test picks the platform, then checks what the UI sent or changes the daemon's state. Every spec runs in both browsers because the app is WebKit on macOS and Linux and WebView2, which is Chromium, on Windows. WebKit differs in ways the tests account for: it doesn't focus a clicked button, it moves the mouse only by whole pixels, and Backspace outside a text field goes back a page.

## Sandboxes

Snappy moves windows, grabs shortcuts, and changes keyboard settings, so every test that needs a real desktop runs in one of its own, never in the session you're working in. Each sandbox has two monitors, or the VM's one, and copies its results to `target/live/<desktop>/`.

- **X11** (`tools/live/x11/`). A Debian container runs Xorg on the dummy driver with two 1920×1080 outputs, the evdev US keymap, Xfwm4, and a D-Bus session. Input comes from XTest, which the server treats like a device. Nothing runs a tray host, so the menu scenario checks only that the daemon registered its tray item.
- **GNOME on Wayland** (`tools/live/gnome/`). One container per GNOME version (46 on Ubuntu 24.04; 47, 48, and 49 on Fedora), each running a headless GNOME Shell with two virtual monitors, its own system and session buses, and animations off. The session script refuses to run outside a container, since it starts GNOME Shell and rewrites GNOME settings. Installing the extension is part of the run: the daemon's first launch must install it, then GNOME Shell restarts as a new login would, and the extension must answer. The scenarios run twice, with the fixture's windows on Wayland and through Xwayland. Input and frames come from a test-only extension loaded only in these containers (`tools/live/gnome/test-extension`): virtual devices that GNOME Shell handles like hardware, and placing and reading fixture windows, which Wayland apps can't do themselves.
- **KDE Plasma on Wayland** (`tools/live/kwin/`). A headless `kwin_wayland --virtual` with two outputs, Xwayland, its own D-Bus session, and no GPU. It runs either as the machine's own KWin inside Bubblewrap, with private `/tmp`, `/run`, and `/var`, its own network namespace, only `/usr`, `/etc`, and a read-only repository visible, and an empty environment, refusing to start if anything of the desktop's session shows through; or, without KWin and Bubblewrap or with `--container`, in a Fedora container, which is its own seal. Without a GPU KWin doesn't composite, so nothing is drawn and there are no screenshots: frames, focus, and which surfaces Snappy has come from throwaway KWin scripts that answer over D-Bus, and whether the overlay shows comes from the daemon's debug log. Input goes through KWin's fake-input protocol, set up before any fixture window opens. Like GNOME's, the live suite runs on Wayland and through Xwayland.
- **macOS** (`tools/live/macos/`). A Tart VM cloned from a macOS base image with SIP off, so the script can grant Accessibility to the exact build by its code signature. It runs headless; nothing appears on your screen.
- **Windows** (`tools/live/windows/`). A Windows 11 ARM64 VM under QEMU, installed once, unattended. Programs started over SSH land in a session with no desktop, so tests run on the console desktop through a scheduled task. Input is injected with `SendInput` by scan code, which Windows handles like a hardware keyboard. While an elevated window is in front, Windows drops injected input, so the tests wait for such windows to close.

The container suites build their binaries once in a shared Ubuntu 24.04 image (`tools/live/build/`), the oldest system they run on, so the same binaries run in every newer image.

## Naming tests

A test is named for what it checks, as a short sentence in plain words:

- Rust: `a_refused_move_reverses_the_switch`, `resizing_outside_the_title_bar_cancels_for_good`.
- TypeScript: `it("errors stay until dismissed, and dismissing tells the daemon")`, `test("deleting a custom layout asks first")`.
- Live scenarios: `switching_workspaces_by_number_parks_and_returns_windows`.

Names are unique within a file, and across the engine scenarios, since each one names its scratch folder. No ticket numbers or other IDs.
