import { describe, expect, it } from "vitest";

import type { Layout, Zone } from "../core/model";
import { MAX_ZONES } from "../core/model";
import {
  HISTORY_LIMIT,
  UNTITLED,
  blankDraft,
  canAddZone,
  duplicateDraft,
  edit,
  emptyOptions,
  endStep,
  followStored,
  hasChanges,
  markSaved,
  openDraft,
  preview,
  redo,
  undo,
  type Snapshot,
} from "./draft";
import { isComplete } from "./saving";

function zone(id: string, x: number): Zone {
  return { id, rect: { x, y: 0, width: 0.1, height: 1 } };
}

function addZone(id: string, x: number) {
  return (snapshot: Snapshot): Snapshot => ({
    ...snapshot,
    layout: { ...snapshot.layout, zones: [...snapshot.layout.zones, zone(id, x)] },
  });
}

const stored: Layout = { id: "custom", name: "Coding", zones: [zone("a", 0)] };

describe("editor draft", () => {
  it("a new draft is an empty canvas named Untitled layout", () => {
    const draft = blankDraft();
    expect(draft.current.layout.name).toBe(UNTITLED);
    expect(draft.current.layout.zones).toEqual([]);
    expect(hasChanges(draft)).toBe(false);
  });

  it("a never-saved layout differs once it has a zone, a name, or options", () => {
    expect(hasChanges(edit(blankDraft(), addZone("z", 0)))).toBe(true);
    expect(hasChanges(edit(blankDraft(), (s) => ({ ...s, layout: { ...s.layout, name: "Mine" } })))).toBe(
      true,
    );
    expect(hasChanges(edit(blankDraft(), (s) => ({ ...s, options: { ...s.options, enabled: false } })))).toBe(
      true,
    );
  });

  it("a saved layout differs only after a change", () => {
    const draft = openDraft(stored, emptyOptions());
    expect(hasChanges(draft)).toBe(false);
    expect(hasChanges(edit(draft, addZone("b", 0.5)))).toBe(true);
  });

  it("undoing back to the saved content means no changes", () => {
    const edited = edit(openDraft(stored, emptyOptions()), addZone("b", 0.5));
    expect(hasChanges(undo(edited))).toBe(false);
    expect(hasChanges(redo(undo(edited)))).toBe(true);
  });

  it("history keeps the most recent 60 steps", () => {
    let draft = blankDraft();
    for (let i = 0; i < HISTORY_LIMIT + 5; i += 1) {
      draft = edit(draft, addZone(`z${i}`, 0));
    }
    expect(draft.past).toHaveLength(HISTORY_LIMIT);
    for (let i = 0; i < HISTORY_LIMIT + 5; i += 1) {
      draft = undo(draft);
    }
    expect(draft.current.layout.zones).toHaveLength(5);
  });

  it("a gesture or a name editing session is one undo step", () => {
    let draft = blankDraft();
    for (const name of ["C", "Co", "Cod", "Code"]) {
      draft = preview(draft, (s) => ({ ...s, layout: { ...s.layout, name } }));
    }
    draft = endStep(draft);
    expect(draft.past).toHaveLength(1);
    expect(undo(draft).current.layout.name).toBe(UNTITLED);
  });

  it("new clears the undo history", () => {
    expect(blankDraft().past).toEqual([]);
  });

  it("duplicates get new ids, a copy name, zone shortcuts by order, and no selection shortcut", () => {
    const chord = { code: "Digit1", mods: ["ctrl" as const, "alt" as const, "meta" as const] };
    const draft = duplicateDraft(stored, {
      enabled: true,
      selectShortcut: chord,
      zoneShortcuts: { a: chord },
    });
    const copy = draft.current;
    expect(copy.layout.name).toBe("Coding copy");
    expect(copy.layout.id).not.toBe("custom");
    expect(copy.layout.zones[0]?.id).not.toBe("a");
    expect(copy.options.zoneShortcuts[copy.layout.zones[0]?.id ?? ""]).toEqual(chord);
    expect(copy.options.selectShortcut).toBeNull();
    expect(hasChanges(draft)).toBe(true);
  });

  it("after saving, the saved version is what was sent", () => {
    const edited = edit(openDraft(stored, emptyOptions()), addZone("b", 0.5));
    expect(hasChanges(markSaved(edited, edited.current))).toBe(false);
  });

  it("an unchanged draft follows changes made elsewhere and edits survive them", () => {
    const draft = openDraft(stored, emptyOptions());
    const disabled = { layout: stored, options: { ...emptyOptions(), enabled: false } };
    expect(followStored(draft, disabled).current.options.enabled).toBe(false);
    const edited = edit(draft, addZone("b", 0.5));
    const followed = followStored(edited, disabled);
    expect(followed.current.layout.zones).toHaveLength(2);
    expect(followStored(draft, null).saved).toBeNull();
  });

  it("a draft holds at most 32 zones, and overlaps can't be saved", () => {
    let draft = blankDraft();
    for (let i = 0; i < MAX_ZONES; i += 1) {
      draft = edit(draft, addZone(`z${i}`, (i % 9) * 0.1));
    }
    expect(canAddZone(draft)).toBe(false);

    const overlapping: Snapshot = {
      layout: {
        id: "o",
        name: "Overlap",
        zones: [zone("a", 0), { id: "b", rect: { x: 0.05, y: 0, width: 0.1, height: 1 } }],
      },
      options: emptyOptions(),
    };
    expect(isComplete(overlapping, "macos")).toBe(false);
    expect(isComplete({ ...overlapping, layout: { ...stored } }, "macos")).toBe(true);
  });
});
