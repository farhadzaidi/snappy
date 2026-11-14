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
 │          ├── macos / windows / x11 / gnome / kwin ◄┘
 │          │           linux ◄── x11, gnome, kwin
 │          ├── fake (engine tests)
 │          └── live (live tests)
 ├── ipc ◄── engine, apps/settings/src-tauri
 └── wasm ◄── apps/settings/ui
```

| Crate | Depends on | Role |
|---|---|---|
| `core` | serde, thiserror | Geometry, layouts and presets, the editor's operations, drag intent, swaps, navigation, directional focus, the nearest empty zone and placing the selection, shortcuts, their titles, and conflicts, the settings model and validation, the bounds-settle algorithm, undo, the workspace session, and what a press of Caps Lock was while it is the Snappy key. |
| `platform` | core | The `Platform` trait family (`WindowSystem`, `Input`, `HotKeys`, `Overlay`, `Permissions`, `DesktopGuard`, `Tray`, `LoginItem`) and `PlatformEvent`. |
| `engine` | core, platform, ipc, tokio | Runtime state, event handling, the operation queue, stores, diagnostics. |
| `ipc` | core, platform | `ToSettings`, `ToDaemon`, `Command`, `AppState`, and framing. |
| `macos`, `windows`, `x11`, `gnome`, `kwin` | core, platform, OS bindings | One adapter each; `gnome` drives GNOME Shell on Wayland through the extension in `apps/gnome-extension`, and `kwin` drives KWin on Wayland through the script in `apps/kwin-script`. Every dependency is behind a `cfg(target_os)` table, so each crate is empty elsewhere. |
| `linux` | core, platform | What the Linux adapters share: the single-instance lock and signals, the autostart entry, `/proc` facts, the session kind, physical key codes, the menu's structure, the overlay's look, its drawing, and the finished overlay scenes desktop shells draw, display IDs, the tray icon, warnings for the journal, and a minimal Wayland client. Empty elsewhere. |
| `wasm` | core, wasm-bindgen | A JSON API over `core` for the settings UI. |
| `fake`, `live` | core, platform; `live` also the adapter for its OS | Test support; never linked into the shipped daemon. |

They are separate crates for three reasons:

- **Platform dependencies stay contained.** `objc2`, `windows`, `x11rb`, and `zbus` appear only in their adapter's manifest, so `core` and `engine` build and test on any machine.
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
- **A press cancels.** A mouse press on any window cancels the operation in flight (`drag::pointer_down`).
- **Protected windows.** While a swap or a grouped undo is moving two windows, a press on either one adds it to `Core::protected`. The operation's recovery path (putting a window back after a failure) then leaves that window to the user.
- **Keyboard observation is scoped.** Drag monitoring, which includes modifier key events, is on only during a drag. At idle Snappy sees no keystrokes except its own registered shortcuts and, with Caps Lock as the Snappy key, Caps Lock itself; while Caps Lock is held it notices that another key or a mouse button was used, never which ([Caps Lock as the Snappy key](#caps-lock-as-the-snappy-key)).

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
2. The adapter reports `HotKey(id)`. `actions::hotkey` (`crates/engine/src/actions.rs`) maps the ID back to a `HotKeyAction`. Shortcuts do nothing while the settings app is recording a shortcut or the master switch is off. The debug log records each press as `shortcut`, naming the action (`HotKeyAction::name`, such as `focusLeft` or `switchWorkspace`, with the `workspace` or `layout` it is for), and each registration as `shortcutsRegistered`, every chord with its action named the same way.
3. Workspace actions go to `crates/engine/src/workspaces/switching.rs`. Undo and redo submit `run_undo` and `run_redo`. A layout selection applies the layout to the display under the pointer.
4. The nearest-empty-zone shortcut places the selection when there is one ([Placing the selection](#placing-the-selection)). Without one it reaches `move_to_nearest_empty`. Once the operation runs, it reads the focused window's frame, looks up the windows filling every zone (`Core::occupants`, which leaves out inactive workspaces' windows for a swap too), picks a zone with `core::empty_zone::nearest_empty_zone`, and snaps there exactly like `run_snap`. With no empty zone it moves nothing and posts the quiet status "No empty zone on this display."
5. Zone and arrow shortcuts reach `move_focused`. It reads the focused window and its display, finds the destination zone (the zone whose chord was pressed in that display's layout, or `core::navigate::destination` for an arrow), and submits a swap, or a snap when "When the zone is taken" is set to stack on top. Zones have no shortcuts until the user records some in the editor.
6. Arrow presses that arrive while the previous move is still settling continue from the pending destination (`Core::pending_navigation`), so pressing an arrow three times moves three zones even if the app is slow.
7. Focus shortcuts go to `crates/engine/src/focus.rs` and selection shortcuts to `crates/engine/src/selection.rs` ([Focus and the selection](#focus-and-the-selection)).

## Focus and the selection

These shortcuts work whenever live control is on, with or without workspaces, and are registered only then. None of them moves a window except the nearest-empty-zone shortcut placing the selection.

### Moving focus

Snappy + I, J, K, and L (`focus::focus_toward`, in `crates/engine/src/focus.rs`) focus the next window up, left, down, or right. The candidates are the visible, managed, non-minimized windows of every display (`WindowSystem::visible_windows`), leaving out windows of inactive workspaces and on the parking desktop, hidden tabs, and the focused window itself. `core::focus::focus_target` picks one, given the focused window's frame, the zones of every display's current layout (`drag::zone_slots`), and the candidates ([Behavior at a glance](#behavior-at-a-glance)). While any of those windows or the focused one is in a zone, focus moves between zones that hold a window, the way the arrows move a window between zones, and windows outside zones can't be reached; with none in a zone, it goes to the nearest window by center. The engine asks the platform to focus it (`WindowSystem::focus`, which raises it on every platform), and outlines it in the focus style for 400 ms: a one-shot timer of the engine's, which a newer focus move replaces. With no window that way, nothing moves and the quiet status says so, such as "No window to the right." The debug log records each press as `focusMove`, with the window it started from, the one it focused, the `mode` (`zones` or `windows`), and the `zone` it went to, by display and zone.

### The selection

Snappy + Return (`selection::toggle`) adds the focused window to the selection, `Core::selection`, an ordered list kept in memory only; pressing it on a selected window takes it out. While workspaces run, the window must show on the active workspace. Snappy + `\` (`selection::clear`) empties the selection.

- A selected window that closes leaves the selection.
- Changing native desktops (Spaces, virtual desktops) clears it, since the windows of the desktop left behind no longer show; that is also how pausing workspaces clears it. Losing live control clears it, since its shortcuts go with live control, and so do workspaces stopping or being suspended. Quitting forgets it and moves nothing more than quitting always does.
- A workspace switch takes the selection along and then empties it ([Taking the selection along](#taking-the-selection-along)). Snappy + `.` and `/` send the whole selection instead of the focused window, and so does a workspace's own send shortcut; the windows sent leave the selection.
- The debug log records `selectionAdded`, `selectionSkipped` when the focused window can't be selected (with the reason), `selectionRemoved` (with the reason), `selectionCleared` (with the reason), `selectionTakenAlong` on a switch, with the hidden tabs that went along by the shown tab they sit behind (`tabs`), and `selectionPlaced`.

### Placing the selection

With a selection, Snappy + Space (`selection::place`) moves the selected windows that aren't in a zone into empty zones, as one operation on the queue. Once it runs, it reads the selected windows' frames, looks up the windows filling every zone as the single-window case does, and plans with `core::empty_zone::place_selection` ([Behavior at a glance](#behavior-at-a-glance)). `core::operations::snap_group` then snaps each window in turn as one step of undo; each snap is logged as an ordinary `snap`. Placed windows leave the selection. Selected windows already in a zone stay where they are and stay selected, and so do windows no empty zone was left for, with the quiet status "1 window didn't fit." or "2 windows didn't fit."

### Marked windows

Selected windows, and the window a focus shortcut just focused, are outlined inside their frames. The outlines are a layer of their own, apart from the zone scene: `Overlay::mark` takes a display and its `OverlayMarks`, the display's work area and the windows to outline, each a `MarkedWindow` with its frame and `MarkStyle`, and an empty list takes them away. The styles are:

- `Selected`: the swap source's mint border.
- `Focused`: an orange border, so a focus flash never reads as a selection. A selected window that gets the flash shows only the flash until it ends.

`Core::refresh_marks` (`crates/engine/src/marks.rs`) sends each display its outlines whenever the selection or the flash changes, an outlined window moves (`WindowMoved`), or displays change, and only to displays whose outlines changed. Minimized and hidden windows get none. Every adapter draws the layer on a click-through surface of its own per display that never takes focus, so the outlines stay while zones show and hide ([platforms.md](platforms.md)).

## The Snappy key

The Snappy key is one held key that every default shortcut uses. It stands for modifiers the user chooses, Control+Option+Command on macOS, Ctrl+Alt+Win on Windows, and Ctrl+Alt+Super on Linux unless they choose others. Every default is the Snappy key and exactly one other key, never another modifier on top, and the defaults are the same on every platform ([Behavior at a glance](#behavior-at-a-glance)).

- **What it stands for** is `Settings::snappy_key`, a `SnappyKey` (`crates/core/src/keys.rs`): at least two of Control, Option/Alt, Command/Win/Super, and Shift, written to the settings file as `"snappyKey": ["ctrl", "alt", "meta"]`. The default, `SnappyKey::DEFAULT`, is the same on every platform, and a file without the field gets it, so older files load unchanged. A file whose key has fewer than two modifiers doesn't load.
- **Shortcuts store it as a symbol.** A `KeyChord`'s modifiers are `ChordMods`: the Snappy key or not, plus concrete modifiers, written to settings files as a list such as `["snappy", "shift"]` or `["ctrl", "alt", "meta"]`. The two are different shortcuts even when they press the same keys: the first follows the key, the second never moves.
- **Recording decides which, once.** Holding every modifier of the current key while recording stores the Snappy key plus whatever else is held (`ChordMods::recorded`); anything else stores exactly the modifiers held. The settings UI's recorder does this through the same Rust code (`recordChord` in `crates/wasm`, given the current key). Nothing afterwards changes a chord's form: validation, saving, loading, and changing the key all keep every chord as stored, so a file reads back exactly as it was written.
- **Files from before the Snappy key could be chosen** have no `snappyKey` field. Snappy read every chord in them that held Control, Option, and Command (Ctrl, Alt, and Win or Super) as the Snappy key plus the rest, whether written `["snappy"]` or `["ctrl", "alt", "meta"]`, so such a file still loads that way (`Settings::read_as_before_the_snappy_key`): as recording the chord with the default key would store it. A file with the field is read as written. Every file Snappy saves has the field, so this happens at most once per file.
- **Expansion happens in `KeyChord::combo(key)`.** A `Combo` is the key and the concrete modifiers to hold. Conflicts, reserved chords, the chords the OS keeps, availability probes, and the engine's lookups compare combos expanded with the settings' key, so Snappy + 1 and the key's modifiers held with 1 are the same shortcut. `desired_hotkeys` hands the engine each `HotKey` with its combo already expanded. Adapters only ever receive combos (`HotKeys::register` and `HotKeys::is_available` take `Combo`), so no adapter knows the Snappy key exists.
- **Labels name it "Snappy"**, never the modifiers behind it, in long labels ("Snappy + Shift + 1") and compact ones on recorders and cards ("Snappy + Shift + 1"); other modifiers use the platform's names, full in messages and short on recorders ("Ctrl + Opt + L"). What the key is on the platform, as the help popover, the Snappy key setting, and the recorders' hint say it, comes from `SnappyKey::label`.

### Changing the Snappy key

The settings app sends `SetSnappyKey { modifiers }`, which `commands::set_snappy_key` checks as a whole before anything changes:

1. **The key itself.** Fewer than two modifiers is refused: "The Snappy key needs at least two modifiers."
2. **Every binding with the new expansion.** `Settings::with_snappy_key` sets the key and validates as a settings change does. Conflicts compare combos, so a concrete chord that the new key's Snappy key chords land on clashes with them. A conflict is told from the side the change moved (`bindings::snappy_key_conflict`), the Snappy key chord, whichever of the two was checked first: with Undo recorded by hand as Control + Option + J and the key changed to Control+Option, "Snappy + J for Focus left would collide with Undo window move."; also "Snappy + Z for Undo window move would collide with a standard app command.", or, for a chord the OS keeps, "Snappy + ⌫ for Clear selection wouldn't work." followed by why.
3. **The OS.** `bindings::availability_probes` lists each distinct chord that holds the Snappy key, with the new modifiers, disabled layouts and workspaces included as conflicts include them, and the engine asks `HotKeys::is_available` about each with its own shortcuts unregistered, as while recording, since a probed chord can be one Snappy holds now (Snappy + Shift + 1 in one layout becomes the chord of another layout's Snappy + 1 when Shift joins the key, which layouts may share). The first chord taken refuses the change and names it: "Snappy + Space for Move to the nearest empty zone would be Control + Option + Space, which macOS or another app already uses."

On success the settings are saved and `settings_changed` runs; `configure_hotkeys` registers the new set at once, with no restart (each `HotKey` carries its combo, so a key change alone makes the set differ even when no chord does), and the status says "The Snappy key is now Control + Option." On refusal nothing is saved, the current shortcuts are registered again if probing took them away, and the reply carries the message, which the setting shows under its toggles. The debug log records `snappyKeyChanged` (`from`, `to`, modifier names) and `snappyKeyRefused` (with the `reason`).

What happens to stored chords follows from their form:

- **Snappy key chords follow the key.** They stay stored as `["snappy", …]` and keep reading "Snappy + X"; only their combos change. The modifiers recorded with them stay too, even one the new key includes: with Control+Shift as the key, Snappy + Shift + J presses Control + Shift + J, the same as Snappy + J, and holds Shift again once the key no longer does.
- **Concrete chords never change.** What the user recorded is what they press, whatever the key becomes, including a key made of the chord's modifiers or some of them: the chord still reads "Control + Option + Shift + U", is saved so, and keeps its keys when the key changes again. A change that would put a Snappy key chord on the same keys is refused, as above.

Settings imported from a file keep the file's key, checked as a settings change is, without probing; a chord the OS refuses then fails to register and is reported as taken, as for any other shortcut.

### Caps Lock as the Snappy key

An opt-in setting, `Settings::caps_lock_as_snappy_key` (`"capsLockAsSnappyKey"`, off by default), lets Caps Lock stand for the Snappy key. Held, it acts as the Snappy key's modifiers, whatever they are, so every shortcut works with it as with the keys themselves and nothing about shortcuts changes; tapped alone, it still turns Caps Lock on or off. Holding it never turns Caps Lock on and never repeats.

- **The platform makes the key.** `CapsLock::hold_caps_lock_as(modifiers)` asks the adapter to make Caps Lock, held, act as the Snappy key's modifiers, each with what its OS offers out of the box ([platforms.md](platforms.md)). It refuses, with a sentence for the user, when the system already remaps Caps Lock: a macOS key mapping or Modifier Keys setting, a Windows Scancode Map, an XKB option such as `caps:escape` or `ctrl:nocaps`, or an `xmodmap` on X11. `restore_caps_lock` gives Caps Lock back its usual behavior.
- **The engine decides what a press was** (`crates/engine/src/caps_lock.rs`). Adapters report `PlatformEvent::CapsLock`: Caps Lock went down, something else was used while it was down, or it came up. They may notice that another key or a mouse button was used while Caps Lock is down, to tell a tap from a hold, but never say or record which. The engine times the press with the platform's clock and asks `core::caps_lock::CapsLockPress` what it was: a tap when nothing else was used and it came up within `TAP_LIMIT` (half a second); then it calls `toggle_caps_lock`, which turns Caps Lock and its light on or off on every keyboard as the key itself would. A Snappy shortcut that fires while Caps Lock is down also makes the press a hold, for platforms that can't see every key.
- **When it applies.** While the setting is on, permission is granted, and live control is available (`caps_lock::configure`, run from `refresh_live`, so it follows settings, permission, and live control changes). A new Snappy key holds Caps Lock again with the new modifiers. `core::caps_lock::steps` says what to do from where things stand: hold, hold again, restore, or nothing.
- **Turning it on** (`SetCapsLockAsSnappyKey`) tries the platform first when it can apply now, so a refusal leaves the setting off and the reply, shown under the switch, says why. A refusal later, at start or after the Snappy key changes, keeps the setting, is announced once, and shows under the switch (`AppState::caps_lock_problem`) until a change makes it work.
- **Cleaning up.** Turning the setting off, quitting, and stopping for a restart all restore Caps Lock. While a change is in place, the engine keeps what undoes it, in a form of the adapter's own (`CapsLockUndo`), in `caps-lock.json`. A run that ends without restoring, as a crash does, leaves that file behind; the next start hands it to `undo_caps_lock` before anything else, then holds Caps Lock again if the setting asks, so checking for someone else's remapping never sees Snappy's own.
- **The debug log** records `capsLockHeld` (the modifiers), `capsLockRefused` (the reason), `capsLockRestored`, `capsLockLeftOverUndone`, `capsLockTapped` (how long it was down), and `capsLockReleased` for a hold (how long, and whether something else was used, never what).

## How a settings change works

Every `Command` lands in `commands::handle` (`crates/engine/src/commands.rs`). Changes to settings go through `commands::change`: copy the settings, apply the edit, run `Settings::validate` (which normalizes the layouts and the swap key, and checks every shortcut conflict), save atomically, then reconfigure everything that depends on settings (debug log, workspaces, live control, shortcuts, overlays, tray). A rejected change leaves the old settings in place and replies with the reason, which the settings app shows. Changing the Snappy key goes through its own checks first ([Changing the Snappy key](#changing-the-snappy-key)).

Reading a file (`Settings::from_json`) keeps every shortcut it has, cleared ones (`null`) included, and ignores fields it doesn't know. It reads the Snappy key first, the default when the file has none, since the defaults it fills in and the chords the OS keeps are compared with it. A shortcut field the file lacks gets its default only when no binding the file has already uses that chord, so loading never creates a conflict: a file whose zones use the Snappy key with the digits keeps them, and a workspace switch shortcut it lacks stays unassigned. A shortcut on a chord the OS keeps for itself (`chords_the_system_keeps`) never fired, so it is read as missing rather than failing the file: a macOS file from before Snappy + `\` cleared the selection, holding Snappy + Delete, loads clearing with Snappy + `\`, or with nothing when another binding has that chord, and a zone or other shortcut without a default on Snappy + Delete loads unassigned.

## Workspaces

A workspace is a group of windows; one group is visible at a time. Workspaces are off by default and start with three.

### The session

`core::workspace::Session` (`crates/core/src/workspace/session.rs`) holds each tracked window's workspace, its visible frame, and its parked position while hidden. Every change is all or nothing:

1. Write the journal first (`WorkspaceIo::write_journal`), listing every window that will be parked.
2. Bring incoming windows back before parking outgoing ones, so the screen is never empty mid-switch.
3. Check each move against its target and, on any refusal, reverse the moves already made. A window that closes during the change leaves the session instead, and the change goes on without it.

A switch refuses to start while a member window is full screen or has a sheet open. Minimized windows stay where they are.

### A switch, end to end

`workspaces::request_switch` (`crates/engine/src/workspaces/switching.rs`) refuses when workspaces aren't running, are paused, or the target is already active, and defers the switch while displays are still settling. Otherwise it submits `switch_now`, which enrolls the focused app's windows, drops closed ones, learns the members' tab groups ([Tab groups](#tab-groups)), runs `Session::switch_taking_along` with the selected windows, focuses the last of those that arrived, else the target workspace's last focused window (or Snappy itself when there is none), saves membership and then only the new active workspace ID, refreshes shortcuts and overlays, and records the switch's timings. Every switch goes this way: by number, by step, from the menu, from a workspace's own shortcut, after a send that follows the window, and from the settings app.

### Workspace shortcuts

- **Switching by number.** Workspaces 1 to 9 switch with the Snappy key and their number by default. The shortcut is set when a workspace is made (`Workspace::numbered`, through `Settings::new_workspace` for one the user adds, which takes the lowest free "Workspace n" name), and only when no other binding uses that chord; later workspaces get none. Shortcuts belong to their workspace: reordering or deleting workspaces never changes them, so after a delete a new "Workspace 2" gets Snappy + 2 again only if that chord is free.
- **Steps.** Snappy + `;` and `'` go to the previous or next workspace in saved order, wrapping, counting from a switch still on its way.
- **Sending and staying.** Snappy + `.` and `/` send the focused window to the previous or next workspace (`send_and_stay`) and park it there; the active workspace stays. Focus then moves to the active workspace's most recently focused remaining window (`Session::focus_recent`, which keeps the order windows were focused in), which is its frontmost, so pressing again sends that one. With nothing left, focus is left to the system. With a selection, they send every selected window of the active workspace instead, and focus moves the same way. A workspace's own send shortcut works the same way, and its "send window to … and switch" shortcut sends the focused window, then switches, taking the selection along. The debug log records `workspaceSend` for every send, with whether it stayed, which window got focus, and the hidden tabs that went too (`tabs`). A window joining a workspace is logged as `workspaceJoin`, with `tabOf` naming the hidden tabs when it joined their group, and hidden tabs moved to their shown tab's workspace as `tabsUnified`.

### Taking the selection along

The next switch, however it is made, hands the selection ([The selection](#the-selection)) to `Session::switch_taking_along`, which moves the selected windows of the active workspace into the target workspace without moving them on screen, parks the rest, and gives focus to the last one selected. The selection is then empty. Switching to the workspace already showing is refused, so the selection stays.

### Tab groups

On macOS each native tab is a window of its own, and only the selected tab shows ([platforms.md](platforms.md#macos)). A tab group is one window to workspaces: it never spans two.

- **What the platform says.** `WindowSystem::tab_groups` reports, for the apps owning some windows, each group with a hidden tab the platform has seen: its shown tab, as a `WindowInfo`, and its hidden tabs. Other platforms report none.
- **What the session keeps.** `Session::update_tabs` records which members are hidden tabs and the shown tab each sits behind (`core::workspace::Tabs`), then gives every hidden tab its shown tab's workspace, frame, and parking. Switches, sends, taking the selection along, deleting a workspace, display changes, and quitting act on the shown tab and its hidden tabs follow: a tab taken along or sent, shown or hidden, takes its whole group. Hidden tabs are never placed, contained, journaled, focused, or returned on their own, since moving one changes nothing on screen and focusing one makes its app show it; the journal lists only the shown tab, which brings the group back. A group counts as one window.
- **When the engine asks** (`sync_tabs` in `crates/engine/src/workspaces/members.rs`). Whenever an app's focus changes, for that app, since picking a tab changes which one shows; and before every switch, send, deletion, display change, resume, start, and stop, for every member's app. A shown tab Snappy hasn't met joins the workspace of the tabs behind it, where they are (`join_tab_group`), so a new tab, even one an app opens in a parked window, goes where its group is, and an activation that shows it follows like a member's. When a window is enrolled, a workspace remembered for it wins; that is how a restart puts tabs remembered apart back together, in the shown tab's workspace.
- **Tearing out.** A tab dragged out of its group stops being reported in it and becomes an ordinary member, already in the group's workspace.
- **Selecting.** A tab group is selected as one: the outline is drawn on the shown tab, selecting a tab of a selected group unselects it, and placing or sending the selection moves the group once. Whatever sends a group, the selection or the focused window, alone or before a switch, takes every tab of it out of the selection.

### Parking

How a window is hidden depends on the platform (`core::workspace::ParkingPolicy`):

| Policy | Platform | How |
|---|---|---|
| `CornerSliver` | macOS | macOS pulls fully off-screen windows back, so the window is moved to an exposed lower corner with a 1-point sliver left on screen. |
| `OffScreen` | Windows | The window is moved entirely outside every display: left of the leftmost one with 64 pixels to spare, level with the top of the highest. |
| `Desktop` | X11, GNOME and KDE Plasma on Wayland | The window is moved to a native desktop of Snappy's own, named "Snappy" on X11 and KDE Plasma, and its frame is left alone. |

Apps that bring a parked window back on their own are parked again, at most once a second (containment, `workspaces::window_moved`).

### Membership, the journal, and restarts

Restarting the daemon while apps keep running, to install a new build or after a crash, changes nothing on screen: every window keeps its workspace, parked windows stay parked, and the active workspace stays active. Only quitting brings parked windows back.

- **Recovery keys.** Windows are recognized across daemon processes by a `RecoveryKey`: the process ID, the process's start time, and a token for the window that stays the same while it exists ([platforms.md](platforms.md) has each adapter's). The start time makes a process ID reused by a later process a different key (`RecoveryKey::same_process`, `same_window`), so nothing saved for a process that has gone is ever applied to another.
- **What is saved, and when.** `workspace-journal.json` lists every parked window with its workspace, home frame, and parked position, and is written before every change ([The session](#the-session)); it exists only while windows are parked. `workspace-membership.json` remembers every tracked window's workspace, written after every switch, send, deletion, display change, and tab change. The active workspace is in `settings.json`, saved on every switch.
- **Restart versus quit.** Quitting (the menu's Quit, `SIGTERM`, the session ending, or turning workspaces off) runs `workspaces::stop`, which returns every parked window, removes the journal, and clears the platform's parking state. A restart request (`PlatformEvent::RestartRequested`: `SIGUSR2` on macOS and Linux, `snappy --stop-for-restart` on Windows) runs `workspaces::detach` instead: it saves the journal and membership as they are and moves nothing. A crash or a forced kill leaves the files as the last change wrote them, which is the same thing.
- **Picking up at launch** (`workspaces::start`, and `recover` in `crates/engine/src/workspaces/recovery.rs`). Each journaled window is looked for by its recovery key, else as the only window of the same process and size at its parked position, and `core::workspace::relaunch` decides what happens to it. One still hidden for a workspace that isn't showing (at a parking position, or on the parking desktop), or minimized, rejoins that workspace without moving (`rejoin`) and stays journaled. One at its parked spot whose workspace is now showing or was deleted comes back to its home frame. One found somewhere else has been brought back by the user or its app while Snappy was away; it is left alone and joins the active workspace like any visible window, rather than being hidden again. Entries whose process is gone, or whose process ID now belongs to another launch, are dropped. Every other window joins as at any start: visible ones the active workspace, minimized ones the workspace membership remembers for them.
- **Tabs.** Only shown tabs are journaled, so a parked group rejoins as one window. The new daemon learns hidden tabs as they are picked, as at any start, and a tab picked for the first time joins its group's workspace ([Tab groups](#tab-groups)).
- **The selection** is never saved; a restart starts with none.
- **After a reboot or logout** every saved process has gone, so nothing saved is applied. Apps that reopen their windows at a parked position are brought just inside the screen and join the active workspace.
- If a journaled window can't be found while its process still runs, or can't be brought back, workspaces stay off and the entries stay for the next try.
- The debug log's `workspacesStarted` record says what launch did: `rejoined` (windows per workspace), `returned`, `leftAlone`, `staleJournal`, and `staleMembership`; each rejoined window also gets a `workspaceJoin` record with `"rejoined": true`. A restart request logs `workspacesDetached` with how many windows stay parked.

### Other desktops and display changes

Workspaces belong to the native desktop (Space, virtual desktop) that was current when they started (`DesktopGuard`). On any other desktop they pause without changing anything.

When live control is lost while workspaces run, as when GNOME's extension is turned off or restarts or KWin starts over, they are suspended rather than stopped: the session is kept, nothing moves, and switches are refused. When live control comes back (`LiveControlChanged`), the engine binds the desktop again and hides the windows of inactive workspaces that the platform showed meanwhile (`Session::contain`), announcing any it couldn't. When displays change, the engine waits 350 ms for the change to settle, re-places windows without revealing parked ones, and retries once.

## Files

`apps/snappy/src/paths.rs` decides where files live:

| | macOS | Windows | Linux |
|---|---|---|---|
| `settings.json` | `~/Library/Application Support/Snappy/` | `%APPDATA%\Snappy\config\` | `~/.config/snappy/` |
| State | same folder as settings | `%LOCALAPPDATA%\Snappy\data\` | `~/.local/state/snappy/` |
| Logs | `~/Library/Logs/Snappy/` | `%LOCALAPPDATA%\Snappy\data\Logs\` | `~/.local/state/snappy/logs/` |

When `SNAPPY_HOME` names a folder, everything goes there instead: `settings.json`, `state/`, and `logs/`. The live tests use it to run against scratch files.

- **State:** `workspace-journal.json`, `workspace-membership.json`, `caps-lock.json` while Caps Lock is held as the Snappy key, and on macOS the single-instance lock, `daemon.lock`. Linux keeps its lock, `snappy.lock`, in `$XDG_RUNTIME_DIR` when that is set.
- **Logs:** `drops.json` (the last 20 modifier drags and how each ended), `workspace-lifecycle.json` (the last 30 workspace events), `workspace-performance.json` (up to 120 switch timings, written at quit), and the debug log.

Writes are atomic: a temporary file in the same folder, then a rename (`crates/engine/src/store.rs`). State and log files are readable only by the user. An unreadable settings file is reported, Snappy runs on defaults, and the file is never overwritten.

## The settings app

- **Tauri shell** (`apps/settings/src-tauri`). `daemon.rs` reads `ToSettings` frames from standard input on one thread and writes `ToDaemon` frames on another, relaying them to and from the web UI as Tauri events and commands. `window.rs` adds, on macOS only, the Edit menu text fields need there (elsewhere a menu would sit inside the window, and the web view handles the editing shortcuts itself), and reports which display the window is on. The content security policy allows only bundled assets, and the only plugin is the file dialog for import and export.
- **React UI** (`apps/settings/ui/src`). Four pages: Layouts (`layouts/`), Workspaces (`workspaces/`), Editor (`editor/`), and Settings (`settings/`), inside the shell in `shell/`. `state/daemonClient.ts` sends commands and turns `State` messages into what the pages render. The editor keeps its draft and undo history locally and sends `SaveLayout` on save.
- **WebAssembly core** (`crates/wasm`, loaded by `src/core/snappyCore.ts`). Zone frames, editor gestures, guides, conflicts, labels, every shortcut's title (`shortcutTitles`, `workspaceShortcutTitles`, and `layoutShortcutTitles`), the help popover's list of default shortcuts (`defaultShortcuts`, from `core::guide`, built from the defaults themselves), and settings validation all come from `snappy-core`; the UI never computes geometry or names a shortcut itself, so a conflict message and the recorder it is about always say the same.
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
- **Arrow navigation** (`crates/core/src/navigate.rs`). The window starts from the zone it overlaps most, moves to the nearest zone in that direction, prefers aligned neighbors, never wraps, and stops at the screen's edge. Only zones whose centers lie more than half a point past the starting zone's center count. Zones sharing more than half a point of its span across the direction are aligned and beat every other; among them the least gap wins, and among the rest the nearest center. The window's position across the direction, then zone order, settle what is left (`next_zone`, which directional focus shares).
- **Nearest empty zone** (`crates/core/src/empty_zone.rs`). Candidates are the zones of each display's current enabled layout that no other window fills, by the occupancy rule; windows of inactive workspaces don't count, and the zone the window already fills is never one. Zones on the window's own display come first, then other displays'. The nearest is the one whose center is closest to the window's center, the earlier zone on a tie.
- **Directional focus** (`crates/core/src/focus.rs`). While any window on the active workspace is in a zone, by the occupancy rule, focus moves between zones, on every display, choosing as arrow navigation does among the zones that hold a window other than the focused one, so empty zones are skipped and windows outside zones are never reached. A focused window in a zone starts from the zone it overlaps most of those it fills; otherwise focus starts from the point described next, which counts as aligned with a zone it lies more than half a point inside of. Of several windows in the zone chosen, the most recently focused gets focus. When no window is in a zone, focus goes by window centers: only windows whose centers lie more than half a point past the starting point in that direction count. The nearest scores least: its distance ahead plus twice its distance to the side (`SIDEWAYS_WEIGHT`), so a window straight ahead wins over one as far off to the side, and one at 45 degrees wins only when less than a third as far ahead. Scores within a thousandth of a point tie, and a tie goes to the most recently focused window while workspaces run (`Session::recently_focused`), else to the first in the platform's list of windows. Focus never wraps around.
- **Placing the selection** (`place_selection` in `crates/core/src/empty_zone.rs`). Selected windows already filling a zone stay. The others each get an empty zone of their own, empty as for the nearest empty zone. Zones on the display under the pointer are used first; other displays' zones, in display order, only when it hasn't enough, and then every zone of the earlier displays is filled. Among the zones that may be used, windows and zones are paired so that the total distance from window centers to zone centers is least, found exactly by the Hungarian method (`crates/core/src/assignment.rs`) on distances in thousandths of a point. With more windows than empty zones every zone is filled and the windows that would add most to the total are left over. On equal totals, earlier selected windows take earlier zones and later ones are left over.
- **Undo** (`crates/core/src/operations.rs`). 50 moves, one history per workspace or one overall. A swap undoes as one step, and so does placing the selection (`snap_group`).
- **Shortcuts** (`crates/core/src/keys.rs`, `crates/core/src/bindings.rs`). Chords are physical keys plus at least one of Control, Option/Alt, or Command/Win/Super, counting the Snappy key's. The Snappy key is at least two of Control, Option/Alt, Command/Win/Super, and Shift, Control+Option+Command by default on every platform. Standard app commands (Command-C, Ctrl+C, Alt+F4, and others) are reserved, and so are chords the OS swallows without saying so (`chords_the_system_keeps`: Control+Option+Command+Delete on macOS), which a settings file that holds one reads as missing instead of failing to load. Two features can't share a chord, except zone chords in different layouts, and zone and layout chords with workspace chords while workspaces are off. The defaults, the same on every platform, are Snappy + arrows to move between zones, Snappy + I, J, K, and L to move focus up, left, down, and right, Snappy + Return to select or unselect the focused window, Snappy + `\` to clear the selection, Snappy + Space to move to the nearest empty zone or place the selection, Snappy + 1 to 9 to switch to workspaces 1 to 9, Snappy + `;` and `'` to step through workspaces, and Snappy + `.` and `/` to send the focused window or the selection to the previous or next workspace and stay. They are stored as `navigation` (`left`, `right`, `up`, `down`, `nearestEmpty`, `focusUp`, `focusLeft`, `focusDown`, `focusRight`, `selectWindow`, `clearSelection`) and `workspaces` (`previous`, `next`, `sendPrevious`, `sendNext`) in the settings file. Zones, layout selection, per-workspace sends, and undo and redo have none.
- **Focus after a switch** (`crates/core/src/workspace/focus.rs`). For 0.75 seconds after a switch or send, app activations are ignored, because the switch itself causes some.
- **Limits** (`crates/core/src/settings.rs`). 100 custom layouts, 32 workspaces, names up to 80 characters.
