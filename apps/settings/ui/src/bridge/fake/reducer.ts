// What the fake daemon does with each command: apply it to a copy of the settings, validate
// with the real `snappy-core` rules, and keep the old state if validation fails.

import { findLayout, isPreset, optionsFor } from "../../core/layouts";
import { modifierLabel } from "../../core/labels";
import type { AppState, Command, KeyChord, Layout, Settings, Workspace } from "../../core/model";
import { MAX_WORKSPACES } from "../../core/model";
import { core } from "../../core/snappyCore";

export interface Outcome {
  state: AppState;
  error: string | null;
  /** The availability answer to a `probeShortcut`. */
  probe?: boolean;
}

export interface FakeSystem {
  /** Whether the system and other apps leave a chord free. */
  isAvailable(chord: KeyChord): boolean;
}

export function applyCommand(state: AppState, command: Command, system: FakeSystem): Outcome {
  const outcome = handle({ ...state, addedWorkspace: null }, command, system);
  return { ...outcome, state: withDerivedState(outcome.state) };
}

function handle(state: AppState, command: Command, system: FakeSystem): Outcome {
  const settings = state.settings;
  switch (command.kind) {
    case "setSnapping":
      return setSnapping(state, command.enabled);
    case "setDragModifier":
      return save(state, { ...settings, snapping: { ...settings.snapping, dragModifier: command.modifier } });
    case "setSwapModifier":
      return save(state, { ...settings, snapping: { ...settings.snapping, swapModifier: command.modifier } });
    case "setWhenTaken":
      return save(state, { ...settings, snapping: { ...settings.snapping, whenTaken: command.choice } });
    case "setShortcutsEnabled":
      return save(state, {
        ...settings,
        snapping: { ...settings.snapping, shortcutsEnabled: command.enabled },
      });
    case "setNavigationShortcut":
      return save(state, {
        ...settings,
        navigation: { ...settings.navigation, [command.direction]: command.chord },
      });
    case "applyLayout":
      return applyLayout(state, command.display, command.layout);
    case "saveLayout":
      return saveLayout(state, command.layout, command);
    case "deleteLayout":
      return deleteLayout(state, command.layout);
    case "setLayoutEnabled":
      return setLayoutEnabled(state, command.layout, command.enabled);
    case "showPreview":
      return done({ ...state, snapping: { ...state.snapping, previewVisible: true } });
    case "hidePreview":
      return done({ ...state, snapping: { ...state.snapping, previewVisible: false } });
    case "probeShortcut":
      return { ...done(state), probe: system.isAvailable(command.chord) };
    case "setWorkspacesEnabled":
      return setWorkspacesEnabled(state, command.enabled);
    case "addWorkspace":
      return addWorkspace(state);
    case "updateWorkspace":
      return saveWorkspaces(
        state,
        settings.workspaces.items.map((item) =>
          item.id === command.workspace.id ? command.workspace : item,
        ),
      );
    case "moveWorkspaces":
      return saveWorkspaces(state, moveItems(settings.workspaces.items, command.from, command.to));
    case "deleteWorkspace":
      return deleteWorkspace(state, command.workspace);
    case "setWorkspaceStepShortcut":
      return save(state, {
        ...settings,
        workspaces: { ...settings.workspaces, [command.step]: command.chord },
      });
    case "setHistoryShortcut":
      return save(state, { ...settings, history: { ...settings.history, [command.step]: command.chord } });
    case "setLoginItem":
      return done({ ...state, loginItemEnabled: command.enabled });
    case "setDebugLog":
      return save(state, { ...settings, debugLog: command.enabled });
    case "importSettings":
      return withStatus(
        save(state, { ...settings, workspaces: { ...settings.workspaces, enabled: false } }),
        "Layouts and preferences imported.",
      );
    case "exportSettings":
      return withStatus(done(state), "Layouts and preferences exported.");
    case "dismissStatus":
      return done({ ...state, status: null });
    case "beginRecording":
    case "endRecording":
    case "openPermissionSettings":
      return done(state);
  }
}

function done(state: AppState): Outcome {
  return { state, error: null };
}

/** Validate and keep `settings`, or keep the old state and report why. */
function save(state: AppState, settings: Settings): Outcome {
  const validation = core.validateSettings(settings, state.platform);
  if ("invalid" in validation) {
    return { state, error: validation.invalid };
  }
  return done({ ...state, settings: validation.valid });
}

function withStatus(outcome: Outcome, message: string, notify = true): Outcome {
  if (outcome.error !== null) {
    return outcome;
  }
  const id = (outcome.state.status?.id ?? 0) + 1;
  return { ...outcome, state: { ...outcome.state, status: { id, message, isError: false, notify } } };
}

/** Live control and workspaces run only with permission. */
function withDerivedState(state: AppState): AppState {
  const granted = state.snapping.permissionGranted;
  return {
    ...state,
    snapping: { ...state.snapping, live: state.settings.snapping.enabled && granted },
    workspaces: { ...state.workspaces, running: state.settings.workspaces.enabled && granted },
  };
}

function setSnapping(state: AppState, enabled: boolean): Outcome {
  const settings = { ...state.settings, snapping: { ...state.settings.snapping, enabled } };
  const granted = state.snapping.permissionGranted;
  const saved = save(
    { ...state, snapping: { ...state.snapping, waitingForPermission: enabled && !granted } },
    settings,
  );
  const dragModifier = modifierLabel(settings.snapping.dragModifier, state.platform);
  if (!enabled) {
    return withStatus(saved, "Snapping is paused.");
  }
  return granted
    ? withStatus(saved, `Snapping is on. Hold ${dragModifier} while dragging a window.`)
    : withStatus(saved, "Allow Snappy in Accessibility. Snapping will start automatically.");
}

/**
 * Assign a layout to a display and make it the default. While workspaces run, it belongs to
 * that display in the active workspace only.
 */
function assignLayout(settings: Settings, displayId: string, layoutId: string): Settings {
  const workspaces = settings.workspaces;
  if (workspaces.enabled) {
    const items = workspaces.items.map((item) =>
      item.id === workspaces.activeId
        ? { ...item, displays: { ...item.displays, [displayId]: layoutId } }
        : item,
    );
    return { ...settings, workspaces: { ...workspaces, items } };
  }
  return {
    ...settings,
    layouts: {
      ...settings.layouts,
      displays: { ...settings.layouts.displays, [displayId]: layoutId },
      defaultLayoutId: layoutId,
    },
  };
}

function displayName(state: AppState, displayId: string): string {
  return state.displays.find((display) => display.id === displayId)?.name ?? "this display";
}

function applyLayout(state: AppState, displayId: string, layoutId: string): Outcome {
  const layout = findLayout(state.settings, layoutId);
  if (!layout) {
    return done(state);
  }
  const saved = save(state, assignLayout(state.settings, displayId, layoutId));
  return withStatus(saved, `${layout.name} selected for ${displayName(state, displayId)}.`, false);
}

function saveLayout(
  state: AppState,
  layout: Layout,
  command: Extract<Command, { kind: "saveLayout" }>,
): Outcome {
  const settings = state.settings;
  const custom = isPreset(layout.id)
    ? settings.layouts.custom
    : settings.layouts.custom.some((item) => item.id === layout.id)
      ? settings.layouts.custom.map((item) => (item.id === layout.id ? layout : item))
      : [...settings.layouts.custom, layout];
  const updated: Settings = {
    ...settings,
    layouts: {
      ...settings.layouts,
      custom,
      options: { ...settings.layouts.options, [layout.id]: command.options },
    },
  };
  const name = findLayout(updated, layout.id)?.name ?? layout.name;
  return withStatus(save(state, assignLayout(updated, command.display, layout.id)), `${name} saved.`);
}

function deleteLayout(state: AppState, layoutId: string): Outcome {
  const settings = state.settings;
  const keep = (assignments: Record<string, string>) =>
    Object.fromEntries(Object.entries(assignments).filter(([, assigned]) => assigned !== layoutId));
  const { [layoutId]: _removed, ...options } = settings.layouts.options;
  const updated: Settings = {
    ...settings,
    layouts: {
      defaultLayoutId:
        settings.layouts.defaultLayoutId === layoutId ? "focus" : settings.layouts.defaultLayoutId,
      displays: keep(settings.layouts.displays),
      custom: settings.layouts.custom.filter((layout) => layout.id !== layoutId),
      options,
    },
    workspaces: {
      ...settings.workspaces,
      items: settings.workspaces.items.map((item) => ({ ...item, displays: keep(item.displays) })),
    },
  };
  return withStatus(save(state, updated), "Custom layout deleted.");
}

function setLayoutEnabled(state: AppState, layoutId: string, enabled: boolean): Outcome {
  const layout = findLayout(state.settings, layoutId);
  if (!layout) {
    return done(state);
  }
  const options = { ...optionsFor(state.settings, layout), enabled };
  const updated: Settings = {
    ...state.settings,
    layouts: {
      ...state.settings.layouts,
      options: { ...state.settings.layouts.options, [layoutId]: options },
    },
  };
  return withStatus(save(state, updated), `${layout.name} ${enabled ? "enabled" : "disabled"}.`);
}

function saveWorkspaces(state: AppState, items: Workspace[]): Outcome {
  return save(state, { ...state.settings, workspaces: { ...state.settings.workspaces, items } });
}

/** The first time, every workspace starts from each display's current layout. */
function setWorkspacesEnabled(state: AppState, enabled: boolean): Outcome {
  const workspaces = state.settings.workspaces;
  const unassigned = workspaces.items.every((item) => Object.keys(item.displays).length === 0);
  const assignments = Object.fromEntries(
    state.displays.map((display) => [
      display.id,
      state.settings.layouts.displays[display.id] ?? state.settings.layouts.defaultLayoutId,
    ]),
  );
  const items =
    enabled && unassigned
      ? workspaces.items.map((item) => ({ ...item, displays: { ...assignments } }))
      : workspaces.items;
  return save(state, { ...state.settings, workspaces: { ...workspaces, enabled, items } });
}

/** A new workspace: the lowest unused number, no shortcuts, the active layouts. */
function addWorkspace(state: AppState): Outcome {
  const workspaces = state.settings.workspaces;
  if (workspaces.items.length >= MAX_WORKSPACES) {
    return done(state);
  }
  const number = lowestUnused((n) => workspaces.items.some((item) => item.name === `Workspace ${n}`));
  const idNumber = lowestUnused((n) => workspaces.items.some((item) => item.id === `workspace-${n}`));
  const active = workspaces.items.find((item) => item.id === workspaces.activeId);
  const added: Workspace = {
    ...core.numberedWorkspace(`workspace-${idNumber}`, number),
    displays: { ...active?.displays },
  };

  const saved = save(state, {
    ...state.settings,
    workspaces: { ...workspaces, items: [...workspaces.items, added] },
  });
  return saved.error === null ? { ...saved, state: { ...saved.state, addedWorkspace: added.id } } : saved;
}

function lowestUnused(taken: (n: number) => boolean): number {
  let n = 1;
  while (taken(n)) {
    n += 1;
  }
  return n;
}

/** Move the items at `from` so they land before the item that was at `to`. */
export function moveItems<T>(items: T[], from: number[], to: number): T[] {
  const moving = items.filter((_, index) => from.includes(index));
  const staying = items.filter((_, index) => !from.includes(index));
  const insertAt = to - from.filter((index) => index < to).length;
  return [...staying.slice(0, insertAt), ...moving, ...staying.slice(insertAt)];
}

/** At least one workspace stays; deleting the active one activates the first other. */
function deleteWorkspace(state: AppState, workspaceId: string): Outcome {
  const workspaces = state.settings.workspaces;
  if (workspaces.items.length < 2) {
    return done(state);
  }
  const items = workspaces.items.filter((item) => item.id !== workspaceId);
  const activeId =
    workspaces.activeId === workspaceId ? (items[0]?.id ?? workspaces.activeId) : workspaces.activeId;
  return save(state, { ...state.settings, workspaces: { ...workspaces, activeId, items } });
}
