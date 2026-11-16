// The editor's keyboard shortcuts. Each runs an action the editor already offers, with the
// platform's standard keys where there are some.

import type { KeyChord, Modifier, PlatformName } from "../core/model";
import { DELETE_KEY, isDeleting, type KeyPress } from "../ui/keyPress";

export type EditorCommand = "undo" | "redo" | "copy" | "cut" | "paste" | "duplicate" | "delete" | "merge";

/** The commands with a button of their own, which shows their keys. */
export type ButtonCommand = "merge" | "duplicate" | "delete";

const MERGE_KEY = "KeyM";
const DUPLICATE_KEY = "KeyD";

/** Keys pressed with Command on macOS, or Control elsewhere, and whether Shift is part of each. */
const COMMAND_KEYS: Readonly<Record<string, { command: EditorCommand; shift: boolean }>> = {
  KeyC: { command: "copy", shift: false },
  KeyX: { command: "cut", shift: false },
  KeyV: { command: "paste", shift: false },
  // As in drawing apps; neither the systems nor the web views give Command-D or Control-D a job here.
  [DUPLICATE_KEY]: { command: "duplicate", shift: false },
  // Command-M minimizes the window on macOS, so Merge takes Shift as well, on every platform.
  [MERGE_KEY]: { command: "merge", shift: true },
};

/**
 * The keys that run a command with a button of its own, to show beside the button: Merge's
 * and Duplicate's chords, or Delete alone.
 */
export function commandChord(command: ButtonCommand, platform: PlatformName): KeyChord {
  const held: Modifier = platform === "macos" ? "meta" : "ctrl";
  switch (command) {
    case "merge":
      return { code: MERGE_KEY, mods: ["shift", held] };
    case "duplicate":
      return { code: DUPLICATE_KEY, mods: [held] };
    case "delete":
      return { code: DELETE_KEY, mods: [] };
  }
}

/**
 * The command a key press asks for: Command (Control on Windows and Linux) with Z to undo,
 * Shift-Z to redo, C, X, and V for the clipboard, D to duplicate, and Shift-M to merge;
 * Delete or Backspace alone deletes.
 */
export function editorCommand(press: KeyPress, platform: PlatformName): EditorCommand | null {
  const macos = platform === "macos";
  const command = macos ? press.metaKey : press.ctrlKey;
  if (command && press.code === "KeyZ") {
    return press.shiftKey ? "redo" : "undo";
  }

  const otherCommand = macos ? press.ctrlKey : press.metaKey;
  if (press.altKey || otherCommand) {
    return null;
  }
  if (!command) {
    return isDeleting(press) && !press.shiftKey ? "delete" : null;
  }
  const key = COMMAND_KEYS[press.code];
  return key && key.shift === press.shiftKey ? key.command : null;
}
