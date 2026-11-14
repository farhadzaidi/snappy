import { describe, expect, it } from "vitest";

import {
  commandModifierHint,
  compactLabel,
  longLabel,
  modifierLabel,
  modifierName,
  recorderHelp,
  snappyKeyLabel,
  unavailableMessage,
} from "./labels";
import type { KeyChord, Modifier } from "./model";

const DEFAULT_KEY: Modifier[] = ["ctrl", "alt", "meta"];

const zoneOne: KeyChord = { code: "Digit1", mods: ["snappy"] };

describe("labels", () => {
  it("the Snappy key is named as itself, whatever modifiers it stands for", () => {
    expect(longLabel(zoneOne, "macos")).toBe("Snappy + 1");
    expect(compactLabel(zoneOne, "windows")).toBe("Snappy + 1");
    expect(compactLabel({ code: "Comma", mods: ["snappy", "shift"] }, "macos")).toBe("Snappy + Shift + ,");
    expect(snappyKeyLabel(DEFAULT_KEY, "macos")).toBe("Control + Option + Command");
    expect(snappyKeyLabel(DEFAULT_KEY, "linux")).toBe("Ctrl + Alt + Super");
  });

  it("the Snappy key's label follows the modifiers chosen for it", () => {
    expect(snappyKeyLabel(["alt", "ctrl"], "macos")).toBe("Control + Option");
    expect(snappyKeyLabel(["shift", "meta"], "windows")).toBe("Shift + Win");
    expect(compactLabel(zoneOne, "linux")).toBe("Snappy + 1");
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
    expect(recorderHelp("macos", DEFAULT_KEY)).toBe(
      "Click and press a combination with Control, Option, or Command; hold Control + Option + Command for the Snappy key. Escape cancels; Delete clears.",
    );
    expect(recorderHelp("linux", ["ctrl", "shift"])).toBe(
      "Click and press a combination with Ctrl, Alt, or Super; hold Ctrl + Shift for the Snappy key. Escape cancels; Delete clears.",
    );
  });

  it("a refused probe names the shortcut", () => {
    expect(unavailableMessage(zoneOne, "macos")).toBe(
      "Snappy + 1 is already used by macOS or another app. Choose another shortcut.",
    );
  });
});
