import { describe, expect, it } from "vitest";

import type { KeyChord, Layout, Zone } from "../core/model";
import type { DaemonClient } from "../state/daemonClient";
import { createSession } from "../state/session";
import { emptyOptions } from "./draft";
import { EditorStore } from "./editorStore";

function zone(id: string, x: number, y: number, width: number, height: number): Zone {
  return { id, rect: { x, y, width, height } };
}

/** Four quarters of the canvas: a and b on top, c and d below. */
const grid: Layout = {
  id: "custom",
  name: "Grid",
  zones: [
    zone("a", 0, 0, 0.5, 0.5),
    zone("b", 0.5, 0, 0.5, 0.5),
    zone("c", 0, 0.5, 0.5, 0.5),
    zone("d", 0.5, 0.5, 0.5, 0.5),
  ],
};

const chord: KeyChord = { code: "KeyB", mods: ["snappy"] };

/** What the editor has said since `editing` opened its layout. */
let announced: string[] = [];

function editing(layout: Layout = grid, zoneShortcuts: Record<string, KeyChord> = {}): EditorStore {
  // These actions never reach the daemon, only Save does; they only say why they can't act.
  announced = [];
  const client: Partial<DaemonClient> = { announce: (message) => void announced.push(message) };
  const editor = new EditorStore(client as DaemonClient, createSession());
  editor.open(layout, { ...emptyOptions(), zoneShortcuts });
  return editor;
}

const ids = (editor: EditorStore) => editor.draft.get().current.layout.zones.map((item) => item.id);
const rectOf = (editor: EditorStore, id: string) =>
  editor.draft.get().current.layout.zones.find((item) => item.id === id)?.rect;

describe("editor selection actions", () => {
  it("Shift-click adds zones to the selection and takes them out again", () => {
    const editor = editing();
    expect(editor.draft.get().selection).toEqual(["a"]);
    editor.toggleSelected("b");
    expect(editor.draft.get().selection).toEqual(["a", "b"]);
    editor.toggleSelected("a");
    expect(editor.draft.get().selection).toEqual(["b"]);
    editor.select("c");
    expect(editor.draft.get().selection).toEqual(["c"]);
  });

  it("merging keeps the first zone grown, moves its shortcut along, and undoes in one step", () => {
    const editor = editing(grid, { b: chord });
    editor.select("b");
    editor.toggleSelected("a");
    expect(editor.mergeRefusal()).toBeNull();
    editor.mergeSelection();
    expect(ids(editor)).toEqual(["a", "c", "d"]);
    expect(rectOf(editor, "a")).toEqual({ x: 0, y: 0, width: 1, height: 0.5 });
    expect(editor.draft.get().current.options.zoneShortcuts).toEqual({ a: chord });
    expect(editor.draft.get().selection).toEqual(["a"]);

    editor.undo();
    expect(ids(editor)).toEqual(["a", "b", "c", "d"]);
    expect(editor.draft.get().current.options.zoneShortcuts).toEqual({ b: chord });
  });

  it("zones that don't fill a rectangle say why and stay as they are", () => {
    const editor = editing();
    expect(editor.mergeRefusal()).toBe("Shift-click another zone to merge them.");
    editor.toggleSelected("d");
    expect(editor.mergeRefusal()).toBe("Zones merge only when together they fill a rectangle with no gaps.");
    editor.mergeSelection();
    expect(ids(editor)).toEqual(["a", "b", "c", "d"]);
    expect(announced).toEqual(["Zones merge only when together they fill a rectangle with no gaps."]);
  });

  it("deleting removes every selected zone and its shortcut in one step", () => {
    const editor = editing(grid, { b: chord });
    editor.select("b");
    editor.toggleSelected("d");
    editor.deleteSelection();
    expect(ids(editor)).toEqual(["a", "c"]);
    expect(editor.draft.get().current.options.zoneShortcuts).toEqual({});
    expect(editor.draft.get().selection).toEqual(["a"]);
    editor.undo();
    expect(ids(editor)).toEqual(["a", "b", "c", "d"]);
  });

  it("cutting then pasting puts the zones back where they were, as new zones", () => {
    const editor = editing();
    editor.select("b");
    editor.toggleSelected("d");
    editor.cutSelection();
    expect(ids(editor)).toEqual(["a", "c"]);

    editor.paste();
    const zones = editor.draft.get().current.layout.zones;
    expect(zones).toHaveLength(4);
    expect(zones.slice(2).map((item) => item.rect)).toEqual([grid.zones[1]?.rect, grid.zones[3]?.rect]);
    expect(editor.draft.get().selection).toEqual(zones.slice(2).map((item) => item.id));

    editor.undo();
    expect(ids(editor)).toEqual(["a", "c"]);
  });

  it("pasting where the copied zones still are finds the nearest free spot, or says there is none", () => {
    const half = { id: "half", name: "Half", zones: [zone("left", 0, 0, 0.5, 1)] };
    const editor = editing(half);
    editor.copySelection();
    editor.paste();
    expect(editor.draft.get().current.layout.zones[1]?.rect).toEqual({ x: 0.5, y: 0, width: 0.5, height: 1 });
    expect(announced).toEqual([]);
    editor.paste();
    expect(editor.draft.get().current.layout.zones).toHaveLength(2);
    expect(announced).toEqual(["No free space for the copies."]);
  });

  it("copying zones of one layout pastes them into another", () => {
    const editor = editing();
    editor.select("d");
    editor.copySelection();
    editor.open({ id: "other", name: "Other", zones: [zone("left", 0, 0, 0.5, 1)] }, emptyOptions());
    editor.paste();
    expect(editor.draft.get().current.layout.zones[1]?.rect).toEqual(grid.zones[3]?.rect);
  });

  it("duplicating copies every selected zone together without touching the clipboard", () => {
    const top = { id: "top", name: "Top", zones: [zone("a", 0, 0, 0.5, 0.5), zone("b", 0.5, 0, 0.5, 0.5)] };
    const editor = editing(top);
    editor.toggleSelected("b");
    expect(editor.duplicateRefusal()).toBeNull();
    editor.duplicateSelection();
    const zones = editor.draft.get().current.layout.zones;
    expect(zones.slice(2).map((item) => item.rect)).toEqual([
      { x: 0, y: 0.5, width: 0.5, height: 0.5 },
      { x: 0.5, y: 0.5, width: 0.5, height: 0.5 },
    ]);
    editor.paste();
    expect(editor.draft.get().current.layout.zones).toHaveLength(4);
    expect(announced).toEqual(["Copy zones first."]);
  });

  it("duplicating says why when the copies don't fit", () => {
    const editor = editing();
    editor.select("a");
    expect(editor.duplicateRefusal()).toBe("No free space for the copies.");
    editor.duplicateSelection();
    expect(ids(editor)).toEqual(["a", "b", "c", "d"]);
    expect(announced).toEqual(["No free space for the copies."]);
  });

  it("duplicating past the most zones a layout holds says so", () => {
    const strips = Array.from({ length: 32 }, (_, index) => zone(`z${index}`, index / 32, 0, 1 / 32, 0.5));
    const editor = editing({ id: "strips", name: "Strips", zones: strips });
    expect(editor.duplicateRefusal()).toBe("A layout holds at most 32 zones.");
  });

  it("a cut through the layout splits every zone it crosses as one step and selects the clicked part", () => {
    const editor = editing();
    editor.cut("through", "x", 0.25, "c");
    const zones = editor.draft.get().current.layout.zones;
    expect(zones).toHaveLength(6);
    expect(rectOf(editor, "a")?.width).toBe(0.25);
    expect(rectOf(editor, "c")?.width).toBe(0.25);
    expect(rectOf(editor, "b")?.width).toBe(0.5);
    const afterC = zones[zones.findIndex((item) => item.id === "c") + 1];
    expect(editor.draft.get().selection).toEqual([afterC?.id]);

    editor.undo();
    expect(ids(editor)).toEqual(["a", "b", "c", "d"]);
  });
});
