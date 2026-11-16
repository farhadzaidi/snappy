import { useRef } from "react";

import { aspectRatio, pixelSize } from "../../core/layouts";
import type { Size, UnitRect, Zone } from "../../core/model";
import { core, type Handle } from "../../core/snappyCore";
import { useObservable } from "../../state/observable";
import { useSelectedDisplay, useServices } from "../../state/services";
import { useElementSize } from "../../ui/useElementSize";
import { soleSelection } from "../draft";
import { zoneBoxSize, ZONE_INSET_PX } from "./canvasGeometry";
import { useCanvasGesture } from "./useCanvasGesture";
import { useCanvasKeys } from "./useCanvasKeys";
import type { CutUnderPointer } from "./useCutPreview";

const HANDLE_CURSORS: Record<Handle, string> = {
  north: "ns-resize",
  south: "ns-resize",
  east: "ew-resize",
  west: "ew-resize",
  northWest: "nwse-resize",
  southEast: "nwse-resize",
  northEast: "nesw-resize",
  southWest: "nesw-resize",
};

/** The canvas at the editor display's shape, fitted into the space available. */
export function Canvas() {
  const area = useRef<HTMLDivElement>(null);
  const available = useElementSize(area);
  const aspect = aspectRatio(useSelectedDisplay());
  const width = Math.floor(Math.min(available.width, available.height * aspect));
  const size = { width, height: Math.floor(width / aspect) };

  return (
    <div ref={area} className="canvas-area">
      {size.width > 0 && <CanvasSurface size={size} />}
    </div>
  );
}

function CanvasSurface({ size }: { size: Size }) {
  const { editor } = useServices();
  const draft = useObservable(editor.draft);
  const interaction = useCanvasGesture(editor, size);
  const onKeyDown = useCanvasKeys(editor);
  const zones = draft.current.layout.zones;
  const readOnly = editor.isReadOnly;
  const editable = !readOnly && draft.tool === "select";
  const resizable = soleSelection(draft)?.id ?? null;

  // Focusable, so a click on it gives it focus and the clipboard and Delete keys reach it.
  return (
    <div
      className="canvas"
      role="application"
      aria-label="Layout canvas"
      tabIndex={0}
      data-tool={draft.tool}
      data-moving={interaction.moving}
      data-read-only={readOnly}
      style={{ width: size.width, height: size.height }}
      onKeyDown={onKeyDown}
      onPointerDown={interaction.onPointerDown}
      onPointerMove={interaction.onPointerMove}
      onPointerUp={interaction.onPointerUp}
      onPointerCancel={interaction.onPointerUp}
      onPointerLeave={interaction.onPointerLeave}
    >
      {zones.length === 0 && <p className="canvas-empty muted">Drag to draw your first zone</p>}
      {zones.map((zone, index) => (
        <ZoneBox
          key={zone.id}
          zone={zone}
          number={index + 1}
          canvas={size}
          selected={draft.selection.includes(zone.id)}
          resizable={editable && zone.id === resizable}
          cursor={zone.id === resizable ? interaction.hoverHandle : null}
        />
      ))}
      {interaction.cut && <CutLines cut={interaction.cut} zones={zones} canvas={size} />}
      {editable && <Dividers zones={zones} canvas={size} />}
      {interaction.drawing && <DrawingBox rect={interaction.drawing} canvas={size} />}
      <GuideLines guides={interaction.guides} canvas={size} />
    </div>
  );
}

interface ZoneBoxProps {
  zone: Zone;
  number: number;
  canvas: Size;
  selected: boolean;
  /** The one selected zone, with the select tool: it shows its resize handles. */
  resizable: boolean;
  cursor: Handle | null;
}

/** A zone with its number and its size in backing pixels. */
function ZoneBox({ zone, number, canvas, selected, resizable, cursor }: ZoneBoxProps) {
  const pixels = pixelSize(useSelectedDisplay());
  const box = zoneBoxSize(zone, canvas);
  const left = zone.rect.x * canvas.width + ZONE_INSET_PX;
  const top = zone.rect.y * canvas.height + ZONE_INSET_PX;
  const sizeLabel = `${Math.round(zone.rect.width * pixels.width)} × ${Math.round(zone.rect.height * pixels.height)}`;

  return (
    <div
      className="zone"
      role="img"
      data-zone-id={zone.id}
      data-selected={selected}
      aria-label={`Zone ${number}`}
      style={{
        left,
        top,
        width: box.width,
        height: box.height,
        ...(cursor && resizable ? { cursor: HANDLE_CURSORS[cursor] } : {}),
      }}
    >
      <span className="zone-number">{number}</span>
      <span className="zone-size">{sizeLabel}</span>
      {resizable && <Handles size={box} />}
    </div>
  );
}

function Handles({ size }: { size: Size }) {
  return (
    <>
      {core.handlePositions(size).map(({ handle, center }) => (
        <span
          key={handle}
          className="zone-handle"
          data-handle={handle}
          style={{ left: center.x, top: center.y }}
        />
      ))}
    </>
  );
}

/**
 * Where a cut will go: a dashed line across each zone it splits, and for a Shift cut, a fainter
 * one from canvas edge to canvas edge.
 */
function CutLines({ cut, zones, canvas }: { cut: CutUnderPointer; zones: Zone[]; canvas: Size }) {
  const vertical = cut.axis === "x";
  const line = vertical ? { left: cut.at * canvas.width } : { top: cut.at * canvas.height };
  return (
    <>
      {cut.through && <span className="cut-through" data-vertical={vertical} style={line} />}
      {zones
        .filter((zone) => cut.zoneIds.includes(zone.id))
        .map((zone) => {
          const box = zoneBoxSize(zone, canvas);
          const across = vertical
            ? { top: zone.rect.y * canvas.height + ZONE_INSET_PX, height: box.height }
            : { left: zone.rect.x * canvas.width + ZONE_INSET_PX, width: box.width };
          return (
            <span
              key={zone.id}
              className="cut-line"
              data-cut-zone-id={zone.id}
              data-vertical={vertical}
              style={{ ...line, ...across }}
            />
          );
        })}
    </>
  );
}

/** Shared edges, dragged to move a whole seam. */
function Dividers({ zones, canvas }: { zones: Zone[]; canvas: Size }) {
  return (
    <>
      {core.dividers(zones).map((divider) => {
        const vertical = divider.axis === "x";
        const style = vertical
          ? {
              left: divider.line * canvas.width,
              top: divider.start * canvas.height,
              height: (divider.end - divider.start) * canvas.height,
            }
          : {
              top: divider.line * canvas.height,
              left: divider.start * canvas.width,
              width: (divider.end - divider.start) * canvas.width,
            };
        return (
          <div
            key={divider.id}
            className="divider"
            role="separator"
            aria-orientation={vertical ? "vertical" : "horizontal"}
            data-divider-id={divider.id}
            data-vertical={vertical}
            aria-label={vertical ? "Vertical divider" : "Horizontal divider"}
            style={style}
          />
        );
      })}
    </>
  );
}

function DrawingBox({ rect, canvas }: { rect: UnitRect; canvas: Size }) {
  return (
    <div
      className="drawing"
      style={{
        left: rect.x * canvas.width,
        top: rect.y * canvas.height,
        width: rect.width * canvas.width,
        height: rect.height * canvas.height,
      }}
    />
  );
}

/** Dashed lines for the guides an edge lines up with. */
function GuideLines({ guides, canvas }: { guides: { x: number[]; y: number[] }; canvas: Size }) {
  return (
    <>
      {guides.x.map((x) => (
        <span key={`x${x}`} className="guide" data-vertical="true" style={{ left: x * canvas.width }} />
      ))}
      {guides.y.map((y) => (
        <span key={`y${y}`} className="guide" data-vertical="false" style={{ top: y * canvas.height }} />
      ))}
    </>
  );
}
