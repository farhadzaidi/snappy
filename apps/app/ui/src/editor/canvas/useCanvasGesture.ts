// Pointer input on the editor canvas. Positions are converted to canvas fractions and every
// gesture step is computed by `snappy-core`.

import { useRef, useState, type PointerEvent } from "react";

import type { Point, Size, UnitRect, Zone } from "../../core/model";
import { coreLimits } from "../../core/limits";
import { core, type Axis, type Divider, type Guides, type Handle } from "../../core/snappyCore";
import { soleSelection, type Tool } from "../draft";
import type { EditorStore } from "../editorStore";
import { snapTolerance, toUnit, zoneBoxSize, ZONE_INSET_PX } from "./canvasGeometry";
import { useCutPreview, type CutUnderPointer } from "./useCutPreview";

/** A press must travel this far before it draws. */
const DRAW_START_PX = 3;
/** Smaller travel is a press, not a move. */
const MOVE_START_PX = 1;

type Gesture =
  | { kind: "move"; zoneId: string; start: Point; zones: Zone[] }
  | { kind: "resize"; zoneId: string; handle: Handle; start: Point; zones: Zone[] }
  | { kind: "divider"; divider: Divider; start: Point; zones: Zone[] }
  | { kind: "draw"; start: Point };

export interface CanvasInteraction {
  guides: Guides;
  drawing: UnitRect | null;
  cut: CutUnderPointer | null;
  /** The handle under the pointer on the selected zone, for its cursor. */
  hoverHandle: Handle | null;
  moving: boolean;
  onPointerDown: (event: PointerEvent<HTMLDivElement>) => void;
  onPointerMove: (event: PointerEvent<HTMLDivElement>) => void;
  onPointerUp: (event: PointerEvent<HTMLDivElement>) => void;
  onPointerLeave: () => void;
}

const NO_GUIDES: Guides = { x: [], y: [] };

function cutAxis(tool: Tool): Axis | null {
  return tool === "verticalCut" ? "x" : tool === "horizontalCut" ? "y" : null;
}

export function useCanvasGesture(editor: EditorStore, size: Size): CanvasInteraction {
  const gesture = useRef<Gesture | null>(null);
  const [guides, setGuides] = useState<Guides>(NO_GUIDES);
  const [drawing, setDrawing] = useState<UnitRect | null>(null);
  const [hoverHandle, setHoverHandle] = useState<Handle | null>(null);
  const [moving, setMoving] = useState(false);
  const cutting = useCutPreview(editor, size);

  const zones = () => editor.draft.get().current.layout.zones;
  const zoneById = (id: string | undefined) => zones().find((zone) => zone.id === id);

  const pointerOn = (event: PointerEvent<HTMLDivElement>): Point => {
    const bounds = event.currentTarget.getBoundingClientRect();
    return { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
  };

  /** The handle under the pointer on a zone's on-screen box. */
  const handleOn = (zone: Zone, point: Point): Handle | null =>
    core.handleAt(
      {
        x: point.x - zone.rect.x * size.width - ZONE_INSET_PX,
        y: point.y - zone.rect.y * size.height - ZONE_INSET_PX,
      },
      zoneBoxSize(zone, size),
    );

  const targetOf = (event: PointerEvent<HTMLDivElement>) => {
    const element = (event.target as HTMLElement).closest<HTMLElement>("[data-zone-id], [data-divider-id]");
    return {
      zone: zoneById(element?.dataset.zoneId),
      dividerId: element?.dataset.dividerId,
    };
  };

  const finish = () => {
    gesture.current = null;
    setGuides(NO_GUIDES);
    setDrawing(null);
    setMoving(false);
  };

  /** The gesture a press starts on a zone: resizing by a handle of the one selected zone, or moving. */
  const zoneGesture = (zone: Zone, point: Point): Gesture => {
    const wasSelected = soleSelection(editor.draft.get())?.id === zone.id;
    const handle = wasSelected ? handleOn(zone, point) : null;
    return handle
      ? { kind: "resize", zoneId: zone.id, handle, start: point, zones: zones() }
      : { kind: "move", zoneId: zone.id, start: point, zones: zones() };
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || size.width <= 0) {
      return;
    }
    const point = pointerOn(event);
    const { zone, dividerId } = targetOf(event);
    const readOnly = editor.isReadOnly;
    editor.commitPendingField();

    const axis = cutAxis(editor.draft.get().tool);
    if (axis) {
      if (zone && !readOnly) {
        cutting.cut(zone, point, axis, event.shiftKey);
      }
      return;
    }
    if (zone && event.shiftKey) {
      editor.toggleSelected(zone.id);
      return;
    }
    if (zone) {
      const gestureOnZone = zoneGesture(zone, point);
      editor.select(zone.id);
      gesture.current = readOnly ? null : gestureOnZone;
    } else if (dividerId && !readOnly) {
      const divider = core.dividers(zones()).find((item) => item.id === dividerId);
      if (divider) {
        gesture.current = { kind: "divider", divider, start: point, zones: zones() };
      }
    } else if (!readOnly && zones().length < coreLimits().zones) {
      gesture.current = { kind: "draw", start: point };
    }
    if (gesture.current) {
      event.currentTarget.setPointerCapture(event.pointerId);
      editor.beginStep();
    }
  };

  const hover = (event: PointerEvent<HTMLDivElement>, point: Point) => {
    const { zone } = targetOf(event);
    const axis = cutAxis(editor.draft.get().tool);
    if (axis) {
      if (zone && !editor.isReadOnly) {
        editor.select(zone.id);
        cutting.follow(zone, point, axis, event.shiftKey);
      } else {
        cutting.clear();
      }
      return;
    }
    const selected = zone && soleSelection(editor.draft.get())?.id === zone.id && !editor.isReadOnly;
    setHoverHandle(selected ? handleOn(zone, point) : null);
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const point = pointerOn(event);
    const current = gesture.current;
    if (!current) {
      hover(event, point);
      return;
    }
    const delta = toUnit({ x: point.x - current.start.x, y: point.y - current.start.y }, size);
    const travel = Math.hypot(point.x - current.start.x, point.y - current.start.y);
    const tolerance = snapTolerance(size);

    switch (current.kind) {
      case "move": {
        if (travel <= MOVE_START_PX) {
          return;
        }
        const edit = core.dragMove({ zones: current.zones, id: current.zoneId, delta, tolerance });
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
          tolerance,
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
          tolerance: vertical ? tolerance.width : tolerance.height,
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
          from: toUnit(current.start, size),
          to: toUnit(point, size),
          tolerance,
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
      cutting.clear();
      setHoverHandle(null);
    }
  };

  return {
    guides: cutting.preview?.guides ?? guides,
    drawing,
    cut: cutting.preview,
    hoverHandle,
    moving,
    onPointerDown,
    onPointerMove,
    onPointerUp,
    onPointerLeave,
  };
}
