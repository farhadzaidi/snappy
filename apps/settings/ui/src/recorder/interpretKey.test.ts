import { describe, expect, it } from "vitest";

import type { Modifier } from "../core/model";
import { interpretKey, type KeyPress } from "./interpretKey";

const DEFAULT_KEY: Modifier[] = ["ctrl", "alt", "meta"];

function interpret(keyPress: KeyPress, snappyKey: Modifier[] = DEFAULT_KEY) {
  return interpretKey(keyPress, snappyKey);
}

function press(code: string, key: string, mods: Partial<KeyPress> = {}): KeyPress {
  return { code, key, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, ...mods };
}

describe("shortcut recorder keys", () => {
  it("escape cancels", () => {
    expect(interpret(press("Escape", "Escape"))).toEqual({ kind: "cancel" });
    expect(interpret(press("Escape", "Escape", { ctrlKey: true }))).toEqual({ kind: "cancel" });
  });

  it("delete and forward delete without modifiers clear", () => {
    expect(interpret(press("Backspace", "Backspace"))).toEqual({ kind: "clear" });
    expect(interpret(press("Delete", "Delete"))).toEqual({ kind: "clear" });
    expect(interpret(press("Backspace", "Backspace", { ctrlKey: true }))).toMatchObject({
      kind: "candidate",
    });
  });

  it("a combination without control, option, or command needs one", () => {
    expect(interpret(press("KeyA", "a"))).toEqual({ kind: "needsCommandModifier" });
    expect(interpret(press("KeyA", "A", { shiftKey: true }))).toEqual({ kind: "needsCommandModifier" });
  });

  it("modifier keys alone are ignored", () => {
    expect(interpret(press("ShiftLeft", "Shift", { shiftKey: true }))).toEqual({ kind: "ignore" });
    expect(interpret(press("MetaRight", "Meta", { metaKey: true }))).toEqual({ kind: "ignore" });
  });

  it("shortcuts record the physical key and the modifiers in file order", () => {
    const outcome = interpret(press("KeyQ", "a", { metaKey: true, ctrlKey: true }));
    expect(outcome).toEqual({
      kind: "candidate",
      chord: { code: "KeyQ", mods: ["ctrl", "meta"], label: "A" },
    });
  });

  it("holding every modifier of the Snappy key records the Snappy key plus the rest", () => {
    const all = { ctrlKey: true, altKey: true, metaKey: true };
    expect(interpret(press("Digit1", "¡", all))).toEqual({
      kind: "candidate",
      chord: { code: "Digit1", mods: ["snappy"] },
    });
    expect(interpret(press("BracketLeft", "{", { ...all, shiftKey: true }))).toEqual({
      kind: "candidate",
      chord: { code: "BracketLeft", mods: ["snappy", "shift"] },
    });
  });

  it("a chosen Snappy key records as the Snappy key, and the old one's modifiers no longer do", () => {
    const controlOption: Modifier[] = ["ctrl", "alt"];
    expect(interpret(press("KeyJ", "∆", { ctrlKey: true, altKey: true }), controlOption)).toEqual({
      kind: "candidate",
      chord: { code: "KeyJ", mods: ["snappy"] },
    });
    expect(
      interpret(press("KeyJ", "∆", { ctrlKey: true, altKey: true, metaKey: true }), controlOption),
    ).toEqual({
      kind: "candidate",
      chord: { code: "KeyJ", mods: ["snappy", "meta"] },
    });
    expect(interpret(press("KeyJ", "∆", { ctrlKey: true, altKey: true }))).toEqual({
      kind: "candidate",
      chord: { code: "KeyJ", mods: ["ctrl", "alt"] },
    });
  });

  it("labels come from the code when option or shift changes the character", () => {
    expect(interpret(press("Digit1", "¡", { altKey: true, metaKey: true }))).toEqual({
      kind: "candidate",
      chord: { code: "Digit1", mods: ["alt", "meta"] },
    });
    expect(interpret(press("ArrowLeft", "ArrowLeft", { ctrlKey: true }))).toEqual({
      kind: "candidate",
      chord: { code: "ArrowLeft", mods: ["ctrl"] },
    });
  });
});
