import { aspectRatio } from "../core/layouts";
import type { Layout } from "../core/model";
import { useAppState, useSelectedDisplay, useServices } from "../state/services";
import { CheckCircleIcon } from "../ui/icons";
import { layoutStatus } from "./library";

interface LayoutCardProps {
  layout: Layout;
  browsed: boolean;
  inUse: Layout | undefined;
}

/** A thumbnail, the name, and "Inactive" or a checkmark. Clicking browses the layout. */
export function LayoutCard({ layout, browsed, inUse }: LayoutCardProps) {
  const app = useAppState();
  const { session } = useServices();
  const status = layoutStatus(app, layout, inUse);

  return (
    <div role="listitem">
      <button
        type="button"
        className="layout-card"
        aria-label={layout.name}
        aria-pressed={browsed}
        data-inactive={status === "inactive"}
        onClick={() => session.update((current) => ({ ...current, browsedLayoutId: layout.id }))}
      >
        <Thumbnail layout={layout} highlighted={browsed} />
        <span className="layout-card-footer">
          <span className="layout-card-name">{layout.name}</span>
          {status === "inactive" && <span className="layout-card-status muted">Inactive</span>}
          {status === "inUse" && (
            <span className="layout-card-status mint" title="In use on this display">
              <CheckCircleIcon size={13} />
            </span>
          )}
        </span>
      </button>
    </div>
  );
}

/** A small schematic of the layout's zones at the selected display's shape. */
function Thumbnail({ layout, highlighted }: { layout: Layout; highlighted: boolean }) {
  const aspect = aspectRatio(useSelectedDisplay());
  const height = 56;
  const width = Math.round(height * aspect);
  return (
    <svg
      className="thumbnail"
      data-highlighted={highlighted}
      width={width}
      height={height}
      aria-hidden="true"
    >
      {layout.zones.map((zone) => (
        <rect
          key={zone.id}
          x={zone.rect.x * width + 1.5}
          y={zone.rect.y * height + 1.5}
          width={Math.max(1, zone.rect.width * width - 3)}
          height={Math.max(1, zone.rect.height * height - 3)}
          rx={3}
        />
      ))}
    </svg>
  );
}
