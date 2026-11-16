# Working in Snappy

Guidance for coding agents working in this repository. [CONTRIBUTING.md](CONTRIBUTING.md) covers setting up, building, running, and testing; the pages in [docs/](docs/README.md) explain how the code is put together. Read the ones that touch what you're changing before you start.

## Safety

Snappy moves other apps' windows, registers global shortcuts, and changes keyboard settings, so a development build can disrupt the desktop it runs on.

- Never move, resize, hide, focus, or close the real windows on the machine you're working on.
- Never run a development or test build of the daemon (`snappy`), the live runner (`snappy-live`), or the fixture app (`snappy-fixture`) in the logged-in desktop session. Use the sandboxes in `tools/live/` (containers, the KWin sandbox, the VMs). An installed copy of Snappy that the user runs is theirs; leave it alone.
- Never change system settings, Accessibility or other privacy grants, login items, keyboard settings, or shortcuts on that machine. Never install a development build's GNOME extension or load its KWin script into the real session.
- The sandboxes, containers, and VMs are yours to start, stop, and reset.
- Anything that takes more than a few seconds (builds, test suites, VMs) runs in the background with its output in a file.

## Privacy

- No network access, telemetry, analytics, crash reporting, or update checks. `apps/snappy/tests/privacy.rs` fails the build if the daemon links a network client.
- Never read, show, log, or store window titles, URLs, window contents, or keystrokes. Diagnostics hold frames, app identifiers, outcomes, and timings only. Caps Lock and keyboard monitoring report that something else was pressed, never what.

## Architecture rules

- `crates/core` holds every rule: geometry, snapping, swaps, navigation, settings and their validation, shortcut conflicts, workspace bookkeeping. It has no OS dependencies and does no I/O, threads, or clocks; where logic needs an effect, core defines a small trait that the engine or tests implement.
- `crates/engine` owns all runtime state and is the only writer of settings, state, and log files. Every window move goes through its window-operation queue.
- Adapters (`crates/macos`, `crates/windows`, `crates/x11`, `crates/gnome`, `crates/kwin`) translate between the engine and the OS. They make no policy decisions: if an adapter is choosing between behaviors, that choice belongs in core or the engine. What more than one Linux adapter needs goes in `crates/linux`. A new capability every adapter must provide is a trait in `crates/platform`, implemented by each adapter and by `crates/fake`.
- The GNOME extension (`crates/gnome/extension`) and the KWin script (`crates/kwin/script`) stay thin: they carry out requests and report events.
- The app computes no rules itself; it asks core through `crates/wasm`.
- Shipped code has no test-only branches. Tests use `crates/fake` or their own fakes; read-only helpers for tests live in each adapter's `reference` module, built only with its `reference` feature.
- No blocking OS call on a UI thread, and nothing polls or wakes up while Snappy is idle. Keyboard events are observed only during a drag or a window operation, and, with Caps Lock as the Snappy key, Caps Lock itself.

## Code

### Errors

- No `unwrap` or `expect` outside tests; the workspace lints deny both. Propagate with `?` or handle the cases.
- Errors are typed enums with `thiserror`. When a message reaches the user, the error's `Display` is that message.
- A failure to write diagnostics never affects window control.

### User-facing text

- Short, plain full sentences that say what happened and what to do: "Leave full screen before tiling this window." Details for developers go to the logs, not the screen.
- Contractions, as people speak: "can't", "doesn't", never "cannot" or "does not".
- Name keys with the platform's own names, and the Snappy key as "Snappy". The same situation reads the same on every platform; each adapter keeps its messages in `messages.rs`.

### Names and constants

- Plain words (`pointer_down`, `layout_for_display`); only established abbreviations (`id`, `pid`, `hwnd`).
- A literal that code compares against or branches on, or a number with meaning, is a named constant or an enum at one home: event and outcome names, protocol names, file names, D-Bus names, key codes, timeouts, limits. One-off literals whose meaning is plain in place stay inline.
- Values shared between Rust and the JavaScript or TypeScript sides (protocol versions, op and event names, error codes) are checked by a Rust test that reads the other side's source.

### Organization

- One concept per module; split a file when it starts holding two. File names say what's in them.
- `mod.rs` and `lib.rs` are barrels: module declarations, re-exports, and the module's doc. Code goes in named files.
- Parallel things are laid out alike: the adapters use the same module names for the same concerns.
- Shared code goes to the narrowest crate every user can reach: `core` or `platform` for every platform, `crates/linux` for the Linux adapters, otherwise the adapter itself.

### Comments

- Say why, or the constraint the code defends, in a line or two. Don't narrate what the next line obviously does.
- A module whose name doesn't say everything starts with a `//!` line. Public items get a `///` line when the name doesn't say it all.
- No change history ("now", "used to", "fixed"), no ticket or spec references, no names of tools or assistants.
- `// SAFETY:` above every `unsafe` block.

### Tests

- Named for what they check, as a sentence: `resizing_outside_the_title_bar_cancels_for_good`.
- A bug fix comes with a test that fails without it.
- Rules are unit-tested in core; behavior across events in the engine's scenarios (`crates/engine/tests/scenarios`); OS behavior in the adapters' integration tests and the live suite. [docs/testing.md](docs/testing.md) describes each layer.

### Dependencies

Ask the owner before adding a dependency. Prefer the standard library and what the workspace already uses. Nothing may add network access.

### Formatting

`cargo fmt` (110 columns) and `cargo clippy --all-targets -- -D warnings` for Rust; strict TypeScript, ESLint, and Prettier for the UI. `tools/check.sh` runs them all; `tools/cross-check.sh` lints the macOS and Windows adapters from any machine.

## Before committing

- `tools/check.sh` and `tools/e2e.sh` pass. A change to an adapter is also linted for its OS (`tools/cross-check.sh`) and run in its sandbox ([CONTRIBUTING.md](CONTRIBUTING.md#testing)).
- One logical change per commit, with a message that says what changed and why. No co-author or attribution lines.
- The repository is `github.com/farhadzaidi/snappy`. Commit locally; push, tag, or release only when the owner asks. Never rewrite pushed history.
