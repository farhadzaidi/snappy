# Working in Snappy

Read [README.md](README.md), then the doc for what you're changing: [docs/architecture.md](docs/architecture.md), [docs/platforms.md](docs/platforms.md), [docs/development.md](docs/development.md), and [docs/testing.md](docs/testing.md).

## Safety

- Never move, resize, hide, focus, or close the owner's real windows.
- Never run a development or test build of the daemon, `snappy-live`, or `snappy-fixture` in the owner's logged-in session. Run them in the Linux container or the test VMs ([docs/testing.md](docs/testing.md#live-tests)). The owner's own installed copy is not covered by this rule; leave it alone.
- Never change system settings, Accessibility or other privacy grants, login items, or keyboard shortcuts on the owner's machine.
- The repository is `github.com/farhadzaidi/snappy`. Commit locally; push only when the owner asks.

## Privacy

- No network access, telemetry, analytics, crash reporting, or update checks.
- Never read, show, log, or store window titles, URLs, window contents, or keystrokes. Diagnostics hold frames, app identifiers, outcomes, and timings only.

## Architecture

- `crates/core` has no OS dependencies and does no I/O. Effects go through traits it defines.
- The engine owns all runtime state and is the only writer of settings, state, and log files.
- Adapters translate between the engine and the OS. They make no policy decisions.
- Every window move goes through the engine's window-operation queue.
- No blocking OS call on a UI thread. Nothing polls or wakes up while Snappy is idle. Keyboard events are observed only during a drag or a window operation.
- Shipped code has no test-only branches. Tests use `crates/fake`.

## Code

- No `unwrap` or `expect` outside tests. Errors are typed with `thiserror`; user-facing errors carry their message.
- Comments say why, in a line or two. No change narration, no ticket or spec IDs, no tool or assistant names.
- Test names say what they check in plain words.
- New dependencies need the owner's approval.
- The rest of the style guide is in [docs/development.md](docs/development.md#code-style-and-rules).

## Before committing

- `tools/check.sh` and `pnpm -C apps/settings/ui e2e` pass. Adapter changes are also checked where they compile ([docs/testing.md](docs/testing.md#before-you-commit)).
- Anything that takes more than about ten seconds (builds, test runs, VMs) runs in the background with its output in a file.
- One commit per logical change, with a message that says what changed and why. Never rewrite pushed history. No co-author or attribution lines.
