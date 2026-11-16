# Sandboxes

Snappy moves windows, grabs shortcuts, and changes keyboard settings. So every test that needs a real desktop runs in a desktop of its own, never in the session you're working in. The scripts live in `tools/live/`, and the commands to run them are in [CONTRIBUTING.md](../CONTRIBUTING.md#testing).

Each sandbox has two monitors, or the VM's one, and copies its results under `target/live/<desktop>/`.

## X11

`tools/live/x11/`. A Debian container runs Xorg on the dummy driver with two 1920×1080 outputs, the evdev US keymap, Xfwm4, and a D-Bus session.

- Input comes from XTest, which the server treats like a device.
- Xfwm4 runs with its compositor on; `SNAPPY_X11_COMPOSITOR=off` runs it without.
- Nothing runs a tray host, so the menu scenario checks only that the daemon registered its tray item.
- Build output goes to a Docker volume, so your own `target/` isn't touched; only the results are copied out.

## GNOME on Wayland

`tools/live/gnome/`. One container per GNOME version: 46 on Ubuntu 24.04; 47, 48, and 49 on Fedora. Each runs a headless GNOME Shell with two virtual monitors, its own system and session buses, and animations off.

- **Refuses to run outside a container.** The session script starts GNOME Shell and rewrites GNOME settings, so it checks first.
- **Installing the extension is part of the run.** The daemon's first launch must install it. Then GNOME Shell restarts, as a new login would, and the extension must answer.
- **Wayland and Xwayland.** The scenarios run twice, with the fixture's windows on Wayland and through Xwayland.
- **A test-only extension** (`tools/live/gnome/test-extension`), loaded only in these containers, provides input and frames: virtual devices that GNOME Shell handles like hardware, and placing and reading fixture windows, which Wayland apps can't do themselves.

## KDE Plasma on Wayland

`tools/live/kwin/`. A headless `kwin_wayland --virtual` with two outputs, Xwayland, its own D-Bus session, and no GPU. It runs one of two ways:

- **Bubblewrap**, when the machine has KWin 6 and Bubblewrap. This is the machine's own KWin, so the KWin under test is the one Snappy will meet. It gets a private `/tmp`, `/run`, and `/var`, its own network namespace, only `/usr`, `/etc`, and a read-only repository visible, and an empty environment. It refuses to start if anything of the desktop's session shows through.
- **A Fedora container**, without KWin and Bubblewrap, or with `--container`. The container is its own seal.

Without a GPU, KWin doesn't composite, so nothing is drawn and there are no screenshots. Frames, focus, and which surfaces Snappy has come from throwaway KWin scripts that answer over D-Bus, and whether the overlay shows comes from the daemon's debug log. Input goes through KWin's fake-input protocol, set up before any fixture window opens.

Like GNOME's, the live suite runs on Wayland and through Xwayland.

## macOS

`tools/live/macos/`. A Tart VM named `snappy-macos`, cloned from `ghcr.io/cirruslabs/macos-sequoia-base`, with SIP off so the script can grant Accessibility to the exact build by its code signature. It runs headless; nothing appears on your screen.

## Windows

`tools/live/windows/`. A Windows 11 ARM64 VM under QEMU on an Apple Silicon Mac, installed once, unattended, by `create.sh`. Its large files live outside the repository, in `SNAPPY_WINDOWS_VM_DIR` (by default `~/VMs/snappy-windows`).

- Programs started over SSH land in a session with no desktop, so tests run on the console desktop through a scheduled task (`desktop.sh`).
- Input is injected with `SendInput` by scan code, which Windows handles like a hardware keyboard.
- While an elevated window is in front, Windows drops injected input, so the tests wait for such windows to close.

## Shared builds

The GNOME suites, and the KDE ones when they run in a container, build their binaries once in a shared Ubuntu 24.04 image (`tools/live/build/`), the oldest system they run on, so the same binaries run in every newer image. The X11 suites build inside their own container.
