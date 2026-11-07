import { useAppState, useSend } from "../state/services";
import { PauseIcon, PlayIcon, RefreshIcon } from "../ui/icons";

/**
 * "Snapping on" while live, "Check access" while waiting for permission, and
 * "Enable snapping" otherwise. A click turns snapping off while live and on otherwise.
 */
export function SnappingButton() {
  const { snapping } = useAppState();
  const send = useSend();
  const title = snapping.live
    ? "Snapping on"
    : snapping.waitingForPermission
      ? "Check access"
      : "Enable snapping";
  const icon = snapping.live ? <PauseIcon /> : snapping.waitingForPermission ? <RefreshIcon /> : <PlayIcon />;

  return (
    <button
      type="button"
      className="snapping-button"
      data-live={snapping.live}
      disabled={snapping.unavailableReason !== null}
      title={snapping.unavailableReason ?? undefined}
      onClick={() => void send({ kind: "setSnapping", enabled: !snapping.live })}
    >
      <span className="status-dot" aria-hidden="true" />
      <span>{title}</span>
      {icon}
    </button>
  );
}
