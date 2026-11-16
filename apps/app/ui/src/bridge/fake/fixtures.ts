import type { PlatformName } from "../../core/model";
import { core } from "../../core/snappyCore";
import type { AppState, Display } from "../protocol";

export const FAKE_DISPLAYS: Display[] = [
  {
    id: "display-1",
    name: "Built-in Display",
    frame: { x: 0, y: 0, width: 1512, height: 982 },
    workArea: { x: 0, y: 33, width: 1512, height: 949 },
    scale: 2,
  },
  {
    id: "display-2",
    name: "Studio Display",
    frame: { x: 1512, y: 0, width: 2560, height: 1440 },
    workArea: { x: 1512, y: 25, width: 2560, height: 1415 },
    scale: 2,
  },
];

/** The builds the fake daemon stands in for, as `?build=` names them in `pnpm dev` and the tests. */
export type FakeBuild = "github" | "app-store";

/** Why the Mac App Store build can't hold Caps Lock, as `crates/macos` says it. */
const CAPS_LOCK_SANDBOXED = "Apps from the App Store can't remap Caps Lock, so it can't be the Snappy key.";

/**
 * A daemon that just started: default settings, permission granted, snapping on. The App Store
 * build declares that Caps Lock can't be the Snappy key.
 */
export function initialState(platform: PlatformName, build: FakeBuild = "github"): AppState {
  const validation = core.validateSettings(core.defaultSettings(platform), platform);
  if (!("valid" in validation)) {
    throw new Error(validation.invalid);
  }
  return {
    settings: validation.valid,
    displays: FAKE_DISPLAYS,
    platform,
    snapping: {
      live: validation.valid.snapping.enabled,
      waitingForPermission: false,
      permissionGranted: true,
      needsPermission: platform === "macos",
      previewVisible: false,
      unavailableReason: null,
      unavailableRemedy: null,
    },
    platformNotes: [],
    workspaces: {
      running: false,
      pauseMessage: null,
      windowCounts: { "workspace-1": 4, "workspace-2": 2, "workspace-3": 0 },
    },
    loginItemEnabled: false,
    debugLogPath: "~/Library/Logs/Snappy/debug.jsonl",
    status: null,
    addedWorkspace: null,
    capsLockProblem: null,
    capsLockUnavailable: build === "app-store" ? CAPS_LOCK_SANDBOXED : null,
  };
}
