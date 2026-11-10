import { useRef } from "react";

import { compactLabel } from "../core/labels";
import { aspectRatio, optionsFor } from "../core/layouts";
import type { Layout } from "../core/model";
import { core } from "../core/snappyCore";
import { useAppState, useSelectedDisplay } from "../state/services";
import { CheckCircleIcon } from "../ui/icons";
import { useElementSize } from "../ui/useElementSize";
import { LayoutActions } from "./LayoutActions";
import { layoutStatus } from "./library";

/** Gap and padding in the preview are scaled by this. */
const PREVIEW_SPACING_FACTOR = 0.4;

/** The browsed layout's zones, its name and status, its shortcut, and its actions. */
export function LayoutPreview({ layout, inUse }: { layout: Layout; inUse: Layout | undefined }) {
  const app = useAppState();
  const status = layoutStatus(app, layout, inUse);
  const shortcut = optionsFor(app.settings, layout).selectShortcut;

  return (
    <section className="preview" aria-label="Layout preview">
      <NumberedLayout layout={layout} />
      <div className="preview-heading">
        <h2 className="preview-name">{layout.name}</h2>
        {status === "inUse" && (
          <span className="preview-status mint">
            <CheckCircleIcon size={14} />
            In use
          </span>
        )}
        {status === "inactive" && <span className="preview-status muted">Inactive</span>}
      </div>
      {shortcut && <p className="preview-shortcut mono mint">{compactLabel(shortcut, app.platform)}</p>}
      <LayoutActions layout={layout} inUse={status === "inUse"} />
    </section>
  );
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
