// When the editor's Save button is enabled.

import type { PlatformName, Settings } from "../core/model";
import { coreLimits } from "../core/limits";
import { core } from "../core/snappyCore";
import { hasChanges, type Draft, type Snapshot } from "./draft";

/** The first shortcut conflict the draft would create, shown in the sidebar. */
export function draftConflict(snapshot: Snapshot, settings: Settings, platform: PlatformName): string | null {
  return core.layoutConflict(settings, snapshot.layout, snapshot.options, platform);
}

/** Why the draft's name keeps it from being saved, shown in the sidebar, or null when it doesn't. */
export function nameProblem(snapshot: Snapshot): string | null {
  const name = snapshot.layout.name;
  if (name.trim() === "") {
    return "Give this layout a name to save it.";
  }
  const limit = coreLimits().nameLength;
  return [...name].length > limit ? `Shorten the name to ${limit} characters to save it.` : null;
}

/** A name, 1 to 32 zones, no overlaps, and valid options. */
export function isComplete(snapshot: Snapshot, platform: PlatformName): boolean {
  const { layout, options } = snapshot;
  const chords = [options.selectShortcut, ...Object.values(options.zoneShortcuts)];
  return (
    core.isValidLayout(layout) &&
    core.isSeparated(layout.zones) &&
    Object.keys(options.zoneShortcuts).length <= coreLimits().zones &&
    chords.every((chord) => chord === null || core.chordProblem(chord, platform) === null)
  );
}

export function canSave(
  draft: Draft,
  fieldPending: boolean,
  settings: Settings,
  platform: PlatformName,
): boolean {
  return (
    (hasChanges(draft) || fieldPending) &&
    isComplete(draft.current, platform) &&
    draftConflict(draft.current, settings, platform) === null
  );
}
