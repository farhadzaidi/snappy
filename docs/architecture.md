# Architecture

How Snappy is put together: its two programs, its crates, the engine at its center, and the paths the main features take through them. [platforms.md](platforms.md) covers how each desktop's adapter does its part, and [testing.md](testing.md) how the tests are layered. Commands for building, running, and testing are in [CONTRIBUTING.md](../CONTRIBUTING.md).

## Two programs

- **The daemon**, `snappy` (`apps/snappy`), runs all the time. It watches input, moves windows, draws the zone overlays, owns the menu bar or tray item, and is the only program that writes Snappy's files. Started with `--background`, as the login item does, it stays quiet; started any other way, it also opens the settings window.
- **The settings app**, `snappy-settings` (`apps/settings`), is started by the daemon on demand and quits when its window closes. It never touches other apps' windows: it sends commands and shows the state the daemon sends back.

```
                 ┌──────────────────────── snappy (daemon) ───────────────────────────┐
  OS events      │  main thread                     engine thread                     │
 ──────────────► │  adapter: hooks, monitors,  ───► engine ──► window-operation queue ─┼──► other apps'
                 │  hotkeys, overlays, tray   ◄───  (core's rules)        │            │    windows
                 │                                        └──► settings, state, logs   │
                 └───────────────────────────────▲────────────────────────────────────┘
                                                 │ piped stdin/stdout, length-prefixed JSON
                 ┌───────────────────────────────┴──── snappy-settings ───────────────┐
                 │  Tauri shell ◄──► React UI ──► snappy-core as WebAssembly           │
                 └────────────────────────────────────────────────────────────────────┘
```

### How they talk

The daemon starts the settings app with piped standard input and output (`apps/snappy/src/settings_app.rs`). The pipes are the only channel, so there is no socket to find or secure, and the settings app can't outlive the daemon that started it.

Each message is a 4-byte little-endian length and then UTF-8 JSON, capped at 8 MB (`crates/ipc`). Both sides open with `Hello` and a protocol version, and the settings app exits on a mismatch. From then on:

- **The settings app sends a `Command`** for each thing the user does: save a layout, record a shortcut, add a workspace, change the Snappy key, and so on.
- **The daemon answers every command with a `Reply`**, carrying a `Refusal` when it said no: a message for the user and, when the message names one of Snappy's shortcuts, that shortcut's `ShortcutId`, so the UI can link to the right recorder. It then sends a fresh `State`, the whole `AppState`, which is all the UI ever renders.

## Crates

```
core ◄── platform ◄── engine ◄── apps/snappy
 ▲          ▲  ▲                    │
 │          │  └── macos, windows, x11, gnome, kwin ◄──┘
 │          │      linux ◄── x11, gnome, kwin
 │          ├── ipc ◄── engine, apps/settings/src-tauri
 │          ├── fake        (engine tests)
 │          └── live        (live tests; also uses engine and the adapters)
 └── wasm ◄── apps/settings/ui
```

| Crate | Role |
|---|---|
| `core` | Every rule: geometry, layouts and zones, the layout editor, drag intent, snapping and settling, swaps, navigation and focus, filling empty zones, shortcuts and their conflicts, the Snappy key and Caps Lock, settings and their validation, undo, workspace sessions, which windows may be managed. No I/O, threads, or clocks. |
| `platform` | The traits every adapter implements (`WindowSystem`, `Input`, `HotKeys`, `CapsLock`, `Overlay`, `Permissions`, `DesktopGuard`, `Tray`, `LoginItem`, gathered as `Platform`), the `PlatformEvent`s adapters report, the overlay's shared look (`overlay_style`), the tray glyph, and the daemon's launch flags. |
| `engine` | Runtime state, the order things happen in, the window-operation queue, and every file Snappy writes. |
| `ipc` | The messages between the daemon and the settings app, and their framing. |
| `macos`, `windows`, `x11`, `gnome`, `kwin` | One adapter per desktop. Each one's dependencies sit behind a `cfg` for its OS, so the crate is empty elsewhere. |
| `linux` | What the Linux adapters share: one daemon per session and its signals, the autostart entry, `/proc` facts, recognizing the session, key codes, the menu, the overlay renderer, display IDs, the tray, XKB and a minimal Wayland client for Caps Lock and overlays, and D-Bus plumbing for the desktop-shell adapters. |
| `wasm` | A JSON API over `core` for the settings UI. |
| `fake`, `live` | Test support: an in-memory desktop for engine tests, and the live test runner and fixture app. Never linked into the daemon. |

They are separate crates for three reasons:

- **OS dependencies stay contained.** `objc2`, `windows`, `x11rb`, and `zbus` appear only in the adapters, `crates/linux`, and the live harness, so `core` and `engine` build and test on any machine.
- **`core` compiles to WebAssembly.** The settings app's editor and shortcut recorder call the same Rust the daemon uses, so a zone drawn in the editor lands exactly where the daemon puts the window, and a conflict the recorder reports is the one the daemon would.
- **The compiler enforces the layering.** `core` can't reach the engine, the engine can't call an OS API, and an adapter can't make a policy decision without depending on something it doesn't.

### Where code goes

| You're changing | It goes in |
|---|---|
| A rule: geometry, snapping, swaps, navigation, settings validation, shortcut conflicts, workspace bookkeeping | `crates/core`, with unit tests beside it |
| When something happens, or what state changes as a result | `crates/engine` |
| How an OS does something | its adapter: `crates/macos`, `crates/windows`, `crates/x11`, `crates/gnome` with its extension in `apps/gnome-extension`, or `crates/kwin` with its script in `apps/kwin-script`; what more than one Linux adapter needs goes in `crates/linux` |
| A capability every adapter must provide | a trait method in `crates/platform`, implemented by each adapter and by `crates/fake` |
| Something the settings app can ask for | a `Command` in `crates/ipc`, handled in `crates/engine/src/commands.rs` |
| A rule the settings UI needs too | `crates/core`, exported through `crates/wasm` and `apps/settings/ui/src/core/snappyCore.ts` |
| The settings UI | `apps/settings/ui/src/<page>` |

## The engine

### Threads

The adapter runs the OS event loop on the main thread, which AppKit and Win32 require, and forwards every event over a channel. The engine runs on a thread of its own, in a single-threaded Tokio runtime with a `LocalSet` (`apps/snappy/src/runtime.rs`). Neither blocks the other, and the engine's state needs no locks. If the engine thread ends for any reason, a panic included, the adapter's loop is stopped too, so the daemon exits rather than holding windows and keys with nothing behind them; the next start picks up from the saved files.

### State

Runtime state is one `EngineState` (`crates/engine/src/state.rs`): settings, displays, whether live control runs, the drag in progress, registered shortcuts, workspaces, the selection, the debug log, and the outbox to the settings app. Its behaviors are split by concern under `state/` (what's on screen, live control, shortcut registration, the overlay, the tray menu, logging, the status message, and what the settings app is sent). The window-operation queue and the undo histories sit beside it, in `ops.rs`. The features that act on it each have a module: `drag`, `actions` (shortcuts), `focus`, `selection`, `marks` (window outlines), `caps_lock`, `commands` (from the settings app), and `workspaces/`. `events.rs` routes each platform event to the features that care about it, and `engine.rs` is what the daemon sees: start, handle an input, shut down, or stop for a restart.

"Live control" is whether Snappy acts on windows right now: snapping is on, the platform's permission is granted, and the platform says it can (on GNOME and KDE, that its extension or script answers). Press monitoring and every shortcut but the workspace ones follow it.

### The window-operation queue

Every window move goes through one queue (`crates/engine/src/ops.rs`): snaps, swaps, undo and redo, placing the selection, workspace switches, and sends.

- **One at a time.** Operations never interleave, so two never fight over a window.
- **The newest wins.** Submitting cancels the operation in flight through its cancel token and waits for it to stop before starting. Settling checks the token before every read and write, so a stale operation stops within a step.
- **A press cancels.** While live control is on, a mouse press on any window cancels the operation in flight, so the user's hand beats Snappy's.
- **Protected windows.** A window pressed while an operation is in flight is marked protected, and the operation's recovery (putting windows back after a failure) leaves it to the user.

### Settling

Apps often acknowledge a position or size before their redraw applies it, insist on a minimum size, or size in character cells. `core::placement::bounds` writes a frame and waits for it to settle: it resizes first when shrinking on one display and moves first when growing would cross a screen edge, re-reads after short waits, accepts the size an app insists on, then puts the window at its final position. The engine drives it through the platform; the rule itself is tested against scripted windows and an instant clock. On success the original frame goes onto the undo history (50 moves, one history per workspace or one overall); on failure the window goes back where it was.

### Settings changes

Every command lands in `crates/engine/src/commands.rs`. A settings change copies the settings, applies the edit (the edits themselves are rules in `core::settings`, which the settings UI's fake daemon runs too), validates the whole (normalizing layouts and checking every shortcut conflict), saves atomically, then reconfigures what depends on settings: the debug log, workspaces, live control, shortcuts, overlays, and the tray. A rejected change leaves the old settings in place and replies with the reason.

### Diagnostics

Diagnostics never hold window titles, window contents, or keystrokes: only frames, app identifiers, outcomes, and timings. Failing to write them never affects window control.

- **The debug log** (`debug_log.rs`) is opt-in: one JSON object per line with a time, a level (`warn` marks a result that differs from what was asked for), and an event from `LogEvent` (`log_event.rs`), the names tests and the live suite match records by. Records are built in closures that run only while the log is on, so they cost nothing otherwise. It starts again after 5 MB, keeping the previous file.
- **Always-on summaries** (`diagnostics.rs`): the last 20 drags made with the drag key and how each ended (`Outcome`, in `outcome.rs`), with its settle steps; the last 30 workspace lifecycle events; and up to 120 switch timings, written at quit.

## How the main features work

### A drag

1. The adapter reports a press. If live control is on, the engine (`drag.rs`) cancels any operation in flight, asks the platform for the window under the pointer, starts a `DragIntent` (`core::placement::drag`) with its frame, and turns drag monitoring on.
2. Each pointer move or modifier change re-reads the frame. The intent decides whether this is a real move rather than a click or a resize: the window must travel a little, and a size change outside the title-bar strip cancels it until the next press.
3. While the drag key is held and the window has moved, the engine finds the layout for the display under the pointer, hit-tests its zones, and asks the adapter to show an `OverlayScene`. Holding the swap key too reverses "When the zone is taken" for this drop, and the hovered zone is captioned "Swap" or "Stack".
4. On release the engine checks the intent and the modifier again and finds the zone. A drop on an empty zone, or one that stacks, becomes a snap; one that swaps becomes a swap, which looks up the windows filling both zones and plans with `core::placement::swap`. Either goes through the queue and the settle loop, and the outcome is recorded in `drops.json` and the debug log.

### A shortcut

1. Whenever settings, displays, or live control change, the engine works out the shortcuts the current state needs (`core::shortcuts::desired_hotkeys`) and registers them if the set differs. Each comes with its `Combo`, the concrete key and modifiers with the Snappy key already expanded, so adapters never know the Snappy key exists.
2. The adapter reports a press by ID, and the engine maps it back to a `HotKeyAction` (`actions.rs`). Shortcuts do nothing while the settings app is recording one.
3. Zone and arrow shortcuts find the destination zone (`core::placement::navigate` for arrows) and submit a swap or a snap, as "When the zone is taken" says. Arrow presses that arrive while the last move is still settling continue from where it was headed, so three presses move three zones even in a slow app.
4. The nearest-empty-zone shortcut picks a zone with `core::placement::empty_zone`, or, with a selection, places every selected window into empty zones at once, pairing windows with zones so the total distance is least, as one undo step.
5. Focus shortcuts (`focus.rs`) ask `core::placement::focus` for the next window in a direction, moving between zones that hold a window when any does and by window centers otherwise, and outline it briefly. Selection shortcuts (`selection.rs`) keep an in-memory, ordered list of windows that sends and workspace switches take along.
6. Selected and just-focused windows are outlined on a layer of their own, apart from the zones (`marks.rs`, `Overlay::mark`), redrawn only on displays whose outlines changed.

### A workspace switch

A workspace is a group of windows, one group shown at a time. `core::workspace::Session` keeps each window's workspace, its frame, and where it waits while hidden, and makes every change all or nothing:

1. Write the journal of windows about to be parked first.
2. Bring incoming windows back before parking outgoing ones, so the screen is never empty mid-switch.
3. Check each move, and on a refusal reverse the moves already made. A window that closes meanwhile just leaves the session.

The engine (`workspaces/switching.rs`) refuses a switch while workspaces are paused or the target is already showing, and holds it while displays settle. Otherwise it enrolls the focused app's windows, takes the selection along, focuses the target workspace's last focused window, saves the active workspace and then membership, and refreshes shortcuts and overlays.

How a window is hidden is the platform's `ParkingPolicy`: on macOS at an exposed lower corner with a sliver left on screen, since macOS pulls fully off-screen windows back; on Windows entirely off screen; on Linux on a native desktop of Snappy's own, frames untouched.

- **Containment** (`workspaces/containment.rs`). When an app or the system brings a parked window back, it is parked again at once, tried once more a second later, and only then reported, naming its workspace.
- **Display changes** (`workspaces/events.rs`). Displays change in bursts, as when a laptop wakes. The engine waits until 350 ms have passed without a change, then re-places every window for the final displays in one session change, retrying once after 750 ms and otherwise hiding inactive workspaces' windows one by one.
- **Other desktops.** Workspaces belong to the native desktop (Space, virtual desktop) they started on (`DesktopGuard`), and pause on any other. When live control is lost, they're suspended without moving anything and resume when it returns.
- **Restarts** (`workspaces/recovery.rs`). Quitting returns every parked window. Stopping for a restart (`SIGUSR2`, or `snappy --stop-for-restart` on Windows), a crash, or a panic leaves the journal and membership files as they are, and the next start puts each window back in its workspace without moving it. Windows are recognized across processes by a `RecoveryKey`: process ID, process start time, and a per-window token from the adapter, so a reused process ID never matches. `core::workspace::relaunch` decides what each journaled window becomes.
- **Tabs.** On macOS each native tab is a window of its own; the session treats a tab group as one window, acting on the shown tab and carrying the hidden ones along.

### The Snappy key and Caps Lock

The Snappy key is one held key that stands for two or more modifiers the user chooses (`core::keys::SnappyKey`). Shortcuts store it as a symbol: a chord's `ChordMods` say "the Snappy key, plus these", or name concrete modifiers, and recording decides which once. Only `KeyChord::combo` expands it, so conflicts, reserved chords, availability probes, and registration all compare concrete combos, and labels always say "Snappy". Changing the key is checked as a whole first: every binding with the new expansion, then each moved chord against the OS (`core::shortcuts::availability_probes`), before anything is saved; then the shortcuts are registered again at once.

Caps Lock can stand for the Snappy key. The work is split three ways:

- **The adapter makes the key**, with what its OS offers (`CapsLock::hold_caps_lock_as`): held, Caps Lock acts as the Snappy key's modifiers. It refuses when something else already remaps Caps Lock, and reports when the key goes down, when something else is used while it's down (never what), and when it comes up.
- **Core decides what a press was** (`core::keys::caps_lock`): a tap when nothing else was used and it came up quickly, which the engine answers by toggling Caps Lock; otherwise a hold.
- **The engine keeps it in step** (`crates/engine/src/caps_lock.rs`) with the setting, the permission, whether the platform can control the desktop, and the Snappy key. While a change is in place it keeps what undoes it in `caps-lock.json`, so a crashed run's change is undone at the next start before anything else. When the system's keyboard settings change under Snappy, the adapter reports Caps Lock lost and the engine holds it again from the new settings, unless they now remap Caps Lock themselves.

## Files

`apps/snappy/src/paths.rs` decides where files live:

| | macOS | Windows | Linux |
|---|---|---|---|
| `settings.json` | `~/Library/Application Support/Snappy/` | `%APPDATA%\Snappy\config\` | `~/.config/snappy/` |
| State | the settings folder | `%LOCALAPPDATA%\Snappy\data\` | `~/.local/state/snappy/` |
| Logs | `~/Library/Logs/Snappy/` | `%LOCALAPPDATA%\Snappy\data\Logs\` | `~/.local/state/snappy/logs/` |

With `SNAPPY_HOME` set, the files go in that folder instead: `settings.json`, `state/`, and `logs/`. Linux's single-instance lock stays in `$XDG_RUNTIME_DIR`.

- **State:** `workspace-journal.json` (only while windows are parked), `workspace-membership.json`, `caps-lock.json` (only while Caps Lock is held as the Snappy key), and the single-instance lock on macOS. Linux keeps its lock in `$XDG_RUNTIME_DIR` when that is set.
- **Logs:** `drops.json`, `workspace-lifecycle.json`, `workspace-performance.json`, and the debug log, `debug.jsonl`.

The engine's store (`crates/engine/src/store/`) writes every file whole, through a temporary file renamed into place, readable only by the user. A settings file that can't be read is reported and never overwritten; Snappy runs on defaults. Each state file records the version of its shape; one that can't be read is moved aside as `<name>.unreadable.json` and Snappy carries on without it.

## The settings app

- **The Tauri shell** (`apps/settings/src-tauri`) relays frames between the pipes and the web UI, adds the Edit menu text fields need on macOS, and reports which display its window is on. Its content security policy allows only bundled assets; its one plugin is the file dialog for importing and exporting settings.
- **The React UI** (`apps/settings/ui/src`) has four pages, Layouts, Editor, Workspaces, and Settings, in the shell in `shell/`. `state/daemonClient.ts` sends commands and turns each `State` into what the pages render.
- **The WebAssembly core** (`crates/wasm`, loaded by `core/snappyCore.ts`) answers everything that is a rule: zone frames, editor gestures and guides, shortcut conflicts, titles and labels, the help list of defaults, limits, and validation. The UI never computes geometry or names a shortcut itself.
- **The fake daemon** (`bridge/fake/`) stands in for the pipe in development and in the UI's tests. It speaks the same messages and applies commands with the real core rules through WebAssembly.

The layout editor's rules are pure functions from zones to zones in `crates/core/src/editor/`: moving, resizing, drawing, cutting, merging, pasting, snapping to guides, and repairing overlaps. A property test checks that no sequence of them leaves zones overlapping, undersized, or more than 32. The UI keeps the draft, its selection, the clipboard, and undo (`editor/draft.ts`, `editor/editorStore.ts`), and sends the finished layout on save.

## Where the rules live

| Rule | Module in `crates/core/src` |
|---|---|
| Zones as fractions of the work area, gaps and padding, the zone under the pointer | `layout.rs` |
| Telling a move from a click or a resize | `placement/drag.rs` |
| Writing a frame and waiting for it to settle | `placement/bounds.rs` |
| Snap, swap, and undo, and their history | `placement/operations.rs` |
| When a window fills a zone, swap planning, "When the zone is taken", the swap key | `placement/swap.rs` |
| Moving between zones with the arrows | `placement/navigate.rs` |
| Moving focus in a direction | `placement/focus.rs` |
| The nearest empty zone, and placing the selection | `placement/empty_zone.rs`, `placement/assignment.rs` |
| Which windows may be managed | `eligibility.rs` |
| Chords, the Snappy key, reserved chords, Caps Lock presses | `keys/` |
| Shortcut IDs and titles, conflicts, what to register, probes, help | `shortcuts/` |
| The settings model, defaults, reading old files, validation, edits, resets | `settings/` |
| Workspace sessions, parking, focus after a switch, relaunch | `workspace/` |
| The layout editor | `editor/` |
