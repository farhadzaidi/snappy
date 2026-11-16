# Testing

How Snappy's tests are layered, what each layer covers, and where a test belongs. The commands to run them are in [CONTRIBUTING.md](../CONTRIBUTING.md#testing).

## The layers

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

Everything that needs nothing runs in `cargo test` on any machine. `tools/check.sh` runs it along with formatting, lints, and the app's checks.

## Where a test goes

- **A rule** is unit-tested in core.
- **Behavior across events, files, and time** is an [engine scenario](engine-scenarios.md).
- **What an OS actually does** is an [adapter integration test](adapter-tests.md) or a [live scenario](live-suite.md).

A bug fix comes with a test that fails without it.

## Core

Every rule in `crates/core` is tested next to its code, with no OS and no real time. Settling runs against scripted windows that lag, refuse, and size in cells, with an instant clock (`placement/bounds.rs`). Workspace sessions run against an in-memory store and window system.

Property tests (`proptest`) check invariants over generated inputs:

- any sequence of editor operations leaves a layout sound: no overlaps, no zone under the minimum size, no more than 32 zones (`editor/properties.rs`);
- a zone's frame stays inside the work area for any zone, work area, and scale (`layout.rs`);
- validating settings that already passed changes nothing, and saving and loading gives them back (`settings/validation.rs`);
- recording any modifiers with any Snappy key keeps exactly what was held, and survives a save (`keys/chord_mods.rs`).

## The daemon

`apps/snappy/tests/` checks the daemon as a whole: `privacy.rs` fails the build if the daemon links a network client, and `bundle.rs` checks what the macOS bundle declares and which system services the daemon calls.

## The app

Unit tests run in Node with the WebAssembly core loaded from disk, so the UI's logic is tested against the real rules.

End-to-end tests run the UI in headless Chromium and WebKit against the fake daemon, which applies commands with the same core rules. A test picks the platform, then checks what the UI sent or changes the daemon's state.

Every spec runs in both browsers, because the app is WebKit on macOS and Linux, and WebView2, which is Chromium, on Windows. WebKit differs in ways the tests account for: it doesn't focus a clicked button, it moves the mouse only by whole pixels, and Backspace outside a text field goes back a page.

## Naming tests

A test is named for what it checks, as a short sentence in plain words:

- Rust: `a_refused_move_reverses_the_switch`, `resizing_outside_the_title_bar_cancels_for_good`.
- TypeScript: `it("errors stay until dismissed, and dismissing tells the daemon")`, `test("deleting a custom layout asks first")`.
- Live scenarios: `switching_workspaces_by_number_parks_and_returns_windows`.

Names are unique within a file, and across the engine scenarios, since each one names its scratch folder. No ticket numbers or other IDs.

## Read next

- [Engine scenarios](engine-scenarios.md) and the fake platform.
- [Adapter tests](adapter-tests.md).
- [The live suite](live-suite.md).
- [Sandboxes](sandboxes.md), where every test that needs a real desktop runs.
