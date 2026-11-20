// Typed access to `snappy-core` through `snappy-wasm`. Every geometry and shortcut rule the
// app applies comes from here; the app never computes zone geometry itself.

import initWasm, * as wasm from "../wasm/snappy_wasm";
import type {
  KeyChord,
  Layout,
  LayoutOptions,
  Modifier,
  NamedShortcut,
  PlatformName,
  Point,
  Refusal,
  Reset,
  Rect,
  Settings,
  ShortcutId,
  ShortcutSection,
  Size,
  UnitRect,
  Workspace,
  WorkspaceShortcut,
  Zone,
} from "./model";

export type Axis = "x" | "y";
export type Handle =
  "northWest" | "north" | "northEast" | "east" | "southEast" | "south" | "southWest" | "west";
export type CoordinateField = "x" | "y" | "width" | "height";

export interface Divider {
  id: string;
  zoneId: string;
  axis: Axis;
  line: number;
  start: number;
  end: number;
}

export interface Guides {
  x: number[];
  y: number[];
}

export interface Edit {
  zones: Zone[];
  guides: Guides;
}

export interface Drawn {
  rect: UnitRect | null;
  guides: Guides;
}

/** What a cut goes through: one zone, or every zone the line crosses. */
export type CutScope = { zone: string } | "through";

/** Where a cut under the pointer would go, before it is made. */
export interface CutPreview {
  /** The cut's line along its axis, as a canvas fraction, snapped to guides. */
  at: number;
  /** The zones the cut splits, in zone order. */
  zoneIds: string[];
  guides: Guides;
}

/** Where copies go, or why they can't be placed. */
export type PlaceOutcome = { placed: UnitRect[] } | { refused: string };

export type MergeOutcome =
  { merged: { zones: Zone[]; zoneShortcuts: Record<string, KeyChord>; kept: string } } | { refused: string };

export interface HandlePosition {
  handle: Handle;
  center: Point;
}

/**
 * A chord's long label split where the Snappy key ends. The Snappy key has no character of
 * its own, so it is drawn first, with its glyph, then a plus and `keys`.
 */
export interface LabelParts {
  snappyKey: boolean;
  /**
   * What follows the Snappy key, or the whole label without it: "Shift ⇧ + 1" on macOS,
   * "Shift + 1" elsewhere.
   */
  keys: string;
}

export interface ChordLabels {
  /** "Snappy + Shift ⇧ + 1", as messages and assistive technology say it. */
  long: string;
  /** The same label in parts, as recorders, cards, and references draw it. */
  parts: LabelParts;
}

export interface ModifierNames {
  name: string;
  /** The name with its symbol, as messages show it: "Shift ⇧". */
  label: string;
}

export type Validation = { valid: Settings } | { invalid: string };

/** A change the app asks the daemon for, by the `snappy-core` rule that makes it. */
export type SettingsEdit =
  | { kind: "assignLayout"; display: string; layout: string }
  | { kind: "saveLayout"; layout: Layout; options: LayoutOptions; display: string }
  | { kind: "removeLayout"; layout: string }
  | { kind: "setLayoutEnabled"; layout: string; enabled: boolean }
  /** Turn workspaces on, with the IDs of the connected displays. */
  | { kind: "enableWorkspaces"; displays: string[] }
  | { kind: "addWorkspace" }
  | { kind: "updateWorkspace"; workspace: Workspace }
  | { kind: "reorderWorkspaces"; from: number[]; to: number }
  | { kind: "removeWorkspace"; workspace: string };

/** A settings edit: nothing to change, the edited settings with any workspace it added, or why not. */
export type EditOutcome =
  "unchanged" | { edited: { settings: Settings; addedWorkspace: string | null } } | { refused: string };

/** A new Snappy key: nothing to change, the settings with it, or why it is refused. */
export type SnappyKeyChange = "unchanged" | { changed: Settings } | { refused: string };

/** Settings put back to their defaults, whether that changes anything, and what to say. */
export interface ResetOutcome {
  settings: Settings;
  changed: boolean;
  message: string;
}

/** A shortcut that belongs to no workspace or layout, which the Settings page lists. */
export type GlobalShortcutId = Extract<ShortcutId, { kind: "navigation" | "workspaceStep" | "history" }>;

/**
 * A shortcut as a section lists it: what it is, its title, what's said under the title if that
 * doesn't say it all, and its chord.
 */
export interface ListedShortcut {
  id: GlobalShortcutId;
  title: string;
  hint?: string;
  chord: KeyChord | null;
}

/** One of the Settings page's sections of global shortcuts. */
export interface SectionOfShortcuts {
  section: ShortcutSection;
  title: string;
  shortcuts: ListedShortcut[];
}

/** The titles of a layout's selection shortcut and of each zone's, in the layout's order. */
export interface LayoutShortcutTitles {
  select: string;
  zones: string[];
}

/** What a family of default shortcuts does, and its keys as one chord labelled with all of them. */
export interface GuideEntry {
  title: string;
  chord: KeyChord;
}

/** A chord to ask the OS about before the Snappy key changes, and what to say if it is taken. */
export interface AvailabilityProbe {
  chord: KeyChord;
  refusal: Refusal;
}

/** The most of each thing the app counts that `snappy-core` allows. */
export interface Limits {
  /** Zones per layout. */
  zones: number;
  workspaces: number;
  /** Window moves that undo remembers. */
  undoSteps: number;
  /** Characters in a layout's or a workspace's name. */
  nameLength: number;
}

/** Load the WebAssembly module. Call once before rendering. */
export async function loadCore(): Promise<void> {
  await initWasm();
}

function call<Response>(exported: (request: string) => string, request: object): Response {
  return JSON.parse(exported(JSON.stringify(request))) as Response;
}

export const core = {
  zoneFrames: (request: { zones: Zone[]; workArea: Rect; spacingFactor: number }) =>
    call<Rect[]>(wasm.zoneFrames, request),

  dividers: (zones: Zone[]) => call<Divider[]>(wasm.dividers, { zones }),
  isSeparated: (zones: Zone[]) => call<boolean>(wasm.isSeparated, { zones }),
  alignEdges: (zones: Zone[]) => call<Zone[] | null>(wasm.alignEdges, { zones }),
  /** Where same-size copies go for Paste and Duplicate, or why they can't go anywhere. */
  placeCopies: (zones: Zone[], copied: UnitRect[]) => call<PlaceOutcome>(wasm.placeCopies, { zones, copied }),
  /** The zones `ids` merged into one, with the zone shortcuts that follow, or why not. */
  mergeZones: (zones: Zone[], ids: string[], zoneShortcuts: Record<string, KeyChord>) =>
    call<MergeOutcome>(wasm.mergeZones, { zones, ids, zoneShortcuts }),

  handleAt: (point: Point, size: Size) => call<Handle | null>(wasm.handleAt, { point, size }),
  handlePositions: (size: Size) => call<HandlePosition[]>(wasm.handlePositions, { size }),

  dragMove: (request: { zones: Zone[]; id: string; delta: Point; tolerance: Size }) =>
    call<Edit>(wasm.dragMove, request),
  dragResize: (request: { zones: Zone[]; id: string; handle: Handle; delta: Point; tolerance: Size }) =>
    call<Edit>(wasm.dragResize, request),
  dragDivider: (request: {
    zones: Zone[];
    zoneId: string;
    axis: Axis;
    line: number;
    delta: number;
    tolerance: number;
  }) => call<Edit>(wasm.dragDivider, request),
  drawZone: (request: { zones: Zone[]; from: Point; to: Point; tolerance: Size }) =>
    call<Drawn>(wasm.drawZone, request),
  cutPreview: (request: { zones: Zone[]; scope: CutScope; axis: Axis; at: number; tolerance: number }) =>
    call<CutPreview>(wasm.cutPreview, request),
  /** Cut at the line `at`, as previewed; `newIds` needs one ID per zone the cut may split. */
  cutZones: (request: { zones: Zone[]; scope: CutScope; axis: Axis; at: number; newIds: string[] }) =>
    call<Zone[] | null>(wasm.cutZones, request),
  setField: (request: { zones: Zone[]; id: string; field: CoordinateField; value: number }) =>
    call<Zone[]>(wasm.setField, request),

  chordLabels: (chord: KeyChord, platform: PlatformName) =>
    call<ChordLabels>(wasm.chordLabels, { chord, platform }),
  /** Why the chord can't be a shortcut, as a recorder says it, or null when it can. */
  chordProblem: (chord: KeyChord, platform: PlatformName) =>
    call<string | null>(wasm.chordProblem, { chord, platform }),
  /**
   * A chord pressed with its modifiers held, as recording stores it: holding every modifier of
   * the Snappy key makes it the Snappy key plus the rest, and anything else stays as held.
   */
  recordChord: (chord: KeyChord, snappyKey: Modifier[]) =>
    call<KeyChord>(wasm.recordChord, { chord, snappyKey }),
  /** The chord with the Snappy key expanded to the modifiers it stands for. */
  expandChord: (chord: KeyChord, snappyKey: Modifier[]) =>
    call<KeyChord>(wasm.expandChord, { chord, snappyKey }),
  /** What the Snappy key is on the platform: "Control + Option + Command". */
  snappyKeyLabel: (snappyKey: Modifier[], platform: PlatformName) =>
    call<string>(wasm.snappyKeyLabel, { snappyKey, platform }),
  /** The settings with a new Snappy key, or why it is refused; "unchanged" for the key they have. */
  changeSnappyKey: (settings: Settings, modifiers: Modifier[], platform: PlatformName) =>
    call<SnappyKeyChange>(wasm.changeSnappyKey, { settings, modifiers, platform }),
  /**
   * A recorder's refusal of `chord`, which the OS or another app holds: the message the
   * Snappy key's probes give, naming `shortcut`, the one being recorded.
   */
  unavailableShortcut: (chord: KeyChord, shortcut: NamedShortcut, platform: PlatformName) =>
    call<Refusal>(wasm.unavailableShortcut, { chord, shortcut, platform }),
  /** What to ask the OS about for settings whose Snappy key just changed. */
  snappyKeyProbes: (settings: Settings, platform: PlatformName) =>
    call<AvailabilityProbe[]>(wasm.snappyKeyProbes, { settings, platform }),
  modifierNames: (modifier: Modifier, platform: PlatformName) =>
    call<ModifierNames>(wasm.modifierNames, { modifier, platform }),
  /** The Settings page's sections of global shortcuts, in order, with their shortcuts' chords. */
  shortcutSections: (settings: Settings) => call<SectionOfShortcuts[]>(wasm.shortcutSections, { settings }),
  /** The titles of a workspace's own shortcuts, naming it `workspace`. */
  workspaceShortcutTitles: (workspace: string) =>
    call<Record<WorkspaceShortcut, string>>(wasm.workspaceShortcutTitles, { workspace }),
  /** The titles of a layout's own shortcuts, naming it `layout`, with one per zone. */
  layoutShortcutTitles: (layout: string, zones: number) =>
    call<LayoutShortcutTitles>(wasm.layoutShortcutTitles, { layout, zones }),
  /** The default shortcuts as help lists them. */
  defaultShortcuts: (platform: PlatformName) => call<GuideEntry[]>(wasm.defaultShortcuts, { platform }),

  limits: () => call<Limits>(wasm.limits, {}),
  presets: () => call<Layout[]>(wasm.presets, {}),
  isValidLayout: (layout: Layout) => call<boolean>(wasm.isValidLayout, { layout }),
  defaultSettings: (platform: PlatformName) => call<Settings>(wasm.defaultSettings, { platform }),
  validateSettings: (settings: Settings, platform: PlatformName) =>
    call<Validation>(wasm.validateSettings, { settings, platform }),
  /** A settings file's text, read as the daemon reads an imported one. */
  readSettings: (contents: string, platform: PlatformName) =>
    call<Validation>(wasm.readSettings, { contents, platform }),
  /** The conflict recording `chord` for `shortcut` would create, named from that shortcut's side. */
  recordingConflict: (settings: Settings, shortcut: ShortcutId, chord: KeyChord, platform: PlatformName) =>
    call<Refusal | null>(wasm.recordingConflict, { settings, shortcut, chord, platform }),
  layoutConflict: (settings: Settings, layout: Layout, options: LayoutOptions, platform: PlatformName) =>
    call<string | null>(wasm.layoutConflict, { settings, layout, options, platform }),
  /** The layout a display uses now, or in `workspace` when one is named. */
  layoutForDisplay: (settings: Settings, display: string, workspace?: string) =>
    call<Layout>(wasm.layoutForDisplay, { settings, display, workspace }),
  layoutOptions: (settings: Settings, layout: Layout) =>
    call<LayoutOptions>(wasm.layoutOptions, { settings, layout }),
  /** The settings with `reset` applied: what the daemon does with a `reset` command. */
  resetSettings: (settings: Settings, reset: Reset, platform: PlatformName) =>
    call<ResetOutcome>(wasm.resetSettings, { settings, reset, platform }),
  /** A settings edit by the daemon's rules, not yet validated: validate it as the daemon does. */
  applyEdit: (settings: Settings, edit: SettingsEdit, platform: PlatformName) =>
    call<EditOutcome>(wasm.applyEdit, { settings, edit, platform }),
};
