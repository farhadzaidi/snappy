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
      shortcut: "left",
      chord: { code: "KeyQ", mods: ["meta"] },
    });
    expect(error).toBe("Command + Q is already assigned to a standard app command.");
    expect(state.settings).toEqual(before.settings);
  });

  it("a zone chord taken by a navigation shortcut is refused on save", () => {
    const { error } = run(initialState("macos"), {
      kind: "saveLayout",
      layout: custom,
      options: { ...noOptions, zoneShortcuts: { a: snappy("ArrowLeft") } },
      display: "display-1",
    });
    expect(error).toBe("Snappy + ← is already assigned to Move window left.");
  });

  it("every platform moves to the nearest empty zone with Snappy + Space", () => {
    for (const platform of ["macos", "windows", "linux"] as const) {
      expect(initialState(platform).settings.navigation.nearestEmpty).toEqual(snappy("Space"));
    }
  });

  it("the nearest-empty-zone shortcut is saved, cleared, and checked for conflicts", () => {
    const start = initialState("macos");
    const changed = run(start, {
      kind: "setNavigationShortcut",
      shortcut: "nearestEmpty",
      chord: snappy("KeyE"),
    });
    expect(changed.error).toBeNull();
    expect(changed.state.settings.navigation.nearestEmpty).toEqual(snappy("KeyE"));

    const clash = run(changed.state, {
      kind: "setNavigationShortcut",
      shortcut: "nearestEmpty",
      chord: snappy("ArrowUp"),
    });
    expect(clash.error).toBe("Snappy + ↑ is already assigned to Move window up.");
    expect(clash.state.settings.navigation.nearestEmpty).toEqual(snappy("KeyE"));

    const cleared = run(changed.state, {
      kind: "setNavigationShortcut",
      shortcut: "nearestEmpty",
      chord: null,
    });
    expect(cleared.state.settings.navigation.nearestEmpty).toBeNull();
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
    expect(added?.switchShortcut).toMatchObject(snappy("Digit2"));
    expect([added?.sendShortcut, added?.sendAndSwitchShortcut]).toEqual([null, null]);
  });

  it("a new workspace switches with its digit unless another shortcut has it", () => {
    const fourth = run(initialState("macos"), { kind: "addWorkspace" });
    expect(fourth.state.settings.workspaces.items.at(-1)?.switchShortcut).toMatchObject(snappy("Digit4"));

    const taken = run(
      initialState("macos"),
      { kind: "setHistoryShortcut", step: "undo", chord: snappy("Digit4") },
      { kind: "addWorkspace" },
    );
    expect(taken.error).toBeNull();
    expect(taken.state.settings.workspaces.items.at(-1)?.switchShortcut).toBeNull();
  });

  it("every platform starts with the same Snappy-key workspace shortcuts and no undo or redo", () => {
    for (const platform of ["macos", "windows", "linux"] as const) {
      const { workspaces, history } = initialState(platform).settings;
      expect(workspaces.previous).toMatchObject(snappy("Semicolon"));
      expect(workspaces.next).toMatchObject(snappy("Quote"));
      expect(workspaces.sendPrevious).toMatchObject(snappy("Period"));
      expect(workspaces.sendNext).toMatchObject(snappy("Slash"));
      expect(workspaces).not.toHaveProperty("takeAlong");
      expect(workspaces.items.map((item) => item.switchShortcut?.code)).toEqual([
        "Digit1",
        "Digit2",
        "Digit3",
      ]);
      expect(history).toEqual({ undo: null, redo: null });
    }
  });

  it("every platform focuses with Snappy + I J K L, selects with Return, and clears with backslash", () => {
    for (const platform of ["macos", "windows", "linux"] as const) {
      const { navigation } = initialState(platform).settings;
      expect(navigation.focusUp).toMatchObject(snappy("KeyI"));
      expect(navigation.focusLeft).toMatchObject(snappy("KeyJ"));
      expect(navigation.focusDown).toMatchObject(snappy("KeyK"));
      expect(navigation.focusRight).toMatchObject(snappy("KeyL"));
      expect(navigation.selectWindow).toMatchObject(snappy("Enter"));
      expect(navigation.clearSelection).toMatchObject({ ...snappy("Backslash"), label: "\\" });
    }
  });

  it("only macOS refuses the Snappy key with Delete, which it never passes on", () => {
    const refused = run(initialState("macos"), {
      kind: "setNavigationShortcut",
      shortcut: "clearSelection",
      chord: snappy("Backspace"),
    });
    expect(refused.error).toBe("macOS reserves Control + Option + Command + Delete; it never reaches apps.");
    expect(refused.state.settings.navigation.clearSelection).toMatchObject(snappy("Backslash"));

    for (const platform of ["windows", "linux"] as const) {
      const accepted = run(initialState(platform), {
        kind: "setNavigationShortcut",
        shortcut: "clearSelection",
        chord: snappy("Backspace"),
      });
      expect(accepted.error).toBeNull();
      expect(accepted.state.settings.navigation.clearSelection).toEqual(snappy("Backspace"));
    }
  });

  it("focus and selection shortcuts are saved, cleared, and checked for conflicts", () => {
    const start = initialState("macos");
    const changed = run(start, {
      kind: "setNavigationShortcut",
      shortcut: "selectWindow",
      chord: snappy("KeyS"),
    });
    expect(changed.error).toBeNull();
    expect(changed.state.settings.navigation.selectWindow).toEqual(snappy("KeyS"));

    const clash = run(changed.state, {
      kind: "setNavigationShortcut",
      shortcut: "clearSelection",
      chord: snappy("KeyS"),
    });
    expect(clash.error).toBe("Snappy + S is already assigned to Select window.");
    expect(clash.state.settings.navigation.clearSelection).toMatchObject(snappy("Backslash"));

    const cleared = run(changed.state, {
      kind: "setNavigationShortcut",
      shortcut: "clearSelection",
      chord: null,
    });
    expect(cleared.state.settings.navigation.clearSelection).toBeNull();
  });

  it("send, and undo and redo shortcuts are saved and checked for conflicts", () => {
    const start = initialState("macos");
    const moved = run(start, { kind: "setWorkspaceStepShortcut", step: "sendNext", chord: snappy("KeyN") });
    expect(moved.state.settings.workspaces.sendNext).toEqual(snappy("KeyN"));
    const taken = run(moved.state, {
      kind: "setNavigationShortcut",
      shortcut: "selectWindow",
      chord: snappy("KeyN"),
    });
    expect(taken.error).toBe("Snappy + N is already assigned to Select window.");
    const undo = run(moved.state, { kind: "setHistoryShortcut", step: "undo", chord: snappy("KeyU") });
    expect(undo.state.settings.history.undo).toEqual(snappy("KeyU"));
    const byHand: KeyChord = { code: "KeyU", mods: ["ctrl", "alt", "meta"] };
    const clash = run(undo.state, { kind: "setHistoryShortcut", step: "redo", chord: byHand });
    expect(clash.error).toBe("Control + Option + Command + U is already assigned to Undo window move.");
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

  it("changing the Snappy key keeps every Snappy shortcut stored as Snappy and says so", () => {
    const before = initialState("windows");
    const { state, error } = run(before, { kind: "setSnappyKey", modifiers: ["ctrl", "alt"] });
    expect(error).toBeNull();
    expect(state.settings.snappyKey).toEqual(["ctrl", "alt"]);
    expect(state.settings.navigation).toEqual(before.settings.navigation);
    expect(state.status?.message).toBe("The Snappy key is now Ctrl + Alt.");
  });

  it("a Snappy key change that clashes is refused with the reason and changes nothing", () => {
    const withUndo = run(initialState("macos"), {
      kind: "setHistoryShortcut",
      step: "undo",
      chord: { code: "KeyJ", mods: ["ctrl", "alt"] },
    }).state;
    const clash = run(withUndo, { kind: "setSnappyKey", modifiers: ["ctrl", "alt"] });
    expect(clash.error).toBe("Snappy + J for Focus left would collide with Undo window move.");
    expect(clash.state.settings).toEqual(withUndo.settings);

    const single = run(withUndo, { kind: "setSnappyKey", modifiers: ["meta"] });
    expect(single.error).toBe("The Snappy key needs at least two modifiers.");
  });

  it("a shortcut recorded without the Snappy key keeps its keys when the key comes to match them", () => {
    const byHand: KeyChord = { code: "KeyU", mods: ["shift", "ctrl", "alt"] };
    const withUndo = run(initialState("macos"), { kind: "setHistoryShortcut", step: "undo", chord: byHand });
    const changed = run(withUndo.state, { kind: "setSnappyKey", modifiers: ["ctrl", "alt", "shift"] });
    expect(changed.error).toBeNull();
    expect(changed.state.settings.history.undo).toEqual(byHand);
    const back = run(changed.state, { kind: "setSnappyKey", modifiers: ["ctrl", "alt", "meta"] });
    expect(back.state.settings.history.undo).toEqual(byHand);
  });

  it("a Snappy key change onto a chord the system holds is refused naming the chord", () => {
    const taken = (chord: KeyChord) =>
      chord.code === "Space" && chord.mods.length === 2 && chord.mods.includes("ctrl");
    const outcome = applyCommand(
      initialState("macos"),
      { kind: "setSnappyKey", modifiers: ["ctrl", "alt"] },
      { isAvailable: (chord) => !taken(chord) },
    );
    expect(outcome.error).toBe(
      "Snappy + Space for Move to the nearest empty zone would be Control + Option + Space, which macOS or another app already uses.",
    );
    expect(outcome.state.settings.snappyKey).toEqual(["ctrl", "alt", "meta"]);
  });

  it("caps lock as the snappy key starts off, turns on and off, and a refusal keeps it off", () => {
    const before = initialState("linux");
    expect(before.settings.capsLockAsSnappyKey).toBe(false);
    const on = run(before, { kind: "setCapsLockAsSnappyKey", enabled: true });
    expect(on.error).toBeNull();
    expect(on.state.settings.capsLockAsSnappyKey).toBe(true);
    expect(on.state.status?.message).toBe(
      "Hold Caps Lock for the Snappy key. Tap it to turn Caps Lock on or off.",
    );
    const off = run(on.state, { kind: "setCapsLockAsSnappyKey", enabled: false });
    expect(off.state.settings.capsLockAsSnappyKey).toBe(false);

    const refusal = "Caps Lock is already remapped by the keyboard option caps:escape.";
    const refused = applyCommand(
      before,
      { kind: "setCapsLockAsSnappyKey", enabled: true },
      { ...freeSystem, capsLockRefusal: refusal },
    );
    expect(refused.error).toBe(refusal);
    expect(refused.state.settings.capsLockAsSnappyKey).toBe(false);
  });
});
