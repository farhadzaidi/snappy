# Development

## Toolchain

| Tool | For | Install |
|---|---|---|
| Rust (stable, edition 2024; 1.99.1 in the Linux image) | everything | `rustup` |
| `wasm32-unknown-unknown` target | the settings UI's WebAssembly core | `rustup target add wasm32-unknown-unknown` |
| `wasm-bindgen` CLI 0.2.130, matching the crate's pinned version | the same | `cargo install wasm-bindgen-cli --version 0.2.130` |
| pnpm 9 and Node | the settings UI | then `pnpm -C apps/settings/ui install` |
| Playwright's Chromium | the UI's end-to-end tests | `pnpm -C apps/settings/ui exec playwright install chromium` |
| `aarch64-pc-windows-msvc` target and `cargo-xwin` | Windows builds from the Mac | `rustup target add aarch64-pc-windows-msvc`, `cargo install --locked cargo-xwin` |
| Docker | Linux builds, the X11 tests, the Linux live suite | Docker Desktop or any engine |
| Tart | the macOS test VM | `brew install cirruslabs/cli/tart` |
| QEMU, wimlib, xorriso | the Windows test VM | `brew install qemu wimlib xorriso` |

`cargo-xwin` downloads the MSVC runtime and Windows SDK libraries on first use and links with the `rust-lld` that ships with Rust, so neither Visual Studio nor LLVM is needed.

## Building and bundling

```sh
cargo build                                # the daemon and every crate for this machine
pnpm -C apps/settings/ui build             # WebAssembly core, then the UI into apps/settings/ui/dist
cargo build -p snappy-settings             # the settings app, embedding the built UI
```

`pnpm -C apps/settings/ui wasm` rebuilds only the WebAssembly core into `apps/settings/ui/src/wasm/`, which is generated and not committed. The UI's `dev`, `build`, and `check` scripts run it first.

### macOS

```sh
tools/macos/signing-identity.sh            # once: create the local signing identity
tools/macos/bundle.sh                      # build and sign dist/Snappy.app
```

macOS ties the Accessibility grant to the app's code signature. `signing-identity.sh` creates a self-signed identity, "Snappy Local Signing", in its own keychain (`~/Library/Keychains/snappy-signing.keychain-db`) whose password lives in `~/.config/snappy/signing-keychain-password`, readable only by you. `bundle.sh` unlocks that keychain itself, so builds sign without a prompt, and every build keeps the same identity, so the grant survives rebuilds. `SNAPPY_SIGN_IDENTITY` overrides the identity. Without one, builds are signed ad hoc and macOS asks for Accessibility again each time.

### Windows

```sh
cargo xwin build --release --target aarch64-pc-windows-msvc -p snappy
cargo xwin clippy --target aarch64-pc-windows-msvc -p snappy-windows -p snappy-live -p snappy --all-targets -- -D warnings
```

The test VM is ARM64, so builds target `aarch64-pc-windows-msvc`. `.cargo/config.toml` links the C runtime statically, because the VM has no Visual C++ redistributable and Snappy shouldn't need one.

### Linux

Everything Linux runs in the development container (`tools/live/linux/Dockerfile`: Debian, Rust, a headless Xorg, Xfwm4, D-Bus, and X utilities):

```sh
tools/live/linux/run.sh cargo build -p snappy
tools/live/linux/run.sh cargo clippy -p snappy-x11 -p snappy-live -p snappy --all-targets -- -D warnings
```

`run.sh` builds the image and runs the command with the repository at `/work`.

### GNOME on Wayland

`crates/gnome` builds on any Linux machine with `cargo build`. Its extension, `apps/gnome-extension`, is plain JavaScript in GNOME Shell's module format, built into the daemon (`crates/gnome/src/extension.rs` lists every file, and a test checks the list matches the folder), so there is nothing to build for it. The daemon installs it at launch ([platforms.md](platforms.md#linux-gnome-on-wayland)).

- Keep the extension thin: it carries out requests and reports events, and every decision stays in Rust.
- A request, reply, or event that changes shape bumps `PROTOCOL_VERSION` in both `service.js` and `crates/gnome/src/protocol.rs`; a test checks they match.
- GNOME Shell's JavaScript API changes between releases. Calls that differ between GNOME 46 and 49 go through `compat.js`, and `tools/live/gnome/live.sh --gnome 46,47,48,49` runs every supported version.
- `tools/check.sh` checks the extension's syntax with Node.
- `tools/live/gnome/live.sh` and `test.sh` build in a container whose output goes to the `snappy-gnome-target` volume and the cargo registry to `snappy-cargo-registry`, so your own `target/` is untouched apart from the binaries copied to `target/live/gnome/`.

## Running safely

The daemon moves other apps' windows, registers global shortcuts, and on macOS asks for Accessibility. **Never run a development or test build of it on the desktop you work at**, and never run `snappy-live` or `snappy-fixture` there either. This is about the builds you are working on; an installed copy is meant to run on your own machine. Use one of these instead:

- **The settings UI in a browser.** `pnpm -C apps/settings/ui dev` serves the UI at `http://localhost:5179` against the in-memory fake daemon. Add `?platform=windows` or `?platform=linux` to see another platform's modifier names and controls. In the browser console, `window.snappyFake` is the fake daemon, for inspecting sent commands or patching its state.
- **The Linux container.** `tools/live/linux/run.sh tools/live/linux/x11-session.sh <command>` runs any command against a headless two-monitor X desktop, and `tools/live/linux/live.sh` runs the real daemon against real windows there.
- **The GNOME containers.** `tools/live/gnome/gnome-session.sh <command>` inside a `snappy-gnome` container runs any command in a headless two-monitor GNOME Shell on Wayland; `tools/live/gnome/live.sh` and `tools/live/gnome/test.sh` run the live suite and the adapter's integration tests there. Never install a development build's extension into your own GNOME session: it replaces the installed copy's, and GNOME loads it at your next login.
- **The test VMs.** `tools/live/macos/run.sh` and `tools/live/windows/live.sh` run the daemon inside VMs that nothing on your screen depends on. [testing.md](testing.md#live-tests) describes them.

## The debug log

The debug log records what Snappy saw, what it decided, and what happened: received events (pointer motion excepted), commands, registered shortcuts, snaps and swaps with each settle step, workspace switches with timings, and errors. Records are JSON lines with a timestamp `t`, a `level` (`warn` marks a result that differs from what was asked for), and an `event`. Window titles and keystrokes are never recorded.

- Turn it on in the settings app (Settings, Troubleshooting, Debug log), which then shows where it is written, or start the daemon with `SNAPPY_DEBUG=1` to force it on for one run; the live tests do this.
- It is `debug.jsonl` in the log folder ([architecture.md](architecture.md#files)). At 5 MB it moves to `debug.1.jsonl` and starts again.
- `crates/engine/src/debug_log.rs` writes it. To add a record, call `Core::trace` (info) or `Core::flag` (warn) with a closure building the JSON; the closure runs only while the log is on, so a record costs nothing otherwise.
- `drops.json` in the same folder keeps the last 20 modifier drags with their outcome and settle steps, even with the debug log off. It is the first place to look when a drop didn't snap.

## Where code goes

| You are changing | It goes in |
|---|---|
| A rule: geometry, snapping, swaps, navigation, settings validation, shortcut conflicts, workspace bookkeeping | `crates/core`, with unit tests beside it |
| When something happens, or what state changes as a result | `crates/engine` |
| How an OS does something | the adapter, `crates/macos`, `crates/windows`, `crates/x11`, or `crates/gnome` with its extension in `apps/gnome-extension`; what both Linux adapters need goes in `crates/linux` |
| A new capability every adapter must provide | a trait method in `crates/platform`, implemented in each adapter and in `crates/fake` |
| Something the settings app can ask for | a `Command` in `crates/ipc`, handled in `crates/engine/src/commands.rs` |
| A rule the settings UI needs too | `crates/core`, exported through `crates/wasm/src/api.rs` and `apps/settings/ui/src/core/snappyCore.ts` |
| UI | `apps/settings/ui/src/<page>` |

### Walkthrough: a new shortcut action

Undo and redo of window moves were added this way; follow `MoveHistory` and `HistoryStep` through the code as a worked example. Say the new action is "center the focused window".

1. **Settings model** (`crates/core/src/settings.rs`). Add the shortcut to `Settings` as an `Option<KeyChord>`, with its default in `Settings::defaults`. Add it to `SettingsFile`, the on-disk shape where every section is optional, so older files load. Include it in the validity checks.
2. **Shortcut ownership** (`crates/core/src/bindings.rs`). Add a `HotKeyAction` variant, emit it from `desired_hotkeys` (inside `if snapping_live` if it moves windows), and list the chord in `global_names` so conflicts name it. Unit-test the defaults for conflicts and the registration.
3. **Core logic** (`crates/core`). Put the decision, such as the centered frame, in a pure function with unit tests.
4. **Engine** (`crates/engine/src/actions.rs`). Handle the new `HotKeyAction` in `hotkey`. Anything that moves a window goes through `Shared::submit`, typically with `run_snap`, so it is cancellable and undoable.
5. **IPC** (`crates/ipc/src/lib.rs`). Add a `Command`, such as `SetCenterShortcut { chord }`, and handle it in `crates/engine/src/commands.rs` through `change`, which validates, saves, and re-registers shortcuts.
6. **Settings UI.** Mirror the settings field and the command in `apps/settings/ui/src/core/model.ts`, add a recorder in `apps/settings/ui/src/settings/SettingsPage.tsx`, give it a local conflict check in `apps/settings/ui/src/recorder/conflicts.ts`, and apply the command in the fake daemon (`apps/settings/ui/src/bridge/fake/reducer.ts`).
7. **Tests.** An engine scenario in `crates/engine/tests/scenarios.rs` that assigns the chord, presses it through the fake platform, and checks where the window landed; a reducer test in `reducer.test.ts`; and a Playwright test in `apps/settings/ui/e2e/settings.spec.ts` for the recorder. Add a live scenario in `crates/live/src/scenarios.rs` when the action depends on how a real OS behaves.

## Code style and rules

### Errors and panics

- No `unwrap` or `expect` outside tests; the workspace lints deny both (`Cargo.toml`, `clippy.toml`). Propagate with `?`, or handle the `None` and `Err` cases explicitly.
- Errors are typed enums with `thiserror`. When a message reaches the user, the error's `Display` is that message, as in `WindowError` (`crates/core/src/error.rs`) and `SettingsError` (`crates/core/src/settings.rs`).
- Failures in diagnostics are ignored: logging never affects window control (`append_log` in `crates/engine/src/store.rs`).

### User-facing messages

Full sentences that say what happened and what to do next, in plain words: "Leave full screen before tiling this window.", "More than one window occupies that zone. Move one out before swapping." Name keys with the platform's own names (`Modifier::name` in `crates/core/src/keys.rs`), and the Snappy key as "Snappy" (`KeyChord::long_label`). Messages for the same situation read the same on every platform; each adapter keeps its own in `src/messages.rs`. Routine confirmations such as "Snapped." are quiet statuses that the settings app doesn't pop up.

### Comments

- Say why, or state the constraint the code defends, in a line or two. Don't narrate what the next line obviously does.
- A module whose file name doesn't say everything starts with a `//!` line saying what it is for. Public items get a `///` line when the name alone doesn't say it all.
- No change history ("now", "used to", "fixed"), no ticket or spec references, no names of tools or assistants.
- `// SAFETY:` above every `unsafe` block, saying why it holds.

### Naming

- Names are plain words: `pointer_down`, `apply_settled`, `layout_for_display`. Avoid abbreviations except established ones (`id`, `pid`, `ax`, `hwnd`).
- Tests are named for what they check, as a sentence: `resizing_outside_the_title_bar_cancels_for_good`, `test("deleting a custom layout asks first")`. See [testing.md](testing.md#naming-tests).

### Organization

- One concept per module; split a file when it starts holding two. Most modules are a few hundred lines; tests sit at the bottom in `mod tests`.
- Small functions with obvious names, logical steps separated by blank lines.
- **Pure logic belongs in `core`.** It has no OS dependencies and does no I/O, threads, or clocks. Where logic needs an effect, `core` defines a small trait and the engine or tests implement it: `BoundsBackend` and `Clock` for settling, `WindowOps` for snaps and swaps, `WorkspaceIo` for workspace changes.
- **Platform code only in adapters.** Adapters translate; they don't decide. If an adapter is choosing between behaviors, that choice belongs in `core` or the engine.
- The engine is the only writer of settings, state, and log files.
- Shipped code has no test-only or demo branches. Tests use `crates/fake` or their own fakes.
- No blocking OS call on a UI thread, and nothing polls or wakes up while Snappy is idle. The macOS permission poll runs only while waiting for a grant.

### Dependencies

Each one is a cost: build time, binary size, and code that touches users' windows. Prefer the standard library and what the workspace already uses: `serde`, `serde_json`, `thiserror`, `tokio` (only `rt`, `time`, `sync`, `macros`), `directories`; `objc2` crates and `block2` on macOS, `windows` on Windows, `x11rb`, `ksni` (on its `async-io` backend, so the daemon runs no second async runtime), `tiny-skia`, and `zbus` (`async-io` and `blocking-api` only) on Linux, plus `softbuffer` in the live fixture app; `wasm-bindgen` in `crates/wasm`; `tauri` and `tauri-plugin-dialog` in the settings app; React and Vite in the UI. Ask the owner before adding anything new. Nothing may add network access, telemetry, or update checks; `apps/snappy/tests/privacy.rs` fails the build if the daemon links a network client.

### Formatting and lints

- Rust: `cargo fmt` (`rustfmt.toml`, 110 columns) and `cargo clippy --all-targets -- -D warnings`.
- TypeScript: strict mode, ESLint (`apps/settings/ui/eslint.config.js`), and Prettier (`apps/settings/ui/.prettierrc.json`, 110 columns): `pnpm -C apps/settings/ui format`.
- `tools/check.sh` runs all of them; see [testing.md](testing.md#before-you-commit).
- Adapter code only compiles on its own OS, so check it there: the Linux container for `crates/x11`, `cargo xwin clippy` for `crates/windows`.

### Commits

- The repository is `github.com/farhadzaidi/snappy`. Commit locally; push only when the owner asks.
- One logical change per commit, with a message that says what changed and why. Never rewrite pushed history.
- Commits carry the owner's identity, set for this repository only. Don't add co-author or attribution lines.

## Never touch the owner's desktop

- Never move, resize, hide, focus, or close the real windows on the machine you're working on.
- Never run a development or test build of the daemon, the live runner, or the fixture app in that session. Use the container or the VMs.
- Never change system settings, Accessibility or other privacy grants, login items, or keyboard shortcuts on that machine.
- The VMs and the container are yours to start, stop, and reset.
