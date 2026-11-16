# Files and diagnostics

The engine is the only writer of Snappy's files. This page covers where they live, how they're written, and what Snappy records for debugging.

## Where files live

`apps/snappy/src/paths.rs` decides where files live:

| | macOS | Windows | Linux |
|---|---|---|---|
| `settings.json` | `~/Library/Application Support/Snappy/` | `%APPDATA%\Snappy\config\` | `~/.config/snappy/` |
| State | the settings folder | `%LOCALAPPDATA%\Snappy\data\` | `~/.local/state/snappy/` |
| Logs | `~/Library/Logs/Snappy/` | `%LOCALAPPDATA%\Snappy\data\Logs\` | `~/.local/state/snappy/logs/` |

With `SNAPPY_HOME` set, the files go in that folder instead: `settings.json`, `state/`, and `logs/`. The tests use this to run the daemon against scratch settings.

## What's in them

**State:**

- `workspace-journal.json`, only while windows are parked;
- `workspace-membership.json`;
- `caps-lock.json`, only while Caps Lock is held as the Snappy key;
- on macOS, the single-instance lock, `daemon.lock`.

Linux keeps its lock, `snappy.lock`, in `$XDG_RUNTIME_DIR` when that's set, even with `SNAPPY_HOME`. Windows uses a named mutex, with no file.

**Logs:**

- `drops.json`, `workspace-lifecycle.json`, and `workspace-performance.json`, described below;
- the debug log, `debug.jsonl`, and the one before it, `debug.1.jsonl`.

## How they're written

The engine's store (`crates/engine/src/store/`) writes every file whole, through a temporary file renamed into place, readable only by the user.

- A settings file that can't be read is reported and never overwritten. Snappy runs on defaults.
- Each state file records the version of its shape. One that can't be read is moved aside as `<name>.unreadable.json`, and Snappy carries on without it.

## Diagnostics

Diagnostics never hold window titles, window contents, or keystrokes: only frames, app identifiers, outcomes, and timings. A failure to write them never affects window control.

### The debug log

The debug log (`debug_log.rs`) is off unless the user turns it on in Settings, or the daemon starts with `SNAPPY_DEBUG=1`.

- It's one JSON object per line, with a time, a level, and an event. The level `warn` marks a result that differs from what was asked for.
- Events come from `LogEvent` (`log_event.rs`). Tests and the live suite match records by these names.
- Records are built in closures that run only while the log is on, so they cost nothing otherwise.
- When it reaches 5 MB it starts again, keeping the previous file.

### Always-on summaries

`diagnostics.rs` keeps a few short summaries, even with the debug log off:

- `drops.json`: the last 20 drags made with the drag key and how each ended (`Outcome`, in `outcome.rs`), with its settle steps;
- `workspace-lifecycle.json`: the last 30 workspace lifecycle events;
- `workspace-performance.json`: up to 120 switch timings, written at quit.

[CONTRIBUTING.md](../CONTRIBUTING.md#debugging) covers using them to debug.
