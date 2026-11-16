# Talking to the app

The daemon and the app talk over the app's standard input and output, which the daemon pipes when it starts the app ([The daemon and the app](daemon-and-app.md#starting-the-app)). The messages and their framing live in `crates/ipc`.

## Framing

Each message is a 4-byte little-endian length followed by that many bytes of UTF-8 JSON. A message can be at most 8 MB (`MAX_MESSAGE_BYTES`).

## The conversation

Both sides open with `Hello` and a protocol version (`PROTOCOL_VERSION`). The app exits on a mismatch, so a daemon and an app from different builds never misread each other.

From then on:

- **The app sends a `Command`** for each thing the user does: save a layout, record a shortcut, add a workspace, change the Snappy key, and so on. Each command carries an ID.
- **The daemon answers every command with a `Reply`** with the same ID. When the daemon said no, the reply carries a `Refusal`: a message for the user, and, when the message names one of Snappy's shortcuts, that shortcut's `ShortcutId`, so the UI can link to the right recorder.
- **The daemon then sends a fresh `State`**, the whole `AppState`. It's all the UI ever renders, so there's no partial update to get wrong.

Two more messages go from the daemon to the app:

- **`ProbeResult`** answers `ProbeShortcut`, a command that asks whether a chord is free to use as a shortcut.
- **`Focus`** asks the app to bring its window forward.

## Adding a command

1. Add a variant to `Command` in `crates/ipc/src/command.rs`.
2. Handle it in `crates/engine/src/commands.rs`. If it changes settings, the edit itself is a rule in `core::settings`, which the app's fake daemon runs too ([The engine](engine.md#settings-changes)).
3. Send it from the UI, and teach the fake daemon in `apps/app/ui/src/bridge/fake/` to apply it.

## Keeping both sides in step

The protocol version, and other values both the Rust and TypeScript sides use, are checked by Rust tests that read the other side's source. For example, a test in `crates/ipc/src/messages.rs` checks that the fake daemon speaks the same protocol version.

In the app, the Tauri shell handles `Hello` and `Focus` itself and relays everything else between the pipes and the web UI. When the daemon closes the pipe, the app quits. See [The app](app.md).
