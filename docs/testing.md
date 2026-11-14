# Testing

## Layers

| Layer | Where | Runs |
|---|---|---|
| Core unit tests | `mod tests` in each `crates/core/src` module, and `crates/core/src/workspace/tests.rs` | anywhere, `cargo test` |
| Engine scenarios | `crates/engine/tests/scenarios/`, plus unit tests in `crates/engine/src` | anywhere, `cargo test` |
| WebAssembly API | `crates/wasm/src/lib.rs`, `crates/wasm/src/gestures.rs` | anywhere, `cargo test` |
| Adapter unit tests | `mod tests` in each adapter module | on the adapter's OS |
| Adapter integration tests | `crates/x11/tests/x11.rs`, `crates/gnome/tests/gnome.rs`, `crates/kwin/tests/kwin.rs`, `crates/windows/tests/windows.rs` | the Linux container, the GNOME containers, the KWin sandbox, the Windows VM |
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
- `crates/core/src/keys.rs`: recording any modifiers held with any Snappy key keeps exactly the modifiers held, records the same chord again from its own keys, and survives a save.

### Engine scenarios

`crates/engine/tests/scenarios/` drives a real `Engine` over `FakePlatform` end to end: events in, window frames, overlays, shortcuts, files, and status messages out. It is one test program with a module per feature (dragging, swaps, history, shortcuts, the nearest empty zone, layouts, focus, the selection, workspaces, sending, restarts, desktop parking, tabs, the Snappy key, Caps Lock as the Snappy key, live control, and settings), and `support.rs` holds what they share:

- `scenario` runs a test on a single-threaded runtime with paused time, so settle waits and the focus gate take no real time.
- `live_engine(name, fake)` starts an engine on the halves layout with snapping live, from a settings file that gives every layout's zones the Snappy key and their number, since zones have no shortcuts by default. `default_workspaces(name, fake)` starts one with workspaces on and every shortcut at its default, for the workspace scenarios that press Snappy + 1 to 9, `.`, `/`, and Return (`press_default`, which presses any default, such as Snappy + I to L, Return, and `\` on a `live_engine` too). `started(name, fake)` starts one without either setup.
- Each test gets a scratch folder from its `name`, so names must be unique across the modules. `paths_kept` reopens the same folder to simulate a restart or a crash.
- `drag`, `release`, `press`, `hotkey_id`, `focus`, `enable_workspaces`, `switch_to`, and `send_to` act like the user. `enable_workspaces` gives the workspaces Control with their number to switch, since `live_engine`'s zones hold the Snappy key's digits.
- The fake records the outlines of marked windows per display with their styles (`FakePlatform::marks`), which the selection and focus scenarios check (`marks` and `marked`). Focusing a fake window raises it to the front of `stacking`.
- The focus and selection scenarios cover focus in each direction, ties going to the most recently focused window, nothing that way, nothing focused, focus between zones past a floating window and from a floating window, stacked windows going to the most recently focused, zones across displays, the debug log's mode and zone, the focus flash appearing and going after 400 ms of paused time, selecting with workspaces off, `\` clearing, a macOS file that cleared with Snappy + Delete loading with `\` and Delete being refused, Space placing two selected windows as one undo step and ignoring a window of an inactive workspace, windows that don't fit staying selected, `.` and `/` sending the selection, and switches taking it along. The Snappy key scenarios change the key to Control+Option and check that every shortcut is registered again at once with the new modifiers and a zone shortcut then moves the window, that a change landing a Snappy shortcut on another binding is refused with both named while nothing is saved or registered again, that a key of one modifier is refused, and that a change onto a chord another app holds is refused naming the chord, with the shortcuts unregistered for probing registered again, while another key whose chords are free is taken, that a change onto a chord Snappy itself holds for another layout's zone is not refused, and that a shortcut recorded without the Snappy key keeps its keys through key changes that come to match them and through a restart. The tab scenarios open a Terminal-like window with three tabs and check that taking it along or sending it moves every tab without ever moving a hidden one, that dragged-out tabs stay in the group's workspace, that a new tab joins its group even while the group is parked, that a restart puts tabs remembered apart into the shown tab's workspace, that sending a tab and switching takes the group's selected tab, hidden behind it, out of the selection rather than along, and that a group is selected and occupies a zone as one window. The Caps Lock scenarios turn the setting on and check the fake holds Caps Lock as the Snappy key's modifiers and the engine keeps what undoes it; that a refusal leaves the setting off and replies with the reason; that a quick press alone toggles Caps Lock, while one with something else used, one held past the limit, one during which a Snappy shortcut fired, and a release without its press don't, and that the debug log never names a key; that presses do nothing while the setting is off; that a new Snappy key holds Caps Lock again with its modifiers; that turning it off, quitting, and stopping for a restart restore Caps Lock and remove the undo file; that a crashed run's change is undone at the next start before Caps Lock is held again, and even once the setting is off; that without permission the setting is kept and Caps Lock is held once access is granted; and that a refusal at start keeps the setting, shows as `capsLockProblem`, and goes once the Snappy key changes and the platform accepts.

### The fake platform

`crates/fake` is an in-memory OS implementing every platform trait:

- **Displays** you can add, remove, and reorder; the main one is 1440×900 at scale 2 with a menu bar.
- **Windows** (`FakeWindow`) with an app ID, process ID, frame, and stacking order, plus behavior: a minimum size, resizing only while fully on one display (`strict_resize`), ignoring every write (`frozen`), hidden by its app, full screen, an open sheet, minimized, or on the parking desktop.
- **Native tabs** as macOS has them: a window hidden as a tab behind another (`tab_of`) is left out of the visible windows, the windows filling zones, and every window but minimized ones, keeps a stale frame, and shows when focused. `add_tab`, `select_tab`, and `tear_out` act like the user, and `tab_groups` reports the groups.
- **Focus and the pointer**, which tests set directly.
- **Permissions**: whether access is granted, how often access was requested and the settings page opened, and whether a grant is being waited for.
- **Caps Lock**: the modifiers Caps Lock stands for while held (`caps_lock_held_as`), a refusal to answer with (`caps_lock_refusal`), what a hold returns to undo it and the earlier runs' changes undone, and how many times Caps Lock was toggled.
- **Shortcuts**: what is registered, how many times, and which chords another app holds (`taken_elsewhere`). The availability probe reports a chord taken when another app holds it or the fake has it registered, as macOS does for Snappy's own.
- **Recorders** for overlay calls, the tray menu, window writes and bounds-change brackets in order, occupancy lookups, self-activation, and the login item, with an injectable login item error.
- **Desktop and time**: whether the managed desktop is current, the parking policy and platform name to imitate, a manual clock, and the delay a bounds change reports.

Apps that lag, ignore one axis, or size in cells are scripted in `core`'s own settle tests rather than in the fake.

### Adapter tests

Adapter unit tests cover the pure parts: eligibility rules, key maps, display ordering, menu structure, overlay styles and rendering. The GNOME adapter's also run anywhere `dbus-daemon` is installed against a fake GNOME Shell on a private bus (`crates/gnome/tests/fake_shell.rs`): what it asks the extension, how it reads the replies, and how the extension's events become engine events, in order. A test that expects nothing more waits for a marker event the fake sends last, not for a set time. The fake bus gets its own temporary home and an in-memory settings backend, so nothing reaches the session the tests run in. They only compile on their own OS, so `cargo test` on a Mac runs the macOS ones. On macOS they also check the default shortcuts against the system shortcuts macOS ships with (`src/system_shortcuts.rs`).

The integration tests drive a real window system and real input, and skip themselves (printing "skipped") unless their sandbox is present:

- `crates/x11/tests/x11.rs` runs when `SNAPPY_X11_TEST_DISPLAY` names a display, which `tools/live/linux/x11-session.sh` sets: `tools/live/linux/run.sh tools/live/linux/x11-session.sh`. `SNAPPY_X11_COMPOSITOR=off` runs the window manager without its compositor.
- `crates/gnome/tests/gnome.rs` runs when `SNAPPY_GNOME_TEST=1`, which `tools/live/gnome/test.sh` sets inside a throwaway GNOME Shell with Snappy's extension and the test helper loaded: `tools/live/gnome/test.sh [--gnome 46,47,48,49]`.
- `crates/kwin/tests/kwin.rs` runs when `SNAPPY_KWIN_TEST=1`, which `tools/live/kwin/test.sh` sets inside a sealed-off headless KWin: `tools/live/kwin/test.sh [test name filter]`. Its last test runs the real daemon there, adds a top and a bottom panel, drags a window with the drag key into a zone while checking the overlay covers the new work area, sends it to another zone by shortcut, and checks where KWin says it went and that the daemon logged no overlay trouble. See [KDE Plasma on Wayland](#kde-plasma-on-wayland).
- `crates/windows/tests/windows.rs` runs when `SNAPPY_WINDOWS_TEST=1`, which `tools/live/windows/desktop.sh` sets: `tools/live/windows/test.sh [test name filter]`.

They run one test at a time, since the tests share one screen, pointer, and keyboard. Each probes every default shortcut (`the_default_shortcuts_are_free` and `the_windows_default_shortcuts_are_free`), so a default the window manager or Windows itself holds fails the suite.

Caps Lock as the Snappy key is unit-tested in each adapter where its rules are pure: merging Snappy's key mapping with the user's on macOS (`src/caps_lock/key_mapping.rs`), the hook's decisions and reading the `Scancode Map` on Windows, `kxkbrc` and telling Caps Lock from the modifier keys on KDE, and the XKB option's files in `crates/linux/src/xkb.rs`. The integration tests drive it for real:

- `crates/windows/tests/windows.rs` (`caps_lock_held_fires_snappy_shortcuts_and_never_turns_itself_on`) injects Caps Lock by scan code: held with →, it reports the press, other input, the shortcut, and the release, and Caps Lock stays as it was; a tap alone is reported without other input and doesn't toggle by itself; `toggle_caps_lock` turns Caps Lock on and off without its own presses being reported; and after `restore_caps_lock` the key toggles Caps Lock as usual.
- `crates/x11/tests/x11.rs` holds Caps Lock as the Snappy key: holding it with → fires the registered shortcut, a lone tap is reported alone, a button counts as something else, holding never locks, toggling flips Lock, and restoring gives back exactly the original keysyms, modifier rows, and repeat. Undoing a crashed run's change twice changes nothing, `caps:escape` is refused, and a keymap reloaded with `setxkbmap` while held gets Caps Lock held again.
- `crates/gnome/tests/fake_shell.rs` covers the hold request (the modifiers, and the keys to grab while held, without Caps Lock), the XKB files being written and removed, the hold sent again to an extension that started over, a refusal naming the user's Caps Lock options, the Caps Lock events in order, and undoing a crashed run's files. `crates/gnome/tests/gnome.rs`, in real GNOME Shells, holds Caps Lock with an arrow and checks the shortcut fires and Caps Lock stays off; that a lone tap reports a press and a release, another key or a click something else used, and the modifiers pressed by hand nothing; that toggling and Shift + Caps Lock flip the state; that a new Snappy key rewrites the option and keeps the state; that restoring brings back Caps Lock's own toggle; that `caps:escape` is refused by name; and that the extension gives Caps Lock back when the daemon goes away. The GNOME live input presses keycode 66 through the test extension, which also reports the keymap's Caps Lock state (`capsLockOn`).
- macOS has no integration test for it: posted keys join the event stream past the HID remap, which only a real keyboard passes through, so the live scenario presses F18 instead, and the remap and the light can only be checked on real hardware.
- KDE's runs only where the KWin sandbox runs; it hasn't run against a real KWin.

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

- `snappy-fixture` opens plain windows on request over standard input and output (JSON lines): normal ones, ones with a minimum size, ones that size in steps like a terminal, and, on macOS, native tabs it can move into windows of their own. Each window reports its own frame, which is the ground truth.
- `snappy-live` writes scratch settings (the platform's defaults with snapping on, the halves layout everywhere, and workspaces on; every default shortcut is kept, and what has no default, the halves layout's two zones and each workspace's two send shortcuts, gets chords the platform leaves free), starts the daemon with `SNAPPY_HOME` pointing at a scratch folder and `SNAPPY_DEBUG=1`, posts pointer and keyboard input the way hardware does, runs the scenarios in `crates/live/src/scenarios/` (a module per feature, with `support.rs` holding the context they run in and what they share), and writes `results.json` plus a screenshot of each failure. The overlay scenario also saves `overlay_caption.png`, a drag with the swap key held, to show the caption each platform draws, and the first selection scenario saves `selection_outline.png`, a window selected and outlined. `--only <prefix>` runs the scenarios whose names start with it.

The workspace scenarios press the defaults users get: Snappy + 1 to 9 to switch by number, Snappy + `;` and `'` to step, Snappy + `/` three times to send three windows one after another while staying, and Snappy + Return to select one window and take it along to workspace 3 by number, two along by a step, and to unselect one again. They check that a selected window's outline shows, as a daemon surface over the work area at the floating layer, and goes when the window arrives.

The restart scenario sends one of two windows to workspace 2, stops the daemon for a restart (`Daemon::restart`: `pkill -USR2` on macOS and Linux, `snappy.exe --stop-for-restart` on Windows) and starts it again, then checks that neither window moved, the first workspace is still active, and the debug log's `workspacesStarted` record has one window rejoining workspace 2. It switches to workspace 2 and back to check the new daemon parks and returns both, then quits and checks the parked window comes back. It runs everywhere the suite runs, since every harness can restart the daemon. The engine scenarios cover the rest with the fake platform: a restart keeps every window's workspace, its parked windows, and the active workspace without a single window write; a crash does the same from the journal alone; a window found by its parked position when its token changed; a parked window whose workspace is showing at launch comes back; a process ID reused by a later launch is never matched; quitting after a restart still returns parked windows; tab groups and desktop parking survive a restart; and the selection doesn't.

On macOS, a tab scenario has the fixture open a window with two more native tabs, selects it, and takes it along to the next workspace, checking in the debug log that both hidden tabs went too; it then moves two tabs into windows of their own, as dragging them out does, and checks that all three windows park on switching back and return together. Other platforms skip it, since only macOS makes each tab a window of its own.

The Snappy key scenario restarts the daemon with the key changed in the run's settings, with workspaces off, to a pair of modifiers the sandbox leaves free with the arrows: Option+Command on macOS, Ctrl+Alt on Windows, and Ctrl+Shift on Linux, since Xfwm4 and GNOME Shell switch desktops with Ctrl+Alt and the arrows and GNOME moves windows between displays with Shift+Super and them. It puts a window in the left zone with the zone's own chord, which the run sets concretely and so keeps its keys (on Windows the default key's modifiers) through the change, presses Snappy + → and Snappy + ← with the new modifiers, and checks the window moves each time, then restarts the daemon with the run's settings. A unit test of `snappy-live` checks every platform's run settings take its key, so a clash shows on any machine before a sandbox run.

The Caps Lock scenario restarts the daemon with Caps Lock as the Snappy key on and workspaces off, checks the debug log says Caps Lock is held, puts a window in the left zone, holds Caps Lock and presses →, checking the window moves to the right zone and Caps Lock stays off, then taps Caps Lock alone twice, checking it turns on and then off and that the daemon logged two taps. It restarts the daemon with the run's settings and, except on macOS, taps Caps Lock twice more, checking it turns on and off as usual again. On macOS posted keys join the event stream past the point where the system remaps keys, so the scenario presses F18, the key Snappy has macOS make of Caps Lock, and reads Caps Lock's state from the event system.

The focus and selection scenarios press Snappy + L, K, I, L, and J across three windows side by side and one below the middle one, on the third workspace so no other app's window, such as the Windows VM's terminal, is in the way, checking in the debug log that each press starts from the window the one before focused and that focus ends on the middle window, which the right zone's shortcut then moves; put a window in each zone and float a third between them, a little lower, then press Snappy + L, J, and K from the left window and L and J from the floating one, checking that every press goes by zones, reaches only the zoned windows, and that K finds nothing, and that the right zone's shortcut then swaps the left window into it; select two floating windows and press Snappy + Space, checking they land in the empty zones nearest them as two snaps and lose their outlines; and restart the daemon with workspaces off to select two windows and clear their outlines with Snappy + `\`, moving nothing, before restarting it with the run's settings.

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

### KDE Plasma on Wayland

```sh
tools/live/kwin/test.sh [test name filter]
```

Needs Bubblewrap, KWin 6, and Xwayland on the machine, and no container image: the KWin under test is the machine's own. `kwin-session.sh` starts it inside a Bubblewrap sandbox with private `/tmp`, `/run`, and `/var`, its own network namespace, only `/usr`, `/etc`, and the repository visible (read-only, apart from `target/live/kwin/out/`), no GPU nodes, and an empty environment; it refuses to start if anything of the desktop's session is visible. Inside, a D-Bus session of its own runs `kwin_wayland --virtual` with two side-by-side 1920×1080 outputs and animations off.

- **Nothing reaches a screen.** Without GPU nodes KWin runs without compositing. It still manages, moves, and drags windows, maps layer-shell surfaces, and runs scripts, so the tests read frames from KWin and never pixels. The overlay is checked by asking KWin, through a throwaway script that reports back over D-Bus, which on-screen display surfaces each process has and where.
- **Panels** are layer-shell surfaces with the `dock` namespace and an exclusive zone, as Plasma's are, made by the tests themselves; one test changes only the reserved space, which KWin tells scripts nothing about.
- **Input** goes through KWin's `org_kde_kwin_fake_input` protocol, which KWin handles like a device. The device is created once, before any fixture window opens: when it appears the seat gains a pointer, and a winit app that is already running trips over that.
- **Frames** in the end-to-end test come from KWin itself: its runner interface finds the fixture window by title, and `getWindowInfo` reads its frame, so the check doesn't go through Snappy.
- KWin's log, with the scripts' messages, is `target/live/kwin/out/kwin.log`; the end-to-end daemon's settings, output, and logs are in `target/live/kwin/out/daemon/`.

The full live suite (`crates/live`) doesn't run on KWin yet.

### macOS

```sh
tools/live/macos/run.sh [scenario prefix]
```

Needs Tart and a VM named `snappy-macos` (`SNAPPY_MACOS_VM` overrides the name), cloned from `ghcr.io/cirruslabs/macos-sequoia-base` with SIP disabled, so the script can grant Accessibility through the VM's TCC database. The script bundles the signed app, builds the runner and fixture, starts the VM headless if needed, copies everything in, grants Accessibility to this exact build by its code signature, runs the scenarios through `tart exec`, and copies `results.json`, screenshots, and the daemon's logs to `target/live/macos/out/`. Nothing appears on your screen.

Posted keys don't pass every point a real keyboard's do: the window server drops a real Control-Option-Command-Delete before any app sees it, but lets a posted one through, so a passing scenario doesn't prove a chord reaches apps ([platforms.md](platforms.md#macos)).

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

When you change an adapter, also lint and test it where it compiles. The GNOME adapter compiles on any Linux machine, and `tools/live/gnome/test.sh` runs its integration tests against real GNOME Shells: `tools/live/linux/run.sh cargo clippy -p snappy-x11 --all-targets -- -D warnings` and `tools/live/linux/run.sh tools/live/linux/x11-session.sh` for X11, `tools/live/kwin/test.sh` for KWin, `cargo xwin clippy --target aarch64-pc-windows-msvc -p snappy-windows --all-targets -- -D warnings` and `tools/live/windows/test.sh` for Windows. Run the live suite for the platform when a change affects how windows really move.
