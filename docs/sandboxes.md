# Testing on a real desktop

This page is for anyone who changed an adapter, or anything else that depends on how a real desktop behaves. It explains the two kinds of tests that need a real desktop, how to run them for each desktop, and how each sandbox keeps them away from yours.

## Never on your own desktop

These tests move windows, press keys, register shortcuts, and change keyboard settings. So they never run in the session you're working in. Each one runs in a desktop of its own: a container, a sealed-off KWin, or a virtual machine. The scripts are in `tools/live/`, and each copies its results to `target/live/<desktop>/`.

Each sandbox has two virtual monitors, except the VMs, which have one.

## The two kinds of desktop tests

### Adapter integration tests

These test one adapter against a real window system with real input: finding windows, reading and writing frames, displays, drags, shortcuts, overlays, hiding windows for workspaces, and Caps Lock. They live with each adapter, such as `crates/x11/tests/x11/`.

- **Only in a sandbox.** Each test skips itself, printing "skipped", unless its sandbox has set the environment variable that says it's inside one: `SNAPPY_X11_TEST_DISPLAY`, `SNAPPY_GNOME_TEST`, `SNAPPY_KWIN_TEST`, or `SNAPPY_WINDOWS_TEST`.
- **One at a time.** The tests share one screen, pointer, and keyboard, so the scripts run them one by one.
- **The default shortcuts must be free.** Each suite checks every default shortcut, so a default that the desktop itself already uses fails the suite.
- **On KDE, the real daemon too.** The KDE suite also runs the whole daemon: dragging a window into a zone around panels, moving it with a shortcut, and switching workspaces.

What these tests need to read from an adapter, such as its view of displays and key codes, lives in the adapter's `reference` module, compiled only with its `reference` feature. That keeps test code out of the shipped daemon.

### The live suite

The live suite, in `crates/live`, is the closest thing to a person using Snappy. It runs the real daemon against real windows with real input, and checks where the windows end up.

It's two programs:

- **`snappy-fixture`** opens plain windows on request: ordinary ones, ones with a minimum size, ones that size in steps like a terminal, and, on macOS, native tabs. Each window reports its own frame, which is the ground truth.
- **`snappy-live`** writes scratch settings, starts the daemon with `SNAPPY_HOME` pointing at a scratch folder and the debug log on, and runs the scenarios. It moves the pointer and presses keys the way hardware does, and writes `results.json` and a screenshot of each failure.

`snappy-live` stops any running Snappy and takes over the pointer and keyboard. So it refuses to start unless a sandbox script has set `SNAPPY_LIVE_SANDBOX=1`.

The scenarios are in `crates/live/src/scenarios/`, one file per feature. How each desktop is driven is in `crates/live/src/desktop/`. A few things to know when reading them:

- **Shortcuts come from the run's settings**, with the Snappy key expanded. So the same scenario presses Control+Option+Command+1 on macOS and Ctrl+Alt+Super+1 on Linux.
- **Scenarios read the debug log** for what Snappy measured and decided, such as how long a move took to settle or what a zone's caption said.
- **Drags let go fast.** They release the drag key right after the mouse button, as fast as input allows. An adapter that reads the modifiers too late loses the drop.
- **Some scenarios don't apply everywhere**, and skip with the reason, which counts as a pass. Tabs and display-size changes run only on macOS. Snapping to a terminal's cell grid is skipped for Wayland windows, which can't ask to be sized in steps.

Posted input doesn't take exactly the path a real keyboard does. On macOS it joins after the point where Caps Lock is remapped, and after the point where the window server swallows Control-Option-Command-Delete. So the Caps Lock scenario presses F18 there instead, and a passing scenario doesn't prove a chord reaches apps ([macOS](platforms/macos.md#limitations)).

## Running the tests

Every live script takes a scenario name prefix, to run only the scenarios whose names start with it. The GNOME, KDE, and Windows adapter test scripts take a test name filter.

### X11

**What you need:** Docker.

```sh
tools/live/x11/run.sh tools/live/x11/x11-session.sh    # the adapter's tests
tools/live/x11/live.sh                                  # the live suite
```

To run any other command in the X11 session, add it after `x11-session.sh`:

```sh
tools/live/x11/run.sh tools/live/x11/x11-session.sh xprop -root
```

**How it works.** A Debian container runs Xorg on the dummy driver, with two 1920×1080 outputs, a US keyboard layout, the Xfwm4 window manager, and a D-Bus session.

- Input comes from XTest, which the X server treats like a real device.
- Xfwm4 runs with its compositor on. `SNAPPY_X11_COMPOSITOR=off` runs it without.
- Nothing shows a tray, so the menu scenario only checks that the daemon registered its tray item.
- Build output goes to a Docker volume, so your own `target/` isn't touched. Only the results are copied out.

### GNOME on Wayland

**What you need:** Docker.

```sh
tools/live/gnome/test.sh                              # the adapter's tests
tools/live/gnome/test.sh --gnome 46,47,48,49          # the same, on several GNOME versions
tools/live/gnome/live.sh                              # the live suite
tools/live/gnome/live.sh --static-workspaces          # with a fixed number of GNOME workspaces
```

GNOME 46 is the default.

**How it works.** Each GNOME version gets a container: 46 on Ubuntu 24.04, and 47, 48, and 49 on Fedora. Each runs a headless GNOME Shell with two virtual monitors, its own system and session buses, and animations off.

- **It refuses to run outside a container.** The session script starts GNOME Shell and rewrites GNOME settings, so it checks first.
- **Installing the extension is part of the test.** The daemon's first launch has to install it. Then GNOME Shell restarts, as it would at the next login, and the extension has to answer.
- **Twice over.** The live scenarios run once with the fixture's windows on Wayland and once through Xwayland.
- **A test-only extension**, `tools/live/gnome/test-extension`, loaded only in these containers, provides the input and frames. It adds virtual devices that GNOME Shell treats like hardware, and places and reads the fixture's windows, which Wayland apps can't do for themselves.

### KDE Plasma on Wayland

**What you need:** KWin 6 and Bubblewrap to run your own KWin, sealed off; otherwise Docker.

```sh
tools/live/kwin/test.sh                  # the adapter's tests, including the daemon end to end
tools/live/kwin/live.sh                  # the live suite
tools/live/kwin/live.sh --container      # in Docker, even with KWin installed
```

To run any other command in the sealed-off KWin:

```sh
tools/live/kwin/kwin-session.sh <command>
```

**How it works.** A headless KWin with two virtual outputs, Xwayland, its own D-Bus session, and no GPU. It runs one of two ways:

- **With Bubblewrap**, when the machine has KWin 6 and Bubblewrap. This is your machine's own KWin, so the KWin under test is the one Snappy will meet. It gets a private `/tmp`, `/run`, and `/var`, its own network namespace, an empty environment, and sees only `/usr`, `/etc`, and a read-only copy of the repository. It refuses to start if anything from your desktop's session shows through.
- **In a Fedora container**, without KWin and Bubblewrap, or with `--container`. The container is the seal.

Without a GPU, KWin doesn't draw anything, so there are no screenshots. Frames, focus, and which surfaces Snappy has come from throwaway KWin scripts that answer over D-Bus. Whether the overlay showed comes from the daemon's debug log. Input goes through KWin's fake-input protocol.

As on GNOME, the live suite runs once on Wayland and once through Xwayland.

### macOS

**What you need:** an Apple Silicon Mac with [Tart](https://tart.run), and a Tart VM named `snappy-macos`, cloned from `ghcr.io/cirruslabs/macos-sequoia-base` with System Integrity Protection turned off. Set it up once by hand. `SNAPPY_MACOS_VM` names a different VM.

```sh
tools/live/macos/live.sh
```

**How it works.** The script starts the VM if it isn't running, copies in the signed build, the runner, and the fixture, grants Accessibility to that exact build, and runs the scenarios. Turning System Integrity Protection off is what lets the script grant Accessibility. The VM runs headless, so nothing appears on your screen.

There's no adapter test script for macOS. Its unit tests run on a Mac with:

```sh
cargo test -p snappy-macos --lib
```

### Windows

**What you need:** an Apple Silicon Mac with QEMU, wimlib, and xorriso, and the cross-compiling tools from [CONTRIBUTING.md](../CONTRIBUTING.md#if-you-changed-an-adapter):

```sh
brew install qemu wimlib xorriso
```

The tests run in a Windows 11 ARM64 VM under QEMU. Create it once. It installs unattended, in about 12 minutes after 5.5 GB of downloads:

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

To see its screen, put it back as it was right after the install, or run a command in it:

```sh
tools/live/windows/screenshot.sh
tools/live/windows/reset.sh
tools/live/windows/ssh.sh "Get-ComputerInfo | Select OsName, OsArchitecture"
```

**How it works.** The VM's large files live outside the repository, in `SNAPPY_WINDOWS_VM_DIR`, which is `~/VMs/snappy-windows` by default.

- Programs started over SSH land in a session with no desktop. So tests run on the VM's own desktop, through a scheduled task (`tools/live/windows/desktop.sh`).
- Input is sent with `SendInput` by scan code, which Windows treats like a hardware keyboard.
- While a window running as administrator is in front, Windows drops sent input, so the tests wait for such windows to close.

## Shared builds

The GNOME suites, and the KDE ones when they run in a container, build Snappy once in a shared Ubuntu 24.04 image (`tools/live/build/`). That's the oldest system they run on, so the same binaries run in every newer image. The X11 suites build inside their own container.
