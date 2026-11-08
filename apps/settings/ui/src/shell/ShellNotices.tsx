import type { Remedy } from "../core/model";
import { useAppState, useSend } from "../state/services";
import { Button } from "../ui/Button";
import { AlertIcon, InfoIcon } from "../ui/icons";

const REMEDY_LABELS: Record<Remedy, string> = {
  logOut: "Log out now",
};

/**
 * Why Snappy can't control windows here, with a button when Snappy can fix it, and notes
 * about the desktop, such as another tool that also snaps windows.
 */
export function UnavailableNotice() {
  const { snapping, platformNotes } = useAppState();
  const send = useSend();
  const remedy = snapping.unavailableRemedy;
  return (
    <>
      {snapping.unavailableReason !== null && (
        <div className="unavailable-notice" role="status">
          <span className="unavailable-notice-icon">
            <AlertIcon />
          </span>
          <span className="unavailable-notice-text">{snapping.unavailableReason}</span>
          {remedy !== null && (
            <Button variant="primary" onClick={() => void send({ kind: "applyRemedy", remedy })}>
              {REMEDY_LABELS[remedy]}
            </Button>
          )}
        </div>
      )}
      {platformNotes.map((note) => (
        <div key={note} className="unavailable-notice platform-note" role="note">
          <span className="platform-note-icon">
            <InfoIcon />
          </span>
          <span className="unavailable-notice-text">{note}</span>
        </div>
      ))}
    </>
  );
}
