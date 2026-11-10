import { describe, expect, it } from "vitest";

import { interpretKey, type KeyPress } from "./interpretKey";

function press(code: string, key: string, mods: Partial<KeyPress> = {}): KeyPress {
  return { code, key, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, ...mods };
}

describe("shortcut recorder keys", () => {
  it("escape cancels", () => {
    expect(interpretKey(press("Escape", "Escape"))).toEqual({ kind: "cancel" });
    expect(interpretKey(press("Escape", "Escape", { ctrlKey: true }))).toEqual({ kind: "cancel" });
  });

  it("delete and forward delete without modifiers clear", () => {
    expect(interpretKey(press("Backspace", "Backspace"))).toEqual({ kind: "clear" });
    expect(interpretKey(press("Delete", "Delete"))).toEqual({ kind: "clear" });
    expect(interpretKey(press("Backspace", "Backspace", { ctrlKey: true }))).toMatchObject({
      kind: "candidate",
    });
  });

  it("a combination without control, option, or command needs one", () => {
    expect(interpretKey(press("KeyA", "a"))).toEqual({ kind: "needsCommandModifier" });
    expect(interpretKey(press("KeyA", "A", { shiftKey: true }))).toEqual({ kind: "needsCommandModifier" });
  });

  it("modifier keys alone are ignored", () => {
    expect(interpretKey(press("ShiftLeft", "Shift", { shiftKey: true }))).toEqual({ kind: "ignore" });
    expect(interpretKey(press("MetaRight", "Meta", { metaKey: true }))).toEqual({ kind: "ignore" });
  });

  it("shortcuts record the physical key and the modifiers in file order", () => {
    const outcome = interpretKey(press("KeyQ", "a", { metaKey: true, ctrlKey: true }));
    expect(outcome).toEqual({
      kind: "candidate",
      chord: { code: "KeyQ", mods: ["ctrl", "meta"], label: "A" },
    });
  });

  it("holding every modifier of the Snappy key records the Snappy key plus the rest", () => {
    const all = { ctrlKey: true, altKey: true, metaKey: true };
    expect(interpretKey(press("Digit1", "¡", all))).toEqual({
      kind: "candidate",
      chord: { code: "Digit1", mods: ["snappy"] },
    });
    expect(interpretKey(press("BracketLeft", "{", { ...all, shiftKey: true }))).toEqual({
      kind: "candidate",
      chord: { code: "BracketLeft", mods: ["snappy", "shift"] },
    });
  });

  it("labels come from the code when option or shift changes the character", () => {
    expect(interpretKey(press("Digit1", "¡", { altKey: true, metaKey: true }))).toEqual({
      kind: "candidate",
      chord: { code: "Digit1", mods: ["alt", "meta"] },
    });
    expect(interpretKey(press("ArrowLeft", "ArrowLeft", { ctrlKey: true }))).toEqual({
      kind: "candidate",
      chord: { code: "ArrowLeft", mods: ["ctrl"] },
    });
  });
});
