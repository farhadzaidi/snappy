// The daemon protocol, mirroring the serde shapes of `snappy-ipc` and the `snappy-platform`
// types it carries.

import type {
  DragModifier,
  HistoryStep,
  KeyChord,
  Layout,
  LayoutOptions,
  Modifier,
  NavigationShortcut,
  PlatformName,
  Rect,
  Refusal,
  Reset,
  Settings,
  SwapModifier,
  WhenTaken,
  Workspace,
  WorkspaceStep,
} from "../core/model";

export interface Display {
  id: string;
  name: string;
  frame: Rect;
  workArea: Rect;
  scale: number;
}

export interface SnappingState {
  live: boolean;
  waitingForPermission: boolean;
  permissionGranted: boolean;
  needsPermission: boolean;
  previewVisible: boolean;
  unavailableReason: string | null;
  /** What Snappy can do about it when asked. */
  unavailableRemedy: Remedy | null;
}

/** Something the platform can do to make live control available. */
export type Remedy = "logOut";

export interface WorkspacesState {
  running: boolean;
  pauseMessage: string | null;
  windowCounts: Record<string, number>;
}

/** A message for the toast. `notify` false means a quiet update. */
export interface Status {
  id: number;
  message: string;
  isError: boolean;
  notify: boolean;
}

export interface AppState {
  settings: Settings;
  displays: Display[];
  platform: PlatformName;
  snapping: SnappingState;
  workspaces: WorkspacesState;
  /** Notes about the desktop, such as another tool that also snaps windows. */
  platformNotes: string[];
  loginItemEnabled: boolean;
  debugLogPath: string;
  status: Status | null;
  addedWorkspace: string | null;
  /** Why Caps Lock isn't the Snappy key though the setting asks for it. */
  capsLockProblem: string | null;
}

export type ToApp =
  | { type: "hello"; protocolVersion: number }
  | ({ type: "state" } & AppState)
  | { type: "reply"; id: number; error: Refusal | null }
  | { type: "probeResult"; available: boolean }
  | { type: "focus" };

export type Command =
  | { kind: "setSnapping"; enabled: boolean }
  | { kind: "setDragModifier"; modifier: DragModifier }
  | { kind: "setSwapModifier"; modifier: SwapModifier }
  | { kind: "setWhenTaken"; choice: WhenTaken }
  | { kind: "setShortcutsEnabled"; enabled: boolean }
  | { kind: "setSnappyKey"; modifiers: Modifier[] }
  | { kind: "setCapsLockAsSnappyKey"; enabled: boolean }
  | { kind: "setNavigationShortcut"; shortcut: NavigationShortcut; chord: KeyChord | null }
  | { kind: "applyLayout"; display: string; layout: string }
  | { kind: "saveLayout"; layout: Layout; options: LayoutOptions; display: string }
  | { kind: "deleteLayout"; layout: string }
  | { kind: "setLayoutEnabled"; layout: string; enabled: boolean }
  | { kind: "showPreview"; display: string; layout: string }
  | { kind: "hidePreview" }
  | { kind: "beginRecording" }
  | { kind: "endRecording" }
  | { kind: "probeShortcut"; chord: KeyChord }
  | { kind: "setWorkspacesEnabled"; enabled: boolean }
  | { kind: "addWorkspace" }
  | { kind: "updateWorkspace"; workspace: Workspace }
  | { kind: "moveWorkspaces"; from: number[]; to: number }
  | { kind: "deleteWorkspace"; workspace: string }
  | { kind: "setWorkspaceStepShortcut"; step: WorkspaceStep; chord: KeyChord | null }
  | { kind: "setHistoryShortcut"; step: HistoryStep; chord: KeyChord | null }
  | { kind: "setLoginItem"; enabled: boolean }
  | { kind: "setDebugLog"; enabled: boolean }
  | { kind: "openPermissionSettings" }
  | { kind: "applyRemedy"; remedy: Remedy }
  | { kind: "reset"; reset: Reset }
  | { kind: "importSettings"; path: string }
  | { kind: "exportSettings"; path: string }
  | { kind: "dismissStatus" };
