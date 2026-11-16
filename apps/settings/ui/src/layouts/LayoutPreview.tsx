import { useRef } from "react";

import { aspectRatio, optionsFor } from "../core/layouts";
import type { Layout } from "../core/model";
import { core } from "../core/snappyCore";
import { useAppState, useSelectedDisplay, useSend } from "../state/services";
import { Button } from "../ui/Button";
import { ChordLabel } from "../ui/Chord";
import { CheckCircleIcon } from "../ui/icons";
import { useElementSize } from "../ui/useElementSize";
import { LayoutActions } from "./LayoutActions";
import { layoutStatus, type LayoutStatus } from "./library";

/** Gap and padding in the preview are scaled by this. */
const PREVIEW_SPACING_FACTOR = 0.4;

/**
 * The browsed layout's zones, then a panel as wide as them: its name, shortcut, and status,
 * with "Use layout" at the end of that line, over its actions.
 */
export function LayoutPreview({ layout, inUse }: { layout: Layout; inUse: Layout | undefined }) {
  const app = useAppState();
  const shortcut = optionsFor(app.settings, layout).selectShortcut;
  const status = layoutStatus(app, layout, inUse);

  return (
    <section className="preview" aria-label="Layout preview">
      <NumberedLayout layout={layout} />
      <div className="preview-details">
        <div className="preview-heading">
          <h2 className="preview-name">{layout.name}</h2>
          {shortcut && <ChordLabel chord={shortcut} platform={app.platform} />}
          <LayoutStatusBadge status={status} />
          {status === null && <UseLayoutButton layout={layout} />}
        </div>
        <LayoutActions layout={layout} />
      </div>
    </section>
  );
}

/** Puts the layout to use on the selected display. */
function UseLayoutButton({ layout }: { layout: Layout }) {
  const send = useSend();
  const display = useSelectedDisplay();
  return (
    <Button
      variant="primary"
      className="use-layout"
      disabled={!display}
      onClick={() => {
        if (display) {
          void send({ kind: "applyLayout", display: display.id, layout: layout.id });
        }
      }}
    >
      Use layout
    </Button>
  );
}

/** "In use" on the selected display, "Disabled", or nothing. */
function LayoutStatusBadge({ status }: { status: LayoutStatus }) {
  switch (status) {
    case "inUse":
      return (
        <span className="preview-status" data-status="inUse">
          <CheckCircleIcon size={13} />
          In use
        </span>
      );
    case "disabled":
      return (
        <span className="preview-status" data-status="disabled">
          Disabled
        </span>
      );
    case null:
      return null;
  }
}

/** The layout's numbered zones at the selected display's shape, with frames from `snappy-core`. */
function NumberedLayout({ layout }: { layout: Layout }) {
  const box = useRef<HTMLDivElement>(null);
  const size = useElementSize(box);
  const frames = core.zoneFrames({
    zones: layout.zones,
    workArea: { x: 0, y: 0, width: size.width, height: size.height },
    spacingFactor: PREVIEW_SPACING_FACTOR,
  });

  return (
    <div className="preview-frame">
      <div ref={box} className="preview-canvas" style={{ aspectRatio: aspectRatio(useSelectedDisplay()) }}>
        {size.width > 0 &&
          frames.map((frame, index) => (
            <div
              key={layout.zones[index]?.id ?? index}
              className="preview-zone"
              aria-label={`Zone ${index + 1}`}
              style={{ left: frame.x, top: frame.y, width: frame.width, height: frame.height }}
            >
              {index + 1}
            </div>
          ))}
      </div>
    </div>
  );
}
