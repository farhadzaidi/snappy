import { aspectRatio } from "../core/layouts";
import type { Layout } from "../core/model";
import { useAppState, useSelectedDisplay, useServices } from "../state/services";
import { CheckCircleIcon } from "../ui/icons";
import { ZoneThumbnail } from "../ui/ZoneThumbnail";
import { layoutStatus, type LayoutStatus } from "./library";

const THUMBNAIL_HEIGHT = 56;

/** What a card's status says after its name, for assistive technology. */
const SPOKEN_STATUS: Record<NonNullable<LayoutStatus>, string> = {
  inUse: "in use on this display",
  disabled: "disabled",
};

interface LayoutCardProps {
  layout: Layout;
  browsed: boolean;
  inUse: Layout | undefined;
}

/** A thumbnail, the name, and "Disabled" or a checkmark. Clicking browses the layout. */
export function LayoutCard({ layout, browsed, inUse }: LayoutCardProps) {
  const app = useAppState();
  const { session } = useServices();
  const status = layoutStatus(app, layout, inUse);
  const display = useSelectedDisplay();

  return (
    <div role="listitem">
      <button
        type="button"
        className="layout-card"
        aria-label={status === null ? layout.name : `${layout.name}, ${SPOKEN_STATUS[status]}`}
        aria-pressed={browsed}
        data-disabled={status === "disabled"}
        onClick={() => session.update((current) => ({ ...current, browsedLayoutId: layout.id }))}
      >
        <ZoneThumbnail
          zones={layout.zones}
          aspect={aspectRatio(display)}
          height={THUMBNAIL_HEIGHT}
          highlighted={() => browsed}
        />
        <span className="layout-card-footer">
          <span className="layout-card-name">{layout.name}</span>
          {status === "disabled" && <span className="layout-card-status muted">Disabled</span>}
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
