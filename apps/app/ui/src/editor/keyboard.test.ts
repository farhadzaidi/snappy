import { describe, expect, it } from "vitest";

import { labelParts } from "../core/labels";
import type { KeyPress } from "../ui/keyPress";
import { commandChord, editorCommand } from "./keyboard";

/** A press of `code`; the editor's keys never look at the character it types. */
function press(code: string, held: Partial<Omit<KeyPress, "code" | "key">> = {}): KeyPress {
  return { code, key: "", metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, ...held };
}

describe("editor shortcuts", () => {
  it("the keys shown beside Merge, Duplicate, and Delete are the ones that run them", () => {
    expect(labelParts(commandChord("merge", "macos"), "macos").keys).toBe("Shift ⇧ + Command ⌘ + M");
    expect(labelParts(commandChord("merge", "windows"), "windows").keys).toBe("Ctrl + Shift + M");
    expect(labelParts(commandChord("duplicate", "macos"), "macos").keys).toBe("Command ⌘ + D");
    expect(labelParts(commandChord("duplicate", "windows"), "windows").keys).toBe("Ctrl + D");
    expect(labelParts(commandChord("delete", "macos"), "macos").keys).toBe("⌫");
    for (const platform of ["macos", "linux"] as const) {
      const merge = commandChord("merge", platform);
      const held = { metaKey: merge.mods.includes("meta"), ctrlKey: merge.mods.includes("ctrl") };
      expect(editorCommand(press(merge.code, { ...held, shiftKey: true }), platform)).toBe("merge");
      const duplicate = commandChord("duplicate", platform);
      expect(editorCommand(press(duplicate.code, held), platform)).toBe("duplicate");
    }
    expect(editorCommand(press(commandChord("delete", "linux").code), "linux")).toBe("delete");
  });

  it("Command runs the clipboard keys on macOS and Control does elsewhere", () => {
    expect(editorCommand(press("KeyC", { metaKey: true }), "macos")).toBe("copy");
    expect(editorCommand(press("KeyX", { metaKey: true }), "macos")).toBe("cut");
    expect(editorCommand(press("KeyV", { metaKey: true }), "macos")).toBe("paste");
    for (const platform of ["windows", "linux"] as const) {
      expect(editorCommand(press("KeyC", { ctrlKey: true }), platform)).toBe("copy");
      expect(editorCommand(press("KeyX", { ctrlKey: true }), platform)).toBe("cut");
      expect(editorCommand(press("KeyV", { ctrlKey: true }), platform)).toBe("paste");
    }
  });

  it("the other platform's command key does nothing", () => {
    expect(editorCommand(press("KeyC", { ctrlKey: true }), "macos")).toBeNull();
    expect(editorCommand(press("KeyC", { metaKey: true }), "windows")).toBeNull();
    expect(editorCommand(press("KeyC", { metaKey: true, ctrlKey: true }), "linux")).toBeNull();
  });

  it("Delete and Backspace alone delete, and with any modifier don't", () => {
    expect(editorCommand(press("Backspace"), "macos")).toBe("delete");
    expect(editorCommand(press("Delete"), "windows")).toBe("delete");
    expect(editorCommand(press("Backspace", { metaKey: true }), "macos")).toBeNull();
    expect(editorCommand(press("Delete", { shiftKey: true }), "linux")).toBeNull();
    expect(editorCommand(press("Backspace", { altKey: true }), "macos")).toBeNull();
  });

  it("merge is Shift with Command or Control, since Command-M minimizes on macOS", () => {
    expect(editorCommand(press("KeyM", { metaKey: true, shiftKey: true }), "macos")).toBe("merge");
    expect(editorCommand(press("KeyM", { ctrlKey: true, shiftKey: true }), "windows")).toBe("merge");
    expect(editorCommand(press("KeyM", { metaKey: true }), "macos")).toBeNull();
    expect(editorCommand(press("KeyC", { metaKey: true, shiftKey: true }), "macos")).toBeNull();
  });

  it("D with Command or Control duplicates, and with Shift as well doesn't", () => {
    expect(editorCommand(press("KeyD", { metaKey: true }), "macos")).toBe("duplicate");
    expect(editorCommand(press("KeyD", { ctrlKey: true }), "linux")).toBe("duplicate");
    expect(editorCommand(press("KeyD", { ctrlKey: true }), "macos")).toBeNull();
    expect(editorCommand(press("KeyD", { metaKey: true, shiftKey: true }), "macos")).toBeNull();
  });

  it("Z undoes and Shift-Z redoes", () => {
    expect(editorCommand(press("KeyZ", { metaKey: true }), "macos")).toBe("undo");
    expect(editorCommand(press("KeyZ", { metaKey: true, shiftKey: true }), "macos")).toBe("redo");
    expect(editorCommand(press("KeyZ", { ctrlKey: true }), "linux")).toBe("undo");
    expect(editorCommand(press("KeyZ"), "macos")).toBeNull();
  });

  it("plain letters and other keys are not shortcuts", () => {
    expect(editorCommand(press("KeyC"), "macos")).toBeNull();
    expect(editorCommand(press("KeyA", { metaKey: true }), "macos")).toBeNull();
    expect(editorCommand(press("Escape"), "windows")).toBeNull();
  });
});
