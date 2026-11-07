import { pixelSize } from "../../core/layouts";
import type { Zone } from "../../core/model";
import { core, type CoordinateField } from "../../core/snappyCore";
import { useObservable } from "../../state/observable";
import { useSelectedDisplay, useServices } from "../../state/services";
import { IconButton } from "../../ui/Button";
import { DuplicateIcon, TrashIcon } from "../../ui/icons";
import { canAddZone } from "../draft";
import { PixelField } from "./PixelField";

const FIELDS: readonly { field: CoordinateField; title: string }[] = [
  { field: "x", title: "X" },
  { field: "y", title: "Y" },
  { field: "width", title: "Width" },
  { field: "height", title: "Height" },
];

/** The selected zone's number, Duplicate, Delete, and its coordinates in pixels. */
export function ZoneInspector() {
  const { editor } = useServices();
  const draft = useObservable(editor.draft);
  const zones = draft.current.layout.zones;
  const index = zones.findIndex((zone) => zone.id === draft.selectedZoneId);
  const zone = zones[index];

  if (!zone) {
    return <p className="muted">Draw a zone on the canvas.</p>;
  }
  const canDuplicate = canAddZone(draft) && core.duplicateRect(zones, zone.id) !== null;

  return (
    <div className="inspector">
      <div className="inspector-heading">
        <h2 className="inspector-title">Zone {index + 1}</h2>
        <IconButton
          label="Duplicate zone"
          disabled={!canDuplicate}
          onClick={() => editor.duplicateZone(zone.id)}
        >
          <DuplicateIcon />
        </IconButton>
        <IconButton label="Delete zone" onClick={() => editor.deleteZone(zone.id)}>
          <TrashIcon />
        </IconButton>
      </div>
      <Coordinates zone={zone} />
    </div>
  );
}

/** Coordinates in backing pixels of the editor's display, from the work area's top-left. */
function Coordinates({ zone }: { zone: Zone }) {
  const { editor } = useServices();
  const pixels = pixelSize(useSelectedDisplay());
  const scaleOf = (field: CoordinateField) =>
    field === "x" || field === "width" ? pixels.width : pixels.height;

  const commit = (field: CoordinateField, value: number): number => {
    const scale = scaleOf(field);
    const zones = editor.setField(zone.id, field, value / scale);
    const result = zones.find((item) => item.id === zone.id)?.rect ?? zone.rect;
    return result[field] * scale;
  };

  return (
    <div className="coordinates">
      {FIELDS.map(({ field, title }) => (
        <PixelField
          key={`${zone.id}-${field}`}
          title={title}
          value={zone.rect[field] * scaleOf(field)}
          onCommit={(value) => commit(field, value)}
        />
      ))}
    </div>
  );
}
