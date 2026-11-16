# Adapter tests

Each adapter is tested at three levels: unit tests of what's pure in it, a fake GNOME Shell for the GNOME adapter, and integration tests against a real desktop in a sandbox.

## Unit tests

Adapter unit tests cover what's pure in each adapter: eligibility facts, key maps, display ordering, menu structure, overlay rendering, and the Caps Lock rules. For Caps Lock, that's merging Snappy's key mapping with the user's on macOS, the hook's decisions and the `Scancode Map` on Windows, `kxkbrc` on KDE, and XKB options and keymaps in `crates/linux`.

On macOS they also check every default shortcut against the shortcuts macOS ships with.

They compile only on their own OS. `tools/cross-check.sh` lints the macOS and Windows adapters from any machine, and CI runs their unit tests on macOS and Windows.

## The fake GNOME Shell

The GNOME adapter also runs against a fake GNOME Shell on a private bus (`crates/gnome/tests/fake_shell/`). These tests check what the adapter asks the extension, how it reads the replies, and how the extension's events become engine events, in order.

The fake bus gets its own home folder and an in-memory settings backend, so nothing reaches the session the tests run in. A test that expects nothing more waits for a marker event the fake sends last, rather than for a set time.

## Integration tests

The integration tests drive a real window system with real input: windows, frames, eligibility, displays, drags, shortcuts, overlays, parking, and Caps Lock.

- **Only in a sandbox.** Each test skips itself, printing "skipped", unless an environment variable its sandbox sets says it's inside one: `SNAPPY_X11_TEST_DISPLAY`, `SNAPPY_GNOME_TEST`, `SNAPPY_KWIN_TEST`, or `SNAPPY_WINDOWS_TEST`.
- **One at a time.** They share one screen, pointer, and keyboard, so they run one test at a time.
- **Default shortcuts.** Each suite probes every default shortcut, so a default the desktop itself holds fails the suite.
- **The real daemon on KDE.** The KDE suite also runs the real daemon in its sandbox end to end: dragging a window into a zone around panels, moving it by shortcut, and switching workspaces with the default shortcuts.

[Sandboxes](sandboxes.md) describes where each suite runs.

## The `reference` modules

What the tests and the live harness read from an adapter, such as Snappy's own view of displays and key codes, lives in each adapter's `reference` module. It's compiled only with the adapter's `reference` feature, so the shipped daemon carries no test code.
