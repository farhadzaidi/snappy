# Development

## Toolchain

| Tool | For | Install |
|---|---|---|
| Rust (stable, edition 2024; 1.99.1 in the Linux image) | everything | `rustup` |
| `wasm32-unknown-unknown` target | the settings UI's WebAssembly core | `rustup target add wasm32-unknown-unknown` |
| `wasm-bindgen` CLI 0.2.130, matching the crate's pinned version | the same | `cargo install wasm-bindgen-cli --version 0.2.130` |
| pnpm 9 and Node | the settings UI | then `pnpm -C apps/settings/ui install` |
| Playwright's Chromium and WebKit | the UI's end-to-end tests | `pnpm -C apps/settings/ui exec playwright install chromium webkit`, once |
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

### The app icon

`assets/icon/snappy.svg` is the only source of Snappy's icon. `tools/icon/generate.sh` renders everything else in `assets/icon/` from it, and those files are committed, so builds need none of its tools:

| File | Used by |
|---|---|
| `snappy.icns` | both macOS bundles (`bundle.sh` copies it to `Contents/Resources/Snappy.icns`), and the settings app, which Tauri also gives it at run time |
| `snappy.ico` | the settings app's executable and window on Windows (`tauri.conf.json` names it; Tauri compiles it into a resource itself, which takes `llvm-rc` when cross-compiling) |
| `snappy.res` | `snappy.exe`, linked by `apps/snappy/build.rs`; it's the same images compiled into a Windows resource, which the linker takes directly, so no resource compiler is needed |
| `snappy.png` | the settings app's window icon on Linux |

Linux builds of the daemon embed the SVG itself and install it in the user's icon theme ([platforms.md](platforms.md#linux-x11)).

The menu bar, notification-area, tray, and GNOME top bar show a monochrome glyph instead, the icon's zones without its colors: a rounded window holding a solid half zone beside two dimmer quarter zones. `assets/icon/snappy-glyph.svg` is its source, drawn on a 16-unit grid so it lands on whole pixels at 16 and 32. It uses fills alone, because GNOME recolors a symbolic icon by setting the fill of every shape and would fill an outline drawn as a stroke. The same script renders it too:

| File | Used by |
|---|---|
| `assets/icon/snappy-glyph.pdf` | the macOS menu bar item, embedded in the daemon and shown as a template image, which the menu bar tints for light and dark |
| `apps/gnome-extension/icons/snappy-symbolic.svg` | the GNOME top bar item, the glyph in the gray of symbolic icons |

The Linux tray and the Windows notification area take pixels, and Windows asks for a size that follows the display's DPI, so `snappy_platform::glyph` draws the same shapes into a coverage mask, which the Linux tray shows in white and Windows in the overlays' mint. A test checks that the SVG and that module draw the same shapes, so change both together.

After changing either SVG, run `tools/icon/generate.sh` (it needs `rsvg-convert` from `brew install librsvg`, macOS's `iconutil`, and Node) and commit the SVG with what it wrote. Its output is the same for the same SVGs, so running it again leaves nothing to commit.

### Installing a new build without losing workspaces

Stop the running daemon for a restart, not a quit, before replacing it. Quitting (`SIGTERM`, `pkill -x snappy`, `killall Snappy`, or the menu's Quit) brings every parked window back on screen; stopping for a restart leaves every window where it is, and the new daemon puts each back in its workspace ([architecture.md](architecture.md#membership-the-journal-and-restarts)). On macOS:

```sh
pkill -USR2 -x snappy; while pgrep -x snappy >/dev/null; do sleep 0.2; done
ditto dist/Snappy.app /Applications/Snappy.app
open -g /Applications/Snappy.app --args --background
```

The loop waits for the old daemon to save its state and exit. `open` launches the new one through Launch Services, so macOS checks Accessibility for Snappy itself; `--background` starts it quietly, as the login item does. On Linux, `pkill -USR2 -x snappy` the same way, replace the binaries, and start `snappy --background`. On Windows, run `snappy.exe --stop-for-restart`, which returns once the daemon has exited, replace the files, and start `snappy.exe --background`.

The same commands work once for upgrading from a build that can't stop for a restart yet. On macOS and Linux such a build doesn't handle `SIGUSR2`, whose default action ends the process at once without running Quit, so its journal stays: it lists each parked window with its workspace, home frame, and parked position under the same recovery keys, and the new build picks it up as after a crash. Windows on the active workspace join it as visible windows always do. On Windows, `--stop-for-restart` fails against such a build; end it once with `taskkill /F /IM snappy.exe` instead, which also skips Quit.

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

### KDE Plasma on Wayland

`crates/kwin` builds on any Linux machine with `cargo build`. Its script, `apps/kwin-script`, is QML and JavaScript in KWin's declarative script format, built into the daemon (`crates/kwin/src/script.rs` lists every file, and a test checks that everything the files import is listed). The daemon writes it to the session's runtime folder and loads it into KWin at every start ([platforms.md](platforms.md#linux-kde-plasma-on-wayland)).

- Keep the script thin: it carries out requests and reports events, and every decision stays in Rust.
- A request, answer, or event that changes shape bumps `PROTOCOL_VERSION` in both `service.js` and `crates/kwin/src/protocol.rs`; a test checks they match.
- QML's `Timer` doesn't run in a KWin that isn't compositing, as in the sandbox, and its JavaScript is QML's dialect (`.pragma library`, `.import`), which Node can't check. `tools/live/kwin/test.sh` loads the script into a real KWin, so syntax errors and API mismatches fail there; KWin's messages, the script's included, are in `target/live/kwin/out/kwin.log`.
- `test.sh` builds with your own cargo into your own `target/`; it needs Bubblewrap, a KWin 6, and Xwayland installed, and no image.

## Running safely

The daemon moves other apps' windows, registers global shortcuts, and on macOS asks for Accessibility. **Never run a development or test build of it on the desktop you work at**, and never run `snappy-live` or `snappy-fixture` there either. This is about the builds you are working on; an installed copy is meant to run on your own machine. Use one of these instead:

- **The settings UI in a browser.** `pnpm -C apps/settings/ui dev` serves the UI at `http://localhost:5179` against the in-memory fake daemon. Add `?platform=windows` or `?platform=linux` to see another platform's modifier names and controls. In the browser console, `window.snappyFake` is the fake daemon, for inspecting sent commands or patching its state.
- **The Linux container.** `tools/live/linux/run.sh tools/live/linux/x11-session.sh <command>` runs any command against a headless two-monitor X desktop, and `tools/live/linux/live.sh` runs the real daemon against real windows there.
- **The GNOME containers.** `tools/live/gnome/gnome-session.sh <command>` inside a `snappy-gnome` container runs any command in a headless two-monitor GNOME Shell on Wayland; `tools/live/gnome/live.sh` and `tools/live/gnome/test.sh` run the live suite and the adapter's integration tests there. Never install a development build's extension into your own GNOME session: it replaces the installed copy's, and GNOME loads it at your next login.
- **The KWin sandbox.** `tools/live/kwin/kwin-session.sh <command>` runs any command against a headless two-output KWin on Wayland, sealed off from your desktop with Bubblewrap; `tools/live/kwin/test.sh` runs the KWin adapter's integration tests and the daemon there. Never load a development build's KWin script into your own session.
- **The test VMs.** `tools/live/macos/run.sh` and `tools/live/windows/live.sh` run the daemon inside VMs that nothing on your screen depends on. [testing.md](testing.md#live-tests) describes them.

## The debug log

The debug log records what Snappy saw, what it decided, and what happened: received events (pointer motion excepted), commands, registered shortcuts, snaps and swaps with each settle step, workspace switches with timings, and errors. Records are JSON lines with a timestamp `t`, a `level` (`warn` marks a result that differs from what was asked for), and an `event`. Window titles and keystrokes are never recorded.

- Turn it on in the settings app (Settings, Debug, Debug log), which then shows where it is written, or start the daemon with `SNAPPY_DEBUG=1` to force it on for one run; the live tests do this.
- It is `debug.jsonl` in the log folder ([architecture.md](architecture.md#files)). At 5 MB it moves to `debug.1.jsonl` and starts again.
- `crates/engine/src/debug_log.rs` writes it. To add a record, call `Core::trace` (info) or `Core::flag` (warn) with a closure building the JSON; the closure runs only while the log is on, so a record costs nothing otherwise.
- `drops.json` in the same folder keeps the last 20 modifier drags with their outcome and settle steps, even with the debug log off. It is the first place to look when a drop didn't snap.

## Where code goes

| You are changing | It goes in |
|---|---|
| A rule: geometry, snapping, swaps, navigation, settings validation, shortcut conflicts, workspace bookkeeping | `crates/core`, with unit tests beside it |
| When something happens, or what state changes as a result | `crates/engine` |
| How an OS does something | the adapter, `crates/macos`, `crates/windows`, `crates/x11`, `crates/gnome` with its extension in `apps/gnome-extension`, or `crates/kwin` with its script in `apps/kwin-script`; what more than one Linux adapter needs goes in `crates/linux` |
| A new capability every adapter must provide | a trait method in `crates/platform`, implemented in each adapter and in `crates/fake` |
| Something the settings app can ask for | a `Command` in `crates/ipc`, handled in `crates/engine/src/commands.rs` |
| A rule the settings UI needs too | `crates/core`, exported through `crates/wasm/src/api.rs` and `apps/settings/ui/src/core/snappyCore.ts` |
| UI | `apps/settings/ui/src/<page>` |

### Walkthrough: a new shortcut action

Undo and redo of window moves were added this way; follow `MoveHistory` and `HistoryStep` through the code as a worked example. Say the new action is "center the focused window".

1. **Settings model** (`crates/core/src/settings.rs`). Add the shortcut to the section it belongs to as an `Option<KeyChord>`, with its default in that section's `defaults`; a default is the Snappy key and one other key. Each section lists its shortcuts once, in an enum: `NavigationShortcut` for `Navigation`, `HistoryStep` for `MoveHistory`, `WorkspaceStep` for `Workspaces`, and `WorkspaceShortcut` for each `Workspace`. Add a variant, put it in the enum's `ALL`, give it a title, which is the only place it is written (the settings app, conflict messages, and probes all show it), and map it to its field in the section's `get` and `slot`. Every shortcut, wherever it lives, is a `ShortcutId` (`crates/core/src/shortcuts.rs`), which reads and sets its chord, gives its default and its title, and is what conflicts and the settings app know it by; everything that walks shortcuts goes through it: the validity checks, reading a file from before the Snappy key could be chosen, conflicts, registration, resets, and the defaults a file without the shortcut gets (`give_defaults`, which gives each its default where no other binding uses its chord). A shortcut with a default also names its settings file field (`field`), so `Settings::from_json` can tell a file that lacks it.
2. **Shortcut ownership** (`crates/core/src/bindings.rs`). Add a `HotKeyAction` variant and map the shortcut to it in `HotKeyAction::of`; `desired_hotkeys` registers it, navigation and history only while snapping is live, and conflicts and the Snappy key's availability probes name it by its title. Unit-test the defaults for conflicts and the registration.
3. **Core logic** (`crates/core`). Put the decision, such as the centered frame, in a pure function with unit tests.
4. **Engine** (`crates/engine/src/actions.rs`). Handle the new `HotKeyAction` in `hotkey`. Anything that moves a window goes through `Shared::submit`, typically with `run_snap`, so it is cancellable and undoable.
5. **IPC** (`crates/ipc/src/lib.rs`). Each section's shortcuts are set by one `Command` keyed by its enum: `SetNavigationShortcut { shortcut, chord }`, `SetHistoryShortcut { step, chord }`, and `SetWorkspaceStepShortcut { step, chord }`, which `crates/engine/src/commands.rs` hands to `change`, which validates, saves, and re-registers shortcuts. A shortcut added to a section needs no command of its own. A command that changes shape bumps `PROTOCOL_VERSION` in `crates/ipc` and in the fake daemon (`apps/settings/ui/src/bridge/fake/fakeDaemon.ts`).
6. **Settings UI.** Mirror the settings field in `apps/settings/ui/src/core/model.ts` (a navigation shortcut's field is also a `NavigationShortcut`) and list it in its section's shortcuts (`ShortcutSection::shortcuts` in `crates/core/src/reset.rs`): the Settings page draws a recorder, with its title, for every shortcut a section lists (`apps/settings/ui/src/settings/ShortcutGroups.tsx`). A new default shortcut also gets a line in help (`crates/core/src/guide.rs`). A recorder's conflict check (`core.recordingConflict`, which knows the shortcut by its `ShortcutId`) and the fake daemon's handling of its command (`apps/settings/ui/src/bridge/fake/reducer.ts`) already cover every shortcut in it.
7. **Tests.** An engine scenario in the feature's module of `crates/engine/tests/scenarios/` that assigns the chord, presses it through the fake platform, and checks where the window landed; a reducer test in `reducer.test.ts`; and a Playwright test in `apps/settings/ui/e2e/settings.spec.ts` for the recorder. Add a live scenario to the feature's module in `crates/live/src/scenarios/` when the action depends on how a real OS behaves.

## Code style and rules

### Errors and panics

- No `unwrap` or `expect` outside tests; the workspace lints deny both (`Cargo.toml`, `clippy.toml`). Propagate with `?`, or handle the `None` and `Err` cases explicitly.
- Errors are typed enums with `thiserror`. When a message reaches the user, the error's `Display` is that message, as in `WindowError` (`crates/core/src/error.rs`) and `SettingsError` (`crates/core/src/settings.rs`).
- Failures in diagnostics are ignored: logging never affects window control (`append_log` in `crates/engine/src/store.rs`).

### User-facing messages

Full sentences that say what happened and what to do next, in plain words: "Leave full screen before tiling this window.", "More than one window occupies that zone. Move one out before swapping." Name keys with the platform's own names (`Modifier::name` in `crates/core/src/keys.rs`), and the Snappy key as "Snappy" (`KeyChord::long_label`). Messages for the same situation read the same on every platform; each adapter keeps its own in `src/messages.rs`. Write as people speak, with contractions: "couldn't" and "can't", never "could not" or "cannot". Routine confirmations such as "Snapped." are quiet statuses that the settings app doesn't pop up.

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
