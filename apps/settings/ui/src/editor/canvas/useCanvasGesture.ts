// Pointer input on the editor canvas. Positions are converted to canvas fractions and every
// gesture step is computed by `snappy-core`.

import { useRef, useState, type PointerEvent } from "react";

import type { Point, Size, UnitRect, Zone } from "../../core/model";
import { MAX_ZONES } from "../../core/model";
import { core, type Axis, type Divider, type Guides, type Handle } from "../../core/snappyCore";
import type { EditorStore } from "../editorStore";
import type { Tool } from "../draft";

/** Snap distance on the canvas, in pixels. */
const SNAP_TOLERANCE_PX = 7;
/** A press must travel this far before it draws. */
const DRAW_START_PX = 3;
/** Smaller travel is a press, not a move. */
const MOVE_START_PX = 1;
/** Zones are drawn inset by this much on every side so neighbors stay apart on screen. */
export const ZONE_INSET_PX = 2;

type Gesture =
  | { kind: "move"; zoneId: string; start: Point; zones: Zone[] }
  | { kind: "resize"; zoneId: string; handle: Handle; start: Point; zones: Zone[] }
  | { kind: "divider"; divider: Divider; start: Point; zones: Zone[] }
  | { kind: "draw"; start: Point };

export interface CutPreview {
  zoneId: string;
  axis: Axis;
  /** Where the cut lands, as a canvas fraction along `axis`. */
  at: number;
}

export interface CanvasInteraction {
  guides: Guides;
  drawing: UnitRect | null;
  cut: CutPreview | null;
  /** The handle under the pointer on the selected zone, for its cursor. */
  hoverHandle: Handle | null;
  moving: boolean;
  onPointerDown: (event: PointerEvent<HTMLDivElement>) => void;
  onPointerMove: (event: PointerEvent<HTMLDivElement>) => void;
  onPointerUp: (event: PointerEvent<HTMLDivElement>) => void;
  onPointerLeave: () => void;
}

const NO_GUIDES: Guides = { x: [], y: [] };

/** The zone's on-screen size: its share of the canvas minus the inset on both sides. */
export function zoneBoxSize(zone: Zone, canvas: Size): Size {
  return {
    width: Math.max(4, zone.rect.width * canvas.width - 2 * ZONE_INSET_PX),
    height: Math.max(4, zone.rect.height * canvas.height - 2 * ZONE_INSET_PX),
  };
}

function cutAxis(tool: Tool): Axis | null {
  return tool === "verticalCut" ? "x" : tool === "horizontalCut" ? "y" : null;
}

export function useCanvasGesture(editor: EditorStore, size: Size): CanvasInteraction {
  const gesture = useRef<Gesture | null>(null);
  const [guides, setGuides] = useState<Guides>(NO_GUIDES);
  const [drawing, setDrawing] = useState<UnitRect | null>(null);
  const [cut, setCut] = useState<CutPreview | null>(null);
  const [hoverHandle, setHoverHandle] = useState<Handle | null>(null);
  const [moving, setMoving] = useState(false);

  const tolerance = (): Size => ({
    width: SNAP_TOLERANCE_PX / size.width,
    height: SNAP_TOLERANCE_PX / size.height,
  });
  const toUnit = (point: Point): Point => ({ x: point.x / size.width, y: point.y / size.height });
  const zones = () => editor.draft.get().current.layout.zones;
  const zoneById = (id: string | undefined) => zones().find((zone) => zone.id === id);

  const pointerOn = (event: PointerEvent<HTMLDivElement>): Point => {
    const bounds = event.currentTarget.getBoundingClientRect();
    return { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
  };

  /** The pointer relative to a zone's on-screen box. */
  const localTo = (zone: Zone, point: Point): Point => ({
    x: point.x - zone.rect.x * size.width - ZONE_INSET_PX,
    y: point.y - zone.rect.y * size.height - ZONE_INSET_PX,
  });

  const targetOf = (event: PointerEvent<HTMLDivElement>) => {
    const element = (event.target as HTMLElement).closest<HTMLElement>("[data-zone-id], [data-divider-id]");
    return {
      zone: zoneById(element?.dataset.zoneId),
      dividerId: element?.dataset.dividerId,
    };
  };

  const cutPreview = (zone: Zone, point: Point, axis: Axis): CutPreview => {
    const unit = toUnit(point);
    const along = axis === "x" ? unit.x : unit.y;
    const limit = axis === "x" ? tolerance().width : tolerance().height;
    return {
      zoneId: zone.id,
      axis,
      at: core.cutLine({ zones: zones(), id: zone.id, axis, at: along, tolerance: limit }),
    };
  };

  const finish = () => {
    gesture.current = null;
    setGuides(NO_GUIDES);
    setDrawing(null);
    setMoving(false);
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || size.width <= 0) {
      return;
    }
    const draft = editor.draft.get();
    const point = pointerOn(event);
    const { zone, dividerId } = targetOf(event);
    const readOnly = editor.isReadOnly;
    editor.commitPendingField();

    const axis = cutAxis(draft.tool);
    if (axis) {
      if (zone && !readOnly) {
        const preview = cutPreview(zone, point, axis);
        editor.cut(zone.id, axis, preview.at, 0);
        setCut(null);
      }
      return;
    }
    if (zone) {
      const wasSelected = draft.selectedZoneId === zone.id;
      editor.select(zone.id);
      if (readOnly) {
        return;
      }
      const handle = wasSelected ? core.handleAt(localTo(zone, point), zoneBoxSize(zone, size)) : null;
      gesture.current = handle
        ? { kind: "resize", zoneId: zone.id, handle, start: point, zones: zones() }
        : { kind: "move", zoneId: zone.id, start: point, zones: zones() };
    } else if (dividerId && !readOnly) {
      const divider = core.dividers(zones()).find((item) => item.id === dividerId);
      if (divider) {
        gesture.current = { kind: "divider", divider, start: point, zones: zones() };
      }
    } else if (!readOnly && zones().length < MAX_ZONES) {
      gesture.current = { kind: "draw", start: point };
    }
    if (gesture.current) {
      event.currentTarget.setPointerCapture(event.pointerId);
      editor.beginStep();
    }
  };

  const hover = (event: PointerEvent<HTMLDivElement>, point: Point) => {
    const draft = editor.draft.get();
    const { zone } = targetOf(event);
    const axis = cutAxis(draft.tool);
    if (axis) {
      if (zone && !editor.isReadOnly) {
        editor.select(zone.id);
        setCut(cutPreview(zone, point, axis));
      } else {
        setCut(null);
      }
      return;
    }
    const selected = zone && zone.id === draft.selectedZoneId && !editor.isReadOnly;
    setHoverHandle(selected ? core.handleAt(localTo(zone, point), zoneBoxSize(zone, size)) : null);
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const point = pointerOn(event);
    const current = gesture.current;
    if (!current) {
      hover(event, point);
      return;
    }
    const delta = toUnit({ x: point.x - current.start.x, y: point.y - current.start.y });
    const travel = Math.hypot(point.x - current.start.x, point.y - current.start.y);

    switch (current.kind) {
      case "move": {
        if (travel <= MOVE_START_PX) {
          return;
        }
        const edit = core.dragMove({
          zones: current.zones,
          id: current.zoneId,
          delta,
          tolerance: tolerance(),
        });
        editor.previewZones(edit.zones);
        setGuides(edit.guides);
        setMoving(true);
        break;
      }
      case "resize": {
        const edit = core.dragResize({
          zones: current.zones,
          id: current.zoneId,
          handle: current.handle,
          delta,
          tolerance: tolerance(),
        });
        editor.previewZones(edit.zones);
        setGuides(edit.guides);
        break;
      }
      case "divider": {
        const { divider } = current;
        const vertical = divider.axis === "x";
        const edit = core.dragDivider({
          zones: current.zones,
          zoneId: divider.zoneId,
          axis: divider.axis,
          line: divider.line,
          delta: vertical ? delta.x : delta.y,
          tolerance: vertical ? tolerance().width : tolerance().height,
        });
        editor.previewZones(edit.zones);
        setGuides(edit.guides);
        break;
      }
      case "draw": {
        if (travel < DRAW_START_PX) {
          return;
        }
        const drawn = core.drawZone({
          zones: zones(),
          from: toUnit(current.start),
          to: toUnit(point),
          tolerance: tolerance(),
        });
        setDrawing(drawn.rect);
        setGuides(drawn.guides);
        break;
      }
    }
  };

  const onPointerUp = () => {
    const current = gesture.current;
    if (current?.kind === "draw" && drawing) {
      editor.endStep();
      editor.addDrawn(drawing);
    } else if (current) {
      editor.endStep();
    }
    finish();
  };

  const onPointerLeave = () => {
    if (!gesture.current) {
      setCut(null);
      setHoverHandle(null);
    }
  };

  return {
    guides,
    drawing,
    cut,
    hoverHandle,
    moving,
    onPointerDown,
    onPointerMove,
    onPointerUp,
    onPointerLeave,
  };
}
