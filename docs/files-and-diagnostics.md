# Files and diagnostics

This page is for anyone debugging Snappy or changing what it saves. It covers where Snappy keeps its files, how it writes them safely, and what it records to help explain what happened.

Snappy's diagnostics never hold window titles, window contents, or keystrokes. They record frames, app identifiers, outcomes, and timings only, so they're safe to attach to a bug report.

## Where the files are

`apps/snappy/src/paths.rs` decides where files go:

| | macOS | Windows | Linux |
|---|---|---|---|
| Settings | `~/Library/Application Support/Snappy/` | `%APPDATA%\Snappy\config\` | `~/.config/snappy/` |
| State | the same folder as settings | `%LOCALAPPDATA%\Snappy\data\` | `~/.local/state/snappy/` |
| Logs | `~/Library/Logs/Snappy/` | `%LOCALAPPDATA%\Snappy\data\Logs\` | `~/.local/state/snappy/logs/` |

When the daemon starts with `SNAPPY_HOME=<folder>`, everything goes in that folder instead: `settings.json`, a `state/` folder, and a `logs/` folder. The tests use this to run the daemon against scratch settings.

Two builds lay their files out differently. The Mac App Store build keeps them inside its sandbox container ([The Mac App Store](releasing/mac-app-store.md)). A Snappy installed from the Microsoft Store keeps them in its package's `LocalState` folder, laid out as with `SNAPPY_HOME` ([Windows](platforms/windows.md#the-msix-package)).

## What's in them

The settings folder holds `settings.json`.

The state folder holds what Snappy needs to pick up where it left off:

- `workspace-membership.json`, which window belongs to which workspace;
- `workspace-journal.json`, which windows are hidden and where, only while some are;
- `caps-lock.json`, how to undo Caps Lock's change to the keyboard settings, only while it's in place;
- on macOS, `daemon.lock`, which keeps a second daemon from starting.

Linux keeps its lock, `snappy.lock`, in `$XDG_RUNTIME_DIR` instead, even with `SNAPPY_HOME` set. Windows uses a named mutex, with no file.

The logs folder holds the diagnostics described below.

## How the files are written

Only the engine writes files, through its store in `crates/engine/src/store/`. It writes each file whole, to a temporary file that's then renamed into place, so a crash can't leave half a file. Every file is readable only by the user.

When a file can't be read:

- **A broken settings file** is reported, and never overwritten. Snappy runs on its defaults until the user fixes or replaces it.
- **A broken state file** is moved aside as `<name>.unreadable.json`, and Snappy carries on without it. Each state file records which version of its format it's in.

## The debug log

The debug log records what Snappy saw, decided, and did. It's off unless the user turns it on in Settings → Debug, or the daemon starts with `SNAPPY_DEBUG=1`. While it's on, Settings shows where it is.

It's `debug.jsonl` in the logs folder: one JSON object per line, each with a time, a level, and an event name. The level `warn` marks a result that differs from what was asked for. When the file reaches 5 MB, Snappy starts a new one and keeps the previous one as `debug.1.jsonl`.

The event names are defined in `crates/engine/src/log_event.rs`. Tests and the live suite look for records by these names, so renaming one means updating them too.

Each record is built only while the log is on, so the log costs nothing when it's off.

## Summaries that are always on

A few short files in the logs folder are kept up to date even with the debug log off:

- **`drops.json`** holds the last 20 drags made with the drag key, how each one ended, and the steps it took to settle.
- **`workspace-lifecycle.json`** holds the last 30 times something outside Snappy disturbed workspaces: a hidden window coming back on screen, or the OS switching to another desktop.
- **`workspace-performance.json`** holds up to 120 workspace switch timings, written when Snappy quits.

They're written by `crates/engine/src/diagnostics.rs`. A failure to write any diagnostics never affects moving windows.

## When something didn't work

**A drop didn't snap.** Look at `drops.json` first. It says how each recent drag ended: for example, snapped, the drag key let go before the button, dropped outside every zone, or the window couldn't be moved.

**Anything else.** Turn the debug log on, do the thing again, and look at the last records in `debug.jsonl`, or attach the file to a bug report.

**Trying something without touching your settings.** Run the daemon with `SNAPPY_HOME` set to an empty folder, inside a sandbox, never on your own desktop ([CONTRIBUTING.md](../CONTRIBUTING.md#running-snappy-safely)).
