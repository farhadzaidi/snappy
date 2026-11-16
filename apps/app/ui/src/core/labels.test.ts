import { describe, expect, it } from "vitest";

import {
  chordKeys,
  labelParts,
  longLabel,
  modifierLabel,
  modifierName,
  recorderHelp,
  snappyKeyLabel,
  splitAtShortcuts,
} from "./labels";
import { PLATFORMS, type KeyChord, type Modifier, type NamedShortcut } from "./model";
import { core } from "./snappyCore";

const DEFAULT_KEY: Modifier[] = ["ctrl", "alt", "meta"];

const zoneOne: KeyChord = { code: "Digit1", mods: ["snappy"] };
const halvesZoneOne: NamedShortcut = {
  id: { kind: "zone", layout: "halves", zone: "halves-0" },
  title: "Zone 1 of Halves",
};

describe("labels", () => {
  it("the Snappy key is named as itself, whatever modifiers it stands for", () => {
    expect(longLabel(zoneOne, "macos")).toBe("Snappy + 1");
    expect(longLabel({ code: "Comma", mods: ["snappy", "shift"] }, "macos")).toBe("Snappy + Shift ⇧ + ,");
    expect(snappyKeyLabel(DEFAULT_KEY, "macos")).toBe("Control ⌃ + Option ⌥ + Command ⌘");
    expect(snappyKeyLabel(DEFAULT_KEY, "linux")).toBe("Ctrl + Alt + Super");
  });

  it("the Snappy key's label follows the modifiers chosen for it", () => {
    expect(snappyKeyLabel(["alt", "ctrl"], "macos")).toBe("Control ⌃ + Option ⌥");
    expect(snappyKeyLabel(["shift", "meta"], "windows")).toBe("Shift + Win");
    expect(longLabel(zoneOne, "linux")).toBe("Snappy + 1");
  });

  it("labels name each modifier with its symbol on macOS and by name elsewhere, in display order", () => {
    const chord: KeyChord = { code: "KeyL", mods: ["meta", "alt"] };
    expect(longLabel(chord, "macos")).toBe("Option ⌥ + Command ⌘ + L");
    expect(labelParts(chord, "macos")).toEqual({ snappyKey: false, keys: "Option ⌥ + Command ⌘ + L" });
    expect(labelParts(chord, "windows")).toEqual({ snappyKey: false, keys: "Alt + Win + L" });
    expect(labelParts({ code: "KeyF", mods: ["alt", "ctrl"] }, "macos").keys).toBe(
      "Control ⌃ + Option ⌥ + F",
    );
    expect(labelParts({ code: "KeyQ", mods: ["meta"], label: "A" }, "macos").keys).toBe("Command ⌘ + A");
  });

  it("label parts leave the Snappy key to its glyph", () => {
    const shifted: KeyChord = { code: "KeyJ", mods: ["snappy", "shift"] };
    expect(labelParts(shifted, "macos")).toEqual({ snappyKey: true, keys: "Shift ⇧ + J" });
    expect(labelParts(shifted, "linux")).toEqual({ snappyKey: true, keys: "Shift + J" });
    expect(labelParts(zoneOne, "windows")).toEqual({ snappyKey: true, keys: "1" });
  });

  it("messages split around each whole shortcut they name", () => {
    const { message } = core.unavailableShortcut(zoneOne, halvesZoneOne, "macos");
    expect(splitAtShortcuts(message)).toEqual([
      { long: "Snappy + 1", snappyKey: true, keys: "1" },
      " for Zone 1 of Halves is already used by macOS or another app.",
    ]);
    expect(
      splitAtShortcuts("Snappy can't control windows. Snappy + Shift ⇧ + J and Snappy + K clash."),
    ).toEqual([
      "Snappy can't control windows. ",
      { long: "Snappy + Shift ⇧ + J", snappyKey: true, keys: "Shift ⇧ + J" },
      " and ",
      { long: "Snappy + K", snappyKey: true, keys: "K" },
      " clash.",
    ]);
  });

  it("messages find every chord core labels, on every platform", () => {
    const chords: KeyChord[] = [
      { code: "KeyF", mods: ["ctrl", "alt"] },
      { code: "KeyJ", mods: ["snappy", "shift"] },
      { code: "ArrowLeft", mods: ["ctrl", "alt", "shift", "meta"] },
    ];
    for (const platform of PLATFORMS) {
      for (const chord of chords) {
        const long = longLabel(chord, platform);
        expect(splitAtShortcuts(`${long} is taken.`)).toEqual([
          { long, ...labelParts(chord, platform) },
          " is taken.",
        ]);
      }
    }
  });

  it("a label's keys come apart at their pluses, a + key included", () => {
    expect(chordKeys("Shift ⇧ + J")).toEqual(["Shift ⇧", "J"]);
    expect(chordKeys("Ctrl + Alt + Num 1")).toEqual(["Ctrl", "Alt", "Num 1"]);
    expect(chordKeys("Shift ⇧ + +")).toEqual(["Shift ⇧", "+"]);
    expect(chordKeys("← → ↑ ↓")).toEqual(["← → ↑ ↓"]);
    expect(chordKeys(labelParts({ code: "Comma", mods: ["snappy", "shift"] }, "macos").keys)).toEqual([
      "Shift ⇧",
      ",",
    ]);
  });

  it("a shortcut in a message ends at its key, before the sentence's punctuation", () => {
    expect(splitAtShortcuts("Another app is using Ctrl + Alt + Num 1, Snappy + ., and Snappy + →.")).toEqual([
      "Another app is using ",
      { long: "Ctrl + Alt + Num 1", snappyKey: false, keys: "Ctrl + Alt + Num 1" },
      ", ",
      { long: "Snappy + .", snappyKey: true, keys: "." },
      ", and ",
      { long: "Snappy + →", snappyKey: true, keys: "→" },
      ".",
    ]);
    expect(splitAtShortcuts("Snappy + 3 (Switch to Workspace 3) is taken.")[0]).toEqual({
      long: "Snappy + 3",
      snappyKey: true,
      keys: "3",
    });
  });

  it("modifiers alone and the word Snappy alone name no shortcut", () => {
    const labelled =
      "Reset the Snappy key to Control ⌃ + Option ⌥ + Command ⌘? Snappy can't use Shift + Win.";
    expect(splitAtShortcuts(labelled)).toEqual([labelled]);
    const named = "Reset the Snappy key to Control + Option + Command?";
    expect(splitAtShortcuts(named)).toEqual([named]);
  });

  it("modifier names follow the platform", () => {
    expect(modifierName("alt", "macos")).toBe("Option");
    expect(modifierName("meta", "windows")).toBe("Win");
    expect(modifierName("meta", "linux")).toBe("Super");
    expect(modifierLabel("shift", "macos")).toBe("Shift ⇧");
    expect(modifierLabel("off", "macos")).toBe("Off");
  });

  it("the recorders' help names the platform's modifiers", () => {
    expect(recorderHelp("macos", DEFAULT_KEY)).toBe(
      "Click, then press a combination with Control, Option, or Command. Hold Control ⌃ + Option ⌥ + Command ⌘ for the Snappy key. Escape cancels, Delete clears.",
    );
    expect(recorderHelp("linux", ["ctrl", "shift"])).toBe(
      "Click, then press a combination with Ctrl, Alt, or Super. Hold Ctrl + Shift for the Snappy key. Escape cancels, Delete clears.",
    );
  });

  it("a recorder's refused probe names the chord and its shortcut, and links to it by ID", () => {
    expect(core.unavailableShortcut(zoneOne, halvesZoneOne, "linux")).toEqual({
      message: "Snappy + 1 for Zone 1 of Halves is already used by Linux or another app.",
      shortcut: halvesZoneOne,
    });
  });
});
