import { useAppState } from "../state/services";
import { AlertIcon } from "../ui/icons";

/** Why Snappy can't control windows here, and what to do about it, while that's the case. */
export function UnavailableNotice() {
  const { snapping } = useAppState();
  if (snapping.unavailableReason === null) {
    return null;
  }
  return (
    <div className="unavailable-notice" role="status">
      <span className="unavailable-notice-icon">
        <AlertIcon />
      </span>
      <span>{snapping.unavailableReason}</span>
    </div>
  );
}
