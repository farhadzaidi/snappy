// The editor's draft: what is being edited, what was last saved, and the undo history.
// Every function returns a new state.

import type { KeyChord, Layout, LayoutOptions } from "../core/model";
import { MAX_ZONES } from "../core/model";

/** Undo keeps this many steps. */
export const HISTORY_LIMIT = 60;
export const UNTITLED = "Untitled layout";

export type Tool = "select" | "verticalCut" | "horizontalCut";
export type SidebarTab = "zone" | "shortcuts";

/** Everything Save writes: the layout and its options. */
export interface Snapshot {
  layout: Layout;
  options: LayoutOptions;
}

export interface Draft {
  current: Snapshot;
  /** The version the daemon holds, or null for a layout never saved. */
  saved: Snapshot | null;
  past: Snapshot[];
  future: Snapshot[];
  /** Where an open undo step began: a canvas gesture or a name-editing session. */
  stepStart: Snapshot | null;
  selectedZoneId: string | null;
  tool: Tool;
  tab: SidebarTab;
}

export function newId(): string {
  return crypto.randomUUID();
}

export function emptyOptions(): LayoutOptions {
  return { enabled: true, selectShortcut: null, zoneShortcuts: {} };
}

function fresh(current: Snapshot, saved: Snapshot | null): Draft {
  return {
    current,
    saved,
    past: [],
    future: [],
    stepStart: null,
    selectedZoneId: current.layout.zones[0]?.id ?? null,
    tool: "select",
    tab: "zone",
  };
}

/** An empty canvas named "Untitled layout". */
export function blankDraft(): Draft {
  return fresh({ layout: { id: newId(), name: UNTITLED, zones: [] }, options: emptyOptions() }, null);
}

/** Open a stored layout for editing. */
export function openDraft(layout: Layout, options: LayoutOptions): Draft {
  const snapshot = { layout, options };
  return fresh(snapshot, snapshot);
}

/**
 * Duplicate a layout: a custom copy named "<name> copy" with new IDs. Zone shortcuts
 * carry over by zone order; the selection shortcut doesn't.
 */
export function duplicateDraft(layout: Layout, options: LayoutOptions): Draft {
  const zones = layout.zones.map((zone) => ({ ...zone, id: newId() }));
  const zoneShortcuts: Record<string, KeyChord> = {};
  layout.zones.forEach((zone, index) => {
    const chord = options.zoneShortcuts[zone.id];
    const copy = zones[index];
    if (chord && copy) {
      zoneShortcuts[copy.id] = chord;
    }
  });
  return fresh(
    {
      layout: { id: newId(), name: `${layout.name} copy`, zones },
      options: { enabled: options.enabled, selectShortcut: null, zoneShortcuts },
    },
    null,
  );
}

/** One complete edit, recorded as one undo step. */
export function edit(draft: Draft, change: (snapshot: Snapshot) => Snapshot): Draft {
  const closed = endStep(draft);
  const next = change(closed.current);
  if (sameSnapshot(next, closed.current)) {
    return closed;
  }
  return { ...closed, current: next, past: remember(closed.past, closed.current), future: [] };
}

/** Open an undo step that a gesture or name edit fills in with `preview`. */
export function beginStep(draft: Draft): Draft {
  return draft.stepStart ? draft : { ...draft, stepStart: draft.current };
}

/** Show an in-progress change without recording it yet. */
export function preview(draft: Draft, change: (snapshot: Snapshot) => Snapshot): Draft {
  const started = beginStep(draft);
  return { ...started, current: change(started.current) };
}

/** Close the open step, recording it if anything changed. */
export function endStep(draft: Draft): Draft {
  const start = draft.stepStart;
  if (!start) {
    return draft;
  }
  if (sameSnapshot(start, draft.current)) {
    return { ...draft, stepStart: null };
  }
  return { ...draft, stepStart: null, past: remember(draft.past, start), future: [] };
}

function remember(past: Snapshot[], snapshot: Snapshot): Snapshot[] {
  return [...past, snapshot].slice(-HISTORY_LIMIT);
}

export function undo(draft: Draft): Draft {
  const closed = endStep(draft);
  const previous = closed.past.at(-1);
  if (!previous) {
    return closed;
  }
  return keepSelection({
    ...closed,
    current: previous,
    past: closed.past.slice(0, -1),
    future: [closed.current, ...closed.future],
  });
}

export function redo(draft: Draft): Draft {
  const closed = endStep(draft);
  const [next, ...rest] = closed.future;
  if (!next) {
    return closed;
  }
  return keepSelection({
    ...closed,
    current: next,
    past: remember(closed.past, closed.current),
    future: rest,
  });
}

/** Keep the selection on a zone that still exists, or move it to the first zone. */
export function keepSelection(draft: Draft): Draft {
  const zones = draft.current.layout.zones;
  if (zones.some((zone) => zone.id === draft.selectedZoneId)) {
    return draft;
  }
  return { ...draft, selectedZoneId: zones[0]?.id ?? null };
}

/**
 * A saved layout differs when its geometry, name, or options changed; a
 * layout never saved differs once it has a zone, a new name, or changed options.
 */
export function hasChanges(draft: Draft): boolean {
  const { current, saved } = draft;
  if (saved) {
    return !sameSnapshot(current, saved);
  }
  return (
    current.layout.zones.length > 0 ||
    current.layout.name !== UNTITLED ||
    !sameValue(current.options, emptyOptions())
  );
}

/** After a save, the saved version is what was sent. */
export function markSaved(draft: Draft, snapshot: Snapshot): Draft {
  return { ...draft, saved: snapshot };
}

/**
 * Follow changes made elsewhere to the stored version. An unchanged draft takes the new
 * version; edits stay and are compared against it. A deleted layout counts as never saved.
 */
export function followStored(draft: Draft, stored: Snapshot | null): Draft {
  if (!draft.saved || (stored && sameSnapshot(stored, draft.saved))) {
    return draft;
  }
  if (!stored) {
    return { ...draft, saved: null };
  }
  return hasChanges(draft)
    ? { ...draft, saved: stored }
    : keepSelection({ ...draft, current: stored, saved: stored });
}

export function canAddZone(draft: Draft): boolean {
  return draft.current.layout.zones.length < MAX_ZONES;
}

export function sameSnapshot(a: Snapshot, b: Snapshot): boolean {
  return sameValue(a, b);
}

/** Structural equality for JSON-shaped values, ignoring object key order. */
export function sameValue(a: unknown, b: unknown): boolean {
  if (a === b) {
    return true;
  }
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) {
    return false;
  }
  if (Array.isArray(a) || Array.isArray(b)) {
    return (
      Array.isArray(a) &&
      Array.isArray(b) &&
      a.length === b.length &&
      a.every((item, i) => sameValue(item, b[i]))
    );
  }
  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  const keys = Object.keys(left).filter((key) => left[key] !== undefined);
  const otherKeys = Object.keys(right).filter((key) => right[key] !== undefined);
  return keys.length === otherKeys.length && keys.every((key) => sameValue(left[key], right[key]));
}
