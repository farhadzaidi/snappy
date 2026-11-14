// How shortcuts and modifiers are named on each platform.

import type { KeyChord, Modifier, PlatformName, SwapModifier } from "./model";
import { core } from "./snappyCore";

/** "Snappy + 1" or "Ctrl + Opt + L", used on recorders and cards. */
export function compactLabel(chord: KeyChord, platform: PlatformName): string {
  return core.chordLabels(chord, platform).compact;
}

/** "Snappy + 1" or "Control + Option + L", used in messages. */
export function longLabel(chord: KeyChord, platform: PlatformName): string {
  return core.chordLabels(chord, platform).long;
}

/** What the Snappy key is: "Control + Option + Command". */
export function snappyKeyLabel(snappyKey: Modifier[], platform: PlatformName): string {
  return core.snappyKeyLabel(snappyKey, platform);
}

export function modifierName(modifier: Modifier, platform: PlatformName): string {
  return core.modifierNames(modifier, platform).name;
}

/** The modifier with its symbol, as messages show it: "Shift ⇧". "Off" for no swap key. */
export function modifierLabel(modifier: Modifier | SwapModifier, platform: PlatformName): string {
  return modifier === "off" ? "Off" : core.modifierNames(modifier, platform).label;
}

function commandModifiers(platform: PlatformName): [string, string, string] {
  return [modifierName("ctrl", platform), modifierName("alt", platform), modifierName("meta", platform)];
}

/** "Include Control, Option, or Command." */
export function commandModifierHint(platform: PlatformName): string {
  const [ctrl, alt, meta] = commandModifiers(platform);
  return `Include ${ctrl}, ${alt}, or ${meta}.`;
}

export function recorderHelp(platform: PlatformName, snappyKey: Modifier[]): string {
  const [ctrl, alt, meta] = commandModifiers(platform);
  const snappy = snappyKeyLabel(snappyKey, platform);
  return `Click and press a combination with ${ctrl}, ${alt}, or ${meta}; hold ${snappy} for the Snappy key. Escape cancels; Delete clears.`;
}

const SYSTEM_NAMES: Record<PlatformName, string> = { macos: "macOS", windows: "Windows", linux: "Linux" };

/** Why the availability probe refused a shortcut. */
export function unavailableMessage(chord: KeyChord, platform: PlatformName): string {
  return `${longLabel(chord, platform)} is already used by ${SYSTEM_NAMES[platform]} or another app. Choose another shortcut.`;
}
