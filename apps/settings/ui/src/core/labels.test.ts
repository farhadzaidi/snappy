import { describe, expect, it } from "vitest";

import {
  commandModifierHint,
  compactLabel,
  defaultZoneShortcuts,
  longLabel,
  modifierLabel,
  modifierName,
  recorderHelp,
  snappyKeyLabel,
  unavailableMessage,
} from "./labels";
import type { KeyChord } from "./model";

const zoneOne: KeyChord = { code: "Digit1", mods: ["snappy"] };

describe("labels", () => {
  it("the Snappy key is named as itself, whatever modifiers it stands for", () => {
    expect(longLabel(zoneOne, "macos")).toBe("Snappy + 1");
    expect(compactLabel(zoneOne, "windows")).toBe("Snappy + 1");
    expect(compactLabel({ code: "Digit1", mods: ["ctrl", "alt", "meta"] }, "linux")).toBe("Snappy + 1");
    expect(compactLabel({ code: "Comma", mods: ["snappy", "shift"] }, "macos")).toBe("Snappy + Shift + ,");
    expect(snappyKeyLabel("macos")).toBe("Control + Option + Command");
    expect(snappyKeyLabel("linux")).toBe("Ctrl + Alt + Super");
  });

  it("long and compact labels follow display order on each platform", () => {
    const chord: KeyChord = { code: "KeyL", mods: ["meta", "alt"] };
    expect(longLabel(chord, "macos")).toBe("Option + Command + L");
    expect(compactLabel(chord, "macos")).toBe("Opt + Cmd + L");
    expect(compactLabel(chord, "windows")).toBe("Alt + Win + L");
    expect(compactLabel({ code: "ArrowLeft", mods: ["shift", "ctrl"] }, "macos")).toBe("Ctrl + Shift + ←");
    expect(compactLabel({ code: "KeyQ", mods: ["meta"], label: "A" }, "macos")).toBe("Cmd + A");
  });

  it("modifier names follow the platform", () => {
    expect(modifierName("alt", "macos")).toBe("Option");
    expect(modifierName("meta", "windows")).toBe("Win");
    expect(modifierName("meta", "linux")).toBe("Super");
    expect(modifierLabel("shift", "macos")).toBe("Shift ⇧");
    expect(modifierLabel("off", "macos")).toBe("Off");
  });

  it("the hint and help name the platform's modifiers", () => {
    expect(commandModifierHint("macos")).toBe("Include Control, Option, or Command.");
    expect(commandModifierHint("windows")).toBe("Include Ctrl, Alt, or Win.");
    expect(recorderHelp("macos")).toBe(
      "Click and press a combination with Control, Option, or Command. Escape cancels; Delete clears.",
    );
  });

  it("default zone shortcuts read as the Snappy key, then 1–9", () => {
    expect(defaultZoneShortcuts("macos")).toBe("Snappy + 1–9");
  });

  it("a refused probe names the shortcut", () => {
    expect(unavailableMessage(zoneOne, "macos")).toBe(
      "Snappy + 1 is already used by macOS or another app. Choose another shortcut.",
    );
  });
});
