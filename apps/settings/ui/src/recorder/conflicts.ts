// The local conflict check for each kind of recorder: put the candidate in place and run
// the same rules every settings save runs.

import type {
  Direction,
  HistoryStep,
  KeyChord,
  PlatformName,
  Settings,
  Workspace,
  WorkspaceStep,
} from "../core/model";
import { core } from "../core/snappyCore";

export type WorkspaceSlot = "switchShortcut" | "sendShortcut" | "sendAndSwitchShortcut";

export function navigationConflict(
  settings: Settings,
  direction: Direction,
  chord: KeyChord,
  platform: PlatformName,
): string | null {
  return core.settingsConflict(
    { ...settings, navigation: { ...settings.navigation, [direction]: chord } },
    platform,
  );
}

export function nearestEmptyConflict(
  settings: Settings,
  chord: KeyChord,
  platform: PlatformName,
): string | null {
  return core.settingsConflict(
    { ...settings, navigation: { ...settings.navigation, nearestEmpty: chord } },
    platform,
  );
}

export function workspaceConflict(
  settings: Settings,
  workspace: Workspace,
  slot: WorkspaceSlot,
  chord: KeyChord,
  platform: PlatformName,
): string | null {
  const items = settings.workspaces.items.map((item) =>
    item.id === workspace.id ? { ...item, [slot]: chord } : item,
  );
  return core.settingsConflict({ ...settings, workspaces: { ...settings.workspaces, items } }, platform);
}

export function workspaceStepConflict(
  settings: Settings,
  step: WorkspaceStep,
  chord: KeyChord,
  platform: PlatformName,
): string | null {
  return core.settingsConflict(
    { ...settings, workspaces: { ...settings.workspaces, [step]: chord } },
    platform,
  );
}

export function historyConflict(
  settings: Settings,
  step: HistoryStep,
  chord: KeyChord,
  platform: PlatformName,
): string | null {
  return core.settingsConflict({ ...settings, history: { ...settings.history, [step]: chord } }, platform);
}
