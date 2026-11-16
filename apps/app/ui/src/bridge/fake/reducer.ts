// What the fake daemon does with each command: apply it to a copy of the settings by the
// daemon's own `snappy-core` rules, validate, and keep the old state if validation fails.

import { findLayout } from "../../core/layouts";
import { modifierLabel, snappyKeyLabel } from "../../core/labels";
import type { KeyChord, Modifier, NamedShortcut, Reset, Settings } from "../../core/model";
import { core, type SettingsEdit } from "../../core/snappyCore";
import type { AppState, Command } from "../protocol";

export interface Outcome {
  state: AppState;
  error: string | null;
  /** The shortcut `error` names, for the app to point to. */
  shortcut?: NamedShortcut | null;
  /** The availability answer to a `probeShortcut`. */
  probe?: boolean;
  /** The settings file's text an `exportSettings` asked for. */
  exported?: string;
}

/** An outcome that may have changed nothing, which posts no message. */
type Edited = Outcome & { unchanged?: true };

export interface FakeSystem {
  /** Whether the system and other apps leave a chord free. */
  isAvailable(chord: KeyChord): boolean;
  /** Why Caps Lock can't be the Snappy key, as when the system already remaps it. */
  capsLockRefusal?: string | undefined;
}

/** A refusal is the reply's error and, as the daemon posts it, the status too. */
export function applyCommand(state: AppState, command: Command, system: FakeSystem): Outcome {
  const outcome = handle({ ...state, addedWorkspace: null }, command, system);
  const posted = outcome.error === null ? outcome.state : withError(outcome.state, outcome.error);
  return { ...outcome, state: withDerivedState(posted) };
}

function withError(state: AppState, message: string): AppState {
  const id = (state.status?.id ?? 0) + 1;
  return { ...state, status: { id, message, isError: true, notify: true } };
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
    case "setSnappyKey":
      return setSnappyKey(state, command.modifiers, system);
    case "setCapsLockAsSnappyKey":
      return setCapsLockAsSnappyKey(state, command.enabled, system);
    case "setNavigationShortcut":
      return save(state, {
        ...settings,
        navigation: { ...settings.navigation, [command.shortcut]: command.chord },
      });
    case "applyLayout":
      return applyLayout(state, command.display, command.layout);
    case "saveLayout":
      return saveLayout(state, command);
    case "deleteLayout":
      return withStatus(
        edit(state, { kind: "removeLayout", layout: command.layout }),
        "Custom layout deleted.",
      );
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
      return edit(state, { kind: "addWorkspace" });
    case "updateWorkspace":
      return edit(state, { kind: "updateWorkspace", workspace: command.workspace });
    case "moveWorkspaces":
      return edit(state, { kind: "reorderWorkspaces", from: command.from, to: command.to });
    case "deleteWorkspace":
      return edit(state, { kind: "removeWorkspace", workspace: command.workspace });
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
    case "reset":
      return reset(state, command.reset);
    case "importSettings":
      return importSettings(state, command.contents);
    case "exportSettings":
      return { ...done(state), exported: JSON.stringify(settings, null, 2) };
    case "dismissStatus":
      return done({ ...state, status: null });
    case "beginRecording":
    case "endRecording":
    case "openPermissionSettings":
    case "applyRemedy":
      return done(state);
  }
}

/**
 * Replace the settings with a settings file's text, read as the daemon reads it. Like the
 * daemon, import turns workspaces off and never changes whether snapping is on.
 */
function importSettings(state: AppState, contents: string): Outcome {
  const read = core.readSettings(contents, state.platform);
  if ("invalid" in read) {
    return { state, error: `Import failed: ${read.invalid}` };
  }
  const imported = read.valid;
  const outcome = save(state, {
    ...imported,
    snapping: { ...imported.snapping, enabled: state.settings.snapping.enabled },
    workspaces: { ...imported.workspaces, enabled: false },
  });
  if (outcome.error !== null) {
    return { ...outcome, error: `Import failed: ${outcome.error}` };
  }
  return withStatus(outcome, "Layouts and preferences imported.");
}

/** Resetting everything turns workspaces off, as the daemon does after bringing windows back. */
function reset(state: AppState, what: Reset): Outcome {
  const outcome = core.resetSettings(state.settings, what, state.platform);
  return withStatus(save(state, outcome.settings), outcome.message);
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

/** Apply a settings edit by the daemon's own rules and save it, as the daemon does. */
function edit(state: AppState, change: SettingsEdit): Edited {
  const outcome = core.applyEdit(state.settings, change, state.platform);
  if (outcome === "unchanged") {
    return { ...done(state), unchanged: true };
  }
  if ("refused" in outcome) {
    return { state, error: outcome.refused };
  }
  const saved = save(state, outcome.edited.settings);
  const added = outcome.edited.addedWorkspace;
  return saved.error === null && added !== null
    ? { ...saved, state: { ...saved.state, addedWorkspace: added } }
    : saved;
}

/**
 * Change the Snappy key as the daemon does: check every shortcut with the new modifiers, then
 * ask about each chord the change moves, refusing with the reason when anything clashes.
 */
function setSnappyKey(state: AppState, modifiers: Modifier[], system: FakeSystem): Outcome {
  const change = core.changeSnappyKey(state.settings, modifiers, state.platform);
  if (change === "unchanged") {
    return done(state);
  }
  if ("refused" in change) {
    return { state, error: change.refused };
  }
  const taken = core
    .snappyKeyProbes(change.changed, state.platform)
    .find((probe) => !system.isAvailable(probe.chord));
  if (taken) {
    return { state, error: taken.refusal.message, shortcut: taken.refusal.shortcut };
  }
  const label = snappyKeyLabel(change.changed.snappyKey, state.platform);
  return withStatus(done({ ...state, settings: change.changed }), `The Snappy key is now ${label}.`);
}

/** Hold Caps Lock as the Snappy key, refused with the reason when the system remaps it already. */
function setCapsLockAsSnappyKey(state: AppState, enabled: boolean, system: FakeSystem): Outcome {
  if (enabled === state.settings.capsLockAsSnappyKey) {
    return done(state);
  }
  if (enabled && state.capsLockUnavailable !== null) {
    return { state, error: state.capsLockUnavailable };
  }
  if (enabled && system.capsLockRefusal !== undefined) {
    return { state, error: system.capsLockRefusal };
  }
  const message = enabled
    ? "Hold Caps Lock for the Snappy key. Tap it to turn Caps Lock on or off."
    : "Caps Lock works as usual again.";
  return withStatus(save(state, { ...state.settings, capsLockAsSnappyKey: enabled }), message);
}

/** Post `message` once `outcome` changed something; refusals and no-ops say nothing. */
function withStatus(outcome: Edited, message: string, notify = true): Outcome {
  if (outcome.error !== null || outcome.unchanged) {
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

function displayName(state: AppState, displayId: string): string {
  return state.displays.find((display) => display.id === displayId)?.name ?? "this display";
}

/** Unknown and turned-off layouts change nothing, and say nothing. */
function applyLayout(state: AppState, displayId: string, layoutId: string): Outcome {
  const name = findLayout(state.settings, layoutId)?.name ?? "";
  const applied = edit(state, { kind: "assignLayout", display: displayId, layout: layoutId });
  return withStatus(applied, `${name} selected for ${displayName(state, displayId)}.`, false);
}

/** Presets keep their names whatever the editor sends, so the message names the saved one. */
function saveLayout(state: AppState, command: Extract<Command, { kind: "saveLayout" }>): Outcome {
  const { layout, options, display } = command;
  const saved = edit(state, { kind: "saveLayout", layout, options, display });
  const name = findLayout(saved.state.settings, layout.id)?.name ?? layout.name;
  return withStatus(saved, `${name} saved.`);
}

function setLayoutEnabled(state: AppState, layoutId: string, enabled: boolean): Outcome {
  const name = findLayout(state.settings, layoutId)?.name ?? "";
  const changed = edit(state, { kind: "setLayoutEnabled", layout: layoutId, enabled });
  return withStatus(changed, `${name} ${enabled ? "enabled" : "disabled"}.`);
}

/** The first time, every workspace starts from each connected display's current layout. */
function setWorkspacesEnabled(state: AppState, enabled: boolean): Outcome {
  if (enabled) {
    return edit(state, { kind: "enableWorkspaces", displays: state.displays.map((display) => display.id) });
  }
  return save(state, { ...state.settings, workspaces: { ...state.settings.workspaces, enabled } });
}
