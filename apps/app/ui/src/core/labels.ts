// How shortcuts and modifiers are named on each platform.

import {
  MODIFIERS,
  PLATFORMS,
  type KeyChord,
  type Modifier,
  type PlatformName,
  type SwapModifier,
} from "./model";
import { core, type LabelParts } from "./snappyCore";

/** The Snappy key's name in long labels, and so in every message that names a shortcut. */
export const SNAPPY_KEY_NAME = "Snappy";

/** What joins the keys of a shortcut's label, as core joins them: "Shift ⇧ + J". */
const KEY_SEPARATOR = " + ";

/** How a long label starts when the shortcut holds the Snappy key. */
const SNAPPY_KEY_IN_SHORTCUT = `${SNAPPY_KEY_NAME}${KEY_SEPARATOR}`;

/** "Shift ⇧ + 1" after the Snappy key, or "Control ⌃ + L": how recorders and cards draw it. */
export function labelParts(chord: KeyChord, platform: PlatformName): LabelParts {
  return core.chordLabels(chord, platform).parts;
}

/**
 * The keys named by a label's part after the Snappy key, in order: "Shift ⇧ + J" gives
 * "Shift ⇧" and "J". A key's own label is the character it types or its name, never a joined pair, so
 * "Shift ⇧ + +" gives "Shift ⇧" and "+".
 */
export function chordKeys(keys: string): string[] {
  return keys.split(KEY_SEPARATOR);
}

/** "Snappy + 1" or "Control ⌃ + Option ⌥ + L", used in messages and for assistive technology. */
export function longLabel(chord: KeyChord, platform: PlatformName): string {
  return core.chordLabels(chord, platform).long;
}

/** A shortcut as a message writes it: its long label, and that label split as shortcuts are drawn. */
export interface ShortcutInText extends LabelParts {
  long: string;
}

/**
 * The message as text and the shortcuts it names, in order, so each shortcut can be set apart
 * as one reference: "Snappy + J is taken." gives "Snappy + J", the Snappy key with "J", then
 * " is taken.".
 */
export function splitAtShortcuts(message: string): (string | ShortcutInText)[] {
  const pieces: (string | ShortcutInText)[] = [];
  let from = 0;
  for (const match of message.matchAll(shortcutPattern())) {
    const long = match[0];
    const snappyKey = long.startsWith(SNAPPY_KEY_IN_SHORTCUT);
    pieces.push(message.slice(from, match.index));
    pieces.push({ long, snappyKey, keys: snappyKey ? long.slice(SNAPPY_KEY_IN_SHORTCUT.length) : long });
    from = match.index + long.length;
  }
  pieces.push(message.slice(from));
  return pieces.filter((piece) => piece !== "");
}

let shortcutPatternCache: RegExp | undefined;

/**
 * A long label inside a message: modifiers joined by pluses, each by its name and, on macOS,
 * its symbol, then one key, which is a single word or "Num 1" and may be followed by the
 * sentence's punctuation. A key is never a modifier, so "Control ⌃ + Option ⌥ + Command ⌘"
 * names no shortcut.
 */
function shortcutPattern(): RegExp {
  if (!shortcutPatternCache) {
    const names = new Set([SNAPPY_KEY_NAME]);
    for (const platform of PLATFORMS) {
      for (const modifier of MODIFIERS) {
        const { name, label } = core.modifierNames(modifier, platform);
        names.add(name).add(label);
      }
    }
    // Longest first, so "Shift ⇧" is tried before "Shift".
    const name = `(?:${[...names].sort((a, b) => b.length - a.length).join("|")})`;
    const end = String.raw`(?=[.,;:!?)]?(?:\s|$))`;
    shortcutPatternCache = new RegExp(
      String.raw`\b(?:${name} \+ )+(?!${name}${end})(?:Num \d|\S+?)${end}`,
      "g",
    );
  }
  return shortcutPatternCache;
}

/** What the Snappy key is: "Control ⌃ + Option ⌥ + Command ⌘". */
export function snappyKeyLabel(snappyKey: Modifier[], platform: PlatformName): string {
  return core.snappyKeyLabel(snappyKey, platform);
}

export function modifierName(modifier: Modifier, platform: PlatformName): string {
  return core.modifierNames(modifier, platform).name;
}

/** The modifier with its symbol, as every label shows it: "Shift ⇧". "Off" for no swap key. */
export function modifierLabel(modifier: Modifier | SwapModifier, platform: PlatformName): string {
  return modifier === "off" ? "Off" : core.modifierNames(modifier, platform).label;
}

function commandModifiers(platform: PlatformName): [string, string, string] {
  return [modifierName("ctrl", platform), modifierName("alt", platform), modifierName("meta", platform)];
}

export function recorderHelp(platform: PlatformName, snappyKey: Modifier[]): string {
  const [ctrl, alt, meta] = commandModifiers(platform);
  const snappy = snappyKeyLabel(snappyKey, platform);
  return `Click, then press a combination with ${ctrl}, ${alt}, or ${meta}. Hold ${snappy} for the Snappy key. Escape cancels, Delete clears.`;
}
