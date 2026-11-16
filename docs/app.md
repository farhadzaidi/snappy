# The app

The app (`apps/app`) is the window where people pick layouts, edit them, manage workspaces, and change settings. It shows state and sends commands; it computes no rules itself.

## The Tauri shell

`apps/app/src-tauri` is a small Rust program around the web UI. It:

- relays frames between the pipes to the daemon and the web UI ([Talking to the app](ipc.md));
- adds the Edit menu that text fields need for the standard editing shortcuts on macOS;
- reports which display its window is on.

Its content security policy (`tauri.conf.json`) allows only bundled assets. Its one plugin is the file dialog, for importing and exporting settings.

## The React UI

`apps/app/ui/src` has four pages: Layouts, Editor, Workspaces, and Settings, inside the shell in `shell/`. `state/daemonClient.ts` sends commands and turns each `State` from the daemon into what the pages render.

## The WebAssembly core

`crates/wasm` compiles core's rules to WebAssembly, and `core/snappyCore.ts` loads it. It answers everything that's a rule: zone frames, editor gestures and guides, shortcut conflicts, titles and labels, the help list of defaults, limits, and validation. The UI never computes geometry or names a shortcut itself.

This is why a zone drawn in the editor lands exactly where the daemon puts the window, and why a conflict the shortcut recorder reports is the one the daemon would.

## The layout editor

The editor's rules are pure functions from zones to zones in `crates/core/src/editor/`: moving, resizing, drawing, cutting, merging, pasting, snapping to guides, and repairing overlaps. A property test checks that no sequence of them leaves zones overlapping, undersized, or more than 32.

The UI keeps the draft, its selection, the clipboard, and undo (`editor/draft.ts`, `editor/editorStore.ts`), and sends the finished layout on save.

## The fake daemon

`apps/app/ui/src/bridge/fake/` stands in for the pipe in development and in the UI's tests. It speaks the same messages and applies commands with the real core rules through WebAssembly, so the UI behaves as it would against the real daemon.

Running the UI in a browser against the fake daemon is the safe way to work on the app; [CONTRIBUTING.md](../CONTRIBUTING.md#work-on-the-app-in-a-browser) shows how.
