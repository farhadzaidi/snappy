// How the canvas draws zones and measures the pointer, in on-screen pixels.

import type { Axis } from "../../core/snappyCore";
import type { Point, Size, Zone } from "../../core/model";

/** Snap distance on the canvas, in pixels. */
const SNAP_TOLERANCE_PX = 7;
/** Zones are drawn inset by this much on every side so neighbors stay apart on screen. */
export const ZONE_INSET_PX = 2;

/** The zone's on-screen size: its share of the canvas minus the inset on both sides. */
export function zoneBoxSize(zone: Zone, canvas: Size): Size {
  return {
    width: Math.max(4, zone.rect.width * canvas.width - 2 * ZONE_INSET_PX),
    height: Math.max(4, zone.rect.height * canvas.height - 2 * ZONE_INSET_PX),
  };
}

/** A point on the canvas as fractions of its size. */
export function toUnit(point: Point, canvas: Size): Point {
  return { x: point.x / canvas.width, y: point.y / canvas.height };
}

/** The snap distance as fractions of the canvas, per axis. */
export function snapTolerance(canvas: Size): Size {
  return { width: SNAP_TOLERANCE_PX / canvas.width, height: SNAP_TOLERANCE_PX / canvas.height };
}

/** A size's extent along `axis`. */
export function along(size: Size, axis: Axis): number {
  return axis === "x" ? size.width : size.height;
}
