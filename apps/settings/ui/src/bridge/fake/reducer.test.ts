import { describe, expect, it } from "vitest";

import type { AppState, Command, KeyChord, Layout } from "../../core/model";
import { initialState } from "./fixtures";
import { applyCommand, moveItems } from "./reducer";

const freeSystem = { isAvailable: () => true };

function run(state: AppState, ...commands: Command[]) {
  return commands.reduce((outcome, command) => applyCommand(outcome.state, command, freeSystem), {
    state,
    error: null as string | null,
  });
}

const custom: Layout = {
  id: "custom-1",
  name: "Coding",
  zones: [
    { id: "a", rect: { x: 0, y: 0, width: 0.6, height: 1 } },
    { id: "b", rect: { x: 0.6, y: 0, width: 0.4, height: 1 } },
  ],
};
const noOptions = { enabled: true, selectShortcut: null, zoneShortcuts: {} };
const snappy = (code: string): KeyChord => ({ code, mods: ["snappy"] });

describe("fake daemon", () => {
  it("applying assigns the display and the default layout quietly", () => {
    const { state } = run(initialState("macos"), {
      kind: "applyLayout",
      display: "display-1",
      layout: "halves",
    });
    expect(state.settings.layouts.displays["display-1"]).toBe("halves");
    expect(state.settings.layouts.defaultLayoutId).toBe("halves");
    expect(state.status).toMatchObject({ message: "Halves selected for Built-in Display.", notify: false });
  });

  it("applying while workspaces are on assigns the active workspace", () => {
    const { state } = run(
      initialState("macos"),
      { kind: "setWorkspacesEnabled", enabled: true },
      { kind: "applyLayout", display: "display-2", layout: "rows" },
    );
    expect(state.settings.workspaces.items[0]?.displays["display-2"]).toBe("rows");
    expect(state.settings.layouts.displays["display-2"]).toBeUndefined();
  });

  it("saving stores the layout, applies it, and says so", () => {
    const { state, error } = run(initialState("macos"), {
      kind: "saveLayout",
      layout: custom,
      options: noOptions,
      display: "display-1",
    });
    expect(error).toBeNull();
    expect(state.settings.layouts.custom).toEqual([custom]);
    expect(state.settings.layouts.displays["display-1"]).toBe("custom-1");
    expect(state.status?.message).toBe("Coding saved.");
  });

  it("deleting a custom layout removes its options and assignments", () => {
    const { state } = run(
      initialState("macos"),
      { kind: "saveLayout", layout: custom, options: noOptions, display: "display-1" },
      { kind: "deleteLayout", layout: "custom-1" },
    );
    expect(state.settings.layouts.custom).toEqual([]);
    expect(state.settings.layouts.options["custom-1"]).toBeUndefined();
    expect(state.settings.layouts.displays["display-1"]).toBeUndefined();
    expect(state.settings.layouts.defaultLayoutId).toBe("focus");
    expect(state.status?.message).toBe("Custom layout deleted.");
  });

  it("enabling and disabling report the layout", () => {
    const disabled = run(initialState("macos"), { kind: "setLayoutEnabled", layout: "rows", enabled: false });
    expect(disabled.state.settings.layouts.options["rows"]?.enabled).toBe(false);
    expect(disabled.state.status?.message).toBe("Two rows disabled.");
    const enabled = run(disabled.state, { kind: "setLayoutEnabled", layout: "rows", enabled: true });
    expect(enabled.state.status?.message).toBe("Two rows enabled.");
  });

  it("a change that breaks the rules keeps the old settings and reports why", () => {
    const before = initialState("macos");
    const { state, error } = run(before, {
      kind: "setNavigationShortcut",
      direction: "left",
      chord: { code: "KeyQ", mods: ["meta"] },
    });
    expect(error).toBe("Command + Q is already assigned to a standard app command.");
    expect(state.settings).toEqual(before.settings);
  });

  it("a zone chord taken by a navigation shortcut is refused on save", () => {
    const { error } = run(initialState("macos"), {
      kind: "saveLayout",
      layout: custom,
      options: { ...noOptions, zoneShortcuts: { a: { code: "ArrowLeft", mods: ["ctrl", "alt", "meta"] } } },
      display: "display-1",
    });
    expect(error).toBe("Snappy + ← is already assigned to Move window left.");
  });

  it("when the zone is taken starts as swap and can be stack on top", () => {
    const start = initialState("windows");
    expect(start.settings.snapping.whenTaken).toBe("swap");
    const { state, error } = run(start, { kind: "setWhenTaken", choice: "stack" });
    expect(error).toBeNull();
    expect(state.settings.snapping.whenTaken).toBe("stack");
  });

  it("turning snapping on and off posts its messages", () => {
    const on = run(initialState("macos"), { kind: "setSnapping", enabled: true });
    expect(on.state.snapping.live).toBe(true);
    expect(on.state.status?.message).toBe("Snapping is on. Hold Shift ⇧ while dragging a window.");
    const off = run(on.state, { kind: "setSnapping", enabled: false });
    expect(off.state.snapping.live).toBe(false);
    expect(off.state.status?.message).toBe("Snapping is paused.");
  });

  it("a new workspace takes the lowest unused number and is reported as added", () => {
    const { state } = run(
      initialState("macos"),
      { kind: "deleteWorkspace", workspace: "workspace-2" },
      { kind: "addWorkspace" },
    );
    const added = state.settings.workspaces.items.at(-1);
    expect(added?.name).toBe("Workspace 2");
    expect(state.addedWorkspace).toBe(added?.id);
    expect([added?.switchShortcut, added?.sendShortcut, added?.sendAndSwitchShortcut]).toEqual([
      null,
      null,
      null,
    ]);
  });

  it("every platform starts with the same Snappy-key workspace steps and no undo or redo", () => {
    for (const platform of ["macos", "windows", "linux"] as const) {
      const { workspaces, history } = initialState(platform).settings;
      expect(workspaces.previous).toMatchObject(snappy("BracketLeft"));
      expect(workspaces.next).toMatchObject(snappy("BracketRight"));
      expect(workspaces.movePrevious).toMatchObject(snappy("Comma"));
      expect(workspaces.moveNext).toMatchObject(snappy("Period"));
      expect(workspaces.items.every((item) => item.switchShortcut === null)).toBe(true);
      expect(history).toEqual({ undo: null, redo: null });
    }
  });

  it("move-window steps and undo and redo shortcuts are saved and checked for conflicts", () => {
    const start = initialState("macos");
    const moved = run(start, { kind: "setWorkspaceStepShortcut", step: "moveNext", chord: snappy("KeyN") });
    expect(moved.state.settings.workspaces.moveNext).toEqual(snappy("KeyN"));
    const undo = run(moved.state, { kind: "setHistoryShortcut", step: "undo", chord: snappy("KeyU") });
    expect(undo.state.settings.history.undo).toEqual(snappy("KeyU"));
    const byHand: KeyChord = { code: "KeyU", mods: ["ctrl", "alt", "meta"] };
    const clash = run(undo.state, { kind: "setHistoryShortcut", step: "redo", chord: byHand });
    expect(clash.error).toBe("Snappy + U is already assigned to Undo window move.");
    expect(clash.state.settings.history.redo).toBeNull();
  });

  it("reordering moves workspaces with their shortcuts", () => {
    const initial = initialState("macos");
    const switchOne: KeyChord = { code: "Digit1", mods: ["ctrl"] };
    const items = initial.settings.workspaces.items.map((item, index) =>
      index === 0 ? { ...item, switchShortcut: switchOne } : item,
    );
    const start = {
      ...initial,
      settings: { ...initial.settings, workspaces: { ...initial.settings.workspaces, items } },
    };
    const { state } = run(start, { kind: "moveWorkspaces", from: [0], to: 3 });
    expect(state.settings.workspaces.items.map((item) => item.id)).toEqual([
      "workspace-2",
      "workspace-3",
      "workspace-1",
    ]);
    expect(state.settings.workspaces.items[2]?.switchShortcut).toEqual(switchOne);
    expect(moveItems(["a", "b", "c"], [2], 0)).toEqual(["c", "a", "b"]);
  });

  it("deleting the active workspace activates the first other one", () => {
    const { state } = run(initialState("macos"), { kind: "deleteWorkspace", workspace: "workspace-1" });
    expect(state.settings.workspaces.activeId).toBe("workspace-2");
    const last = run(
      state,
      { kind: "deleteWorkspace", workspace: "workspace-2" },
      { kind: "deleteWorkspace", workspace: "workspace-3" },
    );
    expect(last.state.settings.workspaces.items).toHaveLength(1);
  });

  it("the probe answers from the system's shortcut table", () => {
    const outcome = applyCommand(
      initialState("macos"),
      { kind: "probeShortcut", chord: snappy("KeyK") },
      {
        isAvailable: () => false,
      },
    );
    expect(outcome.probe).toBe(false);
  });
});
