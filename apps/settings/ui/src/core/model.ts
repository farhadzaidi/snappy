// The settings model and the daemon protocol, mirroring the serde shapes of `snappy-core`
// and `snappy-ipc`.

export type Modifier = "shift" | "ctrl" | "alt" | "meta";
/** A modifier as a shortcut stores it: a concrete one, or the Snappy key. */
export type ChordModifier = "snappy" | Modifier;
export type DragModifier = "shift" | "alt" | "ctrl";
export type SwapModifier = "meta" | "ctrl" | "alt" | "shift" | "off";
/** What a keyboard move or drop into a zone another window fills does. */
export type WhenTaken = "swap" | "stack";
export type Direction = "left" | "right" | "up" | "down";
export type PlatformName = "macos" | "windows" | "linux";

export interface KeyChord {
  code: string;
  mods: ChordModifier[];
  label?: string;
}

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A rectangle in fractions (0 to 1) of a display's work area. */
export type UnitRect = Rect;

export interface Zone {
  id: string;
  rect: UnitRect;
}

export interface Layout {
  id: string;
  name: string;
  zones: Zone[];
}

export interface LayoutOptions {
  enabled: boolean;
  selectShortcut: KeyChord | null;
  zoneShortcuts: Record<string, KeyChord>;
}

/** Display ID to layout ID. */
export type DisplayAssignments = Record<string, string>;

export interface Workspace {
  id: string;
  name: string;
  displays: DisplayAssignments;
  switchShortcut: KeyChord | null;
  sendShortcut: KeyChord | null;
  sendAndSwitchShortcut: KeyChord | null;
}

export interface Settings {
  version: number;
  snapping: {
    enabled: boolean;
    dragModifier: DragModifier;
    swapModifier: SwapModifier;
    whenTaken: WhenTaken;
    shortcutsEnabled: boolean;
  };
  navigation: Navigation;
  layouts: {
    defaultLayoutId: string;
    displays: DisplayAssignments;
    custom: Layout[];
    options: Record<string, LayoutOptions>;
  };
  workspaces: {
    enabled: boolean;
    activeId: string;
    previous: KeyChord | null;
    next: KeyChord | null;
    movePrevious: KeyChord | null;
    moveNext: KeyChord | null;
    items: Workspace[];
  };
  history: Record<HistoryStep, KeyChord | null>;
  debugLog: boolean;
}

/** Move-between-zones shortcuts, and the one that moves to the nearest empty zone. */
export type Navigation = Record<Direction, KeyChord | null> & { nearestEmpty: KeyChord | null };

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
}

export type ToSettings =
  | { type: "hello"; protocolVersion: number }
  | ({ type: "state" } & AppState)
  | { type: "reply"; id: number; error: string | null }
  | { type: "probeResult"; available: boolean }
  | { type: "previewHidden" }
  | { type: "focus" };

export type Command =
  | { kind: "setSnapping"; enabled: boolean }
  | { kind: "setDragModifier"; modifier: DragModifier }
  | { kind: "setSwapModifier"; modifier: SwapModifier }
  | { kind: "setWhenTaken"; choice: WhenTaken }
  | { kind: "setShortcutsEnabled"; enabled: boolean }
  | { kind: "setNavigationShortcut"; direction: Direction; chord: KeyChord | null }
  | { kind: "setNearestEmptyShortcut"; chord: KeyChord | null }
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
  | { kind: "importSettings"; path: string }
  | { kind: "exportSettings"; path: string }
  | { kind: "dismissStatus" };

export type WorkspaceStep = "previous" | "next" | "movePrevious" | "moveNext";

export type HistoryStep = "undo" | "redo";

export const DIRECTIONS: readonly Direction[] = ["left", "right", "up", "down"];

/** Zones per layout, at most. */
export const MAX_ZONES = 32;
/** Workspaces, at most. */
export const MAX_WORKSPACES = 32;
