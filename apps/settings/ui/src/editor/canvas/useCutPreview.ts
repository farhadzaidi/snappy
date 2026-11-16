// The cut tools' preview: where a click would cut, following the pointer and the Shift key.
// Holding Shift turns the cut into one through the whole layout.

import { useEffect, useState } from "react";

import type { Point, Size, Zone } from "../../core/model";
import { core, type Axis, type CutScope, type Guides } from "../../core/snappyCore";
import type { EditorStore } from "../editorStore";
import { along, snapTolerance, toUnit } from "./canvasGeometry";

/** The cut a click would make: core's preview of it, with the zone and the way it cuts. */
export interface CutUnderPointer {
  axis: Axis;
  /** The zone under the pointer, which a click cuts. */
  zoneId: string;
  /** The cut goes through every zone it crosses, while Shift is held. */
  through: boolean;
  /** The line, as a canvas fraction along `axis`, snapped. */
  at: number;
  /** The zones the cut splits. */
  zoneIds: string[];
  guides: Guides;
}

/** The cut tools at the pointer, at `point` on the canvas over `zone`, cutting along `axis`. */
type AtPointer = (zone: Zone, point: Point, axis: Axis, shiftKey: boolean) => void;

export interface CutPreviewControl {
  preview: CutUnderPointer | null;
  /** Show the cut a click would make. */
  follow: AtPointer;
  clear: () => void;
  /** Make the cut a click there makes, the one `follow` shows. */
  cut: AtPointer;
}

interface Pointer {
  zoneId: string;
  point: Point;
  axis: Axis;
}

export function useCutPreview(editor: EditorStore, canvas: Size): CutPreviewControl {
  const [pointer, setPointer] = useState<Pointer | null>(null);
  const [shiftHeld, setShiftHeld] = useState(false);
  const zones = editor.draft.get().current.layout.zones;
  const preview = pointer && cutPreview(zones, pointer, shiftHeld, canvas);
  const showing = pointer !== null;

  // Pressing or letting go of Shift without moving the pointer switches the preview at once.
  // Only while there is a preview to switch, so Shift typed elsewhere, as in the layout's name,
  // doesn't render the editor.
  useEffect(() => {
    if (!showing) {
      return;
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Shift") {
        setShiftHeld(event.type === "keydown");
      }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("keyup", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keyup", onKey);
    };
  }, [showing]);

  return {
    preview,
    follow: (zone, point, axis, shiftKey) => {
      setPointer({ zoneId: zone.id, point, axis });
      setShiftHeld(shiftKey);
    },
    clear: () => setPointer(null),
    cut: (zone, point, axis, shiftKey) => {
      const made = cutPreview(zones, { zoneId: zone.id, point, axis }, shiftKey, canvas);
      editor.cut(cutScope(made.through, made.zoneId), made.axis, made.at, made.zoneId);
      setPointer(null);
    },
  };
}

function cutPreview(
  zones: Zone[],
  { zoneId, point, axis }: Pointer,
  through: boolean,
  canvas: Size,
): CutUnderPointer {
  const unit = toUnit(point, canvas);
  const found = core.cutPreview({
    zones,
    scope: cutScope(through, zoneId),
    axis,
    at: axis === "x" ? unit.x : unit.y,
    tolerance: along(snapTolerance(canvas), axis),
  });
  return { axis, zoneId, through, ...found };
}

function cutScope(through: boolean, zoneId: string): CutScope {
  return through ? "through" : { zone: zoneId };
}
