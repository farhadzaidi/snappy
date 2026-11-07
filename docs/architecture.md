# Architecture

## Two programs

Snappy is two executables:

- **The daemon**, `snappy` (`apps/snappy`). It runs all the time: it watches input, moves windows, draws the zone overlays, owns the menu bar item, and is the only program that writes Snappy's files. Started with `--background` (as the login item does), it stays quiet; started any other way, it opens the settings window.
- **The settings app**, `snappy-settings` (`apps/settings`). The daemon starts it on demand, from the menu's "Open Snappy…", a second launch, or reopening Snappy.app, and it quits when its window closes. It never touches other apps' windows. It sends commands and shows whatever state the daemon sends back.

```
                 ┌──────────────────────── snappy (daemon) ──────────────────────────┐
  OS events      │  main thread                       engine thread                  │
 ──────────────► │  adapter: event monitors, hooks,   Engine ──► window-operation    │
                 │  hotkeys, observers, overlays,  ─► (core rules)   queue ──────────┼──► other apps'
                 │  tray  ◄── overlay/tray updates ──┘     │                         │    windows
                 │                                         └─► settings, state, logs │
                 └──────────────────────────────▲────────────────────────────────────┘
                                                │ piped stdin/stdout, length-prefixed JSON
                 ┌──────────────────────────────┴──── snappy-settings ───────────────┐
                 │  Tauri shell ◄──► React UI ──► snappy-core as WebAssembly          │
                 └────────────────────────────────────────────────────────────────────┘
```

### How they talk

`apps/snappy/src/settings_app.rs` spawns the settings app with piped standard input and output; standard error goes to the daemon's own. The pipes are the only channel, so there is no socket to find or secure, and the settings app's lifetime is tied to the daemon's.

Each message is a 4-byte little-endian length followed by UTF-8 JSON, capped at 8 MB (`crates/ipc/src/lib.rs`). Both sides send `Hello { protocolVersion }` first.

- **Daemon to settings app** (`ToSettings`): `State` (the full `AppState`: settings, displays, snapping and permission status, workspace status and window counts, the login item, the debug log's path, and the current status message), `Reply { id, error }` for every command, `ProbeResult` for a shortcut availability check, `PreviewHidden`, and `Focus`.
- **Settings app to daemon** (`ToDaemon::Command`): one `Command` per user action, such as `SetSnapping`, `SaveLayout`, `ApplyLayout`, `ShowPreview`, `BeginRecording`, `ProbeShortcut`, `AddWorkspace`, `SetHistoryShortcut`, `SetDebugLog`, `ImportSettings`, and `ExportSettings`. The full list is the `Command` enum.

After every command the daemon replies, then sends a new `State`, which is what the UI renders. On a protocol version mismatch the settings app exits.

## Crates and layering

```
core ◄── platform ◄── engine ◄── apps/snappy
 ▲          ▲           ▲             │
 │          ├── macos / windows / x11 ◄┘
 │          ├── fake (engine tests)
 │          └── live (live tests)
 ├── ipc ◄── engine, apps/settings/src-tauri
 └── wasm ◄── apps/settings/ui
```

| Crate | Depends on | Role |
|---|---|---|
| `core` | serde, thiserror | Geometry, layouts and presets, the editor's operations, drag intent, swaps, navigation, the nearest empty zone, shortcuts and conflicts, the settings model and validation, the bounds-settle algorithm, undo, and the workspace session. |
| `platform` | core | The `Platform` trait family (`WindowSystem`, `Input`, `HotKeys`, `Overlay`, `Permissions`, `DesktopGuard`, `Tray`, `LoginItem`) and `PlatformEvent`. |
| `engine` | core, platform, ipc, tokio | Runtime state, event handling, the operation queue, stores, diagnostics. |
| `ipc` | core, platform | `ToSettings`, `ToDaemon`, `Command`, `AppState`, and framing. |
| `macos`, `windows`, `x11` | core, platform, OS bindings | One adapter each. Every dependency is behind a `cfg(target_os)` table, so each crate is empty elsewhere. |
| `wasm` | core, wasm-bindgen | A JSON API over `core` for the settings UI. |
| `fake`, `live` | core, platform; `live` also the adapter for its OS | Test support; never linked into the shipped daemon. |

They are separate crates for three reasons:

- **Platform dependencies stay contained.** `objc2`, `windows`, and `x11rb` appear only in their adapter's manifest, so `core` and `engine` build and test on any machine.
- **`core` compiles to WebAssembly.** The settings app's editor and shortcut recorder call the same Rust functions the daemon uses, through `crates/wasm`, so a zone drawn in the editor lands exactly where the daemon will put the window.
- **The compiler enforces the layering.** `core` can't reach the engine, the engine can't call an OS API, and an adapter can't make a policy decision without importing something it has no dependency on.

## The engine

`crates/engine` owns all runtime state in one `Core` value (`crates/engine/src/core.rs`): settings, displays, whether live control is on, the drag in progress, registered shortcuts, workspaces, the debug log, and the outbox to the settings app.

### One thread

The engine runs on a dedicated thread inside a Tokio current-thread runtime and `LocalSet` (`apps/snappy/src/runtime.rs`). The OS adapter runs on the main thread, which AppKit and Win32 require, and forwards every event over a channel. The engine never blocks the main thread, and the state needs no locks: `Shared` (`crates/engine/src/ops.rs`) is a `RefCell<Core>` behind an `Rc`.

### The window-operation queue

Every window move goes through `Shared::submit` in `crates/engine/src/ops.rs`. Operations are snaps, swaps, undo and redo, workspace switches, and sends.

- **The newest operation wins.** Submitting cancels the operation in flight through its `CancelToken`, waits for it to stop, then runs the new one. Settling checks the token before every read and write, so a stale operation stops within one step.
- **One at a time.** Operations never interleave, so two moves never fight over a window. A generation counter makes sure only the latest operation's completion clears the in-flight state.
- **A press cancels.** A mouse press on any window cancels the operation in flight (`drag::pointer_down`), and Escape cancels both the drag and the operation.
- **Protected windows.** While a swap or a grouped undo is moving two windows, a press on either one adds it to `Core::protected`. The operation's recovery path (putting a window back after a failure) then leaves that window to the user.
- **Keyboard observation is scoped.** Drag monitoring, which includes key events, is on only during a drag or while an operation is in flight. At idle Snappy sees no keystrokes except its own registered shortcuts.

## How a drag becomes a snap

1. The adapter reports `PointerDown`. `drag::pointer_down` (`crates/engine/src/drag.rs`) ignores it unless live control is on, cancels any operation in flight, hides the layout preview, asks the platform for the window under the pointer (`window_at`), and starts a `DragIntent` (`crates/core/src/drag.rs`) with the window's frame. It turns drag monitoring on.
2. Each `PointerDragged` or `ModifiersChanged` reaches `drag::pointer_moved`, which rereads the frame and feeds it to `DragIntent::observe`. The intent decides whether this is a real move: the window must travel more than 2 points, and a size change of 3 points or more cancels it for good unless the press was on the title-bar strip.
3. While the drag modifier is held and the window has moved, `show_zones` resolves the layout for the display under the pointer (`Settings::layout_for_display`), hit-tests the zones (`core::layout::hit_test`), builds an `OverlayScene`, and calls `Overlay::show`. The setting's own action goes without a caption: occupants are looked up only at the drop, so the overlay can't tell whether the zone is taken. While the swap key reverses it, the hovered zone is captioned with the action that applies instead (`Caption`, "Stack" or "Swap", which adapters draw as given), and for "Swap" the window's current zone is outlined as the swap source. Each change of caption goes to the debug log as `overlayCaption`.
4. `PointerUp` reaches `drag::pointer_up`. `plan_drop` checks the intent and the modifier again, finds the zone under the pointer, and returns a `SwapMove` when the drop swaps or a `SnapRequest` when it stacks. Whether it swaps is "When the zone is taken" (`WhenTaken` in `crates/core/src/swap.rs`, swap by default), reversed while the swap key is held too. A drop that does nothing still writes a drop record when the modifier was involved, saying why.
5. `run_snap` (`crates/engine/src/ops.rs`) calls `core::operations::snap`, which calls `apply_settled` (`crates/core/src/bounds.rs`) through the platform. Settling resizes first when shrinking on the same display, moves first when growing would cross a screen edge, re-reads after each write with short waits, accepts the size an app insists on (minimum sizes, terminal cell grids), and moves to the final position. On success the original frame goes onto the undo history; on failure the window goes back where it was.
6. The engine posts a quiet status ("Snapped."), writes the outcome and the settle steps to `drops.json`, and logs the snap to the debug log when it's on.

A swap runs `run_swap` instead. Its source (`SwapSource`) is the zone the window fills, the zone under a floating window's center (`NearestZone`), or the window's own frame. It looks up the windows filling the source and destination zones at that moment (`WindowSystem::windows_occupying`), plans with `core::swap::plan_swap`, and either snaps exactly like `run_snap` (empty destination), exchanges the two windows (one occupant), or refuses with a message and returns the dragged window (crowded zones, or another window stacked in the window's own zone). The debug log's `swap` record names the source used: `zone`, `nearestZone`, or `floating`.

## How a shortcut press works

1. `Core::configure_hotkeys` registers the shortcuts the current state needs, computed by `core::bindings::desired_hotkeys` from the settings, the displays, and whether live control is on. It hands the adapter each chord's `Combo`, with the Snappy key expanded ([The Snappy key](#the-snappy-key)). It runs again whenever any of those change, and only re-registers when the set differs or a registration failed.
2. The adapter reports `HotKey(id)`. `actions::hotkey` (`crates/engine/src/actions.rs`) maps the ID back to a `HotKeyAction`. Shortcuts do nothing while the settings app is recording a shortcut or the master switch is off.
3. Workspace actions go to `crates/engine/src/workspaces.rs`. Undo and redo submit `run_undo` and `run_redo`. A layout selection applies the layout to the display under the pointer.
4. The nearest-empty-zone shortcut reaches `move_to_nearest_empty`. Once the operation runs, it reads the focused window's frame, looks up the windows filling every zone (`WindowSystem::windows_occupying`, leaving out inactive workspaces' windows as a swap does), picks a zone with `core::empty_zone::nearest_empty_zone`, and snaps there exactly like `run_snap`. With no empty zone it moves nothing and posts the quiet status "No empty zone on this display."
5. Zone and arrow shortcuts reach `move_focused`. It reads the focused window and its display, finds the destination zone (the zone whose chord was pressed in that display's layout, or `core::navigate::destination` for an arrow), and submits a swap, or a snap when "When the zone is taken" is set to stack on top.
6. Arrow presses that arrive while the previous move is still settling continue from the pending destination (`Core::pending_navigation`), so pressing an arrow three times moves three zones even if the app is slow.

## The Snappy key

The Snappy key is one held key that every default shortcut uses. It stands for Control+Option+Command on macOS, Ctrl+Alt+Win on Windows, and Ctrl+Alt+Super on Linux.

- **Its one definition** is `snappy_key_modifiers()` in `crates/core/src/keys.rs`, which returns the concrete modifiers it stands for.
- **Shortcuts store it as a symbol.** A `KeyChord`'s modifiers are `ChordMods`: the Snappy key or not, plus concrete modifiers, written to settings files as a list such as `["snappy", "shift"]`. They are always normalized: holding every modifier the Snappy key stands for is the Snappy key, so a file or recording with `["ctrl", "alt", "meta"]` loads as `["snappy"]`. The settings UI's recorder normalizes through the same Rust code (`normalizeChord` in `crates/wasm`).
- **Expansion happens in `KeyChord::combo`.** A `Combo` is the key and the concrete modifiers to hold. Conflicts, reserved chords, and the engine's lookups compare combos, so Snappy + 1 and Control+Option+Command+1 are the same shortcut. Adapters only ever receive combos (`HotKeys::register` and `HotKeys::is_available` take `Combo`), so no adapter knows the Snappy key exists.
- **Labels name it "Snappy"**, never the modifiers behind it, in long labels ("Snappy + Shift + 1") and compact ones on recorders and cards ("Snappy + Shift + 1"); other modifiers use the platform's names, full in messages and short on recorders ("Ctrl + Opt + L"). The help popover says what the Snappy key is on the platform (`snappy_key_label`).

To make the Snappy key something else, change `snappy_key_modifiers()`. If the new key is not one of the four modifiers, such as Caps Lock held as a "hyper" key, each adapter also needs to turn that key into the modifiers the function returns, by remapping it or intercepting it before the OS sees it. Settings files need no change: they store the symbol.

## How a settings change works

Every `Command` lands in `commands::handle` (`crates/engine/src/commands.rs`). Changes to settings go through `commands::change`: copy the settings, apply the edit, run `Settings::validate` (which normalizes and checks every shortcut conflict), save atomically, then reconfigure everything that depends on settings (debug log, workspaces, live control, shortcuts, overlays, tray). A rejected change leaves the old settings in place and replies with the reason, which the settings app shows.

## Workspaces

A workspace is a group of windows; one group is visible at a time. Workspaces are off by default and start with three.

### The session

`core::workspace::Session` (`crates/core/src/workspace/session.rs`) holds each tracked window's workspace, its visible frame, and its parked position while hidden. Every change is all or nothing:

1. Write the journal first (`WorkspaceIo::write_journal`), listing every window that will be parked.
2. Bring incoming windows back before parking outgoing ones, so the screen is never empty mid-switch.
3. Check each move against its target and, on any refusal, reverse the moves already made.

A switch refuses to start while a member window is full screen or has a sheet open. Minimized windows stay where they are.

### A switch, end to end

`workspaces::request_switch` (`crates/engine/src/workspaces.rs`) refuses when workspaces aren't running, are paused, or the target is already active, and defers the switch while displays are still settling. Otherwise it submits `switch_now`, which enrolls the focused app's windows, drops closed ones, runs `Session::switch_to`, focuses the target workspace's last focused window (or Snappy itself when there is none), saves membership and then only the new active workspace ID, refreshes shortcuts and overlays, and records the switch's timings.

### Parking

How a window is hidden depends on the platform (`core::workspace::ParkingPolicy`):

| Policy | Platform | How |
|---|---|---|
| `CornerSliver` | macOS | macOS pulls fully off-screen windows back, so the window is moved to an exposed lower corner with a 1-point sliver left on screen. |
| `OffScreen` | Windows | The window is moved entirely outside every display: left of the leftmost one with 64 pixels to spare, level with the top of the highest. |
| `Desktop` | X11 | The window is moved to a native desktop of Snappy's own, named "Snappy", and its frame is left alone. |

Apps that bring a parked window back on their own are parked again, at most once a second (containment, `workspaces::window_moved`).

### Membership, the journal, and recovery

- `workspace-membership.json` remembers which workspace each window belongs to, keyed by a recovery key: process ID, process start time, and a per-window token from the adapter. When Snappy restarts while apps keep running, their windows rejoin their workspaces. Entries for apps that quit are dropped.
- `workspace-journal.json` exists only while windows are parked. After a crash, `workspaces::recover` brings every journaled window back: by recovery key, or as the only window at a parked position. A window the user has since moved more than 4 points away is left alone. If some windows can't be brought back, workspaces stay off and the entries stay for the next try.
- Quitting (the menu's Quit, a termination signal, or session end) returns every parked window first.

### Other desktops and display changes

Workspaces belong to the native desktop (Space, virtual desktop) that was current when they started (`DesktopGuard`). On any other desktop they pause without changing anything. When displays change, the engine waits 350 ms for the change to settle, re-places windows without revealing parked ones, and retries once.

## Files

`apps/snappy/src/paths.rs` decides where files live:

| | macOS | Windows | Linux |
|---|---|---|---|
| `settings.json` | `~/Library/Application Support/Snappy/` | `%APPDATA%\Snappy\config\` | `~/.config/snappy/` |
| State | same folder as settings | `%LOCALAPPDATA%\Snappy\data\` | `~/.local/state/snappy/` |
| Logs | `~/Library/Logs/Snappy/` | `%LOCALAPPDATA%\Snappy\data\Logs\` | `~/.local/state/snappy/logs/` |

When `SNAPPY_HOME` names a folder, everything goes there instead: `settings.json`, `state/`, and `logs/`. The live tests use it to run against scratch files.

- **State:** `workspace-journal.json`, `workspace-membership.json`, and on macOS the single-instance lock, `daemon.lock`. Linux keeps its lock, `snappy.lock`, in `$XDG_RUNTIME_DIR` when that is set.
- **Logs:** `drops.json` (the last 20 modifier drags and how each ended), `workspace-lifecycle.json` (the last 30 workspace events), `workspace-performance.json` (up to 120 switch timings, written at quit), and the debug log.

Writes are atomic: a temporary file in the same folder, then a rename (`crates/engine/src/store.rs`). State and log files are readable only by the user. An unreadable settings file is reported, Snappy runs on defaults, and the file is never overwritten.

## The settings app

- **Tauri shell** (`apps/settings/src-tauri`). `daemon.rs` reads `ToSettings` frames from standard input on one thread and writes `ToDaemon` frames on another, relaying them to and from the web UI as Tauri events and commands. `window.rs` adds the Edit menu text fields need and reports which display the window is on. The content security policy allows only bundled assets, and the only plugin is the file dialog for import and export.
- **React UI** (`apps/settings/ui/src`). Four pages: Layouts (`layouts/`), Workspaces (`workspaces/`), Editor (`editor/`), and Settings (`settings/`), inside the shell in `shell/`. `state/daemonClient.ts` sends commands and turns `State` messages into what the pages render. The editor keeps its draft and undo history locally and sends `SaveLayout` on save.
- **WebAssembly core** (`crates/wasm`, loaded by `src/core/snappyCore.ts`). Zone frames, editor gestures, guides, conflicts, labels, and settings validation all come from `snappy-core`; the UI never computes geometry itself.
- **Fake daemon** (`src/bridge/fake/`). In `pnpm dev` and in the tests, `FakeDaemon` stands in for the pipe. It speaks the same messages, applies commands with the real `snappy-core` rules through WebAssembly, records every command, and lets tests change its state (`window.snappyFake`). `src/main.tsx` picks it in development builds and the Tauri bridge in production builds.

## Behavior at a glance

The numbers live in code; this is where to find them.

- **Snapping needs a real move** (`crates/core/src/drag.rs`). The window must travel more than 2 points with the drag modifier held at the drop. Resizing outside the title-bar strip cancels the drag until the next press; inside the strip, size changes some apps make while moving are tolerated.
- **Zones** (`crates/core/src/layout.rs`). Zones are fractions of the work area, at least 4% on each side, up to 32 per layout. Gap and padding are 12 points (scaled by the display's scale on Windows and Linux, where windows are measured in pixels). The zone under the pointer is the smallest one containing it; gaps belong to no zone.
- **Which layout a display uses** (`Settings::layout_for_display` in `crates/core/src/settings.rs`). The active workspace's choice for that display, else the display's own, else the default layout, skipping disabled layouts.
- **Occupancy** (`crates/core/src/swap.rs`). A window fills a zone when every edge is within 12 points, or 4% of the zone's shorter side if that's smaller.
- **Swaps trade zones** (`SwapSource` and `plan_swap` in `crates/core/src/swap.rs`). The occupant goes to the zone the moving window fills. A floating window, filling no zone, sends the occupant to the zone of its display's layout under its original center, or to the zone it overlaps most when the center is in a gap, sized to that zone. Only when there is no such zone (the center is off the layout), that zone is the destination, or a third window already fills it does the occupant take the floating window's exact frame instead. A floating window never blocks a swap, and undo returns both windows to their exact frames.
- **When the zone is taken** (`WhenTaken` in `crates/core/src/swap.rs`). Swap by default, or stack on top. Keyboard moves and drops both follow it; a move into an empty zone is an ordinary snap either way.
- **Swap key** (`SwapModifier` in `crates/core/src/swap.rs`). Held during a drag, it does the opposite of "When the zone is taken" for that drop, and the overlay names that action on the hovered zone. Command on macOS and Ctrl elsewhere by default. It can't be the drag key; a clash falls back to Command or Win or Super.
- **Settling** (`crates/core/src/bounds.rs`). Re-reads after 25, 50, and 100 ms for positions and 16 to 64 ms for sizes. A result more than 3 points off is reported as limited by the app; a size within 32 points per axis is accepted as the app's own step.
- **Arrow navigation** (`crates/core/src/navigate.rs`). The window starts from the zone it overlaps most, moves to the nearest zone in that direction, prefers aligned neighbors, never wraps, and stops at the screen's edge.
- **Nearest empty zone** (`crates/core/src/empty_zone.rs`). Candidates are the zones of each display's current enabled layout that no other window fills, by the occupancy rule; windows of inactive workspaces don't count, and the zone the window already fills is never one. Zones on the window's own display come first, then other displays'. The nearest is the one whose center is closest to the window's center, the earlier zone on a tie.
- **Undo** (`crates/core/src/operations.rs`). 50 moves, one history per workspace or one overall. A swap undoes as one step.
- **Shortcuts** (`crates/core/src/keys.rs`, `crates/core/src/bindings.rs`). Chords are physical keys plus at least one of Control, Option/Alt, or Command/Win/Super, counting the Snappy key's. Standard app commands (Command-C, Ctrl+C, Alt+F4, and others) are reserved. Two features can't share a chord, except zone chords in different layouts. The defaults are Snappy + 1 to 9 for zones, Snappy + arrows to move between zones, Snappy + Space to move to the nearest empty zone, Snappy + `[` and `]` to step through workspaces, and Snappy + `,` and `.` to take the focused window along.
- **Focus after a switch** (`crates/core/src/workspace/focus.rs`). For 0.75 seconds after a switch or send, app activations are ignored, because the switch itself causes some.
- **Limits** (`crates/core/src/settings.rs`). 100 custom layouts, 32 workspaces, names up to 80 characters.
