import type { ReactNode } from "react";

import { aspectRatio, pixelSize } from "../../core/layouts";
import { labelParts } from "../../core/labels";
import type { KeyChord, Zone } from "../../core/model";
import type { CoordinateField } from "../../core/snappyCore";
import { useObservable } from "../../state/observable";
import { usePlatform, useSelectedDisplay, useServices } from "../../state/services";
import { Button } from "../../ui/Button";
import { DuplicateIcon, MergeIcon, TrashIcon } from "../../ui/icons";
import { ZoneThumbnail } from "../../ui/ZoneThumbnail";
import { selectedZones } from "../draft";
import { commandChord } from "../keyboard";
import { PixelField } from "./PixelField";

const FIELDS: readonly { field: CoordinateField; title: string }[] = [
  { field: "x", title: "X" },
  { field: "y", title: "Y" },
  { field: "width", title: "Width" },
  { field: "height", title: "Height" },
];

const SUMMARY_HEIGHT = 84;

/**
 * The selection: a single zone's coordinates, or several zones shown on a small map of the
 * layout, then what can be done with them, each action named.
 */
export function ZoneInspector() {
  const { editor } = useServices();
  const draft = useObservable(editor.draft);
  const zones = draft.current.layout.zones;
  const selected = selectedZones(draft);
  const [first] = selected;

  if (zones.length === 0) {
    return <p className="muted">Draw a zone on the canvas.</p>;
  }
  if (!first) {
    return <p className="muted">Click a zone to select it.</p>;
  }
  const single = selected.length === 1;

  return (
    <div className="inspector">
      <h2 className="inspector-title">
        {single ? `Zone ${zones.indexOf(first) + 1}` : `${selected.length} zones`}
      </h2>
      {single ? <Coordinates zone={first} /> : <SelectionSummary zones={zones} selected={selected} />}
      <SelectionActions single={single} />
    </div>
  );
}

/** Where the selected zones are in the layout, and their numbers. */
function SelectionSummary({ zones, selected }: { zones: Zone[]; selected: Zone[] }) {
  const aspect = aspectRatio(useSelectedDisplay());
  const numbers = selected.map((zone) => zones.indexOf(zone) + 1);
  return (
    <div className="selection-summary">
      <ZoneThumbnail
        zones={zones}
        aspect={aspect}
        height={SUMMARY_HEIGHT}
        highlighted={(zone) => selected.includes(zone)}
        numbered
      />
      <p className="muted">Zones {listed(numbers)}</p>
    </div>
  );
}

/** "1 and 4", or "1, 2, and 4". */
function listed(numbers: number[]): string {
  if (numbers.length < 3) {
    return numbers.join(" and ");
  }
  return `${numbers.slice(0, -1).join(", ")}, and ${numbers.at(-1)}`;
}

/** Merge, Duplicate, and Delete, one under another, Merge and Duplicate saying why when they can't. */
function SelectionActions({ single }: { single: boolean }) {
  const { editor } = useServices();
  const platform = usePlatform();
  const mergeRefusal = editor.mergeRefusal();
  const duplicateRefusal = editor.duplicateRefusal();
  const noun = single ? "zone" : "zones";

  return (
    <div className="selection-actions">
      <SelectionAction
        icon={<MergeIcon />}
        label="Merge"
        name="Merge zones"
        keys={commandChord("merge", platform)}
        refusal={mergeRefusal}
        onClick={() => editor.mergeSelection()}
      />
      {mergeRefusal && <p className="selection-hint muted">{mergeRefusal}</p>}
      <SelectionAction
        icon={<DuplicateIcon />}
        label="Duplicate"
        name={`Duplicate ${noun}`}
        keys={commandChord("duplicate", platform)}
        refusal={duplicateRefusal}
        onClick={() => editor.duplicateSelection()}
      />
      {duplicateRefusal && <p className="selection-hint muted">{duplicateRefusal}</p>}
      <SelectionAction
        icon={<TrashIcon />}
        label="Delete"
        name={`Delete ${noun}`}
        keys={commandChord("delete", platform)}
        destructive
        onClick={() => editor.deleteSelection()}
      />
    </div>
  );
}

/** A labelled action with the keys that also run it at its end, off while it has a refusal. */
function SelectionAction({
  icon,
  label,
  name,
  keys,
  refusal = null,
  destructive = false,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  /** What assistive technology calls it, which names what it acts on. */
  name: string;
  keys: KeyChord;
  refusal?: string | null;
  destructive?: boolean;
  onClick: () => void;
}) {
  const platform = usePlatform();
  return (
    <Button
      className={destructive ? "selection-action selection-action-destructive" : "selection-action"}
      aria-label={name}
      title={refusal ?? name}
      disabled={refusal !== null}
      onClick={onClick}
    >
      {icon}
      <span className="selection-action-label">{label}</span>
      <kbd className="key-hint" aria-hidden="true">
        {labelParts(keys, platform).keys}
      </kbd>
    </Button>
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
