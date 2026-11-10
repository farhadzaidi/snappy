// What one key press means to a shortcut recorder. A press holding every modifier of the
// Snappy key records the Snappy key plus whatever else is held.

import type { KeyChord, Modifier } from "../core/model";
import { core } from "../core/snappyCore";

export interface KeyPress {
  /** The physical key, `KeyboardEvent.code`. */
  code: string;
  /** The character the key produced, `KeyboardEvent.key`. */
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

export type KeyOutcome =
  | { kind: "cancel" }
  | { kind: "clear" }
  | { kind: "ignore" }
  | { kind: "needsCommandModifier" }
  | { kind: "candidate"; chord: KeyChord };

const MODIFIER_CODES = new Set([
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "CapsLock",
  "Fn",
]);

/** Delete (Backspace) and Forward Delete clear the shortcut when pressed alone. */
const CLEARING_CODES = new Set(["Backspace", "Delete"]);

export function interpretKey(press: KeyPress): KeyOutcome {
  if (press.code === "Escape") {
    return { kind: "cancel" };
  }
  if (MODIFIER_CODES.has(press.code) || press.code === "") {
    return { kind: "ignore" };
  }
  const mods = modifiersOf(press);
  if (CLEARING_CODES.has(press.code) && mods.length === 0) {
    return { kind: "clear" };
  }
  if (!press.ctrlKey && !press.altKey && !press.metaKey) {
    return { kind: "needsCommandModifier" };
  }
  const label = keyLabel(press);
  const recorded: KeyChord =
    label === undefined ? { code: press.code, mods } : { code: press.code, mods, label };
  return { kind: "candidate", chord: core.normalizeChord(recorded) };
}

/** Modifiers in the order settings files write them. */
function modifiersOf(press: KeyPress): Modifier[] {
  const held: [boolean, Modifier][] = [
    [press.shiftKey, "shift"],
    [press.ctrlKey, "ctrl"],
    [press.altKey, "alt"],
    [press.metaKey, "meta"],
  ];
  return held.filter(([down]) => down).map(([, modifier]) => modifier);
}

/**
 * The key's character, so non-US layouts show what is printed on the key. Option and
 * Shift change the character a key types, so with either held the label comes from the
 * key code instead.
 */
function keyLabel(press: KeyPress): string | undefined {
  const printable = [...press.key].length === 1 && press.key.trim() !== "";
  if (!printable || press.altKey || press.shiftKey) {
    return undefined;
  }
  return press.key.toUpperCase();
}
