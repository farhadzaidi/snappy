// The settings model, mirroring the serde shapes of `snappy-core`. The daemon protocol is in
// `bridge/protocol.ts`.

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
  /** The modifiers the Snappy key stands for, at least two. */
  snappyKey: Modifier[];
  /** Caps Lock, held, stands for the Snappy key; tapped alone, it still turns Caps Lock on or off. */
  capsLockAsSnappyKey: boolean;
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
    sendPrevious: KeyChord | null;
    sendNext: KeyChord | null;
    items: Workspace[];
  };
  history: Record<HistoryStep, KeyChord | null>;
  debugLog: boolean;
}

/**
 * Move-between-zones shortcuts, the one that moves to the nearest empty zone, and the ones
 * that move focus between windows and change the selection.
 */
export type Navigation = Record<NavigationShortcut, KeyChord | null>;

/** A shortcut of the navigation section, named by the field that holds it. */
export type NavigationShortcut = Direction | "nearestEmpty" | FocusShortcut;

export type FocusShortcut =
  "focusUp" | "focusLeft" | "focusDown" | "focusRight" | "selectWindow" | "clearSelection";

/**
 * Why a change was refused: the message, and the shortcut it names, when it names one of
 * Snappy's, which the app points to.
 */
export interface Refusal {
  message: string;
  shortcut: NamedShortcut | null;
}

/** A shortcut a message names: which one it is, to find it by, and its title as the message writes it. */
export interface NamedShortcut {
  id: ShortcutId;
  title: string;
}

export type WorkspaceStep = "previous" | "next" | "sendPrevious" | "sendNext";

/** The Settings page's sections of global shortcuts, each reset on its own. */
export type ShortcutSection = "moveBetweenZones" | "focusAndSelection" | "workspaceSteps" | "windowHistory";

/** What to put back to its defaults: see `Reset` in `crates/core/src/reset.rs`. */
export type Reset =
  | { scope: "shortcuts"; section: ShortcutSection }
  | { scope: "workspaceShortcuts"; workspace: string }
  | { scope: "settings" }
  | { scope: "everything" };

/** A workspace's own shortcut, named by the field that holds it. */
export type WorkspaceShortcut = "switchShortcut" | "sendShortcut" | "sendAndSwitchShortcut";

export type HistoryStep = "undo" | "redo";

/**
 * One shortcut by what it is, never by its title, which two workspaces or layouts may share:
 * see `ShortcutId` in `crates/core/src/shortcuts.rs`.
 */
export type ShortcutId =
  | { kind: "navigation"; shortcut: NavigationShortcut }
  | { kind: "history"; step: HistoryStep }
  | { kind: "workspaceStep"; step: WorkspaceStep }
  | { kind: "workspace"; workspace: string; shortcut: WorkspaceShortcut }
  | { kind: "layoutSelect"; layout: string }
  | { kind: "zone"; layout: string; zone: string };

export const DIRECTIONS: readonly Direction[] = ["left", "right", "up", "down"];
export const PLATFORMS: readonly PlatformName[] = ["macos", "windows", "linux"];
export const MODIFIERS: readonly Modifier[] = ["ctrl", "alt", "shift", "meta"];
