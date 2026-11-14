// Typed access to `snappy-core` through `snappy-wasm`. Every geometry and shortcut rule the
// settings app applies comes from here; the app never computes zone geometry itself.

import initWasm, * as wasm from "../wasm/snappy_wasm";
import type {
  HistoryStep,
  KeyChord,
  Layout,
  LayoutOptions,
  Modifier,
  NavigationShortcut,
  PlatformName,
  Point,
  Rect,
  Settings,
  Size,
  UnitRect,
  Workspace,
  WorkspaceShortcut,
  WorkspaceStep,
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

export interface HandlePosition {
  handle: Handle;
  center: Point;
}

export interface ChordLabels {
  compact: string;
  long: string;
}

export interface ModifierNames {
  name: string;
  short: string;
  /** The name with its symbol, as messages show it: "Shift ⇧". */
  label: string;
}

export type Validation = { valid: Settings } | { invalid: string };

/** Every shortcut's title, by section and by the field that holds it. */
export interface ShortcutTitles {
  navigation: Record<NavigationShortcut, string>;
  workspaceSteps: Record<WorkspaceStep, string>;
  history: Record<HistoryStep, string>;
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
  refusal: string;
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
  duplicateRect: (zones: Zone[], id: string) => call<UnitRect | null>(wasm.duplicateRect, { zones, id }),

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
  cutLine: (request: { zones: Zone[]; id: string; axis: Axis; at: number; tolerance: number }) =>
    call<number>(wasm.cutLine, request),
  cutZone: (request: {
    zones: Zone[];
    id: string;
    axis: Axis;
    at: number;
    tolerance: number;
    newId: string;
  }) => call<Zone[] | null>(wasm.cutZone, request),
  setField: (request: { zones: Zone[]; id: string; field: CoordinateField; value: number }) =>
    call<Zone[]>(wasm.setField, request),

  chordLabels: (chord: KeyChord, platform: PlatformName) =>
    call<ChordLabels>(wasm.chordLabels, { chord, platform }),
  isValidChord: (chord: KeyChord, platform: PlatformName) =>
    call<boolean>(wasm.isValidChord, { chord, platform }),
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
  /** The settings with a new Snappy key, or why it is refused. */
  changeSnappyKey: (settings: Settings, modifiers: Modifier[], platform: PlatformName) =>
    call<Validation>(wasm.changeSnappyKey, { settings, modifiers, platform }),
  /** What to ask the OS about for settings whose Snappy key just changed. */
  snappyKeyProbes: (settings: Settings, platform: PlatformName) =>
    call<AvailabilityProbe[]>(wasm.snappyKeyProbes, { settings, platform }),
  modifierNames: (modifier: Modifier, platform: PlatformName) =>
    call<ModifierNames>(wasm.modifierNames, { modifier, platform }),
  /** The title the app and conflict messages give each shortcut. */
  shortcutTitles: () => call<ShortcutTitles>(wasm.shortcutTitles, {}),
  /** The titles of a workspace's own shortcuts, naming it `workspace`. */
  workspaceShortcutTitles: (workspace: string) =>
    call<Record<WorkspaceShortcut, string>>(wasm.workspaceShortcutTitles, { workspace }),
  /** The titles of a layout's own shortcuts, naming it `layout`, with one per zone. */
  layoutShortcutTitles: (layout: string, zones: number) =>
    call<LayoutShortcutTitles>(wasm.layoutShortcutTitles, { layout, zones }),
  /** The default shortcuts as help lists them. */
  defaultShortcuts: (platform: PlatformName) => call<GuideEntry[]>(wasm.defaultShortcuts, { platform }),

  presets: () => call<Layout[]>(wasm.presets, {}),
  isValidLayout: (layout: Layout) => call<boolean>(wasm.isValidLayout, { layout }),
  defaultSettings: (platform: PlatformName) => call<Settings>(wasm.defaultSettings, { platform }),
  validateSettings: (settings: Settings, platform: PlatformName) =>
    call<Validation>(wasm.validateSettings, { settings, platform }),
  settingsConflict: (settings: Settings, platform: PlatformName) =>
    call<string | null>(wasm.settingsConflict, { settings, platform }),
  layoutConflict: (settings: Settings, layout: Layout, options: LayoutOptions, platform: PlatformName) =>
    call<string | null>(wasm.layoutConflict, { settings, layout, options, platform }),
  layoutForDisplay: (settings: Settings, display: string) =>
    call<Layout>(wasm.layoutForDisplay, { settings, display }),
  layoutOptions: (settings: Settings, layout: Layout) =>
    call<LayoutOptions>(wasm.layoutOptions, { settings, layout }),
  /** "Workspace <number>", switched to with the Snappy key and its digit when that is free. */
  newWorkspace: (settings: Settings, id: string, number: number) =>
    call<Workspace>(wasm.newWorkspace, { settings, id, number }),
};
