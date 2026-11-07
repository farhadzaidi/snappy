// Typed access to `snappy-core` through `snappy-wasm`. Every geometry and shortcut rule the
// settings app applies comes from here; the app never computes zone geometry itself.

import initWasm, * as wasm from "../wasm/snappy_wasm";
import type {
  KeyChord,
  Layout,
  LayoutOptions,
  Modifier,
  PlatformName,
  Point,
  Rect,
  Settings,
  Size,
  UnitRect,
  Workspace,
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
  /** The stored form of a chord: holding every modifier of the Snappy key makes it the Snappy key. */
  normalizeChord: (chord: KeyChord) => call<KeyChord>(wasm.normalizeChord, { chord }),
  /** What the Snappy key is on the platform: "Control + Option + Command". */
  snappyKeyLabel: (platform: PlatformName) => call<string>(wasm.snappyKeyLabel, { platform }),
  modifierNames: (modifier: Modifier, platform: PlatformName) =>
    call<ModifierNames>(wasm.modifierNames, { modifier, platform }),

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
  numberedWorkspace: (id: string, number: number) => call<Workspace>(wasm.numberedWorkspace, { id, number }),
};
