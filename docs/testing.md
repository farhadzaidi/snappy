# Testing

This page is for anyone writing a test. It explains which kind of test fits which change, what each kind checks, and how tests are named. The tests that need a real desktop, and the sandboxes they run in, have a page of their own: [Testing on a real desktop](sandboxes.md).

## Which kind of test should I write?

- **A rule**, such as where a zone lands or when a settings change is refused: a unit test in core, next to the rule.
- **Behavior over time**, such as what a refused move does to a workspace switch, or what Snappy does after a restart: an engine scenario.
- **What an OS really does**, such as whether a window lands where it was sent: an adapter integration test or a live scenario, run in a [sandbox](sandboxes.md).
- **Something in Snappy's window**: an app unit test, or an end-to-end test for what a user clicks through.

A bug fix comes with a test that fails without it.

## The kinds of test

Most tests need nothing but `cargo test` and run on any machine. `tools/check.sh` runs them along with formatting, lints, and the app's checks.

| Kind | Where | Runs on |
|---|---|---|
| Core unit and property tests | beside the code in `crates/core/src` | any machine |
| Engine scenarios | `crates/engine/tests/scenarios/` | any machine |
| The WebAssembly API | `crates/wasm/src/exports.rs` | any machine |
| The daemon's own checks | `apps/snappy/tests/` | any machine |
| Adapter unit tests | beside the code in each adapter and `crates/linux` | the adapter's OS |
| The GNOME adapter against a fake GNOME Shell | `crates/gnome/tests/fake_shell/` | Linux with `dbus-daemon` |
| Adapter integration tests | `crates/x11/tests/x11/`, `crates/gnome/tests/gnome/`, `crates/kwin/tests/kwin/`, `crates/windows/tests/windows.rs` | the adapter's sandbox |
| The live suite | `crates/live` | each desktop's sandbox |
| App unit tests (Vitest) | `*.test.ts` files beside the code in `apps/app/ui/src` | Node |
| App end-to-end tests (Playwright) | `apps/app/ui/e2e/` | Chromium and WebKit |

## Rules: core's tests

Every rule in `crates/core` is tested beside its code, with no OS and no real time. For example, settling runs against scripted windows that lag, refuse sizes, and size themselves in character cells, with a clock that never really waits (`placement/bounds.rs`). Workspace changes run against an in-memory window list and store.

Property tests, written with `proptest`, try many generated inputs against a promise that must always hold:

- No sequence of editor steps leaves a layout broken: no overlaps, no zone below the minimum size, no more than 32 zones (`editor/properties.rs`).
- A zone's frame stays inside the screen for any zone, screen, and scale (`layout.rs`).
- Checking settings that already passed the check changes nothing, and saving and loading gives them back (`settings/validation.rs`).
- Recording any modifiers, with any Snappy key, keeps exactly what was held, and survives a save (`keys/chord_mods.rs`).

## Behavior over time: engine scenarios

Engine scenarios, in `crates/engine/tests/scenarios/`, run the real engine end to end over a pretend desktop. Events and commands go in. Window frames, overlays, outlines, registered shortcuts, files, and debug log records come out to be checked.

This is where anything about order and state is tested. What does a refused move do to a workspace switch? Do windows rejoin their workspaces after a restart? What does a burst of display changes settle to? Which shortcuts are registered after the Snappy key changes?

The scenarios are one test program with a module per feature, such as `dragging.rs`, `workspaces.rs`, or `restarts.rs`. What they share is in `support.rs`.

- **No real waiting.** Each scenario runs with paused time, so settle waits and display changes take no real time.
- **Helpers act like a user.** A scenario starts from a known state, such as "snapping live on the halves layout", and then drags, releases, presses shortcuts, focuses windows, and switches workspaces.
- **A scratch folder each.** Every test gets a folder named after it, which is why test names must be unique across all the scenario modules. Starting a new engine on the same folder is how a scenario simulates a restart or a crash.

### The pretend desktop

`crates/fake` is a whole desktop in memory. Its windows can misbehave like real apps: insist on a minimum size, resize only while fully on one display, ignore every move, be hidden by their app, be full screen, show a sheet, be minimized, or be a hidden macOS tab.

A test can also add and remove displays, set focus and the pointer, withhold permission, have another app hold a chord, refuse Caps Lock, and move the clock forward.

The fake records everything Snappy asks of it: overlays, outlines, window moves, shortcut registrations, tray menus. So a scenario can check what Snappy showed as well as what it did. When you add something every adapter must do, `crates/fake` implements it too.

## Adapters: unit tests and the fake GNOME Shell

Adapter unit tests cover the parts of an adapter that don't need a live desktop: eligibility facts, key maps, the order of displays, the menu, overlay drawing, and the Caps Lock rules. On macOS they also check every default shortcut against the ones macOS itself uses.

They compile only on their own OS. `tools/cross-check.sh` lints the macOS and Windows adapters from any machine, and CI runs their unit tests on macOS and Windows.

The GNOME adapter also runs against a **fake GNOME Shell** on a private D-Bus (`crates/gnome/tests/fake_shell/`). These tests check what the adapter asks the extension, how it reads the answers, and how the extension's events turn into Snappy's events, in order.

The fake bus gets its own home folder and in-memory settings, so nothing reaches the session the tests run in. A test that expects nothing more to happen waits for a marker event the fake sends last, rather than for a fixed time.

## The daemon's own checks

`apps/snappy/tests/` checks the daemon as a whole. `privacy.rs` fails the build if the daemon links a network client. `bundle.rs` checks what the macOS app declares and which system services the daemon calls.

## The app's tests

**Unit tests** run in Node, with the WebAssembly core loaded from disk, so the UI's logic is tested against the real rules.

**End-to-end tests** run the UI in headless Chromium and WebKit against the fake daemon. A test picks a platform, clicks through the UI, and checks what was sent to the daemon or what the UI shows.

Every test runs in both browsers, because the app runs in WebKit on macOS and Linux, and in WebView2, which is Chromium, on Windows. WebKit differs in a few ways the tests allow for: it doesn't focus a clicked button, it moves the mouse only by whole pixels, and Backspace outside a text field goes back a page.

Some end-to-end tests measure how text lays out, which depends on the fonts installed. So `tools/e2e.sh` runs them in Playwright's container, with the same fonts CI uses. It needs Docker:

```sh
tools/e2e.sh
```

Running them with your own browsers is quicker while you work on one test, but layout tests may disagree with CI. Install the browsers once, then run them:

```sh
pnpm -C apps/app/ui exec playwright install chromium webkit
pnpm -C apps/app/ui e2e
```

## Naming tests

A test's name says what it checks, as a short sentence in plain words:

- Rust: `a_refused_move_reverses_the_switch`, `resizing_outside_the_title_bar_cancels_for_good`.
- TypeScript: `it("errors stay until dismissed, and dismissing tells the daemon")`, `test("deleting a custom layout asks first")`.
- Live scenarios: `switching_workspaces_by_number_parks_and_returns_windows`.

Names are unique within a file, and across all the engine scenarios, since each one names its scratch folder. No ticket numbers or other IDs.
